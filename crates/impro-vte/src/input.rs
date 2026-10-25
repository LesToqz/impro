//! Input side of a pseudo-console session: the loop that feeds the child's input pipe. Portable
//! (generic over `Write`) so its lifetime rules are unit-tested on every host.
//!
//! The pipe's write end belongs to the *session*: the loop ends only when the session says so (a
//! closing marker) or when every sender is gone. A failed write never ends it, because dropping
//! the write end is an end-of-input for the console host and an interactive shell leaves on that.
//! After a failure further input is discarded until the session closes.
use crate::diag::escape;
use std::{
    io::Write,
    sync::mpsc::{Receiver, Sender},
};

/// `None` is the closing marker, `Some` is input for the child.
pub type Message = Option<Vec<u8>>;
pub type InputSender = Sender<Message>;

/// Why the input loop ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEnd {
    /// The session sent the closing marker.
    Closed,
    /// Every sender was dropped (the session is gone).
    Disconnected,
}
impl InputEnd {
    pub fn reason(self) -> &'static str {
        match self {
            Self::Closed => "session closed the input",
            Self::Disconnected => "every sender of the input queue was dropped",
        }
    }
}

/// How many writes are described in the log (the start-up traffic is what matters).
const LOGGED_WRITES: usize = 40;

/// Write queued input to `writer` until the session ends it. `log` receives one line per event.
pub fn pump<W: Write>(
    queue: &Receiver<Message>,
    writer: &mut W,
    log: &mut dyn FnMut(&str),
) -> InputEnd {
    let mut writes = 0usize;
    let mut failed = false;
    let mut lost = 0usize;
    let end = loop {
        match queue.recv() {
            Err(_) => break InputEnd::Disconnected,
            Ok(None) => break InputEnd::Closed,
            Ok(Some(bytes)) => {
                if failed {
                    lost += bytes.len();
                    continue;
                }
                writes += 1;
                if writes <= LOGGED_WRITES {
                    log(&format!(
                        "input write #{writes}: {} bytes [{}]",
                        bytes.len(),
                        escape(&bytes, 48)
                    ));
                }
                if let Err(error) = writer.write_all(&bytes).and_then(|()| writer.flush()) {
                    failed = true;
                    lost += bytes.len();
                    log(&format!(
                        "input write failed: {error}; the pipe stays open and further input is discarded"
                    ));
                }
            }
        }
    };
    if lost > 0 {
        log(&format!("{lost} bytes of input were discarded"));
    }
    end
}
