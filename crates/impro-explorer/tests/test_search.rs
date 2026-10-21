//! Search core and GTK wrapper contracts (run under X11/xvfb).
use impro_explorer::pool::CancelToken;
use impro_explorer::search::*;
use std::path::{Path, PathBuf};

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
    assert_eq!(found, [paths[3].clone(), paths[1].clone(), paths[2].clone(), paths[0].clone()]);
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
    assert_eq!(find_in_text("A.b aXb a.b", "a.b", true, false).unwrap().len(), 1);
    assert_eq!(find_in_text("A.b aXb a.b", "a.b", false, true).unwrap().len(), 3);
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
    assert_eq!((m.line, m.column, m.length), (1, column, query.chars().count()));
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
    assert_eq!(result.file_names, [name.clone()]);
    assert_eq!((result.total_matches, result.total_files), (3, 3));
    assert_eq!(f.search("NEEDLE", |o| o.case_sensitive = true).file_names, [name]);
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
    let provider = |p: &Path| Some(if p == unsaved { "needle needle" } else { "changed" }.to_owned());
    let result = search(f.root(), "needle", &options, Some(&provider), None).unwrap();
    let paths: Vec<_> = result.groups.iter().map(|g| g.path.clone()).collect();
    assert_eq!(paths, [unsaved]);
    assert_eq!(result.total_matches, 2);
    let none = |_: &Path| None;
    let result = search(f.root(), "needle", &SearchOptions::default(), Some(&none), None).unwrap();
    assert_eq!(result.groups[0].path, disk);
}

#[test]
fn test_per_file_and_total_limits() {
    let f = Files::new();
    f.write("a", "needle needle needle");
    f.write("b", "needle needle");
    let result = f.search("needle", |o| o.max_per_file = 2);
    assert_eq!(result.groups.iter().map(|g| g.matches.len()).collect::<Vec<_>>(), [2, 2]);
    assert!(result.truncated);
    let result = f.search("needle", |o| o.max_total = 3);
    assert_eq!(result.groups.iter().map(|g| g.matches.len()).collect::<Vec<_>>(), [3]);
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
    assert!(search(f.root(), "needle", &options, None, Some(&cancel)).unwrap().groups.is_empty());
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
    let result = search(Path::new("/does/not/exist"), "x", &SearchOptions::default(), None, None).unwrap();
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
    assert_eq!(names(true), [".dot", "a.txt", "b.txt", ".hidden/y.txt", "sub/c.txt"]);
    assert_eq!(list_project_files(f.root(), true, 2).len(), 2);
}
