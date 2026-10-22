//! impro's Graph view: the engine (parsing, project index, filters,
//! preferences, force layout) and the native view that draws it.
//!
//! The engine modules are GTK-free; [`view`] holds the scene model, a small
//! software renderer, the controls panel and the [`GraphView`] widget.
//!
//! # Engine data flow
//!
//! ```text
//! files ──scan──▶ ProjectIndex ──build_graph(GraphOptions)──▶ Graph ──▶ Layout ──▶ x/y/radius arrays
//!                   ▲  update_file / remove / rename            │
//!                   └── IndexChange                  delta::diff(old, new) ─▶ GraphDelta
//! ```
//!
//! * [`parse`] turns text into links, tags, properties and source imports.
//! * [`index`] owns the files, resolves references and updates incrementally.
//! * [`query`] is the search language; [`filter`] applies it together with the
//!   visibility toggles, colour rules, the node cap and the local view.
//! * [`prefs`] normalises the persisted preferences blob.
//! * [`layout`] is the force simulation; [`geometry`] holds the drawing maths.
//!
//! # The view
//!
//! Rendering is native: a `gtk::DrawingArea` painted from a [`view::scene::Scene`]
//! by [`view::draw::Painter`] (software rasteriser for markers, connections and
//! arrows; pango for labels), with the controls panel built from GTK widgets
//! (`view/graph.css`). Indexing and graph building run on a worker thread
//! ([`view::host`]); preferences are saved by a write-behind thread
//! ([`view::store`]). Frames run only while something moves: a tick callback is
//! attached while the layout cools, the camera glides or colours ease, and is
//! removed afterwards, so a settled graph costs no CPU. A hidden (unmapped)
//! view runs nothing at all. Reduce motion (the application preference, the
//! saved `reduceMotion` key, `IMPRO_REDUCE_MOTION=1` or GTK's
//! `gtk-enable-animations`) skips easing and inertia.
//!
//! ### Levels of detail
//!
//! A frame is drawn at one of three levels, chosen by the scene's state:
//!
//! * **Settled** (nothing eases): the reference picture, anti-aliased lines,
//!   markers and labels, with the connection layer kept between frames.
//! * **Easing** (hover fades, theme blends; geometry unchanged): the same
//!   picture, but the coverage of each connection and marker group is kept
//!   while its geometry and members stay, so a frame repaints the groups in
//!   their new opacity instead of rasterising them again.
//! * **Moving** (`Scene::moving`: the layout runs, the camera pans, zooms or
//!   flings, sizes ease; `Painter::motion`): lines and markers are drawn without
//!   anti-aliasing, dense scenes draw no direction markers, and only the 96
//!   largest on-screen markers get a label. The first frame after the motion
//!   stops is the settled picture, so nothing outside these three differences
//!   is visible in motion.
//!
//! ## Host API (everything the window needs)
//!
//! ```ignore
//! impro_graph::init();                                   // once, before the first theme
//! let view = GraphView::new(&project_root);              // prefs from settings.json
//! view.set_theme(&theme.ui, &accent);                    // and again on theme / accent change
//! view.set_font(ui_font.as_deref());                     // label font family
//! view.set_reduce_motion(settings["reduce_motion"]);
//! view.set_active_file(editor.current_path());
//! view.set_graph_type(GraphType::parse(&settings["graph_default_type"]).unwrap_or_default());
//!
//! view.connect_open_file(|path, mode| match mode {       // OpenMode::{Current, NewTab, RightPane}
//!     OpenMode::Current | OpenMode::NewTab | OpenMode::RightPane => open_in_editor(path),
//! });
//! view.connect_search_tag(|tag| explorer.focus_search(&tag_query(tag)));   // "tag:#name"
//! view.connect_create_note(|name| create_note(name));    // name without extension
//! view.connect_reveal(|path| explorer.set_current_file(path));
//! view.connect_copy_path(|path| { /* already on the clipboard */ });
//! view.connect_settings_changed(|blob| other_views.for_each(|v| v.apply_prefs(blob)));
//! view.connect_error(|message| eprintln!("Graph view: {message}"));
//!
//! // dock pane: the view is a gtk::Box
//! panel.add_custom_pane("graph", &*view, Some(view.dock_shutdown()));
//! ```
//!
//! Live updates the window forwards: [`GraphView::set_root`] (Open Folder),
//! [`GraphView::file_saved`], [`GraphView::buffer_changed`] (unsaved text, applied
//! 400 ms after the last change; `None` returns to the file on disk),
//! [`GraphView::path_renamed`], [`GraphView::path_deleted`] and
//! [`GraphView::set_active_file`] (highlights the file and moves the local
//! graph). Command palette entries map to [`GraphView::open_global`] (Open
//! graph view), [`GraphView::open_local`] (Open local graph),
//! [`GraphView::rebuild`] (Rebuild graph) and [`GraphView::reset_camera`]
//! (Reset camera). All of these are cheap and coalesce: a burst of changes
//! produces one build, and updates reach the scene as deltas that keep the
//! positions of unaffected nodes.
//!
//! | Surface | Items |
//! | --- | --- |
//! | Construction | [`GraphView::new`], [`GraphView::with_options`] ([`ViewOptions`]: `store`, `scan`), [`init`], [`css`] |
//! | Signals | `connect_open_file(Path, OpenMode)`, `connect_search_tag(&str)`, `connect_create_note(&str)`, `connect_reveal(Path)`, `connect_copy_path(Path)`, `connect_settings_changed(&Value)`, `connect_error(&str)`, `connect_updated(&GraphSummary)` |
//! | Mode | [`GraphView::graph_type`], `set_graph_type`, `open_global`, `open_local`, `open_local_with_depth` |
//! | Live | `set_root`, `root`, `file_saved`, `buffer_changed`, `path_renamed`, `path_deleted`, `set_active_file`, `rebuild`, `reset_camera`, `fit_camera`, `focus_file`, `replay_history`, `replaying` |
//! | Appearance | `set_theme`, `set_font`, `set_reduce_motion` |
//! | Preferences | `prefs`, `apply_prefs`, `flush_prefs`, `save_failed` |
//! | Lifecycle | `shutdown`, `dock_shutdown` |
//! | Inspection | `summary`, `frame_stats`, `status_text`, `notice_text`, `panel_open`, `set_panel_open`, `panel`, `canvas` |
//!
//! ## Behaviour contract
//!
//! * File marker: open in the editor (middle click and Ctrl+click: another
//!   tab); tag marker: search; missing-target marker: create a note. The
//!   right-click menu of a file lists exactly "Open file", "Open another tab",
//!   "Open in right pane", "Locate in project tree", "Copy file location".
//! * A click may move 6 px; wheel zoom keeps the point under the pointer
//!   fixed; dragging the background pans with 86 %-per-frame inertia; dragging a
//!   marker pins it while the layout stays warm; hovering dims everything not
//!   connected to the marker to 28 %.
//! * Status texts: "Mapping this project…", "No files match this view",
//!   "View limited to N files with the most connections" (at the 3 000-node
//!   cap), "Open a file to see its local graph"; the canvas is labelled
//!   "Project files and references".
//! * Preferences, zoom, colour rules and panel/section state persist through
//!   `impro_core::settings::{get_graph, set_graph}` (debounced 250 ms, written on
//!   a background thread, always normalised by [`prefs::normalize`]).

pub mod delta;
pub mod filter;
pub mod geometry;
pub mod index;
pub mod layout;
pub mod model;
pub mod parse;
pub mod prefs;
pub mod query;
pub mod view;

pub use delta::{diff, GraphDelta};
pub use filter::{build, local, GraphOptions, LocalGraph};
pub use index::{CancelToken, IndexChange, ProjectIndex, ScanConfig};
pub use layout::{ForceParams, Layout};
pub use model::{Graph, Node, NodeKind};
pub use prefs::{normalize, GraphPrefs};

/// Registers the view's style sheet with the shared style; call once before
/// the first theme is applied.
pub fn init() {
    impro_ui::register_css("graph.view", view::css);
}

/// The view's style sheet for a theme's UI colours.
pub fn css(ui: &impro_core::themes::UiColors) -> String {
    view::css(ui)
}

pub use view::{
    tag_query, ContextAction, FrameStats, GraphSummary, GraphType, GraphView, OpenMode, ViewOptions,
};
