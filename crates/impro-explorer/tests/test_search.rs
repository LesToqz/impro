//! Search core and GTK wrapper contracts (run under X11/xvfb).
mod common;
use common::*;
use gtk::prelude::*;
use impro_core::themes;
use impro_explorer::pool::CancelToken;
use impro_explorer::search::*;
use impro_explorer::SearchPanel;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Condvar, Mutex};

struct Files {
    temp: tempfile::TempDir,
}

impl Files {
    fn new() -> Self {
        Self {
            temp: tempfile::tempdir().unwrap(),
        }
    }
    fn root(&self) -> &Path {
        self.temp.path()
    }
    fn write(&self, name: &str, text: impl AsRef<[u8]>) -> PathBuf {
        let path = self.root().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }
    fn search(&self, query: &str, mut configure: impl FnMut(&mut SearchOptions)) -> SearchResults {
        let mut options = SearchOptions::default();
        configure(&mut options);
        search(self.root(), query, &options, None, None).unwrap()
    }
    fn needle(&self) -> SearchResults {
        self.search("needle", |_| {})
    }
}

fn find(text: &str, query: &str) -> Vec<Match> {
    find_in_text(text, query, false, false).unwrap()
}

fn tuple(m: &Match) -> (usize, usize, usize, &str) {
    (m.line, m.column, m.length, m.line_text.as_str())
}

#[test]
fn test_skips_directories_and_sorts_relative_paths() {
    let f = Files::new();
    for name in SKIP_DIRS {
        f.write(&format!("{name}/hidden.txt"), "needle");
    }
    let paths: Vec<_> = ["z.txt", "a/x.txt", "B.txt", "A.txt"]
        .iter()
        .map(|n| f.write(n, "needle"))
        .collect();
    let found: Vec<_> = iter_project_files(f.root()).collect();
    assert_eq!(
        found,
        [
            paths[3].clone(),
            paths[1].clone(),
            paths[2].clone(),
            paths[0].clone()
        ]
    );
}

#[test]
fn test_skips_large_binary_invalid_utf8_and_symlinks() {
    let f = Files::new();
    let good = f.write("good", "yes");
    f.write("large", vec![b'x'; MAX_FILE_SIZE + 1]);
    f.write("binary", b"text\0more");
    let mut invalid = vec![b'x'; 9000];
    invalid.push(0xff);
    f.write("invalid", invalid);
    std::os::unix::fs::symlink(&good, f.root().join("link")).unwrap();
    std::os::unix::fs::symlink(f.root(), f.root().join("cycle")).unwrap();
    assert_eq!(iter_project_files(f.root()).collect::<Vec<_>>(), [good]);
}

#[test]
fn test_size_boundary() {
    let f = Files::new();
    let path = f.write("boundary", vec![b'x'; MAX_FILE_SIZE]);
    assert_eq!(iter_project_files(f.root()).collect::<Vec<_>>(), [path]);
}

#[test]
fn test_literal_case_regex_and_multiple_matches() {
    let found = find("A.b aXb a.b", "a.b");
    assert_eq!(found.len(), 2);
    assert_eq!(tuple(&found[0]), (0, 0, 3, "A.b aXb a.b"));
    assert_eq!(tuple(&found[1]), (0, 8, 3, "aXb a.b"));
    assert_eq!(
        find_in_text("A.b aXb a.b", "a.b", true, false)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        find_in_text("A.b aXb a.b", "a.b", false, true)
            .unwrap()
            .len(),
        3
    );
    assert!(find("abc", "").is_empty());
    assert!(find_in_text("abc", "^|$", false, true).unwrap().is_empty());
}

#[test]
fn test_invalid_regex() {
    let error = find_in_text("x", "[", false, true).unwrap_err();
    assert!(error.0.starts_with("Invalid regular expression"), "{error}");
    assert!(!error.0.contains('\n'));
    let f = Files::new();
    let options = SearchOptions {
        regex: true,
        ..SearchOptions::default()
    };
    assert!(search(f.root(), "[", &options, None, None).is_err());
}

#[test]
fn test_unicode_character_coordinates_and_preview_offset() {
    let text = format!("é\n{}<needle>{}   ", "α".repeat(350), "x".repeat(300));
    let m = &find(&text, "<needle>")[0];
    assert_eq!((m.line, m.column, m.length), (1, 350, 8));
    assert!(m.line_text.chars().count() <= 200);
    let start = m.column - m.preview_offset;
    let shown: String = m.line_text.chars().skip(start).take(m.length).collect();
    assert_eq!(shown, "<needle>");
    assert_eq!(find("needle  ", "needle")[0].line_text, "needle");
}

fn assert_preview(line: &str, query: &str, offset: usize, preview: &str) {
    let found = find(&format!("header\n{line}"), query);
    let m = &found[0];
    let column = line.find(query).map(|b| line[..b].chars().count()).unwrap();
    assert_eq!(
        (m.line, m.column, m.length),
        (1, column, query.chars().count())
    );
    assert_eq!(m.preview_offset, offset);
    assert_eq!(m.line_text, preview);
    let start = m.column - m.preview_offset;
    assert!(start <= 4);
    let shown: String = m.line_text.chars().skip(start).take(m.length).collect();
    assert_eq!(shown, query);
}

fn tail(line: &str, from: usize) -> String {
    line.chars().skip(from).collect()
}

#[test]
fn test_late_match_in_line_shorter_than_preview_limit() {
    let line = format!("{}needle{}", "x".repeat(120), "y".repeat(64));
    assert_eq!(line.chars().count(), 190);
    assert_preview(&line, "needle", 116, &tail(&line, 116));
}

#[test]
fn test_early_match_and_indentation() {
    assert_preview("  foo needle tail  ", "needle", 2, "foo needle tail");
    assert_preview("needle tail", "needle", 0, "needle tail");
}

#[test]
fn test_match_at_end_does_not_restore_dropped_context() {
    let line = format!("{}needle", "context ".repeat(20));
    assert_preview(&line, "needle", 156, &tail(&line, 156));
}

#[test]
fn test_wide_characters_use_original_character_offsets() {
    let line = format!("{}針é{}", "\u{3000}\té界🙂".repeat(30), "尾".repeat(10));
    assert_preview(&line, "針é", 147, &tail(&line, 147));
}

#[test]
fn test_very_long_line_keeps_preview_bounded() {
    let line = format!("{}needle{}", "x".repeat(10000), "y".repeat(10000));
    let expected: String = line.chars().skip(9996).take(200).collect();
    assert_preview(&line, "needle", 9996, &expected);
    assert_eq!(find(&line, "needle")[0].line_text.chars().count(), 200);
}

#[test]
fn test_tabs_trimmed_but_matched_whitespace_retained() {
    assert_preview("\t\tfoo\tneedle\t", "needle", 2, "foo\tneedle");
    assert_preview("\t\tneedle\t ", "\tneedle\t ", 1, "\tneedle\t ");
}

#[test]
fn test_line_boundaries_follow_unicode_splitlines() {
    let lines: Vec<_> = splitlines("a\r\nb\rc\u{2028}d\x0be\nf\n").collect();
    assert_eq!(lines, ["a", "b", "c", "d", "e", "f"]);
    assert_eq!(splitlines("").count(), 0);
    assert_eq!(splitlines("\n").collect::<Vec<_>>(), [""]);
    let found = find("one\rneedle", "needle");
    assert_eq!(found[0].line, 1);
}

#[test]
fn test_order_and_basename_literal_matches() {
    let f = Files::new();
    let a = f.write("a.txt", "needle");
    let z = f.write("z.txt", "needle");
    let current = f.write("sub/current.txt", "needle");
    let name = f.write("sub/NEEDLE.txt", "nothing");
    let result = f.search("needle", |o| o.current_path = Some(current.clone()));
    let paths: Vec<_> = result.groups.iter().map(|g| g.path.clone()).collect();
    assert_eq!(paths, [current.clone(), a, z]);
    assert!(result.groups[0].is_current);
    assert_eq!(result.file_names, std::slice::from_ref(&name));
    assert_eq!((result.total_matches, result.total_files), (3, 3));
    assert_eq!(
        f.search("NEEDLE", |o| o.case_sensitive = true).file_names,
        [name]
    );
    assert!(f.search(".*", |o| o.regex = true).file_names.is_empty());
}

#[test]
fn test_binary_files_are_not_name_matches() {
    let f = Files::new();
    f.write("needle.bin", b"\0\0\0");
    f.write("needle.txt", "plain");
    let result = f.needle();
    assert_eq!(result.file_names.len(), 1);
    assert!(result.file_names[0].ends_with("needle.txt"));
}

#[test]
fn test_live_provider_overrides_disk_and_supports_unsaved_file() {
    let f = Files::new();
    let disk = f.write("disk", "needle");
    let unsaved = f.root().join("new.txt");
    let options = SearchOptions {
        current_path: Some(unsaved.clone()),
        ..SearchOptions::default()
    };
    let provider = |p: &Path| {
        Some(
            if p == unsaved {
                "needle needle"
            } else {
                "changed"
            }
            .to_owned(),
        )
    };
    let result = search(f.root(), "needle", &options, Some(&provider), None).unwrap();
    let paths: Vec<_> = result.groups.iter().map(|g| g.path.clone()).collect();
    assert_eq!(paths, [unsaved]);
    assert_eq!(result.total_matches, 2);
    let none = |_: &Path| None;
    let result = search(
        f.root(),
        "needle",
        &SearchOptions::default(),
        Some(&none),
        None,
    )
    .unwrap();
    assert_eq!(result.groups[0].path, disk);
}

#[test]
fn test_per_file_and_total_limits() {
    let f = Files::new();
    f.write("a", "needle needle needle");
    f.write("b", "needle needle");
    let result = f.search("needle", |o| o.max_per_file = 2);
    assert_eq!(
        result
            .groups
            .iter()
            .map(|g| g.matches.len())
            .collect::<Vec<_>>(),
        [2, 2]
    );
    assert!(result.truncated);
    let result = f.search("needle", |o| o.max_total = 3);
    assert_eq!(
        result
            .groups
            .iter()
            .map(|g| g.matches.len())
            .collect::<Vec<_>>(),
        [3]
    );
    assert!(result.truncated);
    assert_eq!(f.search("needle", |o| o.max_total = 0).total_matches, 0);
    assert!(f.search("needle", |o| o.max_total = 0).truncated);
    assert!(!f.search("needle", |o| o.max_total = 5).truncated);
}

#[test]
fn test_cancellation_between_files() {
    let f = Files::new();
    let current = f.write("z", "needle");
    f.write("a", "needle");
    let options = SearchOptions {
        current_path: Some(current.clone()),
        ..SearchOptions::default()
    };
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(search(f.root(), "needle", &options, None, Some(&cancel))
        .unwrap()
        .groups
        .is_empty());
    let cancel = CancelToken::new();
    let trigger = cancel.clone();
    let provider = move |_: &Path| {
        trigger.cancel();
        Some("needle".to_owned())
    };
    let result = search(f.root(), "needle", &options, Some(&provider), Some(&cancel)).unwrap();
    let paths: Vec<_> = result.groups.iter().map(|g| g.path.clone()).collect();
    assert_eq!(paths, [current]);
}

#[test]
fn test_current_provider_runs_before_project_enumeration() {
    use std::sync::{Arc, Mutex};
    let f = Files::new();
    let current = f.write("z", "needle");
    let calls = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
    let seen_at_walk = Arc::new(Mutex::new(None));
    {
        let (calls, seen) = (calls.clone(), seen_at_walk.clone());
        set_walk_hook(
            f.root(),
            Some(Arc::new(move |_| {
                *seen.lock().unwrap() = Some(calls.lock().unwrap().clone());
            })),
        );
    }
    let options = SearchOptions {
        current_path: Some(current.clone()),
        ..SearchOptions::default()
    };
    let recorder = calls.clone();
    let provider = move |p: &Path| {
        recorder.lock().unwrap().push(p.to_path_buf());
        Some("needle".to_owned())
    };
    let result = search(f.root(), "needle", &options, Some(&provider), None).unwrap();
    assert!(result.groups[0].is_current);
    assert_eq!(seen_at_walk.lock().unwrap().clone().unwrap(), [current]);
}

#[test]
fn test_empty_query_and_missing_root() {
    let f = Files::new();
    assert_eq!(f.search("", |_| {}).total_matches, 0);
    let result = search(
        Path::new("/does/not/exist"),
        "x",
        &SearchOptions::default(),
        None,
        None,
    )
    .unwrap();
    assert!(result.groups.is_empty());
}

#[test]
fn test_progress_streams_current_file_first_then_each_file() {
    let f = Files::new();
    let current = f.write("z", "needle");
    f.write("a", "needle");
    f.write("b", "nothing");
    let options = SearchOptions {
        current_path: Some(current),
        ..SearchOptions::default()
    };
    let mut seen = Vec::new();
    let cancel = CancelToken::new();
    search_stream(f.root(), "needle", &options, None, &cancel, &mut |r| {
        seen.push((r.groups.len(), r.total_matches));
    })
    .unwrap();
    assert_eq!(seen, [(1, 1), (2, 2), (2, 2), (2, 2)]);
}

#[test]
fn test_quick_switcher_listing_skips_excluded_and_hidden() {
    let f = Files::new();
    f.write("b.txt", "");
    f.write("a.txt", "");
    f.write(".dot", "");
    f.write("sub/c.txt", "");
    f.write("node_modules/x.js", "");
    f.write(".hidden/y.txt", "");
    let names = |hidden| -> Vec<String> {
        list_project_files(f.root(), hidden, 100)
            .iter()
            .map(|p| relative_to(p, f.root()))
            .collect()
    };
    assert_eq!(names(false), ["a.txt", "b.txt", "sub/c.txt"]);
    assert_eq!(
        names(true),
        [".dot", "a.txt", "b.txt", ".hidden/y.txt", "sub/c.txt"]
    );
    assert_eq!(list_project_files(f.root(), true, 2).len(), 2);
}

// ---------------------------------------------------------------------------------------
// SearchPanel (GTK) -----------------------------------------------------------------------

struct Fixture {
    files: Files,
    a: PathBuf,
    z: PathBuf,
    name: PathBuf,
    window: gtk::OffscreenWindow,
    panel: SearchPanel,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        destroy(&self.window);
    }
}

fn setup() -> Fixture {
    let files = Files::new();
    let a = files.write("a.py", "needle Needle\n<needle> & more");
    let z = files.write("z.py", "needle");
    let name = files.write("needle.txt", "unrelated");
    let window = gtk::OffscreenWindow::new();
    let panel = SearchPanel::new(&gtk::Label::new(Some("Explorer tree")), files.root());
    window.add(&panel);
    window.set_default_size(480, 350);
    window.show_all();
    Fixture {
        files,
        a,
        z,
        name,
        window,
        panel,
    }
}

fn wait_scan(panel: &SearchPanel) {
    let panel = panel.clone();
    pump_until("search to finish", move || {
        !panel.timeout_pending() && !panel.is_searching()
    });
}

fn row_text(panel: &SearchPanel, path: &str, column: i32) -> String {
    let store = panel.store();
    let iter = store.iter_from_string(path).expect("row exists");
    impro_explorer::util::string(&store, &iter, column)
}

fn row_count(panel: &SearchPanel) -> i32 {
    panel.store().iter_n_children(None)
}

fn escape_key() -> gdk::keys::Key {
    gdk::keys::constants::Escape
}

#[test]
fn test_pages_and_clear() {
    run(|| {
        let f = setup();
        assert_eq!(
            f.panel.stack().visible_child_name().as_deref(),
            Some("content")
        );
        f.panel.entry().set_text("needle");
        assert_eq!(
            f.panel.stack().visible_child_name().as_deref(),
            Some("results")
        );
        wait_scan(&f.panel);
        assert_eq!(f.panel.results().total_matches, 4);
        f.panel.clear();
        assert_eq!(f.panel.query(), "");
        assert_eq!(
            f.panel.stack().visible_child_name().as_deref(),
            Some("content")
        );
    });
}

#[test]
fn test_current_group_and_resort() {
    run(|| {
        let f = setup();
        f.panel.set_current_file(Some(&f.z));
        f.panel.search_now(Some("needle"));
        let panel = f.panel.clone();
        pump_until("current group", move || row_count(&panel) > 0);
        assert_eq!(row_text(&f.panel, "0", 3), f.z.to_string_lossy());
        assert!(row_text(&f.panel, "0", 2).contains("current file"));
        wait_scan(&f.panel);
        f.panel.set_current_file(Some(&f.a));
        assert_eq!(row_text(&f.panel, "0", 3), f.a.to_string_lossy());
        wait_scan(&f.panel);
        assert!(f.panel.results().groups[0].is_current);
        assert_eq!(row_text(&f.panel, "0", 3), f.a.to_string_lossy());
    });
}

#[test]
fn test_signal_match_and_file_name() {
    run(|| {
        let f = setup();
        let received = Rc::new(RefCell::new(Vec::new()));
        let sink = received.clone();
        f.panel
            .connect_result_activated(move |_, path, line, column, length| {
                sink.borrow_mut()
                    .push((path.to_path_buf(), line, column, length));
            });
        f.panel.search_now(Some("needle"));
        wait_scan(&f.panel);
        let tree = f.panel.tree();
        let column = tree.column(0).unwrap();
        tree.row_activated(&gtk::TreePath::from_string("0:1"), &column);
        tree.row_activated(&gtk::TreePath::from_string("2:0"), &column);
        assert_eq!(
            *received.borrow(),
            [(f.a.clone(), 0, 7, 6), (f.name.clone(), 0, 0, 0)]
        );
    });
}

#[test]
fn test_group_activation_toggles() {
    run(|| {
        let f = setup();
        f.panel.search_now(Some("needle"));
        wait_scan(&f.panel);
        let path = gtk::TreePath::from_string("0");
        let tree = f.panel.tree();
        assert!(tree.activates_on_single_click());
        assert!(tree.row_expanded(&path));
        f.panel.activate(&path);
        assert!(!tree.row_expanded(&path));
        f.panel.activate(&path);
        assert!(tree.row_expanded(&path));
    });
}

#[test]
fn test_escape_entry_and_list() {
    run(|| {
        let f = setup();
        for widget in [
            f.panel.entry().upcast::<gtk::Widget>(),
            f.panel.tree().upcast(),
        ] {
            f.panel.search_now(Some("needle"));
            assert!(press(&widget, escape_key(), gdk::ModifierType::empty()));
            assert_eq!(f.panel.query(), "");
        }
        f.panel.search_now(Some("needle"));
        f.panel.entry().emit_by_name::<()>("stop-search", &[]);
        assert_eq!(f.panel.query(), "");
    });
}

#[test]
fn test_toggles_change_results() {
    run(|| {
        let f = setup();
        f.panel.search_now(Some("Needle"));
        wait_scan(&f.panel);
        assert_eq!(f.panel.results().total_matches, 4);
        f.panel.case_button().set_active(true);
        wait_scan(&f.panel);
        assert_eq!(f.panel.results().total_matches, 1);
        f.panel.case_button().set_active(false);
        f.panel.search_now(Some("n.edle"));
        wait_scan(&f.panel);
        assert_eq!(f.panel.results().total_matches, 0);
        f.panel.regex_button().set_active(true);
        wait_scan(&f.panel);
        assert_eq!(f.panel.results().total_matches, 4);
    });
}

#[test]
fn test_invalid_regex_message_sync_and_async() {
    run(|| {
        let f = setup();
        f.panel.regex_button().set_active(true);
        f.panel.search_now(Some("["));
        let panel = f.panel.clone();
        pump_until("error", move || {
            panel
                .summary()
                .text()
                .starts_with("Invalid regular expression:")
        });
        f.panel.entry().set_text("(");
        wait_scan(&f.panel);
        let summary = f.panel.summary();
        pump_until("second error", move || {
            summary.text().starts_with("Invalid regular expression:")
        });
        assert_eq!(row_count(&f.panel), 0);
    });
}

#[test]
fn test_markup_escaped_and_highlighted() {
    run(|| {
        let f = setup();
        f.panel.search_now(Some("<needle>"));
        wait_scan(&f.panel);
        let markup = row_text(&f.panel, "0:0", 2);
        assert!(markup.contains("<b><u>&lt;needle&gt;</u></b>"), "{markup}");
        assert!(markup.contains("&amp;"));
    });
}

#[test]
fn test_trimmed_markup_escapes_context_and_match() {
    run(|| {
        let f = setup();
        f.files
            .write("a.py", format!("    {} & <needle> tail", "x".repeat(120)));
        f.panel.search_now(Some("<needle>"));
        wait_scan(&f.panel);
        let markup = row_text(&f.panel, "0:0", 2);
        assert!(markup.starts_with('…'), "{markup}");
        assert!(markup.contains("&amp;"));
        assert!(markup.contains("<b><u>&lt;needle&gt;</u></b>"));
        f.files.write("a.py", "<needle>");
        f.panel.search_now(Some("<needle>"));
        wait_scan(&f.panel);
        assert!(!row_text(&f.panel, "0:0", 2).starts_with('…'));
    });
}

#[test]
fn test_current_results_before_slow_scan_and_superseding_query() {
    run(|| {
        let f = setup();
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let entered = Arc::new(Mutex::new(0usize));
        {
            let (gate, entered) = (gate.clone(), entered.clone());
            set_walk_hook(
                f.files.root(),
                Some(Arc::new(move |_| {
                    let first = {
                        let mut count = entered.lock().unwrap();
                        *count += 1;
                        *count == 1
                    };
                    if first {
                        let (lock, cv) = &*gate;
                        let _ = cv
                            .wait_timeout_while(
                                lock.lock().unwrap(),
                                std::time::Duration::from_secs(3),
                                |open| !*open,
                            )
                            .unwrap();
                    }
                })),
            );
        }
        let release = {
            let gate = gate.clone();
            move || {
                *gate.0.lock().unwrap() = true;
                gate.1.notify_all();
            }
        };
        f.panel.set_current_file(Some(&f.z));
        f.panel.entry().set_text("needle");
        let counter = entered.clone();
        pump_until("slow scan entered", move || *counter.lock().unwrap() >= 1);
        let panel = f.panel.clone();
        pump_until("current file shown first", move || row_count(&panel) > 0);
        assert_eq!(row_text(&f.panel, "0", 3), f.z.to_string_lossy());
        assert!(f.panel.is_searching());
        let old_generation = f.panel.generation();
        f.panel.entry().set_text("unrelated");
        wait_scan(&f.panel);
        assert!(f.panel.generation() > old_generation);
        release();
        assert_eq!(f.panel.results().total_matches, 1);
        assert_eq!(f.panel.results().groups[0].path, f.name);
        // Explicit late delivery must be discarded as well.
        assert!(!f
            .panel
            .deliver_snapshot(old_generation, SearchResults::default(), true));
        assert_eq!(f.panel.results().total_matches, 1);
        set_walk_hook(f.files.root(), None);
    });
}

#[test]
fn test_destroy_during_scan() {
    run(|| {
        let f = setup();
        let gate = Arc::new((Mutex::new(false), Condvar::new()));
        let entered = Arc::new(Mutex::new(false));
        {
            let (gate, entered) = (gate.clone(), entered.clone());
            set_walk_hook(
                f.files.root(),
                Some(Arc::new(move |_| {
                    *entered.lock().unwrap() = true;
                    let (lock, cv) = &*gate;
                    let _ = cv
                        .wait_timeout_while(
                            lock.lock().unwrap(),
                            std::time::Duration::from_secs(3),
                            |open| !*open,
                        )
                        .unwrap();
                })),
            );
        }
        f.panel.entry().set_text("needle");
        let flag = entered.clone();
        pump_until("scan entered", move || *flag.lock().unwrap());
        let generation = f.panel.generation();
        destroy(&f.window);
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        pump(60);
        assert!(!f
            .panel
            .deliver_snapshot(generation, f.panel.results(), true));
        set_walk_hook(f.files.root(), None);
    });
}

#[test]
fn test_provider_called_on_main_thread() {
    run(|| {
        let f = setup();
        let main = std::thread::current().id();
        let threads = Rc::new(RefCell::new(Vec::new()));
        let seen = threads.clone();
        f.panel.set_text_provider(Some(Rc::new(move |_| {
            seen.borrow_mut().push(std::thread::current().id());
            Some("live".to_owned())
        })));
        f.panel.entry().set_text("live");
        wait_scan(&f.panel);
        assert_eq!(f.panel.results().total_matches, 3);
        assert!(!threads.borrow().is_empty());
        assert!(threads.borrow().iter().all(|id| *id == main));
    });
}

#[test]
fn test_root_provider_focus_and_down() {
    run(|| {
        let f = setup();
        f.panel.set_root(Path::new(""));
        f.panel.search_now(Some("needle"));
        wait_scan(&f.panel);
        assert!(f.panel.results().groups.is_empty());
        f.panel.set_root(f.files.root());
        f.panel
            .set_text_provider(Some(Rc::new(|_| Some("buffer".to_owned()))));
        f.panel.search_now(Some("buffer"));
        wait_scan(&f.panel);
        assert_eq!(f.panel.results().total_matches, 3);
        f.panel.focus_search();
        assert_eq!(f.panel.entry().selection_bounds(), Some((0, 6)));
        assert!(press(
            &f.panel.entry(),
            gdk::keys::constants::Down,
            gdk::ModifierType::empty()
        ));
        let (path, _) = f.panel.tree().cursor();
        assert_eq!(path.map(|p| p.to_str().to_string()).as_deref(), Some("0"));
    });
}

#[test]
fn test_font_size_and_escaped_signal() {
    run(|| {
        let f = setup();
        f.panel.set_font_size(15.0);
        for cell in f.panel.cells() {
            assert_eq!(cell.size_points(), 15.0);
        }
        let escaped = Rc::new(RefCell::new(0));
        let count = escaped.clone();
        f.panel.connect_escaped(move |_| *count.borrow_mut() += 1);
        f.panel.search_now(Some("needle"));
        press(&f.panel.entry(), escape_key(), gdk::ModifierType::empty());
        press(&f.panel.tree(), escape_key(), gdk::ModifierType::empty());
        assert_eq!(*escaped.borrow(), 2);
    });
}

#[test]
fn test_enter_moves_focus_into_results() {
    run(|| {
        let f = setup();
        f.panel.focus_search();
        f.panel.entry().set_text("needle");
        f.panel.entry().emit_by_name::<()>("activate", &[]);
        let panel = f.panel.clone();
        pump_until("cursor in results", move || {
            panel.tree().cursor().0.is_some()
        });
        assert!(f.panel.tree().is_focus());
    });
}

#[test]
fn test_css_uses_all_theme_roles_and_parses() {
    run(|| {
        assert!(search_css_text().contains(".impro-search-entry"));
        for theme in themes::themes() {
            let css = impro_explorer::search_css_for(&theme.ui);
            impro_ui::style::create_provider(&css).unwrap_or_else(|e| panic!("{}: {e}", theme.id));
            assert!(!css.contains("{ui["));
        }
    });
}

fn search_css_text() -> String {
    impro_explorer::searchpanel::search_css()
}

#[test]
fn test_narrow_result_keeps_late_match_near_start_and_ellipsizes_tail() {
    run(|| {
        let provider = gtk::CssProvider::new();
        provider
            .load_from_data(search_css_text().as_bytes())
            .unwrap();
        let screen = gdk::Screen::default().unwrap();
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let f = setup();
        let line = format!("{}needle{} end", "context ".repeat(15), " tail".repeat(12));
        assert_eq!((line.find("needle").unwrap(), line.len()), (120, 190));
        f.files.write("a.py", &line);
        destroy(&f.window);
        let window = gtk::OffscreenWindow::new();
        let panel = SearchPanel::new(&gtk::Label::new(Some("Explorer tree")), f.files.root());
        window.add(&panel);
        window.set_default_size(260, 350);
        window.show_all();
        panel.set_font_size(10.0); // Explorer's default.
        panel.search_now(Some("needle"));
        wait_scan(&panel);
        let tree = panel.tree();
        let column = tree.column(0).unwrap();
        let store = panel.store();
        let row = store.iter_from_string("0:0").unwrap();
        column.cell_set_cell_data(&store, &row, false, false);
        let [number, preview] = panel.cells();
        // Read the text produced by the renderer's markup parser.
        let displayed: String = preview.property("text");
        assert!(displayed.starts_with('…'), "{displayed}");
        assert!(displayed.find("needle").unwrap() < 30 + 2);
        assert!(impro_explorer::util::string(&store, &row, 2).contains("<b><u>needle</u></b>"));
        assert_eq!(
            (
                impro_explorer::util::int(&store, &row, 4),
                impro_explorer::util::int(&store, &row, 5),
                impro_explorer::util::int(&store, &row, 6)
            ),
            (0, 120, 6)
        );
        assert_eq!(preview.ellipsize(), pango::EllipsizeMode::End);
        assert_eq!(number.ellipsize(), pango::EllipsizeMode::None);
        // The navigator reserves a 12 px inset on either side.
        let t = tree.clone();
        pump_until("tree width", move || t.allocated_width() == 260 - 24);
        let scroll = tree
            .parent()
            .unwrap()
            .downcast::<gtk::ScrolledWindow>()
            .unwrap();
        let adjustment = scroll.hadjustment();
        assert!(adjustment.upper() <= adjustment.page_size());
        assert!(!scroll.hscrollbar().unwrap().is_child_visible());
        assert!(
            column.cell_get_position(&number).unwrap().0 > 0
                || column.cell_get_position(&number).unwrap().1 > 0
        );
        destroy(&window);
        gtk::StyleContext::remove_provider_for_screen(&screen, &provider);
    });
}

fn show_styled_window(f: &mut Fixture) -> gtk::Window {
    destroy(&f.window);
    impro_ui::apply_theme(themes::get_theme(Some("impro-dark")), None, None, None).unwrap();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(300, 350);
    f.panel = SearchPanel::new(&gtk::Label::new(Some("Explorer tree")), f.files.root());
    window.add(&f.panel);
    window.show_all();
    window.present();
    let w = window.clone();
    pump_until("window active", move || w.is_active());
    window
}

fn pixel(window: &gtk::Window, widget: &impl IsA<gtk::Widget>, x: i32, y: i32) -> (u8, u8, u8) {
    let (x, y) = widget.translate_coordinates(window, x, y).unwrap();
    let gdk_window = window.window().unwrap();
    let pixbuf = common::window_pixbuf(&gdk_window, x, y, 1, 1).expect("pixel");
    let bytes = pixbuf.read_pixel_bytes();
    (bytes[0], bytes[1], bytes[2])
}

fn rgb(hex: &str) -> (u8, u8, u8) {
    let value = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    (value(1), value(3), value(5))
}

#[test]
fn test_entry_geometry_native_clear_icon_and_focus_ring() {
    run(|| {
        let mut f = setup();
        let window = show_styled_window(&mut f);
        let entry = f.panel.entry();
        let secondary = gtk::EntryIconPosition::Secondary;
        assert_eq!(entry.allocated_height(), 30);
        assert!(entry.icon_name(gtk::EntryIconPosition::Primary).is_some());
        assert!(entry.icon_name(secondary).is_none());
        f.panel.search_now(Some("needle"));
        window.show_all();
        assert_eq!(
            entry.icon_name(secondary).as_deref(),
            Some("edit-clear-symbolic")
        );
        assert!(entry.icon_is_activatable(secondary));
        let event = gdk::Event::new(gdk::EventType::ButtonRelease);
        entry.emit_by_name::<()>("icon-release", &[&secondary, &event]);
        assert_eq!(f.panel.query(), "");
        assert_eq!(
            f.panel.stack().visible_child_name().as_deref(),
            Some("content")
        );
        window.show_all();
        assert!(entry.icon_name(secondary).is_none());
        assert_eq!(entry.allocated_height(), 30);
        for name in ["impro-dark", "impro-light"] {
            let theme = themes::get_theme(Some(name));
            impro_ui::apply_theme(theme, None, None, None).unwrap();
            entry.grab_focus();
            window.set_focus_visible(true);
            let ring = rgb(&theme.ui["border_focus"]);
            let (e, w) = (entry.clone(), window.clone());
            pump_until("focus ring", move || pixel(&w, &e, -1, 15) == ring);
            let radius: i32 = entry
                .style_context()
                .style_property_for_state("border-radius", entry.state_flags())
                .get()
                .unwrap_or(0);
            assert_eq!(radius, 5, "{name}");
        }
        destroy(&window);
    });
}

#[test]
fn test_result_rows_share_geometry_and_rounded_overlay_in_both_themes() {
    run(|| {
        let mut f = setup();
        let window = show_styled_window(&mut f);
        f.panel.search_now(Some("needle"));
        wait_scan(&f.panel);
        let tree = f.panel.tree();
        let path = gtk::TreePath::from_string("0:0");
        tree.set_cursor(&path, None::<&gtk::TreeViewColumn>, false);
        f.panel.entry().grab_focus();
        window.set_focus_visible(false);
        let t = tree.clone();
        pump_until("tree allocated", move || t.allocated_width() > 1);
        for name in ["impro-dark", "impro-light"] {
            let theme = themes::get_theme(Some(name));
            impro_ui::apply_theme(theme, None, None, None).unwrap();
            let column = tree.column(0).unwrap();
            let area = tree.background_area(Some(&path), Some(&column));
            let group_area =
                tree.background_area(Some(&gtk::TreePath::from_string("0")), Some(&column));
            assert!((24..=26).contains(&area.height()), "{}", area.height());
            assert_eq!(group_area.height(), area.height());
            let sidebar = rgb(&theme.ui["sidebar_bg"]);
            let overlay = rgb(&theme.ui["overlay"]);
            let mix = |a: u8, b: u8| (f64::from(a) * 0.933 + f64::from(b) * 0.067).round() as u8;
            let tint = (
                mix(sidebar.0, overlay.0),
                mix(sidebar.1, overlay.1),
                mix(sidebar.2, overlay.2),
            );
            let (x, y) = tree.convert_bin_window_to_widget_coords(area.x(), area.y());
            let (t, w) = (tree.clone(), window.clone());
            pump_until("corner is sidebar", move || pixel(&w, &t, x, y) == sidebar);
            let (t, w) = (tree.clone(), window.clone());
            let (cx, cy) = (x + area.width() - 15, y + area.height() / 2);
            pump_until("selection tint", move || {
                let p = pixel(&w, &t, cx, cy);
                let close = |a: u8, b: u8| a.abs_diff(b) <= 1;
                close(p.0, tint.0) && close(p.1, tint.1) && close(p.2, tint.2)
            });
            assert!(!impro_explorer::search_css_for(&theme.ui).contains("transition:"));
        }
        destroy(&window);
    });
}

#[derive(Debug, Default)]
struct Attrs {
    absolute_size: Option<i32>,
    weight: Option<pango::Weight>,
    foregrounds: Vec<(u16, u16, u16)>,
}

fn attributes(list: &pango::AttrList) -> Attrs {
    let mut seen = Attrs::default();
    for attr in list.attributes() {
        match attr.type_() {
            pango::AttrType::AbsoluteSize => {
                seen.absolute_size = attr.downcast_ref::<pango::AttrSize>().map(|a| a.size());
            }
            pango::AttrType::Weight => {
                seen.weight = attr
                    .downcast_ref::<pango::AttrInt>()
                    .map(|a| pango::Weight::__Unknown(a.value()));
                seen.weight = seen.weight.map(|w| match w {
                    pango::Weight::__Unknown(v) => unsafe { glib::translate::from_glib(v) },
                    other => other,
                });
            }
            pango::AttrType::Foreground => {
                if let Some(color) = attr.downcast_ref::<pango::AttrColor>().map(|a| a.color()) {
                    seen.foregrounds
                        .push((color.red(), color.green(), color.blue()));
                }
            }
            _ => {}
        }
    }
    seen
}

#[test]
fn test_group_and_match_attributes_follow_live_theme_switches() {
    run(|| {
        let mut f = setup();
        let window = show_styled_window(&mut f);
        f.panel.search_now(Some("<needle>"));
        wait_scan(&f.panel);
        let column = f.panel.tree().column(0).unwrap();
        let cell = f.panel.cells()[1].clone();
        let store = f.panel.store();
        for theme in themes::themes() {
            impro_ui::apply_theme(theme, None, None, None).unwrap();
            // Resolving a style query and cell data also works before paint.
            let group = store.iter_from_string("0").unwrap();
            let matched = store.iter_from_string("0:0").unwrap();
            column.cell_set_cell_data(&store, &group, false, false);
            let attrs: pango::AttrList = cell.property("attributes");
            let seen = attributes(&attrs);
            assert!(
                seen.absolute_size == Some(12288),
                "{}: {:?}",
                theme.id,
                seen
            );
            assert!(
                seen.weight == Some(pango::Weight::Semibold),
                "{}: {:?}",
                theme.id,
                seen
            );
            let faint = pango::Color::parse(&theme.ui["faint"]).unwrap();
            assert!(
                seen.foregrounds
                    .contains(&(faint.red(), faint.green(), faint.blue())),
                "{}: {:?}",
                theme.id,
                seen
            );
            column.cell_set_cell_data(&store, &matched, false, false);
            let attrs: pango::AttrList = cell.property("attributes");
            let seen = attributes(&attrs);
            let accent = pango::Color::parse(&theme.ui["accent_text"]).unwrap();
            assert!(
                seen.foregrounds
                    .contains(&(accent.red(), accent.green(), accent.blue())),
                "{}: {:?}",
                theme.id,
                seen
            );
            assert!(seen.absolute_size.is_none(), "{:?}", seen);
            let shown: String = cell.property("text");
            assert_eq!(shown, "<needle> & more");
        }
        destroy(&window);
    });
}

#[test]
fn test_search_toggles_have_compact_geometry_and_accessible_names() {
    run(|| {
        let mut f = setup();
        let window = show_styled_window(&mut f);
        for (button, name) in [
            (f.panel.case_button(), "Match case"),
            (f.panel.regex_button(), "Use regular expression"),
        ] {
            assert_eq!(
                atk::prelude::AtkObjectExt::name(&button.accessible().unwrap()).as_deref(),
                Some(name)
            );
            assert!(button.allocated_height() <= 30);
            assert!(button.allocated_width() <= 30);
            button.set_active(true);
        }
        destroy(&window);
    });
}

#[test]
fn test_many_file_names_render_in_exact_order_after_the_content_groups() {
    run(|| {
        let f = setup();
        for i in 0..1500 {
            f.files
                .write(&format!("names/item_{i:04}.txt"), "unrelated");
        }
        f.panel.search_now(Some("item_"));
        wait_scan(&f.panel);
        let results = f.panel.results();
        assert_eq!(results.file_names.len(), 1500);
        assert!(results.groups.is_empty());
        let store = f.panel.store();
        let group = store.iter_first().unwrap();
        assert_eq!(
            impro_explorer::util::string(&store, &group, 2),
            "<b>File names</b>"
        );
        assert_eq!(store.iter_n_children(Some(&group)), 1500);
        let rows: Vec<String> = (0..1500)
            .map(|i| {
                let child = store.iter_nth_child(Some(&group), i).unwrap();
                impro_explorer::util::string(&store, &child, 3)
            })
            .collect();
        let expected: Vec<String> = results
            .file_names
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        assert_eq!(rows, expected);
        assert!(f
            .panel
            .tree()
            .row_expanded(&gtk::TreePath::from_indicesv(&[0])));
        assert_eq!(f.panel.summary().text(), "1500 file name results");
    });
}

#[test]
fn test_binary_files_are_rejected_without_reading_them_whole() {
    let f = Files::new();
    // A megabyte of data with a NUL in the first block: rejected by the leading check.
    let mut binary = vec![b'x'; MAX_FILE_SIZE];
    binary[100] = 0;
    let path = f.write("blob.bin", binary);
    assert_eq!(read_text(&path), None);
    // A NUL after the first 8 KiB is tolerated, as in the original.
    let mut late = vec![b'y'; 20_000];
    late[15_000] = 0;
    let path = f.write("late.dat", late);
    assert!(read_text(&path).is_some());
    assert_eq!(read_text(&f.write("empty", "")), Some(String::new()));
}

#[test]
fn test_incremental_render_appends_without_clearing_and_keeps_file_names_last() {
    run(|| {
        let f = setup();
        for i in 0..30 {
            f.files
                .write(&format!("many/f{i:02}.txt"), "needle one\nneedle two");
        }
        f.panel.search_now(Some("needle"));
        wait_scan(&f.panel);
        let groups = row_count(&f.panel);
        assert_eq!(groups, 33);
        let last = row_text(&f.panel, &(groups - 1).to_string(), 7);
        assert_eq!(last, "group");
        assert_eq!(
            row_text(&f.panel, &(groups - 1).to_string(), 2),
            "<b>File names</b>"
        );
        assert!(f.panel.summary().text().contains("results in"));
    });
}
