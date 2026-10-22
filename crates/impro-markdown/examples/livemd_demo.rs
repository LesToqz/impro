//! Live Markdown QA host: opens a note in the real editor panel.
//!
//! Environment: `IMPRO_DEMO_THEME` (theme id), `IMPRO_DEMO_FILE` (Markdown file to
//! show; a built-in sample otherwise), `IMPRO_DEMO_SIZE` (`WxH`, default 1000x800),
//! `IMPRO_DEMO_CARET` (character offset), `IMPRO_DEMO_SCROLL` (pixels) and
//! `IMPRO_QA_SCREENSHOT` (PNG path; the window closes after capturing).
use gtk::prelude::*;
use impro_editor::EditorPanel;
use std::{path::PathBuf, time::Duration};

const SAMPLE: &str = "# A living document\n\n## Working notes\n\n**Bold ideas**, *gentle emphasis*, and `inline code`.\n\n- A first item\n  - A nested item\n- [ ] A task to finish\n- [x] A completed task\n\n1. First\n2. Second\n\n> A quote worth remembering.\n\n---\n\n```python\ndef greet(name):\n    return f\"Hello {name}\"\n```\n\n| Task | Owner | State |\n| :--- | :---: | ---: |\n| Review | Team | Ready |\n| Write | Me | Next |\n\n[Project link](https://example.org) and [[note|a related note]]\n\n";

fn env_number(name: &str) -> Option<i32> {
    std::env::var(name).ok()?.parse().ok()
}

fn main() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = temp.path().to_path_buf();
    let note: PathBuf = root.join(".impro/Notes/demo.md");
    std::fs::create_dir_all(note.parent().expect("notes dir")).expect("notes dir");
    let text = std::env::var_os("IMPRO_DEMO_FILE")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| SAMPLE.to_string());
    std::fs::write(&note, &text).expect("note");
    if let Ok(source) = std::env::var("IMPRO_DEMO_ASSETS") {
        for entry in std::fs::read_dir(source).into_iter().flatten().flatten() {
            let _ = std::fs::copy(
                entry.path(),
                note.parent().expect("notes dir").join(entry.file_name()),
            );
        }
    }
    gtk::init().expect("GTK display");
    impro_ui::init();
    impro_editor::init();
    impro_markdown::init();
    let theme = impro_core::themes::get_theme(
        std::env::var("IMPRO_DEMO_THEME")
            .ok()
            .as_deref()
            .or(Some("impro-dark")),
    );
    impro_ui::apply_style(Some(theme)).expect("theme CSS");
    let panel = EditorPanel::new(Some(&root));
    panel.apply_theme(theme);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro");
    let size = std::env::var("IMPRO_DEMO_SIZE").unwrap_or_else(|_| "1000x800".into());
    let (w, h) = size.split_once('x').map_or((1000, 800), |(w, h)| {
        (w.parse().unwrap_or(1000), h.parse().unwrap_or(800))
    });
    window.set_default_size(w, h);
    window.add(&panel);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    window.show_all();
    panel.connect_error(|_, message| eprintln!("{message}"));
    let shot = std::env::var("IMPRO_QA_SCREENSHOT").ok();
    let opened = panel.clone();
    panel.open_file_with(note, move |tab| {
        let Some(tab) = tab else { return };
        if let Some(offset) = env_number("IMPRO_DEMO_CARET") {
            tab.buffer.place_cursor(&tab.buffer.iter_at_offset(offset));
        }
        let Some(path) = shot else { return };
        let panel = opened.downgrade();
        let started = std::time::Instant::now();
        glib::timeout_add_local(Duration::from_millis(40), move || {
            let Some(panel) = panel.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let Some(tab) = panel.current_tab() else {
                return glib::ControlFlow::Continue;
            };
            let busy = impro_markdown::livemd::for_view(&tab.view)
                .is_some_and(|live| live.pending() || live.unpainted_blocks() > 0);
            if (busy || !tab.view.is_mapped()) && started.elapsed() < Duration::from_secs(20) {
                return glib::ControlFlow::Continue;
            }
            if let Some(scroll) = env_number("IMPRO_DEMO_SCROLL") {
                tab.source_scroll.vadjustment().set_value(f64::from(scroll));
            }
            let Some(window) = panel
                .toplevel()
                .and_then(|w| w.downcast::<gtk::Window>().ok())
            else {
                return glib::ControlFlow::Break;
            };
            let weak = window.downgrade();
            let path = path.clone();
            glib::timeout_add_local_once(Duration::from_millis(400), move || {
                if let Some(w) = weak.upgrade() {
                    if let Some(native) = w.window() {
                        native
                            .pixbuf(0, 0, w.allocated_width(), w.allocated_height())
                            .expect("window pixels")
                            .savev(&path, "png", &[])
                            .expect("write screenshot");
                    }
                    unsafe { w.destroy() };
                }
                gtk::main_quit();
            });
            glib::ControlFlow::Break
        });
    });
    gtk::main();
}
