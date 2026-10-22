//! Paired full-window scenes; protocol documented in scripts/parity/README.md.
use crate::common::*;
use gtk::prelude::*;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn capture(widget: &impl IsA<gtk::Widget>, path: &std::path::Path) -> Vec<u8> {
    impro::app::capture_window(widget, path).unwrap();
    std::fs::read(path).unwrap()
}

pub fn run() {
    let args: Vec<_> = std::env::args().collect();
    let scene = args.get(2).expect("scene name").as_str();
    if scene.starts_with("notes-") {
        println!("unsupported {scene}");
        std::process::exit(77);
    }
    let root = PathBuf::from(&args[3]);
    let output = PathBuf::from(&args[4]);
    let mut values = impro_core::settings::defaults().clone();
    let fixture: impro_core::settings::Values =
        serde_json::from_str(&std::env::var("IMPRO_PARITY_SETTINGS").expect("fixture settings"))
            .unwrap();
    values.extend(fixture);
    values.insert("theme".into(), args[5].clone().into());
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(false);
    settings.set_gtk_cursor_blink(false);
    settings.set_gtk_font_name(Some("DejaVu Sans 10"));
    let app = gtk::Application::new(Some("dev.impro.Parity"), gio::ApplicationFlags::NON_UNIQUE);
    app.register(None::<&gio::Cancellable>).unwrap();
    let w = impro::MainWindow::with_values(&app, &root, values);
    w.window.set_default_size(1200, 800);
    w.show();
    for name in ["main.py", "README.md", "data.json"] {
        w.editor.open_file(root.join(name));
        poll(|| !w.editor.is_busy());
    }
    poll(|| w.layout_ready() && !w.explorer.is_busy());
    poll(|| w.editor.tabs().iter().all(|t| !t.highlighter.pending()));
    let open = |name: &str| {
        w.editor.open_file(root.join(name));
        poll(|| !w.editor.is_busy());
    };
    match scene {
        "default" | "terminal-bottom" => {}
        "explorer-expanded" => {
            w.explorer.tree().expand_all();
            poll(|| !w.explorer.is_busy());
        }
        "explorer-selected" | "explorer-hover" | "explorer-menu" | "explorer-rename" => {
            w.explorer.select(&root.join("main.py"));
            w.explorer.focus_tree();
            if scene == "explorer-menu" {
                w.explorer.popup_context_menu();
            }
            if scene == "explorer-rename" {
                w.explorer.rename_dialog();
            }
            if scene == "explorer-hover" {
                w.explorer
                    .tree()
                    .set_state_flags(gtk::StateFlags::PRELIGHT, false);
            }
        }
        "search-results" | "search-empty" => {
            let query = if scene == "search-results" {
                "Example"
            } else {
                "missing-value-xyz"
            };
            w.explorer.focus_search(Some(query));
            w.explorer.search().search_now(None);
            poll(|| !w.explorer.search().is_searching());
        }
        "explorer-notes" => {
            w.explorer.show_notes_section(true);
            poll(|| !w.explorer.notes_busy());
        }
        "editor-python" => open("main.py"),
        "editor-json" => open("data.json"),
        "editor-rust" => open("src/main.rs"),
        "editor-toml" => open("settings.toml"),
        "editor-markdown" => open("README.md"),
        "editor-selection-find" => {
            open("main.py");
            let tab = w.editor.current_tab().unwrap();
            tab.buffer.select_range(
                &tab.buffer.iter_at_offset(10),
                &tab.buffer.iter_at_offset(21),
            );
            w.editor.focus_find();
            w.editor.search_entry().set_text("greet");
        }
        "editor-unsaved" => {
            let tab = w.editor.current_tab().unwrap();
            tab.buffer.insert(&mut tab.buffer.end_iter(), "\n");
        }
        "tab-hover" | "tab-close" => {
            let tab = w.editor.tabs()[0].clone();
            tab.header.inner.style_context().add_class("hover");
            if scene == "tab-close" {
                tab.header
                    .close
                    .set_state_flags(gtk::StateFlags::PRELIGHT, false);
            }
        }
        "terminal-right" => {
            w.toggle_terminal_bottom();
            w.toggle_terminal_right();
        }
        "terminal-split" => {
            w.terminal_bottom.new_terminal();
        }
        "terminal-fullscreen" => w.toggle_terminal_fullscreen(),
        "terminal-maximized" => {
            w.terminal_bottom.new_terminal();
            w.terminal_bottom.toggle_maximize();
        }
        "terminal-hover" => {
            w.terminal_bottom
                .add_button()
                .widget()
                .set_state_flags(gtk::StateFlags::PRELIGHT, false);
        }
        "palette" | "palette-query" => {
            w.open_command_palette();
            if scene == "palette-query" {
                w.command_palette().unwrap().set_query("terminal");
            }
        }
        "switcher" => {
            w.open_quick_switcher();
            poll(|| w.quick_switcher().unwrap().index_ready());
        }
        "notice" => w.notify("Parity notice", "info"),
        name if name.starts_with("settings-") => w.open_settings(Some(&name[9..])),
        name if name.starts_with("git-") || name.starts_with("commit-") => {
            let orientation = name.split('-').nth(1).unwrap();
            w.set_setting("gitgraph_orientation", orientation.into(), false)
                .unwrap();
            w.open_gitgraph();
            poll(|| w.gitgraph_views().iter().all(|v| v.is_ready()));
            if name.starts_with("commit-") {
                w.open_commit("HEAD");
                poll(|| w.pending_commits() == 0 && !w.commit_views().is_empty());
                poll(|| w.commit_views().iter().all(|v| v.is_ready()));
            }
        }
        "graph" => {
            w.open_graph();
        }
        _ => panic!("unknown parity scene {scene}"),
    }
    poll(|| !w.editor.is_busy() && w.editor.tabs().iter().all(|t| !t.highlighter.pending()));
    for panel in [&w.terminal_bottom, &w.terminal_right] {
        panel.set_cursor_blink(false);
        poll(|| {
            panel
                .panes()
                .iter()
                .all(|p| p.terminal.is_none() || p.pid().is_some())
        });
        poll(|| {
            panel
                .panes()
                .iter()
                .all(|p| p.terminal.is_none() || terminal_text(p).contains("$ "))
        });
    }
    if !matches!(scene, "palette" | "palette-query" | "switcher") && !scene.starts_with("settings-")
    {
        GtkWindowExt::set_focus(&w.window, None::<&gtk::Widget>);
    }
    frame(&w.window);
    let target: gtk::Widget = if scene.starts_with("settings-") {
        std::ops::Deref::deref(&w.settings_dialog().unwrap())
            .clone()
            .upcast()
    } else if scene == "explorer-rename" {
        w.explorer.open_dialog().unwrap().upcast()
    } else {
        w.window.clone().upcast()
    };
    // Stable captures after completed frames; bounded loop allows CSS transitions
    // and VTE's own paint batching without unbounded sleeps.
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut previous = Vec::new();
    let mut stable = 0;
    while stable < 4 {
        assert!(Instant::now() < deadline, "scene did not settle");
        frame(&w.window);
        let pixels = capture(&target, &output);
        stable = if pixels == previous { stable + 1 } else { 0 };
        previous = pixels;
    }
    println!("settled {scene}");
    w.shutdown();
    unsafe {
        w.window.destroy();
    }
}
