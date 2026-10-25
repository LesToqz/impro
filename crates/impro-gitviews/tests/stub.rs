//! Windows stub host, exercised on any platform: `cargo test -p impro-gitviews --features stub-backend`.
#![cfg(feature = "stub-backend")]
use gtk::prelude::*;
use impro_gitviews::stub::{CommitView, GitGraphView, GraphSettings, NOTICE};
use std::{cell::Cell, rc::Rc};

fn notice_of(widget: &gtk::Box) -> String {
    widget
        .children()
        .into_iter()
        .find_map(|c| c.downcast::<gtk::Label>().ok())
        .expect("notice label")
        .text()
        .to_string()
}

// One test only: GTK may be initialised on a single thread of the test process.
#[test]
fn test_stub_hosts() {
    gtk::init().unwrap();

    let graph = GitGraphView::new("/repo");
    assert_eq!(notice_of(graph.widget()), NOTICE);
    assert!(!graph.is_ready() && !graph.is_closed() && graph.last_data().is_none());
    assert_eq!(graph.orientation(), "horizontal");
    assert!(graph.set_orientation("sideways").is_err());
    graph.set_orientation("vertical").unwrap();
    assert_eq!(graph.settings().orientation, "vertical");
    assert!(graph.set_options(false, true, "never").is_err());
    graph.set_options(false, true, "absolute").unwrap();
    graph.set_max_commits(5);
    let s = graph.settings();
    assert!(!s.show_remotes && s.show_tags && s.time_format == "absolute");
    assert_eq!(s.max_commits, 100, "clamped like the WebKit host");
    graph.set_settings(GraphSettings::default());
    assert_eq!(graph.settings(), GraphSettings::default());

    let changed = Rc::new(Cell::new(0));
    let c = changed.clone();
    graph.connect_root_changed(move || c.set(c.get() + 1));
    graph.set_root("/other");
    assert_eq!(graph.root(), std::path::PathBuf::from("/other"));
    assert_eq!(changed.get(), 1);
    graph.shutdown();
    assert!(graph.is_closed());
    graph.set_root("/ignored");
    assert_eq!(changed.get(), 1, "closed hosts ignore set_root");

    let commit = CommitView::new("/repo", "abc123", None);
    assert_eq!(commit.tab_key(), "commit:abc123");
    assert_eq!(commit.rev(), "abc123");
    assert_eq!(notice_of(commit.widget()), NOTICE);
    commit.connect_open_file(|_, _| unreachable!());
    commit.set_font_size(14);
    commit.shutdown();
    assert!(commit.is_closed());
}
