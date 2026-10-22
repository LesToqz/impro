//! Project Markdown modes share a buffer and enforce preview editing policy
//! (port of the original `test_md_modes`, with the live renderer attached).
mod common;
use common::{poll, pump};
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_editor::{EditorPanel, EditorTab, Response};
use impro_markdown::livemd::{self, LiveMarkdown};
use std::{path::PathBuf, rc::Rc};

const GUIDE: &str = "# Project guide\n\nA **formatted** paragraph.\n\n- [ ] Review changes\n\n[Documentation](https://example.org)\n";

struct Fx {
    dir: tempfile::TempDir,
    root: PathBuf,
    panel: EditorPanel,
    window: gtk::Window,
}
impl Fx {
    fn new() -> Self {
        impro_editor::init();
        impro_markdown::init();
        impro_ui::apply_style(None).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let panel = EditorPanel::new(Some(&root));
        panel.set_dialog_hook(Some(Rc::new(|_, _| Response::Cancel)));
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(900, 640);
        window.add(&panel);
        window.show_all();
        poll(|| !panel.is_busy());
        pump(30);
        Self {
            dir,
            root,
            panel,
            window,
        }
    }
    fn open(&self, name: &str, text: &str) -> Rc<EditorTab> {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        self.panel.open_file(&path);
        poll(|| !self.panel.is_busy());
        let tab = self.panel.tab_for_path(&path).expect("tab");
        self.ready(&tab);
        tab
    }
    fn ready(&self, tab: &EditorTab) {
        let live = livemd::for_view(&tab.view);
        poll(|| {
            tab.view.is_mapped()
                && live
                    .as_ref()
                    .is_none_or(|l| !l.pending() && l.unpainted_blocks() == 0)
        });
        pump(20);
    }
    fn key(&self, key: gdk::keys::Key, state: gdk::ModifierType) -> bool {
        use glib::translate::*;
        let mut event = gdk::Event::new(gdk::EventType::KeyPress);
        unsafe {
            let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
            (*raw).key.keyval = *key;
            (*raw).key.state = state.bits();
        }
        self.panel.handle_key(&event.downcast::<gdk::EventKey>().unwrap())
    }
    fn status(&self) -> String {
        self.panel.status().text()
    }
}
impl Drop for Fx {
    fn drop(&mut self) {
        unsafe { self.window.destroy() };
        pump(5);
    }
}

/// First descendant of `widget` carrying the style class.
fn find_class(widget: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
    if widget.style_context().has_class(class) {
        return Some(widget.clone());
    }
    let container = widget.clone().downcast::<gtk::Container>().ok()?;
    container.children().iter().find_map(|c| find_class(c, class))
}
fn toggle_of(tab: &EditorTab) -> Option<gtk::Box> {
    find_class(tab.page.upcast_ref(), "md-toggle").and_then(|w| w.downcast().ok())
}
/// The `Preview` and `Code` buttons of a tab's toggle.
fn buttons_of(tab: &EditorTab) -> (gtk::ToggleButton, gtk::ToggleButton) {
    let toggle = toggle_of(tab).expect("md toggle");
    let buttons: Vec<gtk::ToggleButton> = toggle
        .children()
        .into_iter()
        .filter_map(|c| c.downcast().ok())
        .collect();
    (buttons[0].clone(), buttons[1].clone())
}
fn text(tab: &EditorTab) -> String {
    impro_editor::buffer_text(&tab.buffer)
}
fn live(tab: &EditorTab) -> LiveMarkdown {
    livemd::for_view(&tab.view).expect("live renderer")
}
fn shortcut() -> gdk::ModifierType {
    gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
}

#[test]
fn test_project_buttons_editability_rendering_and_buffer_survival() {
    common::run(|| {
        let fx = Fx::new();
        let tab = fx.open("guide.md", GUIDE);
        let toggle = toggle_of(&tab).expect("toggle");
        assert!(toggle.is_mapped());
        assert!(!tab.view.is_editable());
        assert!(!tab.view.is_cursor_visible());
        let (preview, source) = buttons_of(&tab);
        assert!(preview.is_active());
        assert!(fx.status().contains("Markdown (Preview)"));
        // Even the block holding the insertion mark stays fully rendered.
        assert!(tab.buffer.start_iter().has_tag(&live(&tab).tag("hidden").unwrap()));
        assert_eq!(tab.chrome.width(), 0);
        let original = text(&tab);
        tab.view.emit_by_name::<()>("insert-at-cursor", &[&"blocked"]);
        fx.key(k::Return, gdk::ModifierType::empty());
        fx.panel.edit("tab");
        for (index, block) in live(&tab).blocks().iter().enumerate() {
            if block.kind == "line" && block.text.starts_with("- [") {
                live(&tab).toggle_task(index, 3);
            }
        }
        let checks: Vec<gtk::CheckButton> = live(&tab)
            .blocks()
            .iter()
            .flat_map(|b| b.widgets.iter().map(|w| w.0.clone()))
            .filter_map(|w| w.downcast::<gtk::CheckButton>().ok())
            .collect();
        assert_eq!(checks.len(), 1);
        assert!(!checks[0].is_sensitive());
        checks[0].set_active(true);
        assert_eq!(text(&tab), original);
        source.clicked();
        fx.ready(&tab);
        assert!(tab.view.is_editable());
        assert!(tab.view.is_cursor_visible());
        assert!(tab.chrome.width() > 0);
        poll(|| !tab.highlighter.pending());
        assert!(fx.status().contains("Markdown (Code)"));
        tab.view.emit_by_name::<()>("insert-at-cursor", &[&"unsaved "]);
        let edited = text(&tab);
        assert_ne!(edited, original);
        preview.clicked();
        fx.ready(&tab);
        tab.undo(false);
        fx.key(k::parenleft, gdk::ModifierType::empty());
        assert_eq!(text(&tab), edited);
        assert!(tab.buffer.is_modified());
        // A plain click on a rendered web link opens it even in the read-only preview.
        let opened = Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let log = opened.clone();
        live(&tab).set_uri_opener(Some(Rc::new(move |uri| log.borrow_mut().push(uri.into()))));
        assert!(live(&tab).press_link(Some(("https://example.org".into(), false)), false));
        assert_eq!(*opened.borrow(), ["https://example.org"]);
    });
}

#[test]
fn test_notes_ignore_default_and_have_no_control() {
    common::run(|| {
        let fx = Fx::new();
        fx.panel.set_md_live_preview(false);
        let tab = fx.open(".impro/Notes/idea.md", GUIDE);
        assert!(toggle_of(&tab).is_none());
        assert_eq!(tab.markdown_mode(), Some("preview"));
        assert!(tab.view.is_editable());
        assert!(fx.status().contains("Markdown (Live)"));
        fx.key(k::M, shortcut());
        assert_eq!(tab.markdown_mode(), Some("source"));
        fx.key(k::M, shortcut());
        assert_eq!(tab.markdown_mode(), Some("preview"));
    });
}

#[test]
fn test_defaults_tab_switching_shortcut_and_outside_root() {
    common::run(|| {
        let fx = Fx::new();
        fx.panel.set_md_live_preview(false);
        let first = fx.open("guide.md", GUIDE);
        assert_eq!(first.markdown_mode(), Some("source"));
        fx.panel.set_md_live_preview(true);
        let second = fx.open("docs/reference.markdown", GUIDE);
        assert_eq!(second.markdown_mode(), Some("preview"));
        fx.panel.open_file(first.path());
        poll(|| !fx.panel.is_busy());
        assert_eq!(first.markdown_mode(), Some("source"));
        fx.key(k::M, shortcut());
        assert_eq!(first.markdown_mode(), Some("preview"));
        buttons_of(&second).1.clicked();
        assert_eq!(first.markdown_mode(), Some("preview"));
        assert_eq!(second.markdown_mode(), Some("source"));
        let outside = fx.open("../outside.md", GUIDE);
        assert!(toggle_of(&outside).is_none());
        assert!(outside.view.is_editable());
        fx.panel.set_markdown_mode("preview", None).unwrap();
        fx.key(k::M, shortcut());
        assert_eq!(outside.markdown_mode(), Some("source"));
        assert!(fx.status().contains("Markdown (Code)"));
    });
}

#[test]
fn test_rename_reclassifies_modes() {
    common::run(|| {
        let fx = Fx::new();
        let tab = fx.open("guide.md", GUIDE);
        let note = fx.root.join(".impro/Notes/note.md");
        std::fs::create_dir_all(note.parent().unwrap()).unwrap();
        fx.panel.on_path_renamed(tab.path(), &note);
        fx.ready(&tab);
        assert!(tab.view.is_editable());
        assert!(toggle_of(&tab).is_none());
        let renamed = fx.root.join("renamed.md");
        fx.panel.on_path_renamed(&note, &renamed);
        fx.ready(&tab);
        assert!(!tab.view.is_editable());
        assert!(toggle_of(&tab).expect("toggle").is_mapped());
        let outside = fx.root.parent().unwrap().join("outside.md");
        fx.panel.on_path_renamed(&renamed, &outside);
        assert_eq!(tab.markdown_mode(), Some("source"));
        assert!(tab.view.is_editable());
        assert!(toggle_of(&tab).is_none());
    });
}

#[test]
fn test_root_switch_reclassifies_existing_tabs_without_losing_edits() {
    common::run(|| {
        let fx = Fx::new();
        let body = format!("# Guide\n\n{}", "A paragraph.\n\n".repeat(120));
        let first = fx.open("guide.md", &body);
        buttons_of(&first).1.clicked();
        first.buffer.insert_at_cursor("unsaved ");
        first.buffer.place_cursor(&first.buffer.iter_at_offset(80));
        fx.ready(&first);
        let adjustment = first.source_scroll.vadjustment();
        adjustment.set_value(180.0);
        let (original, cursor, scroll) = (text(&first), first.cursor(), adjustment.value());
        let buffer = first.buffer.clone();
        let renderer = live(&first);
        let second_root = fx.root.parent().unwrap().join("second");
        std::fs::create_dir_all(second_root.join(".impro/Notes")).unwrap();
        let second_root = second_root.canonicalize().unwrap();
        let note = fx.open("../second/.impro/Notes/idea.md", GUIDE);
        let project = fx.open("../second/guide.markdown", GUIDE);
        assert_eq!(note.markdown_location(), "outside");
        assert_eq!(project.markdown_mode(), Some("source"));
        fx.panel.open_file(first.path());
        poll(|| !fx.panel.is_busy());
        fx.ready(&first);
        // Capture the visible position after switching back to the edited tab.
        adjustment.set_value(scroll);
        let focus = fx.window.focused_widget();
        fx.panel.set_root(Some(&second_root.join("..").join("second")));
        poll(|| !fx.panel.is_busy());
        fx.ready(&first);
        assert_eq!(fx.panel.root(), Some(second_root.clone()));
        assert_eq!(first.markdown_location(), "outside");
        assert_eq!(first.markdown_mode(), Some("source"));
        assert!(toggle_of(&first).is_none());
        assert!(first.view.is_editable());
        assert_eq!(first.buffer, buffer);
        assert!(live(&first).ptr_eq(&renderer));
        assert_eq!(text(&first), original);
        assert!(first.buffer.is_modified());
        assert_eq!(first.cursor(), cursor);
        assert!((adjustment.value() - scroll).abs() <= 1.0);
        assert_eq!(fx.window.focused_widget(), focus);
        assert_eq!(note.markdown_location(), "note");
        assert_eq!(note.markdown_mode(), Some("preview"));
        assert!(note.view.is_editable());
        assert!(toggle_of(&note).is_none());
        assert_eq!(project.markdown_location(), "project");
        assert!(toggle_of(&project).is_some());
        assert_eq!(project.markdown_mode(), Some("source"));
        project.set_markdown_mode("preview");
        fx.panel.set_root(Some(&second_root));
        poll(|| !fx.panel.is_busy());
        assert_eq!(project.markdown_mode(), Some("preview"));
        assert!(!project.view.is_editable());
        for _ in 0..2 {
            fx.panel.set_root(Some(&fx.root));
            poll(|| !fx.panel.is_busy());
            assert_eq!(first.markdown_location(), "project");
            assert_eq!(first.markdown_mode(), Some("source"));
            assert!(toggle_of(&first).is_some());
            for tab in [&note, &project] {
                assert_eq!(tab.markdown_location(), "outside");
                assert_eq!(tab.markdown_mode(), Some("source"));
                assert!(toggle_of(tab).is_none());
            }
            fx.panel.set_root(Some(&second_root));
            poll(|| !fx.panel.is_busy());
            assert_eq!(first.markdown_location(), "outside");
            assert_eq!(note.markdown_location(), "note");
            assert_eq!(note.markdown_mode(), Some("preview"));
            assert!(note.view.is_editable());
            assert!(toggle_of(&note).is_none());
            assert_eq!(project.markdown_location(), "project");
            assert!(toggle_of(&project).is_some());
        }
        fx.panel.set_root(None);
        poll(|| !fx.panel.is_busy());
        assert_eq!(fx.panel.root(), None);
        assert_eq!(note.markdown_location(), "standalone");
        assert!(note.view.is_editable());
        assert!(toggle_of(&project).is_none());
        assert_eq!(text(&first), original);
    });
}

#[test]
fn test_theme_geometry_and_screenshots() {
    common::run(|| {
        let fx = Fx::new();
        let tab = fx.open("guide.md", GUIDE);
        let folder = std::env::var("IMPRO_QA_SHOTS").ok().map(PathBuf::from);
        for name in ["impro-dark", "impro-light"] {
            let theme = impro_core::themes::get_theme(Some(name));
            impro_ui::apply_theme(theme, None, None, None).unwrap();
            fx.panel.set_syntax_style(&theme.syntax);
            fx.panel.set_chrome(&theme.chrome);
            for mode in ["preview", "source"] {
                let (preview, source) = buttons_of(&tab);
                if mode == "preview" { preview } else { source }.clicked();
                fx.ready(&tab);
                pump(150);
                let toggle = toggle_of(&tab).expect("toggle");
                let control = toggle.allocation();
                assert!(tab.source_scroll.margin_top() >= control.height() + 10);
                assert!(control.x() + control.width() <= tab.page.allocated_width());
                if let Some(folder) = &folder {
                    let window = fx.window.window().unwrap();
                    let pixbuf = window
                        .pixbuf(0, 0, fx.window.allocated_width(), fx.window.allocated_height())
                        .unwrap();
                    pixbuf.savev(folder.join(format!("{name}-{mode}.png")), "png", &[]).unwrap();
                }
            }
        }
        let _ = impro_ui::apply_style(None);
    });
}
