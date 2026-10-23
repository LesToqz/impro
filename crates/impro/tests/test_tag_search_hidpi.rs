//! A click on a tag marker in the Graph view focuses the explorer's search entry
//! with `tag:#name`. At a display scale of 2 that used to send the main thread
//! into a loop that allocated until the machine was out of memory, so every test
//! here runs under a watchdog that aborts with a message instead of exhausting RAM.
//!
//! The scale defaults to 2 (`GDK_SCALE=2`) unless the environment sets one, so the
//! file also runs at 1: `GDK_SCALE=1 cargo test --test test_tag_search_hidpi`.
mod common;
use common::*;
use gtk::prelude::*;
use std::{
    sync::Once,
    time::{Duration, Instant},
};

const RSS_LIMIT_MB: u64 = 1500;
const TIME_LIMIT: Duration = Duration::from_secs(120);

/// Resident memory of this process in MB.
fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .map_or(0, |pages| pages * 4 / 1024)
}

/// Selects the display scale and starts the watchdog thread, once per binary:
/// the process aborts when it holds more than [`RSS_LIMIT_MB`] or runs longer
/// than [`TIME_LIMIT`] (a runaway main thread cannot report a failure itself).
fn guard() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("GDK_SCALE").is_none() {
            std::env::set_var("GDK_SCALE", "2");
        }
        let start = Instant::now();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(50));
            let rss = rss_mb();
            if rss > RSS_LIMIT_MB || start.elapsed() > TIME_LIMIT {
                eprintln!(
                    "WATCHDOG: the tag search ran away (rss {rss} MB, {:?} elapsed, limits \
                     {RSS_LIMIT_MB} MB / {TIME_LIMIT:?}); aborting",
                    start.elapsed()
                );
                std::process::abort();
            }
        });
    });
}

fn project(h: &Harness) {
    for i in 0..240 {
        let mut text = format!("# Note {i}\n");
        if i % 12 == 0 {
            text.push_str("#topic [[Missing thing]]\n");
        }
        h.write(&format!("notes/n{i}.md"), &text);
    }
    // the explorer search has no tag operator: the query is plain text
    h.write("notes/about-tags.md", "files are found by tag:#topic\n");
    h.w.explorer.refresh();
    drain();
}

/// What the Graph view does on a tag click.
fn click_tag(h: &Harness, tag: &str) {
    h.w.search_in_files();
    h.w.explorer
        .focus_search(Some(&impro_graph::tag_query(tag)));
}

/// Waits until the search has delivered its results and the window has painted.
fn settled(h: &Harness) {
    let search = h.w.explorer.search();
    poll(|| !search.timeout_pending() && !search.is_searching());
    frame(&h.w.window);
    frame(&h.w.window);
}

#[test]
fn a_tag_click_finds_the_notes_and_the_main_loop_comes_to_rest() {
    guard();
    run(|| {
        let h = Harness::new();
        project(&h);
        click_tag(&h, "topic");
        settled(&h);
        let results = h.w.explorer.search().results();
        assert_eq!(results.total_files, 1, "{results:?}");
        // nothing keeps re-arming: the loop goes idle once the frames are painted
        let context = glib::MainContext::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        while context.pending() {
            assert!(
                Instant::now() < deadline,
                "the main loop never went idle after a tag click"
            );
            context.iteration(false);
        }
    });
}

#[test]
fn focusing_the_search_entry_leaves_its_style_path_alone() {
    guard();
    run(|| {
        let h = Harness::new();
        project(&h);
        let entry = h.w.explorer.search().entry();
        let tree = h.w.explorer.tree();
        let before = entry.path().length();
        for round in 0..5 {
            click_tag(&h, "topic");
            settled(&h);
            tree.grab_focus();
            drain();
            assert_eq!(
                entry.path().length(),
                before,
                "round {round}: the search entry's widget path grew"
            );
        }
        assert!(rss_mb() < RSS_LIMIT_MB);
    });
}

#[test]
fn typing_a_tag_query_does_the_same_as_a_click() {
    guard();
    run(|| {
        let h = Harness::new();
        project(&h);
        let search = h.w.explorer.search();
        let entry = search.entry();
        let before = entry.path().length();
        entry.grab_focus();
        entry.set_text("tag:#topic");
        assert!(search.timeout_pending(), "typing arms the debounce");
        settled(&h);
        assert_eq!(search.results().total_files, 1);
        assert_eq!(entry.path().length(), before);
    });
}
