//! Terminal-backend diagnostics timeline. Set `IMPRO_TERM_DIAG=1` and every lifecycle event of a
//! pseudo-console session prints one line to stderr:
//!
//! ```text
//! [impro-term +  1.234s #2 pid=4321] ClosePseudoConsole returned after 12 ms
//! ```
//!
//! `+seconds` counts from the first diagnostic line of the process, `#n` numbers spawn requests
//! and `pid` is the child's process id (`-` before the process exists). The lines are also kept in
//! memory ([`timeline`]) so tests can dump them when an assertion fails.
use std::{
    fmt,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Instant,
};

const MAX_LINES: usize = 4000;
static START: OnceLock<Instant> = OnceLock::new();
static ORDINAL: AtomicU32 = AtomicU32::new(0);
static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Whether `IMPRO_TERM_DIAG` asks for the timeline (any value except empty or `0`).
pub fn enabled() -> bool {
    std::env::var_os("IMPRO_TERM_DIAG").is_some_and(|v| !v.is_empty() && v != "0")
}
/// All recorded lines, oldest first.
pub fn timeline() -> Vec<String> {
    LINES.lock().map(|l| l.clone()).unwrap_or_default()
}
/// The timeline as one string, for assertion messages.
pub fn timeline_text() -> String {
    timeline().join("\n")
}
/// Forget all recorded lines.
pub fn clear() {
    if let Ok(mut lines) = LINES.lock() {
        lines.clear();
    }
}
/// `+seconds` label of a time point relative to the first diagnostic line.
fn label(now: Instant) -> String {
    let start = *START.get_or_init(|| now);
    format!(
        "{:8.3}s",
        now.saturating_duration_since(start).as_secs_f64()
    )
}

/// Printable form of `bytes` for the timeline: escapes as `\e`, `\r`, `\n`, `\xNN`, at most `max`
/// bytes followed by the count of the omitted rest.
pub fn escape(bytes: &[u8], max: usize) -> String {
    let mut out = String::new();
    for &b in &bytes[..bytes.len().min(max)] {
        match b {
            0x1b => out.push_str("\\e"),
            b'\r' => out.push_str("\\r"),
            b'\n' => out.push_str("\\n"),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\x{b:02x}")),
        }
    }
    if bytes.len() > max {
        out.push_str(&format!("...(+{})", bytes.len() - max));
    }
    out
}

/// Per-session logger.
#[derive(Clone, Debug)]
pub struct Diag(Arc<Inner>);
#[derive(Debug)]
struct Inner {
    ordinal: u32,
    pid: AtomicU32,
}
impl Diag {
    pub fn new() -> Self {
        Self(Arc::new(Inner {
            ordinal: ORDINAL.fetch_add(1, Ordering::Relaxed) + 1,
            pid: AtomicU32::new(0),
        }))
    }
    pub fn set_pid(&self, pid: u32) {
        self.0.pid.store(pid, Ordering::Relaxed);
    }
    /// Record `message` when diagnostics are enabled; the arguments are not even formatted otherwise.
    pub fn log(&self, message: fmt::Arguments<'_>) {
        if !enabled() {
            return;
        }
        let pid = match self.0.pid.load(Ordering::Relaxed) {
            0 => "-".to_owned(),
            pid => pid.to_string(),
        };
        let line = format!(
            "[impro-term +{} #{} pid={}] {}",
            label(Instant::now()),
            self.0.ordinal,
            pid,
            message
        );
        eprintln!("{line}");
        if let Ok(mut lines) = LINES.lock() {
            if lines.len() >= MAX_LINES {
                lines.remove(0);
            }
            lines.push(line);
        }
    }
}
impl Default for Diag {
    fn default() -> Self {
        Self::new()
    }
}
