//! Real repositories exercise porcelain parsing without importing GTK.
use impro_explorer::gitstatus::*;
use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

/// Tests swap `PATH`, so every test that runs Git takes this lock.
static ENV: Mutex<()> = Mutex::new(());

struct Repo {
    _lock: MutexGuard<'static, ()>,
    _temp: tempfile::TempDir,
    root: PathBuf,
}

impl Repo {
    fn new() -> Self {
        let lock = ENV.lock().unwrap_or_else(|e| e.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let repo = Repo {
            _lock: lock,
            _temp: temp,
            root,
        };
        repo.git(&["init", "-q", "--initial-branch=base"]);
        repo.write("clean.txt", "base\n");
        repo.git(&["add", "."]);
        repo.git(&["commit", "-qm", "base"]);
        repo
    }
    fn git(&self, args: &[&str]) -> std::process::Output {
        self.git_checked(args, true)
    }
    fn git_checked(&self, args: &[&str], check: bool) -> std::process::Output {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(!check || output.status.success(), "git {args:?} failed");
        output
    }
    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.root.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
        path
    }
    fn write_new(&self, name: &str) -> PathBuf {
        self.write(name, "new\n")
    }
    fn snapshot(&self) -> Snapshot {
        collect_status(&self.root, MAX_ENTRIES)
    }
    fn snapshot_of(&self, root: &Path) -> Snapshot {
        collect_status(root, MAX_ENTRIES)
    }
    fn p(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

fn map(pairs: &[(&Path, Status)]) -> HashMap<PathBuf, Status> {
    pairs.iter().map(|(p, s)| (p.to_path_buf(), *s)).collect()
}

#[test]
fn test_untracked_staged_and_unchanged() {
    let r = Repo::new();
    let new = r.write_new("new.txt");
    assert_eq!(r.snapshot().files, map(&[(&new, Status::NEW)]));
    r.git(&["add", "new.txt"]);
    assert_eq!(r.snapshot().files, map(&[(&new, Status::NEW)]));
    assert_eq!(r.snapshot().folders[&r.root], Status::NEW);
}

#[test]
fn test_modified_staged_and_modified_again() {
    let r = Repo::new();
    let path = r.write("clean.txt", "changed\n");
    assert_eq!(r.snapshot().files[&path], Status::MODIFIED);
    r.git(&["add", "clean.txt"]);
    assert_eq!(r.snapshot().files[&path], Status::MODIFIED);
    r.write("clean.txt", "changed again\n");
    assert_eq!(r.snapshot().files[&path], Status::MODIFIED);
    let porcelain = r.git(&["status", "--porcelain=v1"]).stdout;
    assert!(String::from_utf8_lossy(&porcelain).contains("MM clean.txt"));
}

#[test]
fn test_added_then_modified_is_modified() {
    let r = Repo::new();
    let path = r.write_new("new.txt");
    r.git(&["add", "new.txt"]);
    r.write("new.txt", "edited after staging");
    assert_eq!(r.snapshot().files[&path], Status::MODIFIED);
}

#[test]
fn test_rename_destination_and_old_folder() {
    let r = Repo::new();
    r.write_new("old/file name.txt");
    r.git(&["add", "."]);
    r.git(&["commit", "-qm", "nested"]);
    std::fs::create_dir(r.p("new")).unwrap();
    r.git(&["mv", "old/file name.txt", "new/rénamed file.txt"]);
    let snapshot = r.snapshot();
    assert_eq!(
        snapshot.files[&r.p("new/rénamed file.txt")],
        Status::MODIFIED
    );
    assert!(!snapshot.files.contains_key(&r.p("old/file name.txt")));
    assert_eq!(snapshot.folders[&r.p("old")], Status::MODIFIED);
    assert_eq!(snapshot.folders[&r.p("new")], Status::MODIFIED);
    assert_eq!(
        r.snapshot_of(&r.p("old")).folders[&r.p("old")],
        Status::MODIFIED
    );
}

#[test]
fn test_deleted_only_decorates_parents() {
    let r = Repo::new();
    r.git(&["rm", "clean.txt"]);
    let snapshot = r.snapshot();
    assert!(snapshot.files.is_empty());
    assert_eq!(snapshot.folders, map(&[(&r.root, Status::MODIFIED)]));
}

#[test]
fn test_unicode_spaces_newlines_and_nested_aggregate() {
    let r = Repo::new();
    let path = r.write_new("outer/inner/你好 spaced\nname.txt");
    let snapshot = r.snapshot();
    assert_eq!(snapshot.files[&path], Status::NEW);
    for folder in ["", "outer", "outer/inner"] {
        assert_eq!(
            snapshot.folders[&r.root.join(folder)],
            Status::NEW,
            "{folder}"
        );
    }
    r.write("clean.txt", "modified");
    assert_eq!(r.snapshot().folders[&r.root], Status::MODIFIED);
}

#[test]
fn test_explorer_subfolder_filters_siblings() {
    let r = Repo::new();
    let path = r.write_new("sub/nested/a.txt");
    r.write_new("outside.txt");
    let snapshot = r.snapshot_of(&r.p("sub"));
    assert_eq!(snapshot.files, map(&[(&path, Status::NEW)]));
    let folders: std::collections::HashSet<_> = snapshot.folders.keys().cloned().collect();
    assert_eq!(
        folders,
        [r.p("sub"), r.p("sub/nested")].into_iter().collect()
    );
}

#[test]
fn test_real_merge_conflict_overrides_modified_and_new() {
    let r = Repo::new();
    r.git(&["checkout", "-qb", "other"]);
    r.write("clean.txt", "other\n");
    r.git(&["commit", "-qam", "other"]);
    r.git(&["checkout", "-q", "base"]);
    r.write("clean.txt", "base change\n");
    r.git(&["commit", "-qam", "base change"]);
    assert!(!r.git_checked(&["merge", "other"], false).status.success());
    r.write_new("untracked.txt");
    let snapshot = r.snapshot();
    assert_eq!(snapshot.files[&r.p("clean.txt")], Status::CONFLICT);
    assert_eq!(snapshot.folders[&r.root], Status::CONFLICT);
}

#[test]
fn test_all_conflict_codes_typechange_copy_and_unstaged_rename() {
    let r = Repo::new();
    let root = r.root.as_path();
    for code in ["UU", "AA", "DD", "AU", "UA", "DU", "UD"] {
        let result = parse_porcelain(format!("{code} f\0").as_bytes(), root, root, MAX_ENTRIES);
        assert_eq!(result.files[&r.p("f")], Status::CONFLICT, "{code}");
    }
    for code in [" T", "T ", " C", " R"] {
        let extra = if matches!(code.trim(), "C" | "R") {
            "old\0"
        } else {
            ""
        };
        let data = format!("{code} f\0{extra}");
        let result = parse_porcelain(data.as_bytes(), root, root, MAX_ENTRIES);
        assert_eq!(result.files[&r.p("f")], Status::MODIFIED, "{code:?}");
    }
}

#[test]
fn test_no_repository_missing_binary_and_timeout() {
    let r = Repo::new();
    let outside = tempfile::tempdir().unwrap();
    assert!(collect_status(outside.path(), MAX_ENTRIES).files.is_empty());
    assert!(collect_status(outside.path(), MAX_ENTRIES)
        .git_dir
        .is_none());
    let saved = std::env::var_os("PATH");
    std::env::set_var("PATH", "");
    let missing = r.snapshot();
    match saved {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    assert!(missing.folders.is_empty());
    assert!(missing.git_dir.is_none());
    // An already elapsed deadline behaves like a timeout.
    let limits = Limits {
        timeout: Duration::ZERO,
        ..Limits::default()
    };
    assert!(collect_status_with(&r.root, &limits, None).files.is_empty());
}

#[test]
fn test_status_caps_real_repo_and_large_parser_input() {
    let r = Repo::new();
    for i in 0..35 {
        r.write_new(&format!("file-{i}"));
    }
    let result = collect_status(&r.root, 10);
    assert_eq!(result.files.len(), 10);
    assert!(result.truncated);
    let data: Vec<u8> = (0..=MAX_ENTRIES)
        .flat_map(|i| format!("?? file-{i}\0").into_bytes())
        .collect();
    let result = parse_porcelain(&data, &r.root, &r.root, MAX_ENTRIES);
    assert_eq!(result.files.len(), MAX_ENTRIES);
    assert!(result.truncated);
}

#[test]
fn test_ignored_files_stay_undecorated() {
    let r = Repo::new();
    r.write(".gitignore", "*.ignored\n");
    let path = r.write_new("file.ignored");
    assert!(!r.snapshot().files.contains_key(&path));
}

#[test]
fn test_partial_porcelain_record_is_discarded() {
    let r = Repo::new();
    let result = parse_porcelain(b"?? complete\0?? unfinished", &r.root, &r.root, MAX_ENTRIES);
    assert_eq!(result.files, map(&[(&r.p("complete"), Status::NEW)]));
}

#[cfg(unix)]
fn fake_git(dir: &Path, script: &str) {
    let executable = dir.join("git");
    std::fs::write(&executable, script).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn with_path<T>(dir: &Path, body: impl FnOnce() -> T) -> T {
    let saved = std::env::var_os("PATH");
    std::env::set_var("PATH", dir);
    let result = body();
    match saved {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    result
}

#[test]
#[cfg(unix)]
fn test_real_subprocess_timeout_and_output_bound() {
    let r = Repo::new();
    let binaries = tempfile::tempdir().unwrap();
    let pidfile = binaries.path().join("pid");
    fake_git(
        binaries.path(),
        &format!(
            "#!/bin/sh\necho $$ > '{}'\nexec /bin/sleep 60\n",
            pidfile.display()
        ),
    );
    let limits = Limits {
        timeout: Duration::from_millis(100),
        ..Limits::default()
    };
    let started = Instant::now();
    let files = with_path(binaries.path(), || {
        collect_status_with(&r.root, &limits, None).files
    });
    assert!(files.is_empty());
    assert!(started.elapsed() < Duration::from_secs(2));
    // The sleeping child must have been killed rather than left running.
    let pid = std::fs::read_to_string(&pidfile).unwrap();
    assert!(!Path::new("/proc").join(pid.trim()).exists());
    fake_git(
        binaries.path(),
        "#!/bin/sh\n/usr/bin/awk 'BEGIN { for (i = 0; i < 10000; i++) printf \"?? file\\0\" }'\n",
    );
    let output = with_path(binaries.path(), || {
        run(
            &r.root,
            &["status"],
            Instant::now() + Duration::from_secs(2),
            None,
            127,
            None,
        )
    })
    .unwrap();
    assert!(output.truncated);
    assert_eq!(output.data.len(), 127);
}

#[test]
#[cfg(unix)]
fn test_cancellation_stops_a_running_scan() {
    let r = Repo::new();
    let binaries = tempfile::tempdir().unwrap();
    fake_git(binaries.path(), "#!/bin/sh\nexec /bin/sleep 60\n");
    let cancel = impro_explorer::pool::CancelToken::new();
    let trigger = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        trigger.cancel();
    });
    let started = Instant::now();
    let snapshot = with_path(binaries.path(), || {
        collect_status_with(&r.root, &Limits::default(), Some(&cancel))
    });
    assert!(snapshot.files.is_empty());
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
#[cfg(unix)]
fn test_commands_use_readonly_environment_and_list_args() {
    let r = Repo::new();
    let binaries = tempfile::tempdir().unwrap();
    let log = binaries.path().join("calls.log");
    let real = String::from_utf8(
        Command::new("sh")
            .args(["-c", "command -v git"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    fake_git(
        binaries.path(),
        &format!(
            "#!/bin/sh\necho \"$LC_ALL $GIT_OPTIONAL_LOCKS $@\" >> '{}'\nexec {} \"$@\"\n",
            log.display(),
            real.trim()
        ),
    );
    r.write_new("x.txt");
    let snapshot = with_path(binaries.path(), || r.snapshot());
    assert!(snapshot.files.contains_key(&r.p("x.txt")));
    let calls = std::fs::read_to_string(&log).unwrap();
    let lines: Vec<_> = calls.lines().collect();
    assert_eq!(lines.len(), 3);
    for line in lines {
        assert!(line.starts_with("C 0 -C "), "{line}");
    }
}

#[test]
fn test_snapshot_diff_reports_only_changed_paths() {
    let a = Snapshot {
        files: map(&[
            (Path::new("/r/a"), Status::NEW),
            (Path::new("/r/b"), Status::MODIFIED),
        ]),
        folders: map(&[(Path::new("/r"), Status::MODIFIED)]),
        ..Snapshot::default()
    };
    let mut b = a.clone();
    assert!(a.diff(&b).is_empty());
    b.files.insert(PathBuf::from("/r/b"), Status::CONFLICT);
    b.files.remove(Path::new("/r/a"));
    b.folders.insert(PathBuf::from("/r"), Status::CONFLICT);
    let mut changed = a.diff(&b);
    changed.sort();
    assert_eq!(
        changed,
        [Path::new("/r"), Path::new("/r/a"), Path::new("/r/b")]
    );
}
