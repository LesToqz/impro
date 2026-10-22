use super::common::*;
use gtk::prelude::*;
use impro_gitviews::{CommitView, GitGraphView};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
use webkit2gtk::WebViewExt;
#[path = "../support/mod.rs"]
mod support;
use support::*;
fn window(widget: &gtk::Box) -> gtk::Window {
    let w = gtk::Window::new(gtk::WindowType::Toplevel);
    w.set_default_size(1000, 700);
    w.add(widget);
    w.show_all();
    w
}
fn send(web: &webkit2gtk::WebView, message: serde_json::Value) {
    js(
        web,
        &format!("webkit.messageHandlers.impro.postMessage({message});true"),
    );
    pump(20);
}
fn graph_ready(g: &GitGraphView) {
    poll(|| g.is_ready() && g.last_data().is_some());
}
fn commit_ready(c: &CommitView) {
    poll(|| c.is_ready() && c.last_data().is_some());
    poll(|| js(c.webview(), "!!document.querySelector('.subject')") == true);
}
struct WorkerHighlight {
    main: std::thread::ThreadId,
    calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}
impl impro_gitviews::diff::Highlighter for WorkerHighlight {
    fn language(&self, path: &str) -> Option<String> {
        path.ends_with(".py").then(|| "python".into())
    }
    fn classify(&self, _path: &str, _text: &str) -> Result<Vec<(usize, usize, String)>, String> {
        assert_ne!(self.main, std::thread::current().id());
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(vec![(0, 1, "variable".into())])
    }
}
pub fn tests() {
    let t = Temp::new();
    let mut repo = GitRepo::new(&t.0);
    let first = repo.commit(&[("a.py", Some(b"x = 1\n"))], "First");
    let sha = repo.commit(&[("a.py", Some(b"x = 2\n"))], "Second");
    let g = GitGraphView::new(&repo.path);
    let errors = Rc::new(RefCell::new(Vec::new()));
    let e = errors.clone();
    g.connect_error(move |s| e.borrow_mut().push(s.to_string()));
    let opened = Rc::new(RefCell::new(Vec::new()));
    let o = opened.clone();
    g.connect_open_commit(move |s| o.borrow_mut().push(s.to_owned()));
    let w = window(g.widget());
    run("test_handshake_loading_data_signals_and_json_queue", || {
        g.select(Some(&sha));
        graph_ready(&g);
        poll(|| js(g.webview(), "ImproGit.stats().commits") == 2);
        assert_eq!(g.last_data().unwrap()["head"]["hash"], sha);
        send(g.webview(), json!({"type":"open-commit","hash":sha}));
        assert_eq!(opened.borrow().as_slice(), std::slice::from_ref(&sha));
        assert!(errors.borrow().is_empty());
    });
    run("test_orientation_options_and_reload", || {
        assert!(g.set_orientation("bad").is_err());
        g.set_orientation("vertical").unwrap();
        g.set_options(false, false, "absolute").unwrap();
        assert!(g.set_options(true, true, "bad").is_err());
        assert_eq!(js(g.webview(), "ImproGit.stats().orientation"), "vertical");
        g.refresh();
        poll(|| g.last_data().is_some());
        assert_eq!(g.orientation(), "vertical");
    });
    run("test_commit_limit_clamps_and_refreshes_once", || {
        g.set_max_commits(-1);
        assert_eq!(g.settings().max_commits, 100);
        g.set_max_commits(50000);
        assert_eq!(g.settings().max_commits, 10000);
        g.set_max_commits(1500);
    });
    run("test_selection_focus_theme_and_page_reload", || {
        g.set_reduce_motion(true);
        g.select(Some(&sha));
        g.focus_head();
        assert_eq!(js(g.webview(), "ImproGit.stats().selected"), sha);
        let theme = impro_core::themes::get_theme(Some("impro-light"));
        g.set_theme(&theme.ui, &theme.ui["accent"]);
        assert_eq!(
            js(
                g.webview(),
                "getComputedStyle(document.body).backgroundColor"
            ),
            "rgb(244, 247, 245)"
        );
    });
    run("test_actual_page_reload_preserves_state", || {
        g.webview().reload();
        pump(100);
        poll(|| g.is_ready());
        assert_eq!(js(g.webview(), "ImproGit.stats().selected"), sha);
        assert_eq!(js(g.webview(), "ImproGit.stats().orientation"), "vertical");
    });
    run("test_monitor_focus_and_hidden_dirty", || {
        let added = repo.commit(&[("b.txt", Some(b"new\n"))], "Third");
        poll(|| g.last_data().is_some_and(|d| d["head"]["hash"] == added));
        g.widget().hide();
        pump(30);
        assert_eq!(js(g.webview(), "ImproGit.stats().paused"), true);
        let newer = repo.commit(&[("b.txt", Some(b"newer\n"))], "Fourth");
        g.refresh();
        pump(400);
        assert_ne!(g.last_data().unwrap()["head"]["hash"], newer);
        g.widget().show();
        poll(|| g.last_data().is_some_and(|d| d["head"]["hash"] == newer));
        assert_eq!(js(g.webview(), "ImproGit.stats().paused"), false);
    });
    run("test_coalesced_refresh_stale_data_and_digest", || {
        pump(400);
        js(g.webview(),"window.__updates=0;window.__setData=ImproGit.setData;ImproGit.setData=(d)=>{window.__updates++;return window.__setData(d)};true");
        for _ in 0..25 {
            g.refresh();
        }
        pump(500);
        assert_eq!(js(g.webview(), "window.__updates"), 0);
    });
    run("test_linked_worktree_watches_common_refs", || {
        let linked = Temp::new();
        repo.git(&[
            "worktree",
            "add",
            "-b",
            "linked",
            linked.0.to_str().unwrap(),
        ]);
        let graph = GitGraphView::new(&linked.0);
        let window = window(graph.widget());
        graph_ready(&graph);
        repo.git(&["branch", "linked-common", &sha]);
        poll(|| {
            graph.last_data().is_some_and(|d| {
                d["commits"].as_array().unwrap().iter().any(|c| {
                    c["refs"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r["name"] == "linked-common")
                })
            })
        });
        graph.shutdown();
        unsafe {
            window.destroy();
        }
    });
    run("test_error_and_empty_states", || {
        let empty = Temp::new();
        let r = GitRepo::new(&empty.0);
        let graph = GitGraphView::new(&r.path);
        let window = window(graph.widget());
        graph_ready(&graph);
        assert_eq!(graph.last_data().unwrap()["error"], "empty");
        assert_eq!(js(graph.webview(), "ImproGit.stats().state"), "empty");
        graph.shutdown();
        unsafe {
            window.destroy();
        }
    });
    let highlight_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let highlighter = std::sync::Arc::new(WorkerHighlight {
        main: std::thread::current().id(),
        calls: highlight_calls.clone(),
    });
    let c = CommitView::new(&repo.path, &sha, Some(highlighter));
    let cw = window(c.widget());
    commit_ready(&c);
    let parent = Rc::new(RefCell::new(Vec::new()));
    let pp = parent.clone();
    c.connect_open_commit(move |s| pp.borrow_mut().push(s.to_owned()));
    run("test_commit_detail_context_copy_and_navigation", || {
        assert_eq!(c.tab_key(), format!("commit:{sha}"));
        assert!(highlight_calls.load(std::sync::atomic::Ordering::Relaxed) > 0);
        assert_eq!(c.last_data().unwrap()["files"][0]["language"], "python");
        assert!(
            js(c.webview(), "document.querySelectorAll('.syntax').length")
                .as_u64()
                .unwrap()
                > 0
        );
        assert_eq!(c.last_data().unwrap()["subject"], "Second");
        send(c.webview(), json!({"type":"open-commit","hash":first}));
        assert_eq!(parent.borrow().as_slice(), std::slice::from_ref(&first));
        let files = Rc::new(RefCell::new(Vec::new()));
        let f = files.clone();
        c.connect_open_file(move |p, n| f.borrow_mut().push((p.to_owned(), n)));
        send(
            c.webview(),
            json!({"type":"open-file","path":"a.py","line":8}),
        );
        poll(|| !files.borrow().is_empty());
        assert_eq!(
            files.borrow()[0],
            (repo.path.join("a.py").canonicalize().unwrap(), 8)
        );
        for path in ["../outside", "/etc/passwd", "missing.py"] {
            send(c.webview(), json!({"type":"open-file","path":path}));
        }
        pump(100);
        assert_eq!(files.borrow().len(), 1);
        send(c.webview(), json!({"type":"copy","text":"quoted \" 🌍"}));
        assert_eq!(
            gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD)
                .wait_for_text()
                .as_deref(),
            Some("quoted \" 🌍")
        );
    });
    run("test_commit_font_size_uses_live_editor_size", || {
        let theme = impro_core::themes::get_theme(None);
        c.set_theme(theme, &theme.ui, &theme.ui["accent"], 13);
        c.set_font_size(19);
        assert_eq!(
            js(
                c.webview(),
                "getComputedStyle(document.documentElement).getPropertyValue('--size').trim()"
            ),
            "19px"
        );
    });
    run(
        "test_commit_lifecycle_hooks_are_optional_in_contract_c",
        || {
            js(
                c.webview(),
                "delete ImproCommit.pause;delete ImproCommit.resume;true",
            );
            c.widget().hide();
            pump(10);
            c.widget().show();
            pump(10);
            assert_eq!(
                js(
                    c.webview(),
                    "document.querySelector('.subject').textContent"
                ),
                "Second"
            );
        },
    );
    run(
        "test_context_results_from_before_reload_are_discarded",
        || {
            send(
                c.webview(),
                json!({"type":"expand","path":"a.py","start":1,"end":200}),
            );
            c.reload();
            commit_ready(&c);
            assert_eq!(c.last_data().unwrap()["hash"], sha);
        },
    );
    run(
        "test_commit_not_found_error_and_shared_context_survives_close",
        || {
            let sibling = CommitView::new(&repo.path, &first, None);
            let sw = window(sibling.widget());
            commit_ready(&sibling);
            c.shutdown();
            assert!(c.is_closed());
            assert_eq!(
                js(
                    sibling.webview(),
                    "document.querySelector('.subject').textContent"
                ),
                "First"
            );
            let missing = CommitView::new(&repo.path, &"0".repeat(40), None);
            let mw = window(missing.widget());
            poll(|| missing.last_data().is_some());
            assert_eq!(missing.last_data().unwrap()["ok"], false);
            poll(|| {
                js(
                    missing.webview(),
                    "document.body.textContent.includes('Commit not found')",
                ) == true
            });
            missing.shutdown();
            sibling.shutdown();
            unsafe {
                mw.destroy();
                sw.destroy();
            }
        },
    );
    run("test_set_root_clears_selection_monitors_and_data", || {
        let empty = Temp::new();
        std::fs::write(empty.0.join(".git"), b"invalid").unwrap();
        let close = Rc::new(std::cell::Cell::new(0));
        let cc = close.clone();
        g.connect_root_changed(move || cc.set(cc.get() + 1));
        g.set_root(&empty.0);
        assert_eq!(close.get(), 1);
        poll(|| g.last_data().is_some_and(|d| d["error"] == "not-a-repo"));
        assert_eq!(
            js(g.webview(), "ImproGit.stats().selected"),
            serde_json::Value::Null
        );
        assert_eq!(g.root(), empty.0);
    });
    run(
        "test_set_root_discards_collection_from_previous_repo",
        || {
            g.set_root(&repo.path);
            g.refresh();
            let empty = Temp::new();
            std::fs::write(empty.0.join(".git"), b"invalid").unwrap();
            g.set_root(&empty.0);
            poll(|| g.last_data().is_some_and(|d| d["error"] == "not-a-repo"));
            pump(100);
            assert_eq!(g.last_data().unwrap()["error"], "not-a-repo");
        },
    );
    run(
        "test_graph_shutdown_cancels_monitors_focus_and_debounce",
        || {
            g.refresh();
            g.shutdown();
            g.shutdown();
            assert!(g.is_closed());
            g.refresh();
            g.focus_head();
            pump(400);
        },
    );
    run(
        "test_destroy_joins_worker_and_late_callbacks_are_harmless",
        || {
            let graph = GitGraphView::new(&repo.path);
            let gw = window(graph.widget());
            graph.refresh();
            graph.shutdown();
            unsafe {
                gw.destroy();
            }
            pump(100);
            assert!(graph.is_closed());
        },
    );
    unsafe {
        cw.destroy();
        w.destroy();
    }
    pump(100);
}
