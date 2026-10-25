//! Shutdown sequencing of one pseudo-console session. Portable and free of Win32/GTK so the
//! quiet-period, drain and close logic is unit-tested on every host.
//!
//! The machine owns no threads and no clock: the driver feeds it events with timestamps and
//! performs the returned [`Action`]s. Sequence (the driver is `windows/conpty.rs`):
//!
//! ```text
//! Running --process exited--> Draining --quiet / max drain--> Closing --exited && reader EOF--> Done
//! Running|Draining --stop requested-----------------------^
//! ```
//!
//! * `Draining`: the child is gone but its last output may still be on its way. The reader keeps
//!   delivering; the phase ends after [`Timing::quiet`] without output, at most
//!   [`Timing::max_drain`] after the exit.
//! * `Closing`: `ClosePseudoConsole` runs on a helper thread (it can block until the output was
//!   read) while the reader keeps draining until EOF. A stop request additionally terminates a
//!   child that outlives [`Timing::grace`]. If the reader has not reached EOF
//!   [`Timing::close_timeout`] after the close began it is cancelled, and abandoned
//!   [`Timing::abandon`] later, so the exit is always reported.
//! * `Done`: exactly one [`Action::Emit`] was produced; nothing else happens afterwards.
use std::time::{Duration, Instant};

/// How often a stuck blocking read is cancelled again (a cancel can race with the read).
pub const CANCEL_REPEAT: Duration = Duration::from_millis(50);
/// `GetExitCodeProcess` result of a process that is still running.
pub const STILL_ACTIVE: u32 = 259;

/// What the driver learned when its process wait fired: whether a zero-timeout wait on the process
/// handle really reports it as signalled and the code `GetExitCodeProcess` returned. A wait that
/// returned for another reason (failure, wrong handle) must never be taken for an exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitProbe {
    pub signalled: bool,
    pub code: u32,
}
impl ExitProbe {
    /// The process has really exited. `STILL_ACTIVE` with an unsignalled handle is a running process;
    /// a signalled handle is an exit whatever its code (a process may exit with 259).
    pub fn is_exit(self) -> bool {
        self.signalled || self.code != STILL_ACTIVE
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    /// Output-free period after the child exited that ends the drain.
    pub quiet: Duration,
    /// Upper bound of the drain, measured from the exit.
    pub max_drain: Duration,
    /// Time a stopped child may run before it is terminated.
    pub grace: Duration,
    /// Time the reader may need to reach EOF after the close began before it is cancelled.
    pub close_timeout: Duration,
    /// Time after the first cancel before the reader is given up on.
    pub abandon: Duration,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            quiet: Duration::from_millis(120),
            max_drain: Duration::from_millis(1000),
            grace: Duration::from_millis(250),
            close_timeout: Duration::from_millis(3000),
            abandon: Duration::from_millis(2000),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Running,
    Draining,
    Closing,
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Call `ClosePseudoConsole` on a helper thread; the reader keeps draining.
    StartClose,
    /// Terminate the child (stop requested and the grace period elapsed).
    Terminate,
    /// Make the reader's blocking read fail (and discard further output).
    CancelRead,
    /// Report the exit to the main context. Produced exactly once; `forced` means the reader never
    /// reached EOF and was abandoned.
    Emit { forced: bool },
}

#[derive(Debug)]
pub struct Lifecycle {
    timing: Timing,
    phase: Phase,
    exited_at: Option<Instant>,
    last_output: Option<Instant>,
    stop_at: Option<Instant>,
    close_started: Option<Instant>,
    cancel_started: Option<Instant>,
    last_cancel: Option<Instant>,
    close_returned: bool,
    reader_eof: bool,
    abandoned: bool,
    terminated: bool,
    spurious_exits: u32,
}

impl Lifecycle {
    pub fn new(timing: Timing) -> Self {
        Self {
            timing,
            phase: Phase::Running,
            exited_at: None,
            last_output: None,
            stop_at: None,
            close_started: None,
            cancel_started: None,
            last_cancel: None,
            close_returned: false,
            reader_eof: false,
            abandoned: false,
            terminated: false,
            spurious_exits: 0,
        }
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    pub fn exited(&self) -> bool {
        self.exited_at.is_some()
    }
    pub fn reader_done(&self) -> bool {
        self.reader_eof
    }
    pub fn close_returned(&self) -> bool {
        self.close_returned
    }

    /// The reader obtained bytes (or finished handing a packet to the main context).
    pub fn output(&mut self, now: Instant) {
        if self.phase != Phase::Done {
            self.last_output = Some(now);
        }
    }
    /// The process-exit wait fired and the driver probed the process. An event that does not describe
    /// a real exit (still running) is counted and ignored; the machine stays where it was.
    pub fn process_probed(&mut self, now: Instant, probe: ExitProbe) -> Vec<Action> {
        if !probe.is_exit() {
            self.spurious_exits += 1;
            return Vec::new();
        }
        self.process_exited(now)
    }
    /// Exit events ignored because the process was still running.
    pub fn spurious_exits(&self) -> u32 {
        self.spurious_exits
    }
    /// The process-exit wait fired.
    pub fn process_exited(&mut self, now: Instant) -> Vec<Action> {
        if self.phase == Phase::Done || self.exited_at.is_some() {
            return Vec::new();
        }
        self.exited_at = Some(now);
        if self.phase == Phase::Running {
            self.phase = Phase::Draining;
        }
        self.advance(now)
    }
    /// The widget was closed or the child was asked to hang up.
    pub fn stop(&mut self, now: Instant) -> Vec<Action> {
        if self.phase == Phase::Done || self.stop_at.is_some() {
            return Vec::new();
        }
        self.stop_at = Some(now);
        let mut actions = Vec::new();
        if matches!(self.phase, Phase::Running | Phase::Draining) {
            self.begin_close(now, &mut actions);
        }
        actions.extend(self.advance(now));
        actions
    }
    /// The output pipe reached EOF, failed, or the reader thread otherwise ended.
    pub fn reader_eof(&mut self, now: Instant) -> Vec<Action> {
        if self.phase == Phase::Done || self.reader_eof {
            return Vec::new();
        }
        self.reader_eof = true;
        self.advance(now)
    }
    /// `ClosePseudoConsole` returned.
    pub fn close_returned_at(&mut self, now: Instant) -> Vec<Action> {
        if self.phase == Phase::Done || self.close_returned {
            return Vec::new();
        }
        self.close_returned = true;
        self.advance(now)
    }
    /// Time-driven progress; call when [`Lifecycle::next_deadline`] passed.
    pub fn poll(&mut self, now: Instant) -> Vec<Action> {
        if self.phase == Phase::Done {
            return Vec::new();
        }
        self.advance(now)
    }

    /// The next instant at which `poll` can produce something, if any. `None` means the driver may
    /// block until the next event.
    pub fn next_deadline(&self) -> Option<Instant> {
        match self.phase {
            Phase::Running | Phase::Done => None,
            Phase::Draining => self.drain_deadline(),
            Phase::Closing => {
                let mut deadline: Option<Instant> = None;
                let mut offer = |t: Instant| deadline = Some(deadline.map_or(t, |d| d.min(t)));
                if let Some(stop) = self.stop_at {
                    if !self.exited() && !self.terminated {
                        offer(stop + self.timing.grace);
                    }
                }
                if !self.reader_eof {
                    match (self.cancel_started, self.last_cancel, self.close_started) {
                        (Some(first), Some(last), _) => {
                            offer(first + self.timing.abandon);
                            offer(last + CANCEL_REPEAT);
                        }
                        (None, _, Some(started)) => offer(started + self.timing.close_timeout),
                        _ => {}
                    }
                }
                deadline
            }
        }
    }

    fn drain_deadline(&self) -> Option<Instant> {
        let exited = self.exited_at?;
        let base = self.last_output.map_or(exited, |o| o.max(exited));
        Some((base + self.timing.quiet).min(exited + self.timing.max_drain))
    }
    fn begin_close(&mut self, now: Instant, actions: &mut Vec<Action>) {
        self.phase = Phase::Closing;
        self.close_started = Some(now);
        actions.push(Action::StartClose);
    }
    fn advance(&mut self, now: Instant) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.phase == Phase::Draining
            && (self.reader_eof || self.drain_deadline().is_some_and(|d| now >= d))
        {
            self.begin_close(now, &mut actions);
        }
        if self.phase == Phase::Closing {
            if let Some(stop) = self.stop_at {
                if !self.exited() && !self.terminated && now >= stop + self.timing.grace {
                    self.terminated = true;
                    actions.push(Action::Terminate);
                }
            }
            if !self.reader_eof {
                if let (Some(first), Some(last)) = (self.cancel_started, self.last_cancel) {
                    if now >= first + self.timing.abandon {
                        self.reader_eof = true;
                        self.abandoned = true;
                    } else if now >= last + CANCEL_REPEAT {
                        self.last_cancel = Some(now);
                        actions.push(Action::CancelRead);
                    }
                } else if self
                    .close_started
                    .is_some_and(|s| now >= s + self.timing.close_timeout)
                {
                    self.cancel_started = Some(now);
                    self.last_cancel = Some(now);
                    actions.push(Action::CancelRead);
                }
            }
            if self.exited() && self.reader_eof {
                self.phase = Phase::Done;
                actions.push(Action::Emit {
                    forced: self.abandoned,
                });
            }
        }
        actions
    }
}

/// Finds cursor-position requests (`ESC [ 6 n`, `ESC [ ? 6 n`) in an output stream, also when they
/// are split across reads. conhost may issue one while shutting down and waits for the answer.
#[derive(Clone, Copy, Debug, Default)]
pub struct CprScanner {
    state: u8,
}
/// Answer to a cursor-position request: row 1, column 1.
pub const CPR_REPLY: &[u8] = b"\x1b[1;1R";
impl CprScanner {
    /// Number of requests completed by `bytes`.
    pub fn scan(&mut self, bytes: &[u8]) -> usize {
        let mut found = 0;
        for &b in bytes {
            self.state = match (self.state, b) {
                (_, 0x1b) => 1,
                (1, b'[') => 2,
                (2, b'?') => 2,
                (2, b'6') => 3,
                (3, b'n') => {
                    found += 1;
                    0
                }
                _ => 0,
            };
        }
        found
    }
}
