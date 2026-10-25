//! Embedded read-only Git pages and their GTK hosts.
//!
//! Call [`init`] at startup (it creates no WebKit objects). Construct a
//! [`GitGraphView`] only when a pane opens, and put `widget()` in that pane.
//! `refresh`, `focus_head`, `grab_focus`, `select`, `set_root`, `shutdown` implement
//! the window commands and lifecycle. Refresh preserves commit tabs; on
//! `connect_root_changed`, close old repository commit tabs in the editor.
//!
//! Connect `connect_open_commit` on both hosts to the window's commit opener.
//! Validate/resolve abbreviated hashes on a worker, deduplicate by
//! `commit:<full hash>`, create [`CommitView`] with an optional [`SyntaxAdapter`],
//! then add `widget()` using the editor's view-tab API at the right end.
//! `tab_key()` supplies the stable key; `rev()` supplies the label/tooltip hash.
//! `connect_open_file` supplies an existing canonical working-tree path and a
//! **one-based** line; subtract one for the editor. Parent chips reuse the commit
//! opener. `connect_error` supplies page/JavaScript errors to window notices.
//!
//! On editor selection changes, call `select(Some(hash))` on every graph, or
//! `select(None)` for other tabs. Call `shutdown` when a pane/tab closes; widget
//! destruction and dropping the final handle also clean up. Git pages share a
//! process; closing one leaves its siblings alive. Workers are bounded by Git
//! deadlines and discard cancelled/stale results without blocking GTK on close.
//!
//! Apply `set_theme` and `set_reduce_motion` to every host on preference changes;
//! apply `CommitView::set_font_size` for live editor zoom. Graph settings use
//! [`GraphSettings`] with `set_settings`/`settings`, or the individual
//! `set_orientation`, `set_options`, `set_max_commits` methods. `load_settings`
//! reads the raw core blob on a worker; `GraphSettings::save` writes it on a
//! worker. `connect_orientation_changed` lets the window synchronize other panes
//! and the dialog. Monitor bursts/focus-in refresh debounce for 300 ms. Hidden
//! graph panes defer dirty collections and pause the page until mapped again.
//!
//! The window owns chooser/palette entries (Open git graph, Refresh, Jump to
//! HEAD, Close commit tabs), Ctrl+Alt+H, dock choice/capacity, and the tab registry.
//! `webview`, `last_data`, `is_ready`, `is_closed`, `root` and `orientation`
//! support host inspection. `timings()` returns [`PaintTimings`] with
//! construction-to-paint and data-delivery-to-paint durations. [`assets`] exposes standalone embedded HTML for QA.

pub mod assets;
pub mod diff;
#[cfg(unix)]
pub mod gitviews;
#[cfg(windows)]
#[path = "gitviews_windows.rs"]
pub mod gitviews;
pub mod log;
pub mod palette;
mod types;
pub use gitviews::{CommitView, GitGraphView, GraphSettings, PaintTimings, SyntaxAdapter};

/// No native CSS is needed: both pages own their styles.
pub fn css(_: &impro_core::themes::UiColors) -> String {
    String::new()
}
/// Startup hook. Deliberately does not construct a WebView or context.
pub fn init() {}
