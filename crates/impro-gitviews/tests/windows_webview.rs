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

// Time budgets. A phase budget is how long one step may take; it is deliberately shorter
// than the host's own 60 s creation timer and the overall budget, so a failure says which
// step stalled: a slow first browser launch shows `environment`/`controller` far into the
// timeline, a logic error shows a notice with its code well before any timeout.
const PHASE_SECONDS: u64 = 30;
const READY_SECONDS: u64 = 45;
const OVERALL_SECONDS: u64 = 240;

type Dump = Box<dyn Fn() -> String>;
thread_local! {
    static START: Instant = Instant::now();
    static HOSTS: RefCell<Vec<(String, Dump)>> = const { RefCell::new(Vec::new()) };
}
/// Remember a page host so every failure prints its timeline.
fn track(name: &str, dump: impl Fn() -> String + 'static) {
    HOSTS.with(|hosts| hosts.borrow_mut().push((name.to_owned(), Box::new(dump))));
}
/// Release the retained host handles so dropping the views tears them down as usual.
fn untrack_all() {
    HOSTS.with(|hosts| hosts.borrow_mut().clear());
}
fn timelines() -> String {
    HOSTS.with(|hosts| {
        hosts
            .borrow()
            .iter()
            .map(|(name, dump)| format!("--- {name} host diagnostics ---\n{}", dump()))
            .collect::<Vec<_>>()
            .join("\n")
    })
}
fn die(headline: &str) -> ! {
    panic!(
        "{headline}\nelapsed in test: {:?}\n{}",
        START.with(|s| s.elapsed()),
        timelines()
    );
}
/// The notice text and the host's diagnostics, or nothing while the page is available.
fn assert_no_notice(name: &str, notice: Option<String>) {
    if let Some(notice) = notice {
        die(&format!(
            "{name}: real-engine test must not silently pass via the notice\nnotice: {notice}"
        ));
    }
}

fn pump() {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_millis(10);
    while context.pending() && Instant::now() < deadline {
        context.iteration(false);
    }
    std::thread::sleep(Duration::from_millis(5));
}
fn wait(label: &str, condition: impl FnMut() -> bool) {
    wait_for(label, PHASE_SECONDS, condition);
}
fn wait_for(label: &str, seconds: u64, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while !condition() {
        if START.with(|s| s.elapsed()) > Duration::from_secs(OVERALL_SECONDS) {
            die(&format!(
                "OVERALL TIMEOUT ({OVERALL_SECONDS}s) while waiting for: {label}"
            ));
        }
        if Instant::now() >= deadline {
            die(&format!(
                "PHASE TIMEOUT ({seconds}s) waiting for: {label}\n\
                 The `phases:` line below shows the last step the host completed; a late \
                 `environment`/`controller` means a slow runner, a stall at \
                 `navigation_started`/`navigation_completed` means the page never loaded."
            ));
        }
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
    wait_for(
        "graph ready handshake (check runtime/loader/fallback reason)",
        READY_SECONDS,
        || {
            assert_no_notice("graph", graph.webview().fallback_reason());
            graph.is_ready()
        },
    );
}

// One test keeps all GTK/COM work on the same thread and serialises environment changes.
#[test]
fn graph_and_commit_use_the_real_windows_engine() {
    START.with(|_| ());
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
    let host = graph.webview().clone();
    track("graph", move || host.diagnostics());
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
    eprintln!(
        "graph host {}",
        graph.webview().diagnostics().lines().next().unwrap_or("")
    );
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
    let host = commit.webview().clone();
    track("commit", move || host.diagnostics());
    layout.pack_start(commit.widget(), true, true, 0);
    commit.widget().show_all();
    let out = errors.clone();
    commit.connect_error(move |error| out.borrow_mut().push(error.to_owned()));
    wait_for("commit ready handshake", READY_SECONDS, || {
        assert_no_notice("commit", commit.webview().fallback_reason());
        commit.is_ready()
    });
    eprintln!(
        "commit host {}",
        commit.webview().diagnostics().lines().next().unwrap_or("")
    );
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
    // The document's own location (about:blank origin or the data: URL, depending on the
    // runtime) must be exactly what it was before the attempts.
    let location = graph_js(&graph, "location.href");
    graph_js(
        &graph,
        "chrome.webview.postMessage('invalid');window.open('https://example.invalid');true",
    );
    graph_js(&graph, "location.href='https://example.invalid';true");
    graph_js(&graph, "location.href='about:blank';true");
    for _ in 0..10 {
        pump();
    }
    assert_eq!(graph_js(&graph, "location.href"), location);
    assert_eq!(graph_js(&graph, "typeof ImproGit.stats"), "function");
    let timeline = graph.webview().diagnostics();
    assert!(
        timeline.contains("CANCELLED"),
        "the host must have cancelled the script navigations:\n{timeline}"
    );
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
    untrack_all();
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
