use crate::{
    gutter::{self, ViewChrome},
    highlight::{self, Highlighter},
    history::History,
    EditorPanel,
};
use gtk::prelude::*;
use impro_markdown::{LiveMarkdown, SlashMenu, MAX_BYTES};
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::SystemTime,
};
/// Stable file-tab handle. Release handles when checking final buffer destruction.
pub struct EditorTab {
    panel: glib::WeakRef<EditorPanel>,
    path: RefCell<PathBuf>,
    resolved_path: RefCell<PathBuf>,
    language_name: Cell<&'static str>,
    pub buffer: gtk::TextBuffer,
    pub view: gtk::TextView,
    pub page: gtk::Overlay,
    pub source_scroll: gtk::ScrolledWindow,
    pub header: TabHeader,
    pub highlighter: Highlighter,
    pub chrome: ViewChrome,
    history: History,
    pub(crate) saved_text: RefCell<String>,
    pub(crate) saved_chars: Cell<i32>,
    pub(crate) mtime: Cell<SystemTime>,
    pub(crate) ending: &'static str,
    pub(crate) deleted: Cell<bool>,
    pub(crate) closed: Cell<bool>,
    pub(crate) saving: Cell<bool>,
    pub(crate) md_renderer: RefCell<Option<Rc<LiveMarkdown>>>,
    pub(crate) slash_menu: RefCell<Option<Rc<SlashMenu>>>,
    md_mode: Cell<&'static str>,
    md_large: Cell<bool>,
    md_toggle: RefCell<Option<gtk::Box>>,
    md_buttons: RefCell<Vec<(String, gtk::ToggleButton)>>,
    md_updating: Cell<bool>,
    restore: Rc<RefCell<Option<glib::SourceId>>>,
    pub(crate) revision: Cell<u64>,
    paint_source: Rc<RefCell<Option<glib::SourceId>>>,
}
impl EditorTab {
    pub(crate) fn new(panel: &EditorPanel, path: PathBuf, doc: crate::file::Document) -> Rc<Self> {
        let trace_start = std::time::Instant::now();
        let trace = |label: &str| {
            if std::env::var_os("IMPRO_EDITOR_TRACE").is_some() {
                eprintln!("tab {label}: {:?}", trace_start.elapsed());
            }
        };
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buffer.set_text(&doc.text);
        buffer.set_modified(false);
        trace("text");
        let view = gtk::TextView::with_buffer(&buffer);
        view.set_pixels_above_lines(1);
        view.set_pixels_below_lines(1);
        view.set_wrap_mode(gtk::WrapMode::None);
        view.set_left_margin(0);
        view.set_right_margin(12);
        if std::env::var_os("IMPRO_EDITOR_TRACE").is_some() {
            let once = Rc::new(Cell::new(false));
            let seen = once.clone();
            view.connect_draw(move |v, _| {
                if !seen.replace(true) {
                    eprintln!(
                        "view draw before: {:?} visible={:?}",
                        trace_start.elapsed(),
                        v.visible_rect()
                    );
                }
                glib::Propagation::Proceed
            });
            let once = Rc::new(Cell::new(false));
            view.connect_local("draw", true, move |_| {
                if !once.replace(true) {
                    eprintln!("view draw after: {:?}", trace_start.elapsed());
                }
                Some(false.to_value())
            });
        }
        trace("view");
        let highlighter = Highlighter::new(
            &view,
            &panel.syntax_style(),
            highlight::language_name(
                &path.to_string_lossy(),
                doc.text.lines().next().unwrap_or(""),
            ),
        );
        trace("syntax");
        let chrome = ViewChrome::new(&view, &panel.chrome());
        chrome.set_line_numbers(panel.line_numbers());
        chrome.set_highlight_line(panel.highlight_line());
        let source_scroll =
            gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        source_scroll.add(&view);
        let page = gtk::Overlay::new();
        page.add(&source_scroll);
        let header = TabHeader::new("Close file", None);
        let history = History::new(&buffer);
        let language_name = highlight::language_name(
            &path.to_string_lossy(),
            doc.text.lines().next().unwrap_or(""),
        );
        let tab = Rc::new(Self {
            panel: panel.downgrade(),
            language_name: Cell::new(language_name),
            path: RefCell::new(path),
            resolved_path: RefCell::new(doc.resolved_path),
            saved_chars: Cell::new(buffer.char_count()),
            buffer: buffer.clone(),
            view: view.clone(),
            page: page.clone(),
            source_scroll,
            header,
            highlighter,
            chrome,
            history,
            saved_text: RefCell::new(doc.text),
            mtime: Cell::new(doc.mtime),
            ending: doc.ending,
            deleted: Cell::new(false),
            closed: Cell::new(false),
            saving: Cell::new(false),
            md_renderer: RefCell::new(None),
            slash_menu: RefCell::new(None),
            md_mode: Cell::new("source"),
            md_large: Cell::new(false),
            md_toggle: RefCell::new(None),
            md_buttons: RefCell::new(Vec::new()),
            md_updating: Cell::new(false),
            restore: Rc::new(RefCell::new(None)),
            revision: Cell::new(0),
            paint_source: Rc::new(RefCell::new(None)),
        });
        let weak = Rc::downgrade(&tab);
        tab.header.close.connect_clicked(move |_| {
            if let Some(t) = weak.upgrade() {
                if let Some(p) = t.panel.upgrade() {
                    p.close_tab(&t, |_| {});
                }
            }
        });
        let weak = Rc::downgrade(&tab);
        tab.header
            .widget
            .connect_button_press_event(move |_, event| {
                if event.button() == 2 {
                    if let Some(t) = weak.upgrade() {
                        if let Some(p) = t.panel.upgrade() {
                            p.close_tab(&t, |_| {});
                        }
                    }
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
        let weak = Rc::downgrade(&tab);
        buffer.connect_changed(move |_| {
            if let Some(t) = weak.upgrade() {
                t.revision.set(t.revision.get() + 1);
                t.flush_edit();
                if !t.history.restoring() {
                    t.sync_dirty();
                }
                if let Some(p) = t.panel.upgrade() {
                    p.update_status();
                }
            }
        });
        let weak = Rc::downgrade(&tab);
        buffer.connect_modified_changed(move |_| {
            if let Some(t) = weak.upgrade() {
                t.update_title();
                if let Some(p) = t.panel.upgrade() {
                    p.dirty_changed();
                }
            }
        });
        let weak = Rc::downgrade(&tab);
        buffer.connect_mark_set(move |_, _, _| {
            if let Some(t) = weak.upgrade() {
                if let Some(p) = t.panel.upgrade() {
                    p.update_status();
                }
            }
        });
        let weak = Rc::downgrade(&tab);
        view.connect_key_press_event(move |_, event| {
            if let Some(t) = weak.upgrade() {
                if let Some(p) = t.panel.upgrade() {
                    return if p.handle_key(event) {
                        glib::Propagation::Stop
                    } else {
                        glib::Propagation::Proceed
                    };
                }
            }
            glib::Propagation::Proceed
        });
        let weak = panel.downgrade();
        view.connect_scroll_event(move |_, event| {
            if event.state().contains(gdk::ModifierType::CONTROL_MASK) {
                if let Some(p) = weak.upgrade() {
                    let step = match event.direction() {
                        gdk::ScrollDirection::Up => 1,
                        gdk::ScrollDirection::Down => -1,
                        _ => {
                            let (_, y) = event.delta();
                            if y > 0.0 {
                                -1
                            } else if y < 0.0 {
                                1
                            } else {
                                0
                            }
                        }
                    };
                    if step != 0 {
                        p.zoom(step);
                    }
                }
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        let weak = Rc::downgrade(&tab);
        page.connect_destroy(move |_| {
            if let Some(t) = weak.upgrade() {
                t.destroy();
            }
        });
        let weak = Rc::downgrade(&tab);
        view.connect_style_updated(move |_| {
            if let Some(t) = weak.upgrade() {
                t.set_tab_stops();
            }
        });
        tab.chrome.set_font_family(panel.font_family().as_deref());
        trace("signals");
        tab.set_font_size(panel.font_size());
        trace("font");
        tab.sync_markdown();
        tab.update_title();
        trace("done");
        tab
    }
    pub fn path(&self) -> PathBuf {
        self.path.borrow().clone()
    }
    pub fn ending(&self) -> &'static str {
        self.ending
    }
    pub fn deleted(&self) -> bool {
        self.deleted.get()
    }
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }
    pub fn cursor(&self) -> i32 {
        self.buffer
            .iter_at_mark(&self.buffer.get_insert().expect("buffer insert mark"))
            .offset()
    }
    pub fn text(&self) -> String {
        crate::buffer_text(&self.buffer)
    }
    pub fn history_len(&self) -> usize {
        self.history.len()
    }
    pub fn markdown_mode(&self) -> Option<&'static str> {
        self.md_renderer
            .borrow()
            .as_ref()
            .map(|_| self.md_mode.get())
    }
    pub fn markdown_location(&self) -> &'static str {
        let Some(panel) = self.panel.upgrade() else {
            return "standalone";
        };
        let Some(root) = panel.root() else {
            return "standalone";
        };
        let path = self.resolved_path.borrow().clone();
        if !path.starts_with(&root) {
            return "outside";
        }
        if path.starts_with(root.join(".impro/Notes"))
            && impro_core::notes::is_note_path(Some(&path))
        {
            "note"
        } else {
            "project"
        }
    }
    pub fn language(&self) -> String {
        if is_markdown(&self.path()) {
            let location = self.markdown_location();
            let mode = match (
                matches!(location, "note" | "standalone"),
                self.md_mode.get() == "preview",
            ) {
                (true, true) => "Live",
                (true, false) => "Source",
                (false, true) => "Preview",
                (false, false) => "Code",
            };
            format!(
                "Markdown ({mode}){}",
                if self.md_large.get() {
                    " — live preview disabled above 1 MiB"
                } else {
                    ""
                }
            )
        } else {
            self.language_name.get().into()
        }
    }
    pub(crate) fn set_path(self: &Rc<Self>, path: PathBuf) {
        self.resolved_path.replace(path.clone());
        self.path.replace(path);
        self.detect_language();
        self.sync_markdown();
        self.sync_slash_menu();
        self.update_title();
    }
    pub(crate) fn set_resolved_path(self: &Rc<Self>, path: PathBuf) {
        self.resolved_path.replace(path);
        self.sync_markdown();
    }
    fn detect_language(&self) {
        let mut end = self.buffer.start_iter();
        end.forward_to_line_end();
        let text = self
            .buffer
            .text(&self.buffer.start_iter(), &end, false)
            .unwrap_or_default();
        let language = highlight::language_name(&self.path().to_string_lossy(), &text);
        self.language_name.set(language);
        self.highlighter
            .set_language(if self.md_mode.get() == "preview" {
                "Plain Text"
            } else {
                language
            });
    }
    pub fn update_title(&self) {
        let path = self.path();
        self.header.label.set_text(&format!(
            "{}{}{}",
            if self.buffer.is_modified() {
                "● "
            } else {
                ""
            },
            path.file_name().unwrap_or_default().to_string_lossy(),
            if self.deleted.get() { " (deleted)" } else { "" }
        ));
        self.header
            .widget
            .set_tooltip_text(Some(&path.to_string_lossy()));
    }
    pub(crate) fn sync_dirty(&self) {
        let unchanged = self.history.at_saved()
            || (self.buffer.char_count() == self.saved_chars.get()
                && self.text() == *self.saved_text.borrow());
        self.buffer.set_modified(self.deleted.get() || !unchanged);
    }
    pub(crate) fn mark_saved(&self, text: String, revision: u64) {
        self.saved_chars.set(text.chars().count() as i32);
        self.saved_text.replace(text);
        self.deleted.set(false);
        if self.revision.get() == revision {
            self.history.mark_saved();
        } else {
            self.history.invalidate_saved();
        }
        self.sync_dirty();
        self.update_title();
    }
    pub fn undo(&self, redo: bool) {
        if !self.view.is_editable() {
            return;
        }
        self.history.undo(redo);
        self.sync_dirty();
        if let Some(p) = self.panel.upgrade() {
            p.update_status();
        }
    }
    pub fn set_font_size(&self, size: i32) {
        self.chrome.set_font_size(size);
        self.set_tab_stops();
        self.style_markdown();
    }
    pub fn set_tab_stops(&self) {
        let width = self.panel.upgrade().map(|p| p.indent_width()).unwrap_or(4);
        let layout = gutter::layout_for(&self.view, &" ".repeat(width as usize));
        let mut tabs = pango::TabArray::new(1, true);
        tabs.set_tab(0, pango::TabAlign::Left, layout.pixel_size().0);
        self.view.set_tabs(&tabs);
    }
    pub(crate) fn style_markdown(&self) {
        let renderer = self.md_renderer.borrow().clone();
        if let (Some(r), Some(p)) = (renderer, self.panel.upgrade()) {
            r.set_style(&p.syntax_style(), &p.chrome(), p.font_size());
        }
    }
    pub(crate) fn sync_markdown(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        if !is_markdown(&self.path()) {
            let renderer = self.md_renderer.take();
            if let Some(r) = renderer {
                r.set_enabled(false);
                r.destroy();
            }
            self.md_mode.set("source");
            self.view.set_editable(true);
            self.view.set_cursor_visible(true);
            if let Some(p) = self.panel.upgrade() {
                self.chrome.set_line_numbers(p.line_numbers());
                self.chrome.set_highlight_line(p.highlight_line());
            }
            self.detect_language();
            self.sync_toggle(false);
            self.sync_slash_menu();
            return;
        }
        let Some(panel) = self.panel.upgrade() else {
            return;
        };
        let created = self.md_renderer.borrow().is_none();
        if created {
            let weak = panel.downgrade();
            let tab = Rc::downgrade(self);
            let renderer = LiveMarkdown::new(
                &self.view,
                move |path| {
                    if let Some(p) = weak.upgrade() {
                        p.open_file(path);
                    }
                },
                move || tab.upgrade().map(|t| t.path()),
            );
            renderer.set_readable_width(panel.md_readable_width());
            self.md_renderer.replace(Some(Rc::new(renderer)));
            self.style_markdown();
        }
        let location = self.markdown_location();
        self.sync_toggle(location == "project");
        let mode = if location == "outside" {
            "source"
        } else if location == "note" || created && panel.md_live_preview() {
            "preview"
        } else {
            self.md_mode.get()
        };
        self.set_markdown_mode(mode);
    }
    fn sync_toggle(self: &Rc<Self>, visible: bool) {
        if !visible {
            let toggle = self.md_toggle.take();
            self.md_buttons.replace(Vec::new());
            if let Some(t) = toggle {
                unsafe {
                    t.destroy();
                }
            }
            self.source_scroll.set_margin_top(0);
            return;
        }
        if self.md_toggle.borrow().is_some() {
            return;
        }
        let toggle = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        toggle.style_context().add_class("md-toggle");
        toggle.set_halign(gtk::Align::End);
        toggle.set_valign(gtk::Align::Start);
        toggle.set_margin_top(5);
        toggle.set_margin_bottom(5);
        toggle.set_margin_end(12);
        let mut buttons = Vec::new();
        for (mode, label, icon) in [
            ("preview", "Preview", "view-reveal-symbolic"),
            ("source", "Code", "text-x-generic-symbolic"),
        ] {
            let button = gtk::ToggleButton::new();
            button.set_relief(gtk::ReliefStyle::None);
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 5);
            let image = gtk::Image::from_icon_name(Some(icon), gtk::IconSize::Menu);
            image.set_pixel_size(14);
            content.pack_start(&image, false, false, 0);
            content.pack_start(&gtk::Label::new(Some(label)), false, false, 0);
            button.add(&content);
            let weak = Rc::downgrade(self);
            button.connect_clicked(move |_| {
                if let Some(t) = weak.upgrade() {
                    if !t.md_updating.get() {
                        t.set_markdown_mode(mode);
                    }
                }
            });
            toggle.add(&button);
            buttons.push((mode.to_string(), button));
        }
        self.md_buttons.replace(buttons);
        self.md_toggle.replace(Some(toggle.clone()));
        self.page.add_overlay(&toggle);
        let scroll = self.source_scroll.downgrade();
        toggle.connect_size_allocate(move |_, rect| {
            if let Some(s) = scroll.upgrade() {
                s.set_margin_top(rect.height() + 10);
            }
        });
        toggle.show_all();
    }
    pub fn set_markdown_mode(&self, mode: &str) {
        let renderer = self.md_renderer.borrow().clone();
        let Some(renderer) = renderer else {
            return;
        };
        let Some(panel) = self.panel.upgrade() else {
            return;
        };
        let location = self.markdown_location();
        let mode = if location == "outside" || mode == "source" {
            "source"
        } else {
            "preview"
        };
        self.md_large
            .set(self.buffer.char_count() as usize > MAX_BYTES || self.text().len() > MAX_BYTES);
        let mode = if mode == "preview" && self.md_large.get() {
            self.md_mode.get()
        } else {
            mode
        };
        let adj = self.source_scroll.vadjustment();
        let scroll = adj.value();
        let cursor = self.cursor();
        self.md_mode.set(mode);
        let live = mode == "preview";
        let editable = !(live && location == "project");
        self.view.set_editable(editable);
        self.view.set_cursor_visible(editable);
        let buttons = self.md_buttons.borrow().clone();
        self.md_updating.set(true);
        for (choice, b) in buttons {
            b.set_active(choice == mode);
        }
        self.md_updating.set(false);
        self.detect_language();
        self.chrome.set_line_numbers(panel.line_numbers() && !live);
        self.chrome
            .set_highlight_line(panel.highlight_line() && !live);
        renderer.set_enabled(live);
        self.sync_slash_menu();
        self.buffer
            .place_cursor(&self.buffer.iter_at_offset(cursor));
        adj.set_value(scroll);
        if let Some(id) = self.restore.take() {
            id.remove();
        }
        let view = self.view.downgrade();
        let source = self.restore.clone();
        let id = glib::idle_add_local_full(glib::Priority::LOW, move || {
            source.take();
            if let Some(v) = view.upgrade() {
                if let Some(a) = v.vadjustment() {
                    a.set_value(scroll);
                }
            }
            glib::ControlFlow::Break
        });
        self.restore.replace(Some(id));
        self.view.grab_focus();
        panel.update_status();
    }
    pub(crate) fn sync_slash_menu(&self) {
        let enabled = self.panel.upgrade().is_some_and(|p| p.slash_commands())
            && self.md_mode.get() == "preview"
            && impro_core::notes::is_note_path(Some(&self.path()))
            && !self.closed.get();
        if enabled && self.slash_menu.borrow().is_none() {
            self.slash_menu
                .replace(Some(Rc::new(SlashMenu::new(&self.view))));
        } else if !enabled {
            if let Some(menu) = self.slash_menu.take() {
                menu.destroy();
            }
        }
        impro_markdown::seltoolbar::sync(self);
    }
    #[allow(deprecated)]
    fn flush_edit(&self) {
        if self.closed.get() || self.paint_source.borrow().is_some() || !self.view.is_mapped() {
            return;
        }
        use glib::translate::ToGlibPtr;
        let weak = self.view.downgrade();
        let source = self.paint_source.clone();
        let id = glib::idle_add_local_full(glib::Priority::from(130), move || {
            source.take();
            if let Some(view) = weak.upgrade() {
                if view.is_mapped() {
                    if let Some(window) = gtk::prelude::WidgetExt::window(&view) {
                        unsafe {
                            gdk::ffi::gdk_window_process_updates(window.to_glib_none().0, 1);
                        }
                    }
                }
            }
            glib::ControlFlow::Break
        });
        self.paint_source.replace(Some(id));
    }
    pub(crate) fn destroy(&self) {
        if self.closed.replace(true) {
            return;
        }
        if let Some(id) = self.restore.take() {
            id.remove();
        }
        if let Some(id) = self.paint_source.take() {
            id.remove();
        }
        if let Some(r) = self.md_renderer.take() {
            r.destroy();
        }
        if let Some(m) = self.slash_menu.take() {
            m.destroy();
        }
        self.highlighter.destroy();
        self.chrome.destroy();
    }
}
pub(crate) fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
}
/// Shared tab-strip widgets for file and arbitrary view tabs.
pub struct TabHeader {
    pub widget: gtk::EventBox,
    pub inner: gtk::Box,
    pub label: gtk::Label,
    pub close: gtk::Button,
    pub divider: gtk::Separator,
}
impl TabHeader {
    pub(crate) fn new(tooltip: &str, icon: Option<&str>) -> Self {
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(pango::EllipsizeMode::End);
        label.set_max_width_chars(32);
        let widget = gtk::EventBox::new();
        widget.set_visible_window(false);
        widget.add_events(gdk::EventMask::ENTER_NOTIFY_MASK | gdk::EventMask::LEAVE_NOTIFY_MASK);
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        inner.style_context().add_class("impro-tab-inner");
        inner.set_margin_top(3);
        inner.set_margin_bottom(3);
        widget.add(&inner);
        if let Some(icon) = icon {
            if gtk::IconTheme::default().is_some_and(|t| t.has_icon(icon)) {
                let image = gtk::Image::from_icon_name(Some(icon), gtk::IconSize::Menu);
                image.set_pixel_size(14);
                inner.pack_start(&image, false, false, 0);
            }
        }
        inner.pack_start(&label, true, true, 0);
        let close = gtk::Button::new();
        let image = gtk::Image::from_icon_name(Some("window-close-symbolic"), gtk::IconSize::Menu);
        image.set_pixel_size(14);
        close.add(&image);
        close.style_context().add_class("impro-tab-close");
        close.set_relief(gtk::ReliefStyle::None);
        close.set_tooltip_text(Some(tooltip));
        inner.pack_start(&close, false, false, 0);
        let divider = gtk::Separator::new(gtk::Orientation::Vertical);
        divider.style_context().add_class("impro-tab-divider");
        divider.set_valign(gtk::Align::Center);
        inner.pack_end(&divider, false, false, 0);
        let weak = inner.downgrade();
        widget.connect_enter_notify_event(move |_, _| {
            if let Some(w) = weak.upgrade() {
                w.style_context().add_class("hover");
            }
            glib::Propagation::Proceed
        });
        let weak = inner.downgrade();
        widget.connect_leave_notify_event(move |_, _| {
            if let Some(w) = weak.upgrade() {
                w.style_context().remove_class("hover");
            }
            glib::Propagation::Proceed
        });
        widget.connect_unmap(|w| w.set_margin_start(0));
        Self {
            widget,
            inner,
            label,
            close,
            divider,
        }
    }
}
