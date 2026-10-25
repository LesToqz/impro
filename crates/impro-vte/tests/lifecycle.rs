//! The pseudo-console shutdown state machine, driven with a virtual clock.
#![cfg(feature = "emulator")]
use impro_vte::{
    diag::escape,
    input::{pump, InputEnd, Message},
    lifecycle::{
        Action, CprScanner, ExitProbe, Lifecycle, Phase, Timing, CANCEL_REPEAT, CPR_REPLY,
        STILL_ACTIVE,
    },
};
use std::{
    io::{self, Write},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
struct Clock(Instant);
impl Clock {
    fn at(&self, n: u64) -> Instant {
        self.0 + ms(n)
    }
}
fn machine() -> (Lifecycle, Clock) {
    (Lifecycle::new(Timing::default()), Clock(Instant::now()))
}
fn emits(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| matches!(a, Action::Emit { .. }))
        .count()
}

/// Advance the virtual clock to `until` in 5 ms steps, polling at deadlines; collect actions.
fn run(life: &mut Lifecycle, clock: &Clock, from: u64, until: u64) -> Vec<(u64, Action)> {
    let mut log = Vec::new();
    let mut t = from;
    while t <= until {
        let due = life.next_deadline().is_some_and(|d| d <= clock.at(t));
        if due {
            for a in life.poll(clock.at(t)) {
                log.push((t, a));
            }
        }
        t += 5;
    }
    log
}

#[test]
fn exit_after_output_waits_for_the_quiet_period_then_closes_and_emits_once() {
    let (mut life, c) = machine();
    for t in [0, 20, 40] {
        life.output(c.at(t));
    }
    assert!(life.process_exited(c.at(60)).is_empty());
    assert_eq!(life.phase(), Phase::Draining);
    // Output keeps arriving after the exit: the quiet period restarts.
    life.output(c.at(100));
    assert!(life.poll(c.at(210)).is_empty());
    assert_eq!(life.poll(c.at(220)), [Action::StartClose]);
    assert_eq!(life.phase(), Phase::Closing);
    assert_eq!(life.reader_eof(c.at(240)), [Action::Emit { forced: false }]);
}

#[test]
fn instant_exit_without_output_closes_after_the_quiet_period_from_the_exit() {
    let (mut life, c) = machine();
    assert!(life.process_exited(c.at(10)).is_empty());
    assert_eq!(life.next_deadline(), Some(c.at(130)));
    assert!(life.poll(c.at(129)).is_empty());
    assert_eq!(life.poll(c.at(130)), [Action::StartClose]);
    assert!(life.close_returned_at(c.at(140)).is_empty());
    assert_eq!(life.reader_eof(c.at(150)), [Action::Emit { forced: false }]);
    assert_eq!(life.phase(), Phase::Done);
    assert_eq!(life.next_deadline(), None);
}

#[test]
fn endless_output_after_exit_is_cut_off_at_the_drain_limit() {
    let (mut life, c) = machine();
    life.process_exited(c.at(0));
    let mut closed_at = None;
    for t in (0..3000).step_by(10) {
        life.output(c.at(t));
        if life.poll(c.at(t)).contains(&Action::StartClose) {
            closed_at = Some(t);
            break;
        }
    }
    assert_eq!(closed_at, Some(1000));
}

#[test]
fn exit_before_any_output_then_late_output_is_still_delivered_by_the_reader() {
    // The machine only times the drain; output after the close began does not delay the exit.
    let (mut life, c) = machine();
    life.process_exited(c.at(0));
    assert_eq!(life.poll(c.at(120)), [Action::StartClose]);
    life.output(c.at(130));
    life.output(c.at(500));
    assert_eq!(life.reader_eof(c.at(520)), [Action::Emit { forced: false }]);
}

#[test]
fn eof_before_exit_closes_immediately_when_the_exit_is_observed() {
    let (mut life, c) = machine();
    assert!(life.reader_eof(c.at(5)).is_empty());
    assert_eq!(life.phase(), Phase::Running);
    assert_eq!(
        life.process_exited(c.at(20)),
        [Action::StartClose, Action::Emit { forced: false }]
    );
    assert_eq!(life.phase(), Phase::Done);
}

#[test]
fn eof_during_the_drain_skips_the_remaining_quiet_period() {
    let (mut life, c) = machine();
    life.process_exited(c.at(0));
    assert_eq!(
        life.reader_eof(c.at(30)),
        [Action::StartClose, Action::Emit { forced: false }]
    );
}

#[test]
fn double_events_emit_exactly_once() {
    let (mut life, c) = machine();
    life.process_exited(c.at(0));
    assert!(life.process_exited(c.at(1)).is_empty());
    life.poll(c.at(200));
    let first = life.reader_eof(c.at(210));
    assert_eq!(emits(&first), 1);
    assert!(life.reader_eof(c.at(211)).is_empty());
    assert!(life.process_exited(c.at(212)).is_empty());
    assert!(life.close_returned_at(c.at(213)).is_empty());
    assert!(life.close_returned_at(c.at(214)).is_empty());
    assert!(life.stop(c.at(215)).is_empty());
    assert!(life.poll(c.at(10_000)).is_empty());
    life.output(c.at(10_001));
    assert_eq!(life.phase(), Phase::Done);
}

/// A blocking `ClosePseudoConsole`: it only returns once the reader drained the pipe. The
/// closure is the model of the close call, the loop the model of the reader.
#[test]
fn blocking_close_returns_only_after_the_reader_drained() {
    let (mut life, c) = machine();
    let mut pipe_bytes = 10_000usize; // buffered, not yet read
    let mut close_started = false;
    let mut close_returned = false;
    let mut reader_done = false;
    let mut emitted = Vec::new();
    // The blocking close: true when it may return.
    let close_may_return = |pipe: usize| pipe == 0;

    life.process_exited(c.at(0));
    let mut t = 0u64;
    while t < 5000 && emitted.is_empty() {
        let mut actions = Vec::new();
        // Reader: 2 KiB per 5 ms tick while data remains, then EOF once close returned.
        if pipe_bytes > 0 {
            pipe_bytes = pipe_bytes.saturating_sub(2048);
            // Reading while the close is not yet started still counts as output.
            if !close_started {
                life.output(c.at(t));
            }
        }
        if close_started && !close_returned && close_may_return(pipe_bytes) {
            close_returned = true;
            actions.extend(life.close_returned_at(c.at(t)));
        }
        if close_returned && !reader_done {
            reader_done = true;
            actions.extend(life.reader_eof(c.at(t)));
        }
        if life.next_deadline().is_some_and(|d| d <= c.at(t)) {
            actions.extend(life.poll(c.at(t)));
        }
        for a in actions {
            match a {
                Action::StartClose => close_started = true,
                Action::Emit { forced } => emitted.push((t, forced)),
                other => panic!("unexpected {other:?}"),
            }
        }
        t += 5;
    }
    assert_eq!(pipe_bytes, 0);
    assert!(close_started && close_returned && reader_done);
    assert_eq!(emitted.len(), 1);
    assert!(!emitted[0].1, "not forced: the reader drained and hit EOF");
    assert!(emitted[0].0 >= 120, "after the quiet period");
}

#[test]
fn a_close_that_never_returns_cancels_then_abandons_the_reader_and_still_emits() {
    let (mut life, c) = machine();
    life.process_exited(c.at(0));
    let log = run(&mut life, &c, 0, 10_000);
    let cancels: Vec<u64> = log
        .iter()
        .filter(|(_, a)| *a == Action::CancelRead)
        .map(|(t, _)| *t)
        .collect();
    // Close begins at 120 ms; the first cancel is `close_timeout` later, repeated every 50 ms
    // until `abandon` after the first cancel.
    assert_eq!(cancels.first(), Some(&3120));
    assert!(cancels
        .windows(2)
        .all(|w| w[1] - w[0] >= CANCEL_REPEAT.as_millis() as u64));
    assert!(cancels.last().unwrap() < &5120);
    let emitted: Vec<_> = log
        .iter()
        .filter(|(_, a)| matches!(a, Action::Emit { .. }))
        .collect();
    assert_eq!(emitted.len(), 1);
    assert_eq!(*emitted[0], (5120, Action::Emit { forced: true }));
}

#[test]
fn cancelling_the_read_ends_the_reader_and_emits_unforced() {
    let (mut life, c) = machine();
    life.process_exited(c.at(0));
    let log = run(&mut life, &c, 0, 3200);
    assert!(log.contains(&(3120, Action::CancelRead)));
    assert_eq!(
        life.reader_eof(c.at(3125)),
        [Action::Emit { forced: false }]
    );
}

#[test]
fn stop_while_running_closes_at_once_and_terminates_after_the_grace_period() {
    let (mut life, c) = machine();
    assert_eq!(life.stop(c.at(100)), [Action::StartClose]);
    assert_eq!(life.phase(), Phase::Closing);
    assert_eq!(life.next_deadline(), Some(c.at(350)));
    assert!(life.poll(c.at(349)).is_empty());
    assert_eq!(life.poll(c.at(350)), [Action::Terminate]);
    // Terminating is requested once; the exit then follows from the process wait.
    assert!(life.poll(c.at(400)).is_empty());
    assert!(life.process_exited(c.at(360)).is_empty());
    assert_eq!(life.reader_eof(c.at(370)), [Action::Emit { forced: false }]);
}

#[test]
fn a_child_that_leaves_within_the_grace_period_is_not_terminated() {
    let (mut life, c) = machine();
    life.stop(c.at(0));
    assert!(life.process_exited(c.at(100)).is_empty());
    assert!(life.poll(c.at(400)).iter().all(|a| *a != Action::Terminate));
    assert_eq!(life.reader_eof(c.at(410)), [Action::Emit { forced: false }]);
}

#[test]
fn stop_during_the_drain_closes_immediately() {
    let (mut life, c) = machine();
    life.process_exited(c.at(0));
    assert_eq!(life.stop(c.at(10)), [Action::StartClose]);
    assert!(life.stop(c.at(11)).is_empty());
    assert_eq!(life.reader_eof(c.at(20)), [Action::Emit { forced: false }]);
}

#[test]
fn stop_after_eof_and_exit_does_nothing() {
    let (mut life, c) = machine();
    life.process_exited(c.at(0));
    life.reader_eof(c.at(1));
    assert_eq!(life.phase(), Phase::Done);
    assert!(life.stop(c.at(2)).is_empty());
}

#[test]
fn running_machine_never_asks_for_a_deadline() {
    let (life, _) = machine();
    assert_eq!(life.next_deadline(), None);
}

#[test]
fn cursor_position_requests_are_found_across_chunk_boundaries() {
    let mut scanner = CprScanner::default();
    assert_eq!(scanner.scan(b"plain text"), 0);
    assert_eq!(scanner.scan(b"a\x1b[6nb"), 1);
    assert_eq!(scanner.scan(b"\x1b[?6n\x1b[6n"), 2);
    assert_eq!(scanner.scan(b"\x1b["), 0);
    assert_eq!(scanner.scan(b"6"), 0);
    assert_eq!(scanner.scan(b"n"), 1);
    assert_eq!(scanner.scan(b"\x1b[5n\x1b[16n\x1b[6"), 0);
    assert_eq!(scanner.scan(b"x"), 0);
    assert_eq!(CPR_REPLY, b"\x1b[1;1R");
}

// --- exit events must describe a real exit -----------------------------------------------------

#[test]
fn an_exit_event_for_a_running_process_is_not_an_exit() {
    // A wait that returned while the child still runs (STILL_ACTIVE, handle not signalled).
    let running = ExitProbe {
        signalled: false,
        code: STILL_ACTIVE,
    };
    assert!(!running.is_exit());
    let (mut life, c) = machine();
    assert!(life.process_probed(c.at(10), running).is_empty());
    assert!(life.process_probed(c.at(20), running).is_empty());
    assert_eq!(life.spurious_exits(), 2);
    assert_eq!(life.phase(), Phase::Running);
    assert!(!life.exited());
    assert_eq!(life.next_deadline(), None);
    // The reader ending while the shell still "runs" must not close anything either.
    assert!(life.reader_eof(c.at(30)).is_empty());
    assert_eq!(life.phase(), Phase::Running);
    // The real exit afterwards behaves normally, exactly once.
    let real = ExitProbe {
        signalled: true,
        code: 0,
    };
    assert!(real.is_exit());
    // The reader had already ended, so the close and the report follow the exit at once.
    assert_eq!(
        life.process_probed(c.at(40), real),
        [Action::StartClose, Action::Emit { forced: false }]
    );
    assert_eq!(life.phase(), Phase::Done);
    assert!(life.process_probed(c.at(41), real).is_empty());
    assert_eq!(life.spurious_exits(), 2);
}

#[test]
fn probe_classification() {
    for (signalled, code, exit) in [
        (false, STILL_ACTIVE, false),
        (true, STILL_ACTIVE, true), // a process may really exit with 259
        (true, 0, true),
        (false, 0, true),
        (false, 7, true),
    ] {
        assert_eq!(ExitProbe { signalled, code }.is_exit(), exit);
    }
}

// --- the input pipe belongs to the session ------------------------------------------------------

/// A writer that records what it received and fails once `fail_after` writes happened.
struct Pipe {
    received: Vec<u8>,
    writes: usize,
    fail_after: usize,
}
impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.writes >= self.fail_after {
            return Err(io::Error::from(io::ErrorKind::BrokenPipe));
        }
        self.writes += 1;
        self.received.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn pipe(fail_after: usize) -> Pipe {
    Pipe {
        received: Vec::new(),
        writes: 0,
        fail_after,
    }
}

#[test]
fn input_loop_lives_until_the_session_closes_it_even_if_the_request_sender_is_gone() {
    let (session, queue) = mpsc::channel::<Message>();
    // The spawn request owned a clone of the sender; it is dropped long before the session ends.
    let request = session.clone();
    request.send(Some(b"echo early\r".to_vec())).unwrap();
    drop(request);
    let worker = thread::spawn(move || {
        let mut target = pipe(usize::MAX);
        let mut lines = Vec::new();
        let end = pump(&queue, &mut target, &mut |l| lines.push(l.to_owned()));
        (end, target.received, lines)
    });
    thread::sleep(Duration::from_millis(100));
    assert!(
        !worker.is_finished(),
        "dropping the request ended the input"
    );
    session.send(Some(b"dir\r".to_vec())).unwrap();
    session.send(None).unwrap();
    let (end, received, lines) = worker.join().unwrap();
    assert_eq!(end, InputEnd::Closed);
    assert_eq!(received, b"echo early\rdir\r");
    assert!(lines.iter().any(|l| l.contains("input write #1: 11 bytes")));
}

#[test]
fn input_loop_ends_when_every_sender_is_gone() {
    let (session, queue) = mpsc::channel::<Message>();
    session.send(Some(b"a".to_vec())).unwrap();
    drop(session);
    let mut target = pipe(usize::MAX);
    assert_eq!(
        pump(&queue, &mut target, &mut |_| {}),
        InputEnd::Disconnected
    );
    assert_eq!(target.received, b"a");
}

#[test]
fn a_failed_write_keeps_the_pipe_open_and_discards_the_rest() {
    let (session, queue) = mpsc::channel::<Message>();
    for chunk in [&b"ok"[..], b"lost", b"more"] {
        session.send(Some(chunk.to_vec())).unwrap();
    }
    let worker = thread::spawn(move || {
        let mut target = pipe(1);
        let mut lines = Vec::new();
        let end = pump(&queue, &mut target, &mut |l| lines.push(l.to_owned()));
        (end, target, lines)
    });
    // The loop must still be there after the failure (it only ends on the marker).
    thread::sleep(Duration::from_millis(100));
    assert!(!worker.is_finished(), "a write error ended the input loop");
    session.send(Some(b"after".to_vec())).unwrap();
    session.send(None).unwrap();
    let (end, target, lines) = worker.join().unwrap();
    assert_eq!(end, InputEnd::Closed);
    assert_eq!(target.received, b"ok");
    assert!(lines.iter().any(|l| l.contains("input write failed")));
    assert!(lines
        .iter()
        .any(|l| l.contains("13 bytes of input were discarded")));
}

#[test]
fn only_the_first_writes_are_described() {
    let (session, queue) = mpsc::channel::<Message>();
    for _ in 0..100 {
        session.send(Some(b"x".to_vec())).unwrap();
    }
    session.send(None).unwrap();
    let mut target = pipe(usize::MAX);
    let mut lines = Vec::new();
    pump(&queue, &mut target, &mut |l| lines.push(l.to_owned()));
    assert_eq!(target.received.len(), 100);
    assert_eq!(lines.len(), 40);
}

#[test]
fn escaped_bytes_are_readable_and_bounded() {
    assert_eq!(escape(b"\x1b[?9001h\x1b[I", 64), "\\e[?9001h\\e[I");
    assert_eq!(escape(b"a\r\n\\\x00\xff", 64), "a\\r\\n\\\\\\x00\\xff");
    assert_eq!(escape(b"abcdef", 3), "abc...(+3)");
}
