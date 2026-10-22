//! Paired full-window scenes; protocol documented in scripts/parity/README.md.
use crate::common::*;
use crate::js;
use gtk::prelude::*;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn capture(widget: &impl IsA<gtk::Widget>, path: &std::path::Path) -> Vec<u8> {
    if std::env::args().nth(2).as_deref() == Some("explorer-menu") {
        let widget = widget.as_ref();
        let (_, x, y) = widget.window().unwrap().origin();
        let root = gdk::Screen::default().unwrap().root_window().unwrap();
        root.pixbuf(x, y, widget.allocated_width(), widget.allocated_height())
            .unwrap()
            .savev(path, "png", &[])
            .unwrap();
    } else {
        impro::app::capture_window(widget, path).unwrap();
    }
    std::fs::read(path).unwrap()
}

pub fn run() {
    fn labels(widget: &gtk::Widget, output: &mut Vec<String>) {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            let text = label.text().to_string();
            if !text.is_empty() {
                output.push(text);
            }
        }
        if let Some(container) = widget.downcast_ref::<gtk::Container>() {
            container.forall(|child| labels(child, output));
        }
    }
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
    w.window.set_default_size(1190, 750);
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
                let tree = w.explorer.tree();
                let path = tree
                    .model()
                    .unwrap()
                    .path(&w.explorer.find_row(&root.join("main.py")).unwrap())
                    .unwrap();
                let area = tree.background_area(Some(&path), None::<&gtk::TreeViewColumn>);
                let (_, x, y) = tree.bin_window().unwrap().origin();
                tree.display()
                    .default_seat()
                    .unwrap()
                    .pointer()
                    .unwrap()
                    .warp(
                        &gdk::Screen::default().unwrap(),
                        x + 100,
                        y + area.y() + area.height() / 2,
                    );
                frame(&w.window);
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
            poll(|| {
                w.gitgraph_views()
                    .iter()
                    .all(|v| v.is_ready() && v.last_data().is_some())
            });
            for view in w.gitgraph_views() {
                poll(|| view.timings().data_to_paint.is_some());
                poll(|| js(view.webview(), "ImproGit.stats().commits") == 2);
            }
            if name.starts_with("commit-") {
                w.open_commit(&std::env::var("IMPRO_PARITY_HEAD").unwrap());
                poll(|| w.pending_commits() == 0 && !w.commit_views().is_empty());
                poll(|| {
                    w.commit_views().iter().all(|v| {
                        v.is_ready()
                            && v.last_data().is_some()
                            && v.timings().data_to_paint.is_some()
                    })
                });
            }
        }
        "graph" => {
            let graph = w.open_graph().unwrap();
            poll(|| graph.summary().files_indexed > 0 && graph.widget().allocated_width() > 1 && graph.panel().root.allocated_width() > 1);
            let summary = graph.summary();
            let ui = impro_ui::current_theme().unwrap().ui.clone();
            let palette = impro_graph::view::palette::Palette::from_theme(
                &ui,
                &ui["accent"],
                impro_core::settings::accent_presets(),
            );
            let mut controls = Vec::new();
            labels(graph.panel().root.upcast_ref(), &mut controls);
            let structure = serde_json::json!({
                "control_labels": controls,
                "control_panel_width": graph.panel().root.allocated_width(),
                "nodes": summary.nodes, "edges": summary.lines,
                "defaults": graph.prefs().to_value(),
                "panel_open": graph.panel_open(),
                "legend": {"fill":palette.fill.to_hex(),"tag":palette.tag.to_hex(),
                    "attachment":palette.attachment.to_hex(),"unresolved":palette.unresolved.to_hex(),
                    "code":palette.code.to_hex()},
                "panel_size": [graph.widget().allocated_width(), graph.widget().allocated_height()],
            });
            std::fs::write(
                output.with_extension("json"),
                serde_json::to_vec_pretty(&structure).unwrap(),
            )
            .unwrap();
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
    if scene == "graph" {
        capture(&target, &output);
    }
    while scene != "graph" && stable < 4 {
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
