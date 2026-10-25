//! Backend-independent value types of the terminal API.
/// Clipboard representation understood by VTE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Format {
    Text = 1,
    Html = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum CursorShape {
    Block = 0,
    Ibeam = 1,
    Underline = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum CursorBlinkMode {
    System = 0,
    On = 1,
    Off = 2,
}
bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, Default)]
    pub struct PtyFlags: u32 { const DEFAULT = 0; const NO_SESSION = 32; const NO_CTTY = 64; }
}
