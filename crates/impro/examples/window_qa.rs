//! Process-isolated Git integration and full-window performance/visual harness.
#[cfg(unix)]
#[path = "../tests/common/mod.rs"]
mod common;
#[cfg(unix)]
#[path = "parity/scenes.rs"]
mod scenes;
#[cfg(unix)]
use common::*;
#[cfg(unix)]
use gtk::prelude::*;
#[cfg(unix)]
use javascriptcore::ValueExt;
#[cfg(unix)]
use serde_json::{json, Value};
#[cfg(unix)]
use std::{
    cell::RefCell,
    path::Path,
    process::Command,
    rc::Rc,
    time::{Duration, Instant},
};
#[cfg(unix)]
use webkit2gtk::WebViewExt;

#[cfg(unix)]
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
#[cfg(unix)]
fn js(web: &webkit2gtk::WebView, script: &str) -> Value {
    let output = Rc::new(RefCell::new(None));
    let result = output.clone();
    web.evaluate_javascript(script, None, None, gio::Cancellable::NONE, move |value| {
        *result.borrow_mut() = Some(value.map(|v| {
            v.to_json(0)
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null)
        }));
    });
    poll(|| output.borrow().is_some());
    let value = output.borrow_mut().take().unwrap().unwrap();
    value
}
#[cfg(unix)]
fn main() {
    gtk::init().expect("X11 display");
    let mode = std::env::args().nth(1).unwrap_or_else(|| "git".into());
    if mode == "scene" {
        scenes::run();
        return;
    }
    if mode == "controls" {
        controls();
        return;
    }
    if mode == "capture" {
        capture();
        return;
    }
    if mode == "performance" {
        performance();
        return;
    }
    let mut values = impro_core::settings::defaults().clone();
    values.insert("terminal_autostart".into(), false.into());
    let h = Harness::with(values);
    git(h.dir.path(), &["init", "-q"]);
    git(h.dir.path(), &["config", "user.name", "Window QA"]);
    git(h.dir.path(), &["config", "user.email", "qa@example.test"]);
    h.write("a.py", "VALUE = 1\n");
    h.write("README.md", "# Project\n");
    git(h.dir.path(), &["add", "a.py", "README.md"]);
    git(h.dir.path(), &["commit", "-qm", "Initial files"]);
    let parent = git(h.dir.path(), &["rev-parse", "HEAD"]);
    h.write("a.py", "VALUE = 2\n");
    git(h.dir.path(), &["add", "a.py"]);
    git(h.dir.path(), &["commit", "-qm", "Prepare the release"]);
    let head = git(h.dir.path(), &["rev-parse", "HEAD"]);
    match mode.as_str() {
        "chooser" => {
            for panel in [&h.w.terminal_bottom, &h.w.terminal_right] {
                panel.set_no_show_all(false);
                panel.show_all();
                for kind in ["gitgraph", "graph", "terminal"] {
                    panel.add_button().choose(kind);
                }
                assert_eq!(panel.pane_count(), 3);
                assert_eq!(panel.terminal_count(), 1);
                panel.close_kind("terminal");
                panel.close_kind("graph");
                assert_eq!(panel.pane_count(), 1);
                assert!(panel.get_visible());
                panel.toggle_maximize();
                panel.restore();
                h.w.toggle_dock(panel);
                h.w.toggle_dock(panel);
                assert_eq!(panel.pane_count(), 1);
                assert_eq!(panel.terminal_count(), 0);
            }
            assert_eq!(h.w.gitgraph_views().len(), 2);
        }
        "accelerators" => {
            let tab = h.open("accelerators.py", "pass\n");
            tab.buffer.set_text("changed\n");
            h.accelerate("<Control>s");
            poll(|| !h.w.editor.has_unsaved());
            tab.buffer.set_text("save all\n");
            h.accelerate("<Control><Shift>s");
            poll(|| !h.w.editor.has_unsaved());
            for accel in [
                "<Control>grave",
                "<Control>grave",
                "<Control><Shift>5",
                "<Control><Shift>F11",
                "<Control><Shift>F11",
                "<Control>b",
                "<Control>b",
                "<Control><Shift>f",
                "<Control><Shift>p",
            ] {
                h.accelerate(accel);
            }
            h.w.overlay.hide_prompt();
            for accel in [
                "<Control><Alt>g",
                "<Control><Shift><Alt>g",
                "<Control><Alt>h",
                "<Control><Alt>grave",
                "<Control>comma",
                "<Control><Alt>t",
            ] {
                h.accelerate(accel);
            }
            assert!(h.w.settings_dialog().is_some());
            assert!(!h.w.graph_views().is_empty());
            assert!(!h.w.gitgraph_views().is_empty());
            tab.buffer.set_text("quit cancelled\n");
            h.accelerate("<Control>q");
            assert!(h.w.window.get_visible());
        }
        "capacity" => {
            for panel in [&h.w.terminal_bottom, &h.w.terminal_right] {
                panel.set_max_panes(1);
                h.w.add_dock_pane(panel, "terminal");
            }
            assert!(h.w.open_gitgraph().is_none());
            assert!(h.w.open_gitgraph().is_none());
            assert_eq!(h.w.overlay.notices().len(), 1);
            h.w.terminal_right.close_current();
            let graph = h.w.open_gitgraph().unwrap();
            assert!(h.w.terminal_right.has_gitgraph());
            assert!(!h.w.terminal_bottom.has_gitgraph());
            poll(|| graph.is_ready());
        }
        "hash" => {
            h.w.open_commit(&head[..8]);
            h.w.open_commit(&head[..8]);
            assert_eq!(h.w.pending_commits(), 1);
            poll(|| h.w.commit_views().len() == 1);
            assert_eq!(h.w.commit_views()[0].rev(), head);
            h.w.close_commit_tabs();
            for hash in ["HEAD", "--all", "abcd~1", "deadbeef;echo hi", "deadbeef\n"] {
                h.w.open_commit(hash);
            }
            assert_eq!(h.w.pending_commits(), 0);
            assert!(h.w.commit_views().is_empty());
            let other = tempfile::tempdir().unwrap();
            h.w.open_commit(&head[..9]);
            h.w.set_root(other.path());
            poll(|| h.w.root() == other.path());
            poll(|| h.w.pending_commits() == 0);
            assert!(h.w.commit_views().is_empty());
        }
        "errors" => {
            let other = tempfile::tempdir().unwrap();
            h.w.set_root(other.path());
            poll(|| h.w.root() == other.path());
            let graph = h.w.open_gitgraph().unwrap();
            poll(|| graph.is_ready() && graph.last_data().is_some());
            poll(|| {
                js(graph.webview(), "window.ImproGit && ImproGit.stats().state") == "not-a-repo"
            });
            for error in ["first", "first", "second"] {
                h.w.view_error("Git graph", error);
            }
            assert_eq!(h.w.overlay.notices().len(), 1);
        }
        _ => {
            h.accelerate("<Control><Alt>h");
            let graph = h.w.gitgraph_views()[0].clone();
            poll(|| graph.is_ready() && graph.last_data().is_some());
            assert_eq!(
                graph.last_data().unwrap()["commits"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert!(h.w.terminal_bottom.has_gitgraph());
            assert!(!h.w.terminal_right.has_gitgraph());
            h.w.open_gitgraph();
            assert_eq!(h.w.gitgraph_views().len(), 1);
            let encoded = serde_json::to_string(&head).unwrap();
            poll(|| {
                js(
                    graph.webview(),
                    &format!("ImproGit._debug.screenPos({encoded})"),
                )
                .is_array()
            });
            js(graph.webview(), &format!("(() => {{ const c=document.querySelector('canvas'),r=c.getBoundingClientRect(),p=ImproGit._debug.screenPos({encoded}); for (const type of ['pointerdown','pointerup']) c.dispatchEvent(new PointerEvent(type,{{clientX:r.left+p[0],clientY:r.top+p[1],pointerId:1,button:0,buttons:type==='pointerdown'?1:0,bubbles:true}})); return true; }})()"));
            poll(|| h.w.commit_views().len() == 1);
            let commit = h.w.commit_views()[0].clone();
            poll(|| commit.is_ready() && commit.last_data().is_some());
            assert_eq!(h.w.selected_commit().as_deref(), Some(head.as_str()));
            if mode == "settings" {
                h.w.open_settings(None);
                for (key, value) in [
                    ("gitgraph_orientation", json!("vertical")),
                    ("gitgraph_max_commits", json!(100)),
                    ("gitgraph_show_remotes", json!(false)),
                    ("gitgraph_show_tags", json!(false)),
                    ("gitgraph_time_format", json!("absolute")),
                ] {
                    h.w.set_setting(key, value, true).unwrap();
                }
                assert_eq!(graph.orientation(), "vertical");
                assert_eq!(graph.settings().max_commits, 100);
                assert!(!graph.settings().show_tags);
                h.w.terminal_right.set_no_show_all(false);
                h.w.terminal_right.show_all();
                h.w.add_dock_pane(&h.w.terminal_right, "gitgraph");
                let other = h.w.gitgraph_views()[1].clone();
                assert_eq!(other.settings(), graph.settings());
                poll(|| {
                    js(
                        graph.webview(),
                        "window.ImproGit && ImproGit.stats().orientation",
                    ) == "vertical"
                });
                js(
                    graph.webview(),
                    "document.getElementById('b-horizontal').click(); true",
                );
                poll(|| other.orientation() == "horizontal");
                assert_eq!(
                    h.w.settings_dialog().unwrap().values()["gitgraph_orientation"],
                    "horizontal"
                );
            } else if mode == "feature" {
                h.w.set_setting("feature_gitgraph", false.into(), false)
                    .unwrap();
                assert!(graph.is_closed());
                assert!(!commit.is_closed());
                assert_eq!(h.w.commit_views().len(), 1);
                h.w.set_setting("feature_gitgraph", true.into(), false)
                    .unwrap();
                assert_eq!(h.w.gitgraph_views().len(), 1);
                assert!(!commit.is_closed());
            } else if mode == "theme" {
                h.w.set_theme("impro-light", false);
                h.w.set_setting("accent", "#ff0088".into(), false).unwrap();
                h.w.set_setting("editor_font", "DejaVu Sans Mono".into(), false)
                    .unwrap();
                h.w.set_setting("ui_font", "DejaVu Sans".into(), false)
                    .unwrap();
                h.w.set_setting("editor_font_size", 21.into(), false)
                    .unwrap();
                poll(|| {
                    js(commit.webview(), "getComputedStyle(document.documentElement).getPropertyValue('--size').trim()") == "21px"
                });
                h.w.set_setting("reduce_motion", false.into(), false)
                    .unwrap();
                poll(|| js(graph.webview(), "ImproGit.stats().reduceMotion") == false);
                h.w.set_setting("reduce_motion", true.into(), false)
                    .unwrap();
                poll(|| js(graph.webview(), "ImproGit.stats().reduceMotion") == true);
            } else if mode == "refresh" {
                h.write("fresh.py", "VALUE = 42\n");
                git(h.dir.path(), &["add", "fresh.py"]);
                git(h.dir.path(), &["commit", "-qm", "Fresh commit"]);
                poll(|| {
                    graph
                        .last_data()
                        .is_some_and(|data| data["commits"].as_array().unwrap().len() == 3)
                });
                assert!(!commit.is_closed());
                let other = tempfile::tempdir().unwrap();
                h.w.set_root(other.path());
                poll(|| h.w.root() == other.path());
                assert!(commit.is_closed());
                assert_eq!(graph.root(), other.path());
            } else if mode == "links" {
                poll(|| js(commit.webview(), "!!document.querySelector('.file-name')") == true);
                js(
                    commit.webview(),
                    "document.querySelector('.file-name').click(); true",
                );
                wait_path(&h.w, &h.dir.path().join("a.py"));
                assert_eq!(h.w.editor.current_tab().unwrap().cursor(), 0);
                h.w.open_commit_file(&h.dir.path().join("removed.py"), 1);
                poll(|| {
                    h.w.overlay
                        .notices()
                        .iter()
                        .any(|n| n.text() == "File not in the working tree")
                });
            } else {
                poll(|| js(graph.webview(), "ImproGit.stats().selected") == head);
                h.w.terminal_right.set_no_show_all(false);
                h.w.terminal_right.show_all();
                h.w.add_dock_pane(&h.w.terminal_right, "gitgraph");
                let other = h.w.gitgraph_views()[1].clone();
                poll(|| other.is_ready());
                poll(|| js(other.webview(), "ImproGit.stats().selected") == head);
                let key = format!("commit:{head}");
                let tab =
                    h.w.editor
                        .view_tabs()
                        .into_iter()
                        .find(|t| t.key == key)
                        .unwrap();
                assert_eq!(tab.header.label.text(), &head[..7]);
                assert_eq!(
                    h.w.editor.notebook().page_num(&tab.page),
                    Some(h.w.editor.notebook().n_pages() - 1)
                );
                js(
                    commit.webview(),
                    "document.querySelector('.parent').click(); true",
                );
                poll(|| h.w.selected_commit().as_deref() == Some(parent.as_str()));
                assert_eq!(h.w.commit_views().len(), 2);
                h.w.open_commit(&head[..7]);
                poll(|| h.w.pending_commits() == 0);
                assert_eq!(h.w.commit_views().len(), 2);
                let file = h.dir.path().join("a.py");
                h.w.editor.open_file(&file);
                wait_path(&h.w, &file);
                poll(|| js(graph.webview(), "ImproGit.stats().selected") == Value::Null);
                h.w.terminal_bottom.close_kind("gitgraph");
                assert!(graph.is_closed());
                assert!(!commit.is_closed());
                h.w.editor.close_view_tab(&key);
                assert!(commit.is_closed());
            }
        }
    }
    h.w.close_commit_tabs();
    h.w.shutdown();
    assert!(h.w.graph_views().is_empty());
    println!("scenario-ok {mode}");
}
#[cfg(unix)]
fn capture() {
    let root = std::env::args_os()
        .nth(2)
        .map(std::path::PathBuf::from)
        .expect("fixture root");
    let output = std::env::args_os()
        .nth(3)
        .map(std::path::PathBuf::from)
        .expect("PNG path");
    let theme = std::env::args()
        .nth(4)
        .unwrap_or_else(|| "impro-dark".into());
    let mut values = impro_core::settings::defaults().clone();
    values.insert("theme".into(), theme.into());
    values.insert("reduce_motion".into(), true.into());
    let app = gtk::Application::new(Some("dev.impro.Capture"), gio::ApplicationFlags::NON_UNIQUE);
    app.register(None::<&gio::Cancellable>).unwrap();
    let w = impro::MainWindow::with_values(&app, &root, values);
    w.window.set_default_size(1190, 750);
    w.show();
    for name in ["main.py", "README.md", "data.json"] {
        w.editor.open_file(root.join(name));
        poll(|| !w.editor.is_busy());
    }
    w.terminal_bottom.set_cursor_blink(false);
    poll(|| !w.explorer.is_busy() && w.terminal_bottom.panes().iter().all(|p| p.pid().is_some()));
    poll(|| {
        w.layout_ready()
            && w.terminal_bottom
                .panes()
                .iter()
                .all(|p| terminal_text(p).contains("$ "))
    });
    poll(|| w.editor.tabs().iter().all(|t| !t.highlighter.pending()));
    if let Some(state) = std::env::args().nth(5) {
        let flags = match state.as_str() {
            "hover" => gtk::StateFlags::PRELIGHT,
            "pressed" => gtk::StateFlags::ACTIVE | gtk::StateFlags::PRELIGHT,
            "insensitive" => gtk::StateFlags::INSENSITIVE,
            _ => panic!("unknown capture icon state: {state}"),
        };
        fn set_state(widget: &gtk::Widget, flags: gtk::StateFlags) {
            if widget.is::<gtk::Button>()
                || widget.is::<gtk::SearchEntry>()
                || widget.is::<gtk::TreeView>()
            {
                widget.set_sensitive(flags != gtk::StateFlags::INSENSITIVE);
                widget.set_state_flags(flags, false);
            }
            if let Some(container) = widget.downcast_ref::<gtk::Container>() {
                container.forall(|child| set_state(child, flags));
            }
        }
        set_state(w.window.upcast_ref(), flags);
        frame(&w.window);
        let settled = Instant::now();
        poll(|| settled.elapsed() >= Duration::from_millis(1500));
    }
    frame(&w.window);
    impro::app::capture_window(&w.window, &output).unwrap();
    unsafe {
        w.window.destroy();
    }
}
#[cfg(unix)]
fn cpu_ticks() -> u64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
    let fields: Vec<_> = stat
        .rsplit_once(')')
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
}
#[cfg(unix)]
fn performance() {
    let h = Harness::new();
    let tab = h.open("first.rs", "fn main() {}\n");
    h.open("second.rs", "fn second() {}\n");
    poll(|| h.w.editor.tabs().iter().all(|t| !t.highlighter.pending()));
    h.shell(false);
    frame(&h.w.window);
    let fresh = h.write("third.rs", "fn third() {}\n");
    h.w.explorer.refresh();
    poll(|| !h.w.explorer.is_busy());
    h.w.explorer.set_current_file(Some(&fresh));
    poll(|| h.w.explorer.selected_path().as_ref() == Some(&fresh));
    h.w.explorer.focus_tree();
    let tree = h.w.explorer.tree();
    let (model, iter) = tree.selection().selected().unwrap();
    tree.set_cursor(
        &model.path(&iter).unwrap(),
        None::<&gtk::TreeViewColumn>,
        false,
    );
    frame(&h.w.window);
    let press = |accelerator: &str| {
        let key = event(&h.w.window, accelerator);
        let event: &gdk::Event = &key;
        h.w.window.emit_by_name::<bool>("key-press-event", &[event]);
    };
    let started = Instant::now();
    eprintln!("performance: open key");
    press("Return");
    wait_path(&h.w, &fresh);
    frame(&h.w.window);
    h.w.window.display().sync();
    let open_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    eprintln!("performance: switch key");
    press("<Control>Tab");
    wait_path(&h.w, &tab.path());
    frame(&h.w.window);
    h.w.window.display().sync();
    let switch_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    eprintln!("performance: dock key");
    press("<Control>grave");
    assert!(!h.w.terminal_bottom.get_visible());
    frame(&h.w.window);
    h.w.window.display().sync();
    let toggle_ms = started.elapsed().as_secs_f64() * 1000.0;
    h.w.toggle_terminal_bottom();
    frame(&h.w.window);
    if std::env::var_os("IMPRO_QA_QUIET_FOCUS").is_some() {
        GtkWindowExt::set_focus(&h.w.window, None::<&gtk::Widget>);
        frame(&h.w.window);
    }
    // Event-driven deadline; this one-shot belongs only to the measurement harness.
    let done = Rc::new(std::cell::Cell::new(false));
    let finished = done.clone();
    poll(|| {
        !h.w.explorer.git_running()
            && !h.w.explorer.git_timeout_pending()
            && h.w.editor.tabs().iter().all(|t| !t.highlighter.pending())
            && h.w
                .terminal_bottom
                .panes()
                .iter()
                .all(|p| p.gate.as_ref().is_none_or(|g| !g.has_pending_sources()))
    });
    let graph_threads = std::fs::read_dir("/proc/self/task")
        .unwrap()
        .flatten()
        .filter(|entry| {
            std::fs::read_to_string(entry.path().join("comm"))
                .is_ok_and(|name| name.starts_with("impro-graph"))
        })
        .count();
    let web_processes = descendants(std::process::id())
        .into_iter()
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .is_ok_and(|name| name.starts_with("WebKit"))
        })
        .count();
    assert_eq!(graph_threads, 0);
    assert_eq!(web_processes, 0);
    glib::timeout_add_local_once(Duration::from_secs(10), move || finished.set(true));
    let switches = || {
        std::fs::read_to_string(format!("/proc/self/task/{}/status", std::process::id()))
            .unwrap()
            .lines()
            .filter(|line| line.contains("ctxt_switches:"))
            .map(|line| {
                line.split_whitespace()
                    .last()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
            })
            .sum::<u64>()
    };
    let before_switches = switches();
    let before = cpu_ticks();
    let started = Instant::now();
    while !done.get() {
        glib::MainContext::default().iteration(true);
    }
    let idle_ticks = cpu_ticks() - before;
    println!(
        "{}",
        json!({"open_to_paint_ms":open_ms,"switch_to_paint_ms":switch_ms,"toggle_to_paint_ms":toggle_ms,"input":"synthetic GTK key event through ApplicationWindow", "max_allocation_us":h.w.max_allocation_us(),"idle_ticks":idle_ticks,"idle_main_context_switches":switches()-before_switches,"idle_seconds":started.elapsed().as_secs_f64(),"idle_focus":h.w.window.focused_widget().map(|widget| widget.type_().name().to_owned()),"motion_active":impro_ui::motion::active_count(),"motion_clock":impro_ui::motion::clock_attached(),"web_processes_before_git":web_processes,"graph_threads_before_open":graph_threads})
    );
}

#[cfg(unix)]
fn controls() {
    let h = Harness::new();
    let root = std::env::var_os("IMPRO_QA_CONTROLS_DIR").map(std::path::PathBuf::from);
    fn images(widget: &gtk::Widget, output: &mut Vec<gtk::Image>) {
        if let Some(button) = widget.downcast_ref::<gtk::Button>() {
            if button.style_context().has_class("titlebutton") {
                if let Some(image) = button.child().and_then(|w| w.downcast::<gtk::Image>().ok()) {
                    output.push(image);
                }
            }
        } else if let Some(container) = widget.downcast_ref::<gtk::Container>() {
            container.forall(|child| images(child, output));
        }
    }
    for theme in ["impro-dark", "impro-light"] {
        h.w.set_theme(theme, false);
        for (maximized, name) in [(true, "maximized"), (false, "restored")] {
            if maximized {
                h.w.window.maximize();
            } else {
                h.w.window.unmaximize();
            }
            poll(|| h.w.window.is_maximized() == maximized);
            frame(&h.w.window);
            let mut native = Vec::new();
            images(h.w.titlebar.upcast_ref(), &mut native);
            assert_eq!(native.len(), 3);
            for image in native {
                assert_eq!(image.pixel_size(), 32);
            }
            if let Some(root) = &root {
                impro::app::capture_window(
                    &h.w.window,
                    &root.join(format!("controls-{theme}-{name}.png")),
                )
                .unwrap();
            }
        }
    }
    println!("scenario-ok controls");
}

#[cfg(unix)]
fn descendants(pid: u32) -> Vec<u32> {
    let children =
        std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")).unwrap_or_default();
    let mut result = Vec::new();
    for child in children
        .split_whitespace()
        .filter_map(|pid| pid.parse::<u32>().ok())
    {
        result.push(child);
        result.extend(descendants(child));
    }
    result
}

#[cfg(not(unix))]
fn main() {}
