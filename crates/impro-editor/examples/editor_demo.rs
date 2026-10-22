//! Standalone editor QA host. IMPRO_QA_SCREENSHOT captures the settled native window.
use gtk::prelude::*;
use impro_editor::EditorPanel;
use std::{path::PathBuf, rc::Rc, time::Duration};
const PYTHON:&str="#!/usr/bin/env python3\n\nclass Greeting:\n    \"\"\"A small editor example.\"\"\"\n\n    def greet(self, name):\n        # Keep the welcome message simple.\n        message = \"Hello, \" + name\n        return message\n\nprint(Greeting().greet(\"world\"))\n";
fn open_next(panel: EditorPanel, paths: Rc<Vec<PathBuf>>, index: usize) {
    if let Some(path) = paths.get(index).cloned() {
        let next = panel.clone();
        panel.open_file_with(path, move |_| open_next(next, paths, index + 1));
    } else {
        panel.notebook().set_current_page(Some(0));
        if let Some(t) = panel.current_tab() {
            t.buffer.place_cursor(&t.buffer.iter_at_line(5));
        }
        if let Ok(path) = std::env::var("IMPRO_QA_SCREENSHOT") {
            let panel = panel.downgrade();
            glib::timeout_add_local(Duration::from_millis(50), move || {
                let Some(panel) = panel.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if panel.tabs().iter().any(|t| t.highlighter.pending()) {
                    return glib::ControlFlow::Continue;
                }
                let Some(window) = panel
                    .toplevel()
                    .and_then(|w| w.downcast::<gtk::Window>().ok())
                else {
                    return glib::ControlFlow::Break;
                };
                let weak = window.downgrade();
                let path = path.clone();
                glib::timeout_add_local_once(Duration::from_millis(150), move || {
                    if let Some(w) = weak.upgrade() {
                        if let Some(native) = w.window() {
                            native
                                .pixbuf(0, 0, w.allocated_width(), w.allocated_height())
                                .expect("window pixels")
                                .savev(&path, "png", &[])
                                .expect("write screenshot");
                        }
                        unsafe {
                            w.destroy();
                        }
                    }
                    gtk::main_quit();
                });
                glib::ControlFlow::Break
            });
        }
    }
}
fn main() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let root = std::env::var_os("IMPRO_DEMO_FIXTURES")
        .map(PathBuf::from)
        .unwrap_or_else(|| temp.path().to_path_buf());
    std::fs::create_dir_all(&root).expect("fixture directory");
    let fixtures = [
        ("greeting.py", PYTHON),
        (
            "main.rs",
            "fn main() {\n    println!(\"Hello, world!\");\n}\n",
        ),
        ("README.md", "# impro\n\nA **live** Markdown document.\n"),
        (
            "settings.json",
            "{\n  \"name\": \"impro\",\n  \"enabled\": true,\n  \"indent\": 4\n}\n",
        ),
    ];
    for (name, text) in fixtures {
        std::fs::write(root.join(name), text).expect("fixture file");
    }
    gtk::init().expect("GTK display");
    impro_editor::init();
    let theme = impro_core::themes::get_theme(
        std::env::var("IMPRO_DEMO_THEME")
            .ok()
            .as_deref()
            .or(Some("impro-dark")),
    );
    impro_ui::apply_style(Some(theme)).expect("theme CSS");
    let panel = EditorPanel::default();
    panel.apply_theme(theme);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro");
    window.set_default_size(900, 600);
    window.add(&panel);
    window.connect_delete_event(|_, _| {
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    window.show_all();
    panel.connect_error(|_, message| eprintln!("{message}"));
    let paths = Rc::new(fixtures.iter().map(|(name, _)| root.join(name)).collect());
    open_next(panel, paths, 0);
    gtk::main();
}
