//! Safe GTK3 terminal bindings for the shared gtk-rs 0.18 stack.
//!
//! One public API, two backends: the libvte-2.91 binding on Unix (`unix.rs`) and a stub on
//! Windows (`windows/mod.rs`) that shows a notice and never runs a child. The `stub-backend`
//! feature compiles the stub on any platform as `impro_vte::stub`, so its logic is testable
//! on Linux; it is never re-exported there.
mod types;
pub use types::{CursorBlinkMode, CursorShape, Format, PtyFlags};

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::*;

#[cfg(any(windows, feature = "stub-backend"))]
#[path = "windows/mod.rs"]
pub mod stub;
#[cfg(windows)]
pub use stub::*;
