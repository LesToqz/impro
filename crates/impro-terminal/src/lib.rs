//! Bottom and right terminal docks, sharing limits with arbitrary view widgets.
//!
//! Call [`init`] before the application's first theme application. Construct
//! `TerminalPanel::new(cwd, autostart, stacked, font_size)` on GTK's main thread;
//! `stacked=false` places bottom panes side by side, `true` stacks right panes.
//! The panel is a native `GtkBox`: ordinary GTK packing, visibility, reparenting,
//! focus and signal APIs apply. Shell resolution runs on a worker; VTE spawn is
//! asynchronous and begins after two painted frames with stable geometry, with
//! a one-second fallback. Early keyboard commits are replayed after raw input.
//!
//! Window integration surface:
//!
//! | Purpose | Methods |
//! | --- | --- |
//! | Creation | `new_terminal`, `add_kind`, `add_custom_pane(kind, widget, shutdown)`, `add_graph(Option<&Widget>)`, `add_gitgraph(Option<&Widget>)` |
//! | Lazy views | `set_factory(kind, Fn() -> Widget)`, `clear_factory`; custom factories are never called at construction |
//! | Availability | `set_pane_kinds(&[&str])`, `set_max_panes(usize)`, `max_panes`, `is_full`, `close_kind` |
//! | Counts | `pane_count`, `terminal_count`, `count_kind`, `graph_count`, `gitgraph_count`, `has_graph`, `has_gitgraph` |
//! | View queries | `get_views(kind)`, `graph_views`, `gitgraph_views`, `header_labels`, `panes`, `closing_panes`, `current_pane` |
//! | Focus / close | `focus_terminal`, `focus_pane(id)`, `close_current`, `close_pane(id)` |
//! | Maximize | `toggle_maximize`, `toggle_maximize_pane(id)`, `is_maximized`, `restore` |
//! | Visibility | `hide_dock` keeps children; `show_dock` starts a shell only when empty; `request_hide` emits a request |
//! | Paths / shell | `set_cwd`, `cwd`, `set_shell` affect future shells only; cwd paths preserve Unix bytes |
//! | Appearance | `apply_theme(&Theme)`, `apply_palette(fg, bg, &[String], cursor)`, `set_font_size`, `set_cursor_shape`, `set_cursor_blink`, `set_scrollback` |
//! | Preferences | `apply_settings(&serde_json::Map<_,_>)`, `set_bell`, `set_copy_on_select`; callers load settings off-thread |
//! | Layout | `set_initial_dock_extent(right, pixels)`, `commit_layout`, `stacked`, `body`, `overlay`, `add_button`, `toast` |
//! | Controls / QA | `handle_key`, `context_menu`, `set_animation_ms`, `set_resize_gate` (future panes), `shutdown` |
//!
//! The default limit is six, clamped to 1–8. All kinds count, while closing panes
//! immediately release their slot and number. Custom kinds are unique per dock;
//! adding a duplicate focuses it and returns false without a limit notice.
//! Register custom kinds through `set_pane_kinds` and supply widgets/factories;
//! this crate has no graph or Git view dependencies. A custom shutdown callback
//! runs exactly once before its closing animation. Use weak panel captures in
//! factories and shutdown callbacks when they refer back to their owning panel.
//!
//! GObject signals, with typed convenience connectors:
//!
//! - `last-closed` / `connect_last_closed`: after the last exit animation; the
//!   window hides the dock here. Closing never automatically respawns a shell.
//! - `maximize-toggled(bool)` / `connect_maximize_toggled`: the window expands
//!   or restores surrounding splitters. Changing the maximized pane emits no
//!   extra `true` signal.
//! - `limit-reached` / `connect_limit_reached`: the notice is already shown.
//! - `feature-disabled(str)` / `connect_feature_disabled`: an unavailable kind.
//! - `hide-requested` / `connect_hide_requested`: explicit window hide request.
//!
//! [`Pane`] snapshots expose `id`, `kind`, `page`, `content`, `header`, `frame`,
//! `terminal`, `view`, `gate`, plus `pid()`, `is_closed()`, `is_closing()` and
//! `cancellable()`. Snapshots keep widgets alive; release them before checking
//! weak references for finalization. Pane identity is stable across renumbering.
//! All state mutations release borrows before GTK or user callbacks execute.
//! `shutdown` is idempotent and runs automatically on widget destruction.
//!
//! [`panesplit`] exports integer apportionment, pairwise boundary movement,
//! `SplitLayout` and the native `PaneSplit` container. Its public methods are
//! `new`, `orientation`, `insert`, `ratios`, `pane_sizes`, `handles`, `equalize`,
//! `move_handle` and `sync_handles`; GTK container removal releases handles.
//! Pointer motion updates existing buffers; double-click equalizes all panes.
//!
//! [`resizegate::ResizeGate`] clips one complete page and suppresses VTE reflow
//! only during its committed allocations, restoring the caller's property.
//! `commit` applies a discrete size immediately; `commit_when_answered` waits
//! for PTY output already in flight. `wait_for_programs` explicitly paces against
//! known terminals. Configure `set_pace_ms`, `set_child_minimum`, `set_reflow_vte`
//! and `set_background`; inspect `target`, `committed`, `waiting_count`,
//! `has_pending_sources` and `commit_count`. Resizes are paced at 50 ms, with one
//! extension when the shell's answer has arrived but VTE has not parsed it.
//! No application timer/tick remains after startup, resizing or motion settles.
//!
//! [`terminal_bar`] exports `PaneHeader` (labels, active/focus/corner/maximize
//! state, three requested-action connectors), `IconButton` (icon fallback and
//! glyph hover), `AddButton` (chooser, kinds, limit, full state and choose
//! callback), `LimitToast` (widget, popup, pending timer), and cancelable `Fade`.
//! The shared motion scheduler honors reduced motion; unmapping settles fades.
//!
//! [`pick_shell`] and [`safe_cwd`] are filesystem helpers for worker threads.
//! See `examples/dock_demo.rs` and the ignored performance integration test.
mod panel;
pub mod panesplit;
pub mod resizegate;
mod startup;
pub mod terminal_bar;
pub use panel::{
    pick_shell, safe_cwd, Pane, TerminalPanel, ANIMATION_MS, DEFAULT_MAX_PANES, HEADER_HEIGHT,
    SCROLLBACK_RANGE,
};

/// Register terminal CSS before applying the startup theme. Safe to repeat.
pub fn init() {
    impro_ui::register_css("terminal.bar", css);
}
/// Render the dock's original chrome rules against an effective UI palette.
pub fn css(ui: &impro_core::themes::UiColors) -> String {
    terminal_bar::css(ui)
}

// GTK may dispose a signal owner before its containing panel's destroy.
fn disconnect(object: &impl glib::prelude::IsA<glib::Object>, handler: glib::SignalHandlerId) {
    use glib::{prelude::*, translate::*};
    unsafe {
        if glib::gobject_ffi::g_signal_handler_is_connected(
            object.as_ref().to_glib_none().0,
            handler.as_raw(),
        ) != 0
        {
            object.disconnect(handler);
        }
    }
}
