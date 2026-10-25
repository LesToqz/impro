//! Windows terminal backend. The primary selection is intentionally unsupported.
mod conpty;
mod widget;
pub use conpty::{hangup_pid, pid_number};
pub use widget::*;

pub fn init() {}
