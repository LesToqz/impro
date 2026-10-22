//! Visible-line-only Cairo/Pango gutter and bounded bracket/caret decorations.
use gtk::prelude::*;
use impro_core::themes::UiColors;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub const BRACKET_RANGE: i32 = 20_000;
pub const PADDING_LEFT: i32 = 10;
pub const PADDING_RIGHT: i32 = 24;
pub fn digits_for(lines: i32) -> i32 {
    lines.max(1).to_string().len().max(3) as i32
}
pub fn valid_color(value: &str) -> bool {
    gdk::RGBA::parse(value).is_ok()
}
pub fn find_match(text: &str, index: usize) -> Option<usize> {
    let c = text.chars().nth(index)?;
    let (other, forward) = match c {
        '(' => (')', true),
        '[' => (']', true),
        '{' => ('}', true),
        ')' => ('(', false),
        ']' => ('[', false),
        '}' => ('{', false),
        _ => return None,
    };
    let mut depth = 0;
    if forward {
        for (i, ch) in text.chars().enumerate().skip(index) {
            if ch == c {
                depth += 1;
            } else if ch == other {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
    } else {
        let end = text
            .char_indices()
            .nth(index)
            .map(|(i, c)| i + c.len_utf8())?;
        for (i, ch) in text[..end].chars().rev().enumerate() {
            if ch == c {
                depth += 1;
            } else if ch == other {
                depth -= 1;
                if depth == 0 {
                    return Some(index - i);
                }
            }
        }
    }
    None
}
pub fn layout_for(view: &gtk::TextView, text: &str) -> pango::Layout {
    let layout = view.create_pango_layout(Some(text));
    let font: pango::FontDescription = view.style_context().font(gtk::StateFlags::NORMAL);
    layout.set_font_description(Some(&font));
    layout
}
/// The font a gutter label resolves to: a throwaway label with the same
/// low-priority fallback rule (the view's family, 12px) beneath the screen's
/// rules, so an explicit interface family (`* {font-family}`) wins exactly as
/// it does for the labels of the reference gutter.
fn label_font(view: &gtk::TextView) -> pango::FontDescription {
    let family = view
        .style_context()
        .font(gtk::StateFlags::NORMAL)
        .family()
        .map(|f| f.to_string())
        .unwrap_or_default();
    let provider = gtk::CssProvider::new();
    let css = format!(
        "label.impro-gutter-num {{font-family: \"{}\"; font-size: 12px; font-feature-settings: \"tnum\";}}",
        family.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let _ = provider.load_from_data(css.as_bytes());
    let label = gtk::Label::new(None);
    let context = label.style_context();
    context.add_class("impro-gutter-num");
    context.add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION - 1);
    let mut font = context.font(gtk::StateFlags::NORMAL);
    font.set_absolute_size(12.0 * pango::SCALE as f64);
    font
}
/// A drawn row in viewport coordinates, exposed for geometry/accessibility adapters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GutterRow {
    pub line: i32,
    pub y: i32,
    pub height: i32,
    pub active: bool,
}
#[derive(Clone)]
pub struct ViewChrome(Rc<Inner>);
struct Inner {
    view: glib::WeakRef<gtk::TextView>,
    buffer: glib::WeakRef<gtk::TextBuffer>,
    area: gtk::DrawingArea,
    pub line_tag: gtk::TextTag,
    pub bracket_tag: gtk::TextTag,
    marks: Vec<gtk::TextMark>,
    line_range: Cell<bool>,
    bracket_count: Cell<usize>,
    colors: RefCell<UiColors>,
    foreground: Cell<gdk::RGBA>,
    active_foreground: Cell<gdk::RGBA>,
    line_numbers: Cell<bool>,
    highlight_line: Cell<bool>,
    font_size: Cell<i32>,
    font_family: RefCell<Option<String>>,
    digits: Cell<i32>,
    width: Cell<i32>,
    measured: Cell<i32>,
    layouts: RefCell<Vec<pango::Layout>>,
    digit_width: Cell<i32>,
    provider: gtk::CssProvider,
    refreshing: Cell<bool>,
    painted_lines: Cell<usize>,
    rows: RefCell<[GutterRow; 1024]>,
    destroyed: Cell<bool>,
    sync_source: RefCell<Option<glib::SourceId>>,
}
impl ViewChrome {
    pub fn new(view: &gtk::TextView, colors: &UiColors) -> Self {
        let buffer = view.buffer().expect("text view buffer");
        let line_tag = gtk::TextTag::new(Some("impro-current-line"));
        let bracket_tag = gtk::TextTag::new(Some("impro-bracket"));
        buffer.tag_table().expect("buffer tag table").add(&line_tag);
        buffer
            .tag_table()
            .expect("buffer tag table")
            .add(&bracket_tag);
        line_tag.set_priority(0);
        let area = gtk::DrawingArea::new();
        area.set_no_show_all(true);
        view.add_child_in_window(&area, gtk::TextWindowType::Left, 0, 0);
        let provider = gtk::CssProvider::new();
        view.style_context()
            .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_USER);
        let default = impro_core::themes::get_theme(Some("dark-plus"))
            .chrome
            .clone();
        let fallback = gdk::RGBA::parse("#858585").expect("constant color");
        let marks = (0..6)
            .map(|_| {
                buffer
                    .create_mark(None, &buffer.start_iter(), true)
                    .expect("buffer mark")
            })
            .collect();
        let this = Self(Rc::new(Inner {
            view: view.downgrade(),
            buffer: buffer.downgrade(),
            area: area.clone(),
            line_tag,
            bracket_tag,
            marks,
            line_range: Cell::new(false),
            bracket_count: Cell::new(0),
            colors: RefCell::new(default),
            foreground: Cell::new(fallback),
            active_foreground: Cell::new(fallback),
            line_numbers: Cell::new(true),
            highlight_line: Cell::new(true),
            font_size: Cell::new(11),
            font_family: RefCell::new(None),
            digits: Cell::new(0),
            width: Cell::new(0),
            measured: Cell::new(0),
            layouts: RefCell::new(Vec::new()),
            digit_width: Cell::new(8),
            provider,
            refreshing: Cell::new(false),
            painted_lines: Cell::new(0),
            rows: RefCell::new([GutterRow::default(); 1024]),
            destroyed: Cell::new(false),
            sync_source: RefCell::new(None),
        }));
        let weak = Rc::downgrade(&this.0);
        area.connect_draw(move |_, cr| {
            if let Some(i) = weak.upgrade() {
                Self(i).draw(cr);
            }
            glib::Propagation::Proceed
        });
        let weak = Rc::downgrade(&this.0);
        buffer.connect_changed(move |_| {
            if let Some(i) = weak.upgrade() {
                let h = Self(i);
                h.update_width();
                h.refresh_tags();
                h.update_labels();
            }
        });
        let weak = Rc::downgrade(&this.0);
        buffer.connect_mark_set(move |b, _, mark| {
            if mark == &b.get_insert().expect("buffer insert mark")
                || Some(mark) == b.selection_bound().as_ref()
            {
                if let Some(i) = weak.upgrade() {
                    let h = Self(i);
                    h.refresh_tags();
                    h.0.area.queue_draw();
                }
            }
        });
        let weak = Rc::downgrade(&this.0);
        view.connect_size_allocate(move |_, _| {
            if let Some(i) = weak.upgrade() {
                Self(i).update_labels();
            }
        });
        let weak = Rc::downgrade(&this.0);
        view.connect_style_updated(move |_| {
            if let Some(i) = weak.upgrade() {
                let h = Self(i);
                h.rebuild_layouts();
                h.refresh_colors();
                h.update_width();
            }
        });
        let weak = Rc::downgrade(&this.0);
        view.connect_map(move |_| {
            if let Some(i) = weak.upgrade() {
                Self(i).update_labels();
            }
        });
        let weak = Rc::downgrade(&this.0);
        view.connect_vadjustment_notify(move |v| {
            if let Some(adj) = v.vadjustment() {
                let weak = weak.clone();
                adj.connect_value_changed(move |_| {
                    if let Some(i) = weak.upgrade() {
                        Self(i).update_labels();
                    }
                });
            }
        });
        if let Some(adj) = view.vadjustment() {
            let weak = Rc::downgrade(&this.0);
            adj.connect_value_changed(move |_| {
                if let Some(i) = weak.upgrade() {
                    Self(i).update_labels();
                }
            });
        }
        let weak = Rc::downgrade(&this.0);
        area.add_events(gdk::EventMask::BUTTON_PRESS_MASK);
        area.connect_button_press_event(move |_, event| {
            if event.button() != 1 {
                return glib::Propagation::Proceed;
            }
            if let Some(i) = weak.upgrade() {
                let h = Self(i);
                if let (Some(v), Some(b)) = (h.0.view.upgrade(), h.0.buffer.upgrade()) {
                    let (first, _) = v.line_at_y(v.visible_rect().y() + event.position().1 as i32);
                    let mut last = first;
                    if !last.forward_line() {
                        last = b.end_iter();
                    }
                    b.select_range(&last, &first);
                    v.grab_focus();
                }
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(&this.0);
        view.connect_destroy(move |_| {
            if let Some(i) = weak.upgrade() {
                Self(i).destroy();
            }
        });
        this.apply(colors);
        this.set_font_size(11);
        this.refresh_tags();
        this
    }
    pub fn drawing_area(&self) -> gtk::DrawingArea {
        self.0.area.clone()
    }
    pub fn width(&self) -> i32 {
        self.0.width.get()
    }
    pub fn painted_lines(&self) -> usize {
        self.0.painted_lines.get()
    }
    pub fn painted_rows(&self) -> Vec<GutterRow> {
        self.0.rows.borrow()[..self.0.painted_lines.get().min(1024)].to_vec()
    }
    pub fn line_tag(&self) -> gtk::TextTag {
        self.0.line_tag.clone()
    }
    pub fn bracket_tag(&self) -> gtk::TextTag {
        self.0.bracket_tag.clone()
    }
    pub fn colors(&self) -> UiColors {
        self.0.colors.borrow().clone()
    }
    pub fn provider(&self) -> gtk::CssProvider {
        self.0.provider.clone()
    }
    pub fn set_line_numbers(&self, enabled: bool) {
        let Some(view) = self.0.view.upgrade() else {
            return;
        };
        if !enabled && self.0.area.parent().is_some() {
            self.0.area.hide();
            view.remove(&self.0.area);
        }
        self.0.line_numbers.set(enabled);
        self.update_width();
        if enabled && self.0.area.parent().is_none() {
            view.add_child_in_window(&self.0.area, gtk::TextWindowType::Left, 0, 0);
        }
        self.update_labels();
    }
    pub fn set_highlight_line(&self, enabled: bool) {
        self.0.highlight_line.set(enabled);
        self.refresh_tags();
    }
    pub fn set_font_size(&self, size: i32) {
        self.0.font_size.set(size);
        self.load_css();
        self.rebuild_layouts();
        self.update_width();
    }
    pub fn set_font_family(&self, family: Option<&str>) {
        self.0.font_family.replace(family.map(str::to_owned));
        self.load_css();
        self.rebuild_layouts();
        self.update_width();
    }
    fn load_css(&self) {
        let family = self.0.font_family.borrow().clone();
        let family = family
            .map(|s| {
                format!(
                    "font-family: \"{}\";",
                    s.replace('\\', "\\\\").replace('"', "\\\"")
                )
            })
            .unwrap_or_default();
        let _=self.0.provider.load_from_data(format!("textview, textview text {{font-size: {}pt; {family}}} textview border.left {{background-color: transparent; border: 0;}}",self.0.font_size.get()).as_bytes());
    }
    pub fn apply(&self, colors: &UiColors) {
        {
            let mut current = self.0.colors.borrow_mut();
            for (k, v) in colors {
                if current.contains_key(k) && valid_color(v) {
                    current.insert(k.clone(), v.clone());
                }
            }
        }
        let current = self.colors();
        if let Some(c) = current.get("current_line_bg") {
            self.0.line_tag.set_paragraph_background(Some(c));
        }
        if let Some(c) = current.get("bracket_bg") {
            self.0.bracket_tag.set_background(Some(c));
        }
        self.refresh_colors();
        self.0.area.queue_draw();
    }
    fn refresh_colors(&self) {
        let colors = self.colors();
        let theme = impro_ui::current_theme();
        let foreground = theme
            .as_ref()
            .and_then(|t| t.ui.get("faint"))
            .or_else(|| colors.get("gutter_fg"));
        let active = theme
            .as_ref()
            .and_then(|t| t.ui.get("muted"))
            .or_else(|| colors.get("gutter_fg_active"));
        if let Some(c) = foreground.and_then(|s| gdk::RGBA::parse(s).ok()) {
            self.0.foreground.set(c);
        }
        if let Some(c) = active.and_then(|s| gdk::RGBA::parse(s).ok()) {
            self.0.active_foreground.set(c);
        }
    }
    fn rebuild_layouts(&self) {
        let Some(view) = self.0.view.upgrade() else {
            return;
        };
        let font = label_font(&view);
        let tabular = pango::AttrList::new();
        tabular.insert(pango::AttrFontFeatures::new("tnum"));
        let layouts: Vec<_> = (0..10)
            .map(|d| {
                let l = view.create_pango_layout(Some(&d.to_string()));
                l.set_font_description(Some(&font));
                l.set_attributes(Some(&tabular));
                l
            })
            .collect();
        self.0.digit_width.set(layouts[0].pixel_size().0);
        self.0.layouts.replace(layouts);
        self.0.digits.set(0);
    }
    pub fn update_width(&self) {
        let (Some(v), Some(b)) = (self.0.view.upgrade(), self.0.buffer.upgrade()) else {
            return;
        };
        let digits = digits_for(b.line_count());
        if self.0.layouts.borrow().is_empty() {
            self.rebuild_layouts();
        }
        if digits != self.0.digits.get() {
            self.0.digits.set(digits);
            self.0.measured.set(
                layout_for(&v, &"0".repeat(digits as usize)).pixel_size().0
                    + PADDING_LEFT
                    + PADDING_RIGHT,
            );
        }
        let width = if self.0.line_numbers.get() {
            self.0.measured.get()
        } else {
            0
        };
        if self.0.width.replace(width) != width {
            v.set_border_window_size(gtk::TextWindowType::Left, width);
        }
    }
    pub fn update_labels(&self) {
        if self.0.area.parent().is_none() || self.0.destroyed.get() {
            return;
        }
        if self.0.sync_source.borrow().is_none() {
            let weak = Rc::downgrade(&self.0);
            let id = glib::idle_add_local_full(glib::Priority::LOW, move || {
                if let Some(inner) = weak.upgrade() {
                    inner.sync_source.take();
                    let h = Self(inner);
                    h.place_area();
                }
                glib::ControlFlow::Break
            });
            self.0.sync_source.replace(Some(id));
        }
        self.place_area();
    }
    fn place_area(&self) {
        if self.0.destroyed.get() || self.0.area.parent().is_none() {
            return;
        }
        let Some(v) = self.0.view.upgrade() else {
            return;
        };
        let rect = v.visible_rect();
        let width = self.width();
        if self.0.area.width_request() != width
            || self.0.area.height_request() != rect.height().max(1)
        {
            self.0.area.set_size_request(width, rect.height().max(1));
        }
        v.move_child(&self.0.area, 0, rect.y());
        let allocation = self.0.area.allocation();
        if allocation.width() != width
            || allocation.height() != rect.height().max(1)
            || allocation.y() != 0
        {
            self.0
                .area
                .size_allocate(&gtk::Allocation::new(0, 0, width, rect.height().max(1)));
        }
        self.0.area.set_visible(self.0.line_numbers.get());
        self.0.area.queue_draw();
    }
    fn draw(&self, cr: &cairo::Context) {
        let trace_start = std::time::Instant::now();
        if !self.0.line_numbers.get() {
            return;
        }
        let (Some(v), Some(b)) = (self.0.view.upgrade(), self.0.buffer.upgrade()) else {
            return;
        };
        let rect = v.visible_rect();
        let (mut iter, _) = v.line_at_y(rect.y());
        let active = b
            .iter_at_mark(&b.get_insert().expect("buffer insert mark"))
            .line();
        let layouts = self.0.layouts.borrow();
        if layouts.len() != 10 {
            return;
        }
        let dw = self.0.digit_width.get();
        let mut count = 0;
        let mut rows = self.0.rows.borrow_mut();
        let mut previous = -1;
        loop {
            let (top, height) = v.line_yrange(&iter);
            if top >= rect.y() + rect.height() || height <= 0 || top <= previous {
                break;
            }
            previous = top;
            let color = if iter.line() == active {
                self.0.active_foreground.get()
            } else {
                self.0.foreground.get()
            };
            cr.set_source_rgba(color.red(), color.green(), color.blue(), color.alpha());
            let mut n = iter.line() + 1;
            let mut x = self.width() - PADDING_RIGHT - dw;
            let y = top - rect.y() + (height - layouts[0].pixel_size().1) / 2;
            loop {
                cr.move_to(x as f64, y as f64);
                pangocairo::functions::show_layout(cr, &layouts[(n % 10) as usize]);
                n /= 10;
                if n == 0 {
                    break;
                }
                x -= dw;
            }
            if let Some(row) = rows.get_mut(count) {
                *row = GutterRow {
                    line: iter.line(),
                    y: top - rect.y(),
                    height,
                    active: iter.line() == active,
                };
            }
            count += 1;
            let line = iter.line();
            iter.forward_line();
            if iter.line() == line {
                break;
            }
        }
        self.0.painted_lines.set(count);
        if std::env::var_os("IMPRO_EDITOR_TRACE").is_some() {
            eprintln!("gutter draw {:?}", trace_start.elapsed());
        }
    }
    pub fn refresh_tags(&self) {
        if self.0.destroyed.get() || self.0.refreshing.replace(true) {
            return;
        }
        let Some(b) = self.0.buffer.upgrade() else {
            self.0.refreshing.set(false);
            return;
        };
        if self.0.line_range.replace(false) {
            b.remove_tag(
                &self.0.line_tag,
                &b.iter_at_mark(&self.0.marks[0]),
                &b.iter_at_mark(&self.0.marks[1]),
            );
        }
        for index in 0..self.0.bracket_count.replace(0) {
            let a = b.iter_at_mark(&self.0.marks[2 + index * 2]);
            let z = b.iter_at_mark(&self.0.marks[3 + index * 2]);
            b.remove_tag(&self.0.bracket_tag, &a, &z);
        }
        let cursor = b.iter_at_mark(&b.get_insert().expect("buffer insert mark"));
        let selection = b.selection_bounds();
        if self.0.highlight_line.get() && selection.is_none_or(|(a, z)| a.line() == z.line()) {
            let mut a = cursor;
            a.set_line_offset(0);
            let mut z = a;
            if !z.forward_line() {
                z = b.end_iter();
            }
            b.move_mark(&self.0.marks[0], &a);
            b.move_mark(&self.0.marks[1], &z);
            b.apply_tag(&self.0.line_tag, &a, &z);
            self.0.line_range.set(true);
        }
        if selection.is_none() {
            for back in [false, true] {
                let mut here = cursor;
                if back && !here.backward_char() {
                    continue;
                }
                let Some(c) = here.char() else {
                    continue;
                };
                if !"()[]{}".contains(c) {
                    continue;
                }
                let mut a = here;
                let mut z = here;
                if "([{".contains(c) {
                    z.forward_chars(BRACKET_RANGE);
                } else {
                    a.backward_chars(BRACKET_RANGE);
                    z.forward_char();
                }
                let text = b.text(&a, &z, true).unwrap_or_default();
                let index = (here.offset() - a.offset()) as usize;
                if let Some(other) = find_match(&text, index) {
                    for (i, pos) in [here.offset(), a.offset() + other as i32]
                        .into_iter()
                        .enumerate()
                    {
                        let a = b.iter_at_offset(pos);
                        let mut z = a;
                        z.forward_char();
                        b.move_mark(&self.0.marks[2 + i * 2], &a);
                        b.move_mark(&self.0.marks[3 + i * 2], &z);
                        b.apply_tag(&self.0.bracket_tag, &a, &z);
                    }
                    self.0.bracket_count.set(2);
                    break;
                }
            }
        }
        self.0.refreshing.set(false);
    }
    pub fn destroy(&self) {
        if self.0.destroyed.replace(true) {
            return;
        }
        if let Some(id) = self.0.sync_source.take() {
            id.remove();
        }
        if let Some(b) = self.0.buffer.upgrade() {
            for m in &self.0.marks {
                if !m.is_deleted() {
                    b.delete_mark(m);
                }
            }
        }
    }
}
pub fn css(ui: &UiColors) -> String {
    impro_ui::render_css(include_str!("gutter.css"), ui)
}
