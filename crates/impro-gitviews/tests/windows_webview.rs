//! Real-engine runner coverage. Run on Windows with the Evergreen runtime installed.
#![cfg(windows)]
use gtk::prelude::*;
use impro_gitviews::{CommitView, GitGraphView};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
mod support;

fn pump() {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_millis(10);
    while context.pending() && Instant::now() < deadline {
        context.iteration(false);
    }
    std::thread::sleep(Duration::from_millis(5));
}
fn wait(label: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(90);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "WebView2 timed out after 90s: {label}"
        );
        pump();
    }
}
fn graph_js(graph: &GitGraphView, script: &str) -> Value {
    let result = Rc::new(RefCell::new(None));
    let out = result.clone();
    graph
        .webview()
        .evaluate_script(script, move |value| *out.borrow_mut() = Some(value));
    wait("graph ExecuteScript completion", || {
        result.borrow().is_some()
    });
    let result = result.borrow_mut().take().unwrap();
    result.expect("graph ExecuteScript failed")
}
fn commit_js(commit: &CommitView, script: &str) -> Value {
    let result = Rc::new(RefCell::new(None));
    let out = result.clone();
    commit
        .webview()
        .evaluate_script(script, move |value| *out.borrow_mut() = Some(value));
    wait("commit ExecuteScript completion", || {
        result.borrow().is_some()
    });
    let result = result.borrow_mut().take().unwrap();
    result.expect("commit ExecuteScript failed")
}
fn graph_ready(graph: &GitGraphView) {
    wait(
        "graph ready handshake (check runtime/loader/fallback reason)",
        || {
            assert_eq!(
                graph.webview().fallback_reason(),
                None,
                "real-engine test must not silently pass via the notice"
            );
            graph.is_ready()
        },
    );
}

// One test keeps all GTK/COM work on the same thread and serialises environment changes.
#[test]
fn graph_and_commit_use_the_real_windows_engine() {
    gtk::init().expect("GTK3 must initialise on the Windows runner");
    std::env::remove_var("IMPRO_GIT_PAGES");
    impro_gitviews::init();
    let temp = support::Temp::new();
    let mut repo = support::GitRepo::new(&temp.0.join("repository"));
    let first = repo.commit(&[("sample.txt", Some(b"first\n"))], "first");
    let last = repo.commit(&[("sample.txt", Some(b"first\nsecond\n"))], "second");
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(900, 650);
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let before = gtk::Entry::new();
    layout.pack_start(&before, false, false, 0);
    let graph = GitGraphView::new(&repo.path);
    graph.set_orientation("vertical").unwrap(); // Queued before native realization.
    graph.set_reduce_motion(true);
    layout.pack_start(graph.widget(), true, true, 0);
    let after = gtk::Entry::new();
    layout.pack_start(&after, false, false, 0);
    window.add(&layout);
    let errors = Rc::new(RefCell::new(Vec::new()));
    let out = errors.clone();
    graph.connect_error(move |error| out.borrow_mut().push(error.to_owned()));
    let opened = Rc::new(RefCell::new(None));
    let out = opened.clone();
    graph.connect_open_commit(move |hash| *out.borrow_mut() = Some(hash.to_owned()));
    window.show_all();
    graph_ready(&graph);
    wait("graph worker data and first paint", || {
        graph.last_data().is_some() && graph.timings().open_to_first_paint.is_some()
    });
    assert_eq!(graph_js(&graph, "ImproGit.stats().commits"), 2);
    assert_eq!(graph_js(&graph, "ImproGit.stats().orientation"), "vertical");
    eprintln!("WebView2 graph ready; two repository rows painted; pre-ready settings delivered");
    let geometry = graph_js(&graph, "({width:innerWidth,ratio:devicePixelRatio})");
    let physical = graph.widget().allocated_width() * graph.widget().scale_factor();
    let rendered = geometry["width"].as_f64().unwrap() * geometry["ratio"].as_f64().unwrap();
    assert!((rendered - f64::from(physical)).abs() <= 2., "WebView2 must fill the GTK native child in physical pixels: {geometry}, GTK physical width={physical}");
    eprintln!("WebView2 DPI geometry: {geometry}; GTK physical width={physical}");

    // The existing standalone fixture must also render unchanged through WebView2.
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/gitgraph_deleted_branches.json")).unwrap();
    let count = fixture["commits"]
        .as_array()
        .expect("fixture commits")
        .len();
    graph_js(&graph, &format!("ImproGit.setData({fixture});true"));
    assert_eq!(graph_js(&graph, "ImproGit.stats().commits"), count);
    graph_js(
        &graph,
        &format!(
            "webkit.messageHandlers.impro.postMessage({});true",
            json!({"type":"open-commit","hash":first})
        ),
    );
    wait("graph click-to-open message roundtrip", || {
        opened.borrow().as_deref() == Some(&first)
    });

    graph.widget().hide();
    wait("graph pause after unmap", || {
        graph_js(&graph, "ImproGit.stats().paused") == true
    });
    let data = graph.last_data().unwrap();
    graph.refresh();
    for _ in 0..10 {
        pump();
    }
    assert!(
        std::sync::Arc::ptr_eq(&data, &graph.last_data().unwrap()),
        "hidden refresh must defer collection"
    );
    graph.widget().show();
    wait("graph resume after map", || {
        graph_js(&graph, "ImproGit.stats().paused") == false
    });
    eprintln!("WebView2 graph bridge roundtrip and hide/show pause-resume passed");

    let commit = CommitView::new(&repo.path, &last, None);
    layout.pack_start(commit.widget(), true, true, 0);
    commit.widget().show_all();
    let out = errors.clone();
    commit.connect_error(move |error| out.borrow_mut().push(error.to_owned()));
    wait("commit ready handshake", || {
        assert_eq!(commit.webview().fallback_reason(), None);
        commit.is_ready()
    });
    wait("commit worker data and first paint", || {
        commit.last_data().is_some() && commit.timings().open_to_first_paint.is_some()
    });
    assert_eq!(
        commit_js(&commit, "document.querySelector('.subject').textContent"),
        "second"
    );
    let opened_file = Rc::new(Cell::new(false));
    let out = opened_file.clone();
    let expected = repo.path.join("sample.txt").canonicalize().unwrap();
    commit.connect_open_file(move |path, line| {
        assert_eq!(path, expected);
        assert_eq!(line, 2);
        out.set(true);
    });
    commit_js(&commit, "webkit.messageHandlers.impro.postMessage({type:'open-file',path:'sample.txt',line:2});true");
    wait(
        "commit open-file message resolves the working-tree path",
        || opened_file.get(),
    );
    commit.set_font_size(16);
    commit.set_reduce_motion(true);
    eprintln!("WebView2 commit ready, rendered, and open-file roundtrip passed");

    // Invalid payloads, navigation attempts and new windows cannot replace the document.
    graph_js(
        &graph,
        "chrome.webview.postMessage('invalid');window.open('https://example.invalid');true",
    );
    graph_js(&graph, "location.href='https://example.invalid';true");
    for _ in 0..10 {
        pump();
    }
    assert_eq!(graph_js(&graph, "location.href"), "about:blank");
    assert!(
        errors.borrow().is_empty(),
        "unexpected Git page errors: {:?}",
        errors.borrow()
    );
    // Closing a different pane while its asynchronous creation is pending must
    // leave the live sibling usable and discard the late completed handlers.
    let closing = GitGraphView::new(&repo.path);
    layout.pack_start(closing.widget(), true, true, 0);
    closing.widget().show_all();
    closing.shutdown();
    closing.shutdown();
    unsafe {
        closing.widget().destroy();
    }
    drop(closing);
    for _ in 0..20 {
        pump();
    }
    assert!(graph.is_ready());
    assert_eq!(graph_js(&graph, "typeof ImproGit.stats"), "function");
    eprintln!("WebView2 close during creation left its sibling alive");
    commit.shutdown();
    commit.shutdown();
    graph.shutdown();
    graph.shutdown();
    assert!(graph.is_closed() && commit.is_closed());
    unsafe {
        window.destroy();
    }
    drop(commit);
    drop(graph);
    eprintln!("WebView2 navigation policy and idempotent shutdown passed");

    std::env::set_var("IMPRO_GIT_PAGES", "notice");
    let fallback = GitGraphView::new(&repo.path);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.add(fallback.widget());
    window.show_all();
    wait("forced notice fallback", || {
        fallback.webview().fallback_reason().is_some()
    });
    wait("fallback label is visible", || {
        fallback
            .widget()
            .children()
            .iter()
            .any(|w| w.is::<gtk::Label>())
    });
    assert!(!fallback.is_ready());
    assert!(fallback
        .webview()
        .fallback_reason()
        .unwrap()
        .contains("IMPRO_GIT_PAGES=notice"));
    fallback.shutdown();
    fallback.shutdown();
    unsafe {
        window.destroy();
    }
    std::env::remove_var("IMPRO_GIT_PAGES");
    eprintln!("WebView2 forced fallback passed");
}
