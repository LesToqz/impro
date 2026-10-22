//! Editable Markdown decoration without inserting presentation into the source.
//!
//! Text marks attach widgets to source ranges. Window children are intentional:
//! `TextChildAnchor`s insert U+FFFC into a buffer, changing offsets, search and
//! undo. All rendering here is tags and `TextView` children; only explicit user
//! edits write to the buffer.
//!
//! # Work model
//!
//! * [`model`] is pure: it splits text into blocks and plans the tags of a line.
//! * Block *structure* is rebuilt for the edited region only (a dirty range that
//!   follows later edits), in slices of at most [`SLICE`] of main-thread time.
//! * Blocks are *painted* (tags and wanted widgets) nearest the viewport first,
//!   so opening a large note decorates the first screen before the rest. Painting
//!   runs from a `glib` idle; small interactive edits use a priority above the
//!   frame redraw so the block under the caret is decorated before it is drawn.
//! * Widgets (list markers, quote bars, code cards, images, tables) exist only
//!   near the viewport: the layout pass realizes them in a band around it and
//!   releases the ones far outside.
//! * Images decode on a worker and are scaled once; code highlighting runs on a
//!   worker for fenced blocks that are close to the viewport.
//!
//! Block offsets are plain character offsets patched from the buffer's
//! `insert-text`/`delete-range` signals, so no per-block `TextMark` is needed.
//! Tables use [`crate::livetable`]; [`CodeHighlighter`] supplies syntax runs.
mod code;
mod host;
mod images;
mod input;
mod layout;
pub mod model;
mod paint;
mod structure;
mod style;
mod widgets;

pub use widgets::{tools_copy_button, tools_copy_visible, tools_language_button, Tools};

pub use model::Kind;
pub use style::{css, view_css};

use crate::livetable::{TableBlock, Tables};
use gtk::prelude::*;
use model::{LinkSpan, Want};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    rc::{Rc, Weak},
    sync::Arc,
    time::{Duration, Instant},
};

/// Largest document (bytes) the live renderer takes on; the editor falls back to source.
pub const MAX_BYTES: usize = 1024 * 1024;
/// Main-thread budget of one decoration slice.
pub const SLICE: Duration = Duration::from_millis(4);
/// Pointer width of the readable column.
pub const COLUMN: i32 = 700;
/// Blocks beyond the visible ones whose widgets stay realized (each side).
const BAND: usize = 24;

/// One run of highlighted code: `[start, end)` in characters of the text given
/// to the highlighter, and the theme role (`keyword`, `string`, ...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeRun {
    pub start: usize,
    pub end: usize,
    pub kind: String,
}
/// Syntax highlighter for fenced code: `(language, text) -> runs`. It is called
/// from a worker thread, so it must not touch GTK. The default is plain text.
pub type CodeHighlighter = Arc<dyn Fn(&str, &str) -> Vec<CodeRun> + Send + Sync>;

/// What a realized widget is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Marker,
    Quote,
    Rule,
    Image,
    Table,
    CodeCard,
    CodeSideLeft,
    CodeSideRight,
    CodeFooter,
    Tools,
}
impl Role {
    /// Name used by the original (`marker`, `code-card`, ...).
    pub fn name(self) -> &'static str {
        match self {
            Role::Marker => "marker",
            Role::Quote => "quote",
            Role::Rule => "rule",
            Role::Image => "image",
            Role::Table => "table",
            Role::CodeCard => "code-card",
            Role::CodeSideLeft => "code-side-left",
            Role::CodeSideRight => "code-side-right",
            Role::CodeFooter => "code-footer",
            Role::Tools => "tools",
        }
    }
}
/// A realized widget: where it sits (block-relative offset) and why.
#[derive(Clone)]
pub(crate) struct Slot {
    pub widget: gtk::Widget,
    pub rel: i32,
    pub role: Role,
    pub data: i32,
}
/// Facts about fenced code a card needs after painting.
#[derive(Clone, Debug, Default)]
pub(crate) struct CodeInfo {
    pub language: String,
    pub first_len: i32,
    /// Block-relative end of the code content (before a closing fence).
    pub end_rel: i32,
    /// Highlight generation already requested/applied for `(text hash)`.
    pub highlighted: Option<u64>,
}
/// Load state of one `![alt](path)` image.
pub(crate) enum ImageState {
    /// Not requested yet (the block is far from the viewport); alt text shows.
    Pending,
    /// The worker is decoding it.
    Requested,
    /// Loaded, being re-decoded for a new column width.
    Rescaling(images::Loaded),
    /// Not a loadable local file: stays plain `image`-styled text.
    Failed,
    Ready(images::Loaded),
}
pub(crate) struct ImageSlot {
    /// Absolute-at-paint start of `![`, stored relative to the block.
    pub rel_a: i32,
    pub rel_b: i32,
    pub target: String,
    pub state: ImageState,
    pub tag: Option<gtk::TextTag>,
}

/// Decoration state of one block (its extent lives in [`Blocks::spans`]).
pub(crate) struct Deco {
    pub id: u64,
    pub kind: Kind,
    pub text: String,
    pub painted: bool,
    pub list_level: usize,
    pub number: usize,
    /// Space this block leaves after itself; the next block adds it above.
    pub below: i32,
    pub lead: i32,
    pub spacing: Option<&'static str>,
    pub closed: bool,
    pub hot: bool,
    pub links: Vec<LinkSpan>,
    pub pills: Vec<(i32, i32)>,
    pub wants: Vec<(i32, Want)>,
    /// Registry tags applied by the last paint (to remove them again).
    pub applied: Vec<gtk::TextTag>,
    /// Private tags (replacements) owned by this block.
    pub tags: Vec<gtk::TextTag>,
    pub pad: Option<gtk::TextTag>,
    pub lead_tag: Option<gtk::TextTag>,
    pub widgets: Vec<Slot>,
    pub table: Option<TableBlock>,
    pub images: Vec<ImageSlot>,
    pub code: Option<CodeInfo>,
}
impl Deco {
    pub fn new(id: u64, kind: Kind, text: String) -> Self {
        Self {
            id,
            kind,
            text,
            painted: false,
            list_level: 0,
            number: 0,
            below: 0,
            lead: 0,
            spacing: None,
            closed: false,
            hot: false,
            links: Vec::new(),
            pills: Vec::new(),
            wants: Vec::new(),
            applied: Vec::new(),
            tags: Vec::new(),
            pad: None,
            lead_tag: None,
            widgets: Vec::new(),
            table: None,
            images: Vec::new(),
            code: None,
        }
    }
}

/// Sorted block extents next to their decoration state.
#[derive(Default)]
pub(crate) struct Blocks {
    pub spans: Vec<(i32, i32)>,
    #[allow(clippy::vec_box)]
    pub items: Vec<Box<Deco>>,
}
impl Blocks {
    pub fn len(&self) -> usize {
        self.items.len()
    }
    /// Index of the last block starting at or before `offset` (0 when none).
    pub fn index(&self, offset: i32) -> usize {
        self.spans
            .partition_point(|s| s.0 <= offset)
            .saturating_sub(1)
    }
    pub fn insert(&mut self, index: usize, span: (i32, i32), deco: Box<Deco>) {
        self.spans.insert(index, span);
        self.items.insert(index, deco);
    }
    pub fn remove(&mut self, index: usize) -> Box<Deco> {
        self.spans.remove(index);
        self.items.remove(index)
    }
    /// `n` characters were inserted at `at` (before the buffer changed).
    pub fn shift_insert(&mut self, at: i32, n: i32) {
        let first = self.spans.partition_point(|s| s.1 <= at);
        for span in &mut self.spans[first..] {
            if span.0 > at {
                span.0 += n;
            }
            span.1 += n;
        }
        if first == self.spans.len() {
            if let Some(last) = self.spans.last_mut() {
                if last.1 == at {
                    last.1 += n;
                }
            }
        }
    }
    /// `[a, b)` is about to be deleted.
    pub fn shift_delete(&mut self, a: i32, b: i32) {
        let n = b - a;
        let map = |x: i32| {
            if x <= a {
                x
            } else if x >= b {
                x - n
            } else {
                a
            }
        };
        let first = self.spans.partition_point(|s| s.1 <= a);
        for span in &mut self.spans[first..] {
            span.0 = map(span.0);
            span.1 = map(span.1);
        }
    }
    pub fn find_id(&self, id: u64) -> Option<usize> {
        self.items.iter().position(|b| b.id == id)
    }
}

/// The rebuild in progress: scanning the dirty range and replacing blocks.
pub(crate) struct Work {
    pub stop: i32,
    pub index: usize,
    pub next: i32,
    pub chunk: String,
    pub chunk_base: i32,
    pub byte: usize,
    pub complete: bool,
    pub size: usize,
}
/// Where painting continues: outward from a focus block.
#[derive(Default)]
pub(crate) struct Aim {
    pub down: usize,
    pub up: isize,
    pub flip: bool,
    pub valid: bool,
}
/// Colours and fonts derived by `set_style`.
#[derive(Default, Clone)]
pub struct StyleState {
    pub chrome: impro_core::themes::UiColors,
    pub size: i32,
    pub bg: String,
    pub fg: String,
    pub muted: String,
    pub border: String,
    pub overlay: String,
    pub surface: String,
    pub body_font: String,
    pub code_font: String,
}
/// How web links open.
pub type UriOpener = Rc<dyn Fn(&str)>;
type Idle = Option<(glib::SourceId, i32)>;

pub(crate) type LiveInner = Inner;

pub(crate) struct Inner {
    #[allow(dead_code)]
    pub id: u64,
    pub view: gtk::TextView,
    pub buffer: gtk::TextBuffer,
    pub insert_mark: gtk::TextMark,
    pub bound_mark: gtk::TextMark,
    pub open_file: Box<dyn Fn(&Path)>,
    pub source_path: Box<dyn Fn() -> Option<PathBuf>>,
    pub highlighter: RefCell<Option<CodeHighlighter>>,
    pub enabled: Cell<bool>,
    pub destroyed: Cell<bool>,
    pub readable_width: Cell<bool>,
    pub editing: Cell<bool>,
    pub clamping: Cell<bool>,
    pub silent: Cell<u32>,
    pub pointer: Cell<bool>,
    pub hot: Cell<i32>,
    pub hits: Cell<usize>,
    pub spent: Cell<Duration>,
    pub painted_burst: Cell<usize>,
    pub next_id: Cell<u64>,
    pub unpainted: Cell<usize>,
    pub max_slice: Cell<Duration>,
    pub painted_blocks: Cell<usize>,
    pub blocks: RefCell<Blocks>,
    pub dirty: Cell<Option<(i32, i32)>>,
    pub work: RefCell<Option<Work>>,
    pub aim: RefCell<Aim>,
    pub idle: RefCell<Idle>,
    pub caret_idle: RefCell<Option<glib::SourceId>>,
    pub layout_idle: RefCell<Option<glib::SourceId>>,
    pub fill_idle: RefCell<Option<glib::SourceId>>,
    pub toolbar: RefCell<Option<crate::seltoolbar::SelectionToolbar>>,
    pub active: RefCell<BTreeSet<u64>>,
    pub last_caret: Cell<Option<i32>>,
    pub tags: RefCell<HashMap<String, gtk::TextTag>>,
    pub style: RefCell<StyleState>,
    pub provider: gtk::CssProvider,
    pub accent_provider: gtk::CssProvider,
    pub accent: RefCell<Option<(String, String, String)>>,
    pub detail_labels: RefCell<Vec<gtk::Label>>,
    pub placeholder: RefCell<Option<gtk::Label>>,
    pub hover_tools: RefCell<Option<gtk::Widget>>,
    pub language_popover: RefCell<Option<gtk::Popover>>,
    pub feedback: RefCell<Vec<glib::SourceId>>,
    pub handlers: RefCell<Vec<(glib::Object, glib::SignalHandlerId)>>,
    pub frame: RefCell<Option<(gdk::FrameClock, glib::SignalHandlerId)>>,
    pub view_gaps: Cell<(i32, i32)>,
    pub tables: RefCell<Option<Tables>>,
    pub band: Cell<(usize, usize)>,
    pub hint: Cell<Option<i32>>,
    pub margin: Cell<i32>,
    pub uri_opener: RefCell<Option<UriOpener>>,
    pub last_scan: Cell<usize>,
    pub epoch: Cell<u64>,
    pub jobs: images::Jobs,
    pub weak: Weak<Inner>,
}

thread_local! {
    static INSTANCES: Cell<u64> = const { Cell::new(0) };
    static REGISTRY: RefCell<Vec<(glib::WeakRef<gtk::TextView>, Weak<Inner>)>> =
        const { RefCell::new(Vec::new()) };
}

/// The live renderer attached to `view`, if any (for hosts and tests).
pub fn for_view(view: &gtk::TextView) -> Option<LiveMarkdown> {
    REGISTRY.with(|r| {
        r.borrow()
            .iter()
            .find(|(v, _)| v.upgrade().as_ref() == Some(view))
            .and_then(|(_, w)| w.upgrade())
            .map(LiveMarkdown)
    })
}
/// Views that currently have a live renderer.
pub(crate) fn registered_views() -> Vec<gtk::TextView> {
    REGISTRY.with(|r| {
        r.borrow()
            .iter()
            .filter(|(_, w)| w.upgrade().is_some())
            .filter_map(|(v, _)| v.upgrade())
            .collect()
    })
}
pub(crate) fn unregister(view: &gtk::TextView) {
    REGISTRY.with(|r| {
        r.borrow_mut()
            .retain(|(v, _)| v.upgrade().is_some_and(|v| &v != view));
    });
}

/// Live Markdown decoration of one `GtkTextView`. Handles are cheap clones of
/// one shared state; call [`LiveMarkdown::destroy`] to release the decoration.
#[derive(Clone)]
pub struct LiveMarkdown(pub(crate) Rc<Inner>);

impl LiveMarkdown {
    /// `open_file` opens a local link target; `path_getter` names the document
    /// (relative links and wiki names resolve against it).
    pub fn new<O, P>(view: &gtk::TextView, open_file: O, path_getter: P) -> Self
    where
        O: Fn(&Path) + 'static,
        P: Fn() -> Option<PathBuf> + 'static,
    {
        let buffer = view.buffer().expect("text view buffer");
        let insert_mark = buffer.get_insert().expect("buffer insert mark");
        let bound_mark = buffer.selection_bound().expect("buffer selection mark");
        let id = INSTANCES.with(|n| {
            n.set(n.get() + 1);
            n.get()
        });
        let inner = Rc::new_cyclic(|weak| Inner {
            id,
            view: view.clone(),
            buffer: buffer.clone(),
            insert_mark,
            bound_mark,
            open_file: Box::new(open_file),
            source_path: Box::new(path_getter),
            highlighter: RefCell::new(None),
            enabled: Cell::new(false),
            destroyed: Cell::new(false),
            readable_width: Cell::new(true),
            editing: Cell::new(false),
            clamping: Cell::new(false),
            silent: Cell::new(0),
            pointer: Cell::new(false),
            hot: Cell::new(0),
            hits: Cell::new(0),
            spent: Cell::new(Duration::ZERO),
            painted_burst: Cell::new(0),
            next_id: Cell::new(1),
            unpainted: Cell::new(0),
            max_slice: Cell::new(Duration::ZERO),
            painted_blocks: Cell::new(0),
            blocks: RefCell::new(Blocks::default()),
            dirty: Cell::new(None),
            work: RefCell::new(None),
            aim: RefCell::new(Aim::default()),
            idle: RefCell::new(None),
            caret_idle: RefCell::new(None),
            layout_idle: RefCell::new(None),
            fill_idle: RefCell::new(None),
            toolbar: RefCell::new(None),
            active: RefCell::new(BTreeSet::new()),
            last_caret: Cell::new(None),
            tags: RefCell::new(HashMap::new()),
            style: RefCell::new(StyleState {
                size: 11,
                ..Default::default()
            }),
            provider: gtk::CssProvider::new(),
            accent_provider: gtk::CssProvider::new(),
            accent: RefCell::new(None),
            detail_labels: RefCell::new(Vec::new()),
            placeholder: RefCell::new(None),
            hover_tools: RefCell::new(None),
            language_popover: RefCell::new(None),
            feedback: RefCell::new(Vec::new()),
            handlers: RefCell::new(Vec::new()),
            frame: RefCell::new(None),
            view_gaps: Cell::new((view.pixels_above_lines(), view.pixels_below_lines())),
            tables: RefCell::new(None),
            band: Cell::new((0, 0)),
            hint: Cell::new(None),
            margin: Cell::new(0),
            uri_opener: RefCell::new(None),
            last_scan: Cell::new(0),
            epoch: Cell::new(0),
            jobs: images::Jobs::default(),
            weak: weak.clone(),
        });
        inner.connect();
        inner.tag_hidden();
        REGISTRY.with(|r| {
            let mut r = r.borrow_mut();
            r.retain(|(v, w)| v.upgrade().is_some() && w.upgrade().is_some());
            r.push((view.downgrade(), Rc::downgrade(&inner)));
        });
        Self(inner)
    }
    pub(crate) fn from_inner(inner: Rc<Inner>) -> Option<Self> {
        (!inner.destroyed.get()).then_some(Self(inner))
    }
    pub(crate) fn downgrade(&self) -> Weak<Inner> {
        Rc::downgrade(&self.0)
    }
    pub fn view(&self) -> gtk::TextView {
        self.0.view.clone()
    }
    /// Whether both handles share one renderer.
    pub fn ptr_eq(&self, other: &LiveMarkdown) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    /// The document path the renderer resolves links against.
    pub fn path(&self) -> Option<PathBuf> {
        (self.0.source_path)()
    }
    /// The selection toolbar this renderer currently owns.
    pub fn toolbar(&self) -> Option<crate::seltoolbar::SelectionToolbar> {
        self.0.toolbar.borrow().clone()
    }
    pub(crate) fn set_toolbar(&self, toolbar: Option<crate::seltoolbar::SelectionToolbar>) {
        self.0.toolbar.replace(toolbar);
    }
    /// Replace how web links open (default: the desktop's URI handler).
    pub fn set_uri_opener(&self, opener: Option<UriOpener>) {
        self.0.uri_opener.replace(opener);
    }
    /// Plain text unless a highlighter is installed.
    pub fn set_code_highlighter(&self, highlighter: Option<CodeHighlighter>) {
        self.0.highlighter.replace(highlighter);
    }
    pub fn destroy(&self) {
        self.0.destroy();
    }
    pub fn set_enabled(&self, enabled: bool) {
        self.0.set_enabled(enabled);
        crate::seltoolbar::refresh_for(&self.0.view);
    }
    pub fn enabled(&self) -> bool {
        self.0.enabled.get()
    }
    pub fn set_readable_width(&self, enabled: bool) {
        self.0.readable_width.set(enabled);
        self.0.resize();
    }
    /// Apply the editor's syntax colours, chrome and point size.
    pub fn set_style(
        &self,
        syntax: &impro_core::themes::SyntaxColors,
        chrome: &impro_core::themes::UiColors,
        font_size: i32,
    ) {
        self.0.set_style(syntax, chrome, font_size);
    }
    /// Enter/Tab/Backspace/space and friends through `blockedit::transform`.
    pub fn edit_key(&self, event: &gdk::EventKey, indent_tabs: bool, indent_width: i32) -> bool {
        self.0.edit_key(event, indent_tabs, indent_width)
    }
    /// Open `target` (web link, local file, wiki name); false when it cannot be resolved.
    pub fn activate_link(&self, target: &str) -> bool {
        self.0.activate_link(target)
    }
    /// Toggle the task at block-relative `relative` of block `index`.
    pub fn toggle_task(&self, index: usize, relative: i32) {
        self.0.toggle_task_at(index, relative);
    }
}

/// Read-only views for tests and diagnostics.
#[derive(Clone)]
pub struct BlockInfo {
    pub kind: &'static str,
    pub text: String,
    pub start: i32,
    pub end: i32,
    pub lead: i32,
    pub below: i32,
    pub list_level: usize,
    pub number: usize,
    pub closed: bool,
    pub painted: bool,
    pub links: usize,
    /// `(widget, block-relative offset, role name, data)`.
    pub widgets: Vec<(gtk::Widget, i32, &'static str, i32)>,
}
impl LiveMarkdown {
    /// Snapshot of every block.
    pub fn blocks(&self) -> Vec<BlockInfo> {
        let blocks = self.0.blocks.borrow();
        blocks
            .items
            .iter()
            .zip(&blocks.spans)
            .map(|(b, &(start, end))| BlockInfo {
                kind: b.kind.name(),
                text: b.text.clone(),
                start,
                end,
                lead: b.lead,
                below: b.below,
                list_level: b.list_level,
                number: b.number,
                closed: b.closed,
                painted: b.painted,
                links: b.links.len(),
                widgets: b
                    .widgets
                    .iter()
                    .map(|s| (s.widget.clone(), s.rel, s.role.name(), s.data))
                    .collect(),
            })
            .collect()
    }
    /// How many times the renderer asked GTK to hit-test a layout (pointer, links).
    pub fn hit_tests(&self) -> usize {
        self.0.hits.get()
    }
    /// True while every block showing in the viewport is painted (diagnostics).
    pub fn viewport_painted(&self) -> bool {
        match self.0.visible_blocks() {
            Some((first, last)) => {
                let blocks = self.0.blocks.borrow();
                (first..=last.min(blocks.len().saturating_sub(1)))
                    .all(|i| blocks.items.get(i).is_none_or(|b| b.painted))
            }
            None => false,
        }
    }
    pub fn block_count(&self) -> usize {
        self.0.blocks.borrow().len()
    }
    /// Realized widgets of one role (`marker`, `tools`, `table`, ...), in document order.
    pub fn widgets(&self, role: &str) -> Vec<gtk::Widget> {
        self.0
            .blocks
            .borrow()
            .items
            .iter()
            .flat_map(|b| b.widgets.iter())
            .filter(|s| s.role.name() == role)
            .map(|s| s.widget.clone())
            .collect()
    }
    /// A named style tag (`hidden`, `strong`, `h1`, `syntax-keyword`, ...).
    pub fn tag(&self, name: &str) -> Option<gtk::TextTag> {
        self.0.tags.borrow().get(name).cloned()
    }
    /// Work (rebuild, paint, caret or layout) is still queued.
    pub fn pending(&self) -> bool {
        self.0.pending()
    }
    /// Longest main-thread slice of decoration work seen so far.
    pub fn max_slice_time(&self) -> Duration {
        self.0.max_slice.get()
    }
    pub fn painted_blocks(&self) -> usize {
        self.0.painted_blocks.get()
    }
    pub fn unpainted_blocks(&self) -> usize {
        self.0.unpainted.get()
    }
    pub fn size(&self) -> i32 {
        self.0.style.borrow().size
    }
    pub fn chrome(&self) -> impro_core::themes::UiColors {
        self.0.style.borrow().chrome.clone()
    }
    pub fn provider_css(&self) -> String {
        self.0.provider.to_str().to_string()
    }
    pub fn column_width(&self) -> i32 {
        self.0.column_width()
    }
    /// Re-run the per-frame placement of widgets and details now.
    pub fn position(&self) {
        self.0.position();
    }
    pub fn placeholder(&self) -> Option<gtk::Label> {
        self.0.placeholder.borrow().clone()
    }
    pub fn detail_labels(&self) -> Vec<gtk::Label> {
        self.0.detail_labels.borrow().clone()
    }
    pub fn language_popover(&self) -> Option<gtk::Popover> {
        self.0.language_popover.borrow().clone()
    }
    /// Hit-test hook: pointer moved to window coordinates.
    pub fn pointer_moved(&self, x: f64, y: f64) {
        self.0.motion(x, y);
    }
    /// Hit-test hook: pointer left the view (`inferior` = moved into a child).
    pub fn pointer_left(&self, inferior: bool) {
        self.0.leave(inferior);
    }
    /// Link target and "source revealed" flag under window coordinates.
    pub fn link_at(&self, x: f64, y: f64) -> Option<(String, bool)> {
        self.0.link_at(x, y)
    }
    /// Primary-press handling with a link lookup result supplied by the caller.
    pub fn press_link(&self, link: Option<(String, bool)>, ctrl: bool) -> bool {
        self.0.press_with(link, ctrl)
    }
    pub fn tables(&self) -> Option<Tables> {
        self.0.tables.borrow().clone()
    }
    /// Pending image/highlight jobs on workers.
    pub fn jobs_in_flight(&self) -> usize {
        self.0.jobs.in_flight()
    }
}

impl Inner {
    pub fn pending(&self) -> bool {
        self.idle.borrow().is_some()
            || self.caret_idle.borrow().is_some()
            || self.layout_idle.borrow().is_some()
            || self.fill_idle.borrow().is_some()
    }
    pub fn track(&self, object: &impl IsA<glib::Object>, id: glib::SignalHandlerId) {
        self.handlers
            .borrow_mut()
            .push((object.clone().upcast(), id));
    }
    pub fn offset_of(&self, mark: &gtk::TextMark) -> i32 {
        self.buffer.iter_at_mark(mark).offset()
    }
    pub fn caret(&self) -> i32 {
        self.offset_of(&self.insert_mark)
    }
    pub fn char_count(&self) -> i32 {
        self.buffer.char_count()
    }
    pub fn iter(&self, offset: i32) -> gtk::TextIter {
        self.buffer.iter_at_offset(offset)
    }
    pub fn fresh_id(&self) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }
    pub fn note_slice(&self, started: Instant) {
        let took = started.elapsed();
        if took > self.max_slice.get() {
            self.max_slice.set(took);
        }
    }
}
