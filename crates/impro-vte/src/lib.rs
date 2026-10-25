//! GTK3 terminals: libvte on Unix and ConPTY on Windows.
mod types;
pub use types::{CursorBlinkMode, CursorShape, Format, PtyFlags};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(feature = "stub-backend")]
#[path = "stub.rs"]
pub mod stub;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(any(windows, feature = "emulator"))]
pub mod diag;
#[cfg(any(windows, feature = "emulator"))]
pub mod emu;
#[cfg(any(windows, feature = "emulator"))]
pub mod input;
#[cfg(any(windows, feature = "emulator"))]
pub mod lifecycle;
