mod support;
use impro_gitviews::{
    diff::{self, Highlighter, Limits},
    log,
};
use serde_json::{json, Value};
#[cfg(unix)]
use std::fs;
use std::time::{Duration, Instant};
use support::*;
fn detail(r: &GitRepo, rev: &str, limits: Limits) -> Value {
    let d = diff::commit_detail(&r.path, rev, limits, None);
    assert_eq!(d["ok"], true, "{d}");
    d
}
fn head(r: &GitRepo) -> Value {
    detail(r, "HEAD", Limits::default())
}
fn file<'a>(d: &'a Value, path: &str) -> &'a Value {
    d["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == path)
        .unwrap()
}
fn line_texts(f: &Value) -> Vec<&str> {
    f["hunks"][0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["text"].as_str().unwrap())
        .collect()
}
#[test]
fn test_headers_section_counts_markers_tabs_and_crlf() {
    let data=b"diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -4,2 +4,3 @@ def example():\n kept\r\n-\told\r\n+\tnew\r\n+last\n\\ No newline at end of file\n@@ -10 +11 @@ other\n-old\n+new\n";
    let (h, large, cut) = diff::parse_patch(data, 3000);
    assert!(!large && !cut);
    assert_eq!(h.len(), 2);
    for (k, n) in [
        ("old_start", 4),
        ("old_lines", 2),
        ("new_start", 4),
        ("new_lines", 3),
    ] {
        assert_eq!(h[0][k], n);
    }
    assert_eq!(h[0]["section"], "def example():");
    let coords: Vec<_> = h[0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| json!([l["t"], l["o"], l["n"]]))
        .collect();
    assert_eq!(
        coords,
        vec![
            json!([" ", 4, 4]),
            json!(["-", 5, null]),
            json!(["+", null, 5]),
            json!(["+", null, 6]),
            json!(["\\", null, null])
        ]
    );
    assert_eq!(h[0]["lines"][1]["text"], "\told\r");
    assert_eq!(h[0]["lines"][4]["text"], "\\ No newline at end of file");
    assert_eq!(h[1]["old_lines"], 1);
    assert_eq!(h[1]["new_lines"], 1);
    assert!(diff::parse_patch(data, 2).1);
}
#[test]
fn test_long_lines_and_controls() {
    let (h, large, cut) = diff::parse_patch(
        format!("@@ -0,0 +1 @@\n+{}\u{1b}\n", "x".repeat(3000)).as_bytes(),
        3000,
    );
    assert!(cut && !large);
    let l = &h[0]["lines"][0];
    assert_eq!(l["cut"], true);
    let s = l["text"].as_str().unwrap();
    assert_eq!(s.chars().count(), 2000);
    assert!(s.ends_with('…'));
    assert!(!s.contains('\u{1b}'));
}
#[test]
fn test_root_metadata_body_huge_message_and_refs() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    let body = "Many details 🌍\n".repeat(30000).trim().to_owned();
    let sha = r.commit_as(
        &[("a.py", Some(b"def hello():\n    return \"world\"\n"))],
        &format!("Subject\n\n{body}"),
        Some("2025-01-01T12:00:00-03:30"),
        Some(("名", "name@example.test")),
    );
    r.tag("release", true);
    let d = detail(&r, "release", Limits::default());
    assert_eq!(d["hash"], sha);
    assert_eq!(d["subject"], "Subject");
    assert_eq!(d["body"], body);
    assert_eq!(
        d["author"],
        json!({"name":"名","email":"name@example.test","ts":d["author"]["ts"],"tz":-210})
    );
    assert_eq!(d["committer"]["name"], "Grace Example");
    assert_eq!(d["committer"]["tz"], -210);
    assert_eq!(d["parents"], json!([]));
    assert!(d["diff_base"].is_null());
    assert_eq!(d["merge"], false);
    assert_eq!(d["stats"], json!({"files":1,"additions":2,"deletions":0}));
    assert_eq!(
        strings(&d["refs"], "name"),
        ["master", "release"].map(str::to_owned).into()
    );
    for rev in [sha.as_str(), &sha[..9], "master", "HEAD"] {
        assert_eq!(detail(&r, rev, Limits::default())["hash"], sha);
    }
}
#[test]
fn test_metadata_control_sanitising() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    r.commit_as(
        &[("a.txt", Some(b"hello\x1b world\n"))],
        "Subject\u{1b}\n\nBody\u{1}\n\tformatted",
        None,
        Some(("Author\u{1}", "mail@example.test")),
    );
    let d = head(&r);
    assert_eq!(d["subject"], "Subject");
    assert_eq!(d["body"], "Body\n\tformatted");
    assert_eq!(d["author"]["name"], "Author");
    assert_eq!(d["files"][0]["hunks"][0]["lines"][0]["text"], "hello world");
}
#[test]
fn test_empty_commit_and_empty_file() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    r.commit(&[("empty", Some(b""))], "Empty file");
    let d = head(&r);
    assert_eq!(d["files"][0]["status"], "added");
    assert_eq!(d["files"][0]["hunks"], json!([]));
    assert_eq!(d["files"][0]["too_large"], false);
    r.commit(&[], "Empty commit");
    let d = head(&r);
    assert_eq!(d["files"], json!([]));
    assert_eq!(d["stats"], json!({"files":0,"additions":0,"deletions":0}));
}
#[cfg(unix)]
#[test]
fn test_statuses_renames_copies_typechange_mode_binary_and_delete() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    let content = (0..30)
        .map(|i| format!("line {i} with enough content for similarity\n"))
        .collect::<String>();
    r.commit(
        &[
            ("source.txt", Some(content.as_bytes())),
            (
                "rename.txt",
                Some(content.replace("line", "entry").as_bytes()),
            ),
            ("modified.py", Some(b"old = 1\n")),
            ("deleted.txt", Some(b"gone\n")),
            ("type.txt", Some(b"regular\n")),
            ("mode.sh", Some(b"echo hello\n")),
            ("z-after-type.txt", Some(b"before\n")),
        ],
        "Commit",
    );
    fs::remove_file(t.0.join("type.txt")).unwrap();
    symlink("source.txt", t.0.join("type.txt")).unwrap();
    fs::set_permissions(t.0.join("mode.sh"), fs::Permissions::from_mode(0o755)).unwrap();
    r.commit(
        &[
            ("copy.txt", Some(content.as_bytes())),
            ("rename.txt", None),
            (
                "renamed.txt",
                Some(
                    content
                        .replace("line", "entry")
                        .replace("entry 0 ", "changed 0 ")
                        .as_bytes(),
                ),
            ),
            ("modified.py", Some(b"new = 2\n")),
            ("deleted.txt", None),
            ("added.txt", Some(b"new\n")),
            ("mode.sh", Some(b"echo hello\n")),
            ("image.png", Some(b"\x89PNG\0binary\n")),
            ("z-after-type.txt", Some(b"after\n")),
        ],
        "Commit",
    );
    let d = head(&r);
    for (p, s) in [
        ("copy.txt", "copied"),
        ("renamed.txt", "renamed"),
        ("modified.py", "modified"),
        ("deleted.txt", "deleted"),
        ("added.txt", "added"),
        ("type.txt", "typechange"),
    ] {
        assert_eq!(file(&d, p)["status"], s);
    }
    assert_eq!(file(&d, "copy.txt")["old_path"], "source.txt");
    assert_eq!(file(&d, "copy.txt")["similarity"], 100);
    assert_eq!(file(&d, "renamed.txt")["old_path"], "rename.txt");
    assert!(file(&d, "renamed.txt")["similarity"].as_i64().unwrap() > 90);
    assert_eq!(file(&d, "type.txt")["hunks"].as_array().unwrap().len(), 2);
    let link = diff::file_context(&t.0, "HEAD", "type.txt", 1, 2, None);
    assert_eq!(link["ok"], true);
    assert_eq!(link["lines"][0]["text"], "source.txt");
    assert_eq!(
        line_texts(file(&d, "z-after-type.txt")),
        ["before", "after"]
    );
    assert_eq!(
        file(&d, "mode.sh")["mode_change"],
        json!(["100644", "100755"])
    );
    assert_eq!(file(&d, "mode.sh")["hunks"], json!([]));
    assert_eq!(file(&d, "image.png")["binary"], true);
    assert_eq!(file(&d, "image.png")["hunks"], json!([]));
    assert_eq!(
        d["stats"]["files"].as_u64().unwrap() as usize,
        d["files"].as_array().unwrap().len()
    );
    let expected = r.git(&[
        "diff",
        "--numstat",
        "-M",
        "-C",
        "--find-copies-harder",
        "HEAD~1",
        "HEAD",
    ]);
    assert_eq!(
        d["stats"]["additions"],
        expected
            .lines()
            .filter(|l| !l.starts_with('-'))
            .map(|l| l.split('\t').next().unwrap().parse::<i64>().unwrap())
            .sum::<i64>()
    );
}
#[test]
fn test_machine_paths_spaces_unicode_quotes_newlines_tabs_and_leading_dash() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    // Windows rejects `"`, `:`, `\`, newline and tab in file names: use look-alikes there.
    #[cfg(unix)]
    let (names, renamed) = (
        [
            "Привет мир.py",
            "a \"quoted\" file.txt",
            "line\nbreak.py",
            "tab\tname.py",
            "-option.py",
            "a:b.py",
            "literal[1].py",
            "back\\slash.py",
        ],
        "renamed\nnew.py",
    );
    #[cfg(windows)]
    let (names, renamed) = (
        [
            "Привет мир.py",
            "a \u{201c}quoted\u{201d} file.txt",
            "line\u{240a}break.py",
            "tab\u{2409}name.py",
            "-option.py",
            "a\u{a789}b.py",
            "literal[1].py",
            "back\u{ff3c}slash.py",
        ],
        "renamed\u{240a}new.py",
    );
    r.commit(
        &names
            .iter()
            .map(|n| (*n, Some("value = \"🌍\"\n".as_bytes())))
            .collect::<Vec<_>>(),
        "Commit",
    );
    let d = head(&r);
    assert_eq!(
        strings(&d["files"], "path"),
        names.map(str::to_owned).into()
    );
    for f in d["files"].as_array().unwrap() {
        assert_eq!(f["hunks"][0]["lines"][0]["text"], "value = \"🌍\"");
        let context = diff::file_context(&t.0, "HEAD", f["path"].as_str().unwrap(), 1, 2, None);
        assert_eq!(context["ok"], true, "{context}");
        assert_eq!(context["lines"][0]["text"], "value = \"🌍\"");
    }
    r.commit(
        &[
            (names[2], None),
            (renamed, Some("value = \"🌍\"\n".as_bytes())),
        ],
        "Commit",
    );
    let d = head(&r);
    assert_eq!(file(&d, renamed)["old_path"], names[2]);
    assert_eq!(file(&d, renamed)["status"], "renamed");
}
#[test]
fn test_first_parent_merge_and_octopus() {
    let t = Temp::new();
    let mut r = octopus(&t.0.join("octopus"));
    let d = head(&r);
    assert_eq!(d["merge"], true);
    assert_eq!(d["parents"].as_array().unwrap().len(), 3);
    assert_eq!(d["diff_base"], r.git(&["rev-parse", "HEAD^1"]));
    assert_eq!(
        strings(&d["files"], "path"),
        ["feature/auth.py", "feature/api.py"]
            .map(str::to_owned)
            .into()
    );
    r.checkout("topic", true);
    r.commit(&[("side.txt", Some(b"side\n"))], "Commit");
    r.checkout("master", false);
    r.commit(&[("main-only.txt", Some(b"main\n"))], "Commit");
    r.merge(&["topic"], "Merge branch 'topic'");
    assert_eq!(
        strings(&head(&r)["files"], "path"),
        ["side.txt"].map(str::to_owned).into()
    );
}
#[test]
fn test_crlf_no_newline_multi_hunks_and_sections() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    let mut lines = vec!["def first():\n".to_owned(), "\treturn 1\n".into()];
    lines.extend((0..25).map(|i| format!("# gap {i}\n")));
    lines.extend(["def last():\n".into(), "\treturn 2\n".into()]);
    r.commit(
        &[
            ("code.py", Some(lines.concat().as_bytes())),
            ("crlf.txt", Some(b"one\r\ntwo\r\n")),
            ("no-final.txt", Some(b"old")),
        ],
        "Commit",
    );
    lines[1] = "\treturn 3\n".into();
    *lines.last_mut().unwrap() = "\treturn 4\n".into();
    r.commit(
        &[
            ("code.py", Some(lines.concat().as_bytes())),
            ("crlf.txt", Some(b"one\r\nchanged\r\n")),
            ("no-final.txt", Some(b"new")),
        ],
        "Commit",
    );
    let d = head(&r);
    let h = file(&d, "code.py")["hunks"].as_array().unwrap();
    assert_eq!(h.len(), 2);
    assert!(!h[1]["section"].as_str().unwrap().is_empty());
    assert!(h.iter().any(|h| h["lines"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["text"].as_str().unwrap().starts_with('\t'))));
    assert_eq!(
        line_texts(file(&d, "crlf.txt")),
        ["one\r", "two\r", "changed\r"]
    );
    let markers: Vec<_> = file(&d, "no-final.txt")["hunks"][0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l["t"] == "\\")
        .collect();
    assert_eq!(markers.len(), 2);
    assert!(markers
        .iter()
        .all(|l| l["o"].is_null() && l["n"].is_null() && l["k"] == json!([])));
}
#[test]
fn test_caps_keep_true_counts() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    let content = (0..100).map(|i| format!("x = {i}\n")).collect::<String>();
    let names: Vec<_> = (0..4).map(|i| format!("{i}.py")).collect();
    r.commit(
        &names
            .iter()
            .map(|n| (n.as_str(), Some(content.as_bytes())))
            .collect::<Vec<_>>(),
        "Commit",
    );
    let full = head(&r);
    assert_eq!(full["truncated"], false);
    for limits in [
        Limits {
            max_files: 2,
            ..Limits::default()
        },
        Limits {
            max_files: 0,
            ..Limits::default()
        },
        Limits {
            max_lines_per_file: 3,
            ..Limits::default()
        },
        Limits {
            max_bytes: 100,
            ..Limits::default()
        },
        Limits {
            max_bytes: 0,
            ..Limits::default()
        },
    ] {
        let d = detail(&r, "HEAD", limits);
        assert_eq!(d["stats"], full["stats"]);
        assert_eq!(d["truncated"], true, "{d}");
        let files = d["files"].as_array().unwrap();
        if limits.max_files != 300 {
            assert_eq!(files.len(), limits.max_files as usize);
        } else {
            assert!(files.iter().any(|f| f["too_large"] == true));
            assert!(files
                .iter()
                .filter(|f| f["too_large"] == true)
                .all(|f| f["hunks"] == json!([])));
        }
    }
    assert_eq!(
        full["stats"],
        json!({"files":4,"additions":400,"deletions":0})
    );
}
#[test]
fn test_long_line_cap_sets_marker_and_bounds_runs() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    r.commit(
        &[(
            "long.py",
            Some(format!("value = \"{}\"\n", "x".repeat(5000)).as_bytes()),
        )],
        "Commit",
    );
    let d = head(&r);
    let l = &d["files"][0]["hunks"][0]["lines"][0];
    assert_eq!(l["text"].as_str().unwrap().chars().count(), 2000);
    assert_eq!(l["cut"], true);
    assert_eq!(d["truncated"], true);
    assert!(
        l["k"]
            .as_array()
            .unwrap()
            .iter()
            .all(|k| k[0].as_u64().unwrap() < k[1].as_u64().unwrap()
                && k[1].as_u64().unwrap() <= 2000)
    );
}
#[test]
fn test_unknown_revisions_git_missing_and_injections() {
    isolated(
        "test_unknown_revisions_git_missing_and_injections",
        |path| {
            let mut r = GitRepo::new(path);
            r.commit(&[("safe.txt", Some(b"safe"))], "Commit");
            for rev in [
                "missing",
                "--output=owned",
                "-x",
                ";",
                "HEAD;touch owned",
                "HEAD\n--output=owned",
                "",
                "HEAD\0x",
            ] {
                let d = diff::commit_detail(path, rev, Limits::default(), None);
                assert_eq!(d["ok"], false);
                assert_eq!(d["error"], "unknown-revision");
            }
            assert!(!path.join("owned").exists());
            assert_eq!(
                diff::commit_detail(&path.join("missing"), "HEAD", Limits::default(), None)
                    ["error"],
                "not-a-repo"
            );
            std::env::set_var(
                "GIT_CEILING_DIRECTORIES",
                path.parent().unwrap().parent().unwrap(),
            );
            assert_eq!(
                diff::commit_detail(path.parent().unwrap(), "HEAD", Limits::default(), None)
                    ["error"],
                "not-a-repo"
            );
            // A script as the fake `git` is Unix-only.
            #[cfg(unix)]
            {
                let fake = path.join("fake");
                fs::create_dir(&fake).unwrap();
                executable(
                    &fake.join("git"),
                    "#!/usr/bin/python3\nimport time\ntime.sleep(10)\n",
                );
                std::env::set_var("PATH", &fake);
                assert_eq!(
                    diff::commit_detail(path, "HEAD", Limits::default(), None)["error"],
                    "timeout"
                );
            }
            std::env::set_var("PATH", "");
            assert_eq!(
                diff::commit_detail(path, "HEAD", Limits::default(), None)["error"],
                "git-missing"
            );
        },
    );
}
#[test]
fn test_context_clamp_missing_binary_and_traversal() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    let content = (0..400).map(|i| format!("x = {i}\n")).collect::<String>();
    r.commit(
        &[
            ("large.py", Some(content.as_bytes())),
            ("binary", Some(b"\0binary")),
            ("empty", Some(b"")),
        ],
        "Commit",
    );
    let c = |path, start, end| diff::file_context(&t.0, "HEAD", path, start, end, None);
    let d = c("large.py", -10, 50000);
    assert_eq!(d["ok"], true);
    assert_eq!(
        (
            d["start"].as_i64(),
            d["end"].as_i64(),
            d["lines"].as_array().unwrap().len()
        ),
        (Some(1), Some(200), 200)
    );
    let d = c("large.py", 398, 900);
    assert_eq!(
        (
            d["start"].as_i64(),
            d["end"].as_i64(),
            d["lines"].as_array().unwrap().len()
        ),
        (Some(398), Some(400), 3)
    );
    assert_eq!(d["lines"][0]["text"], "x = 397");
    assert_eq!(d["lines"][2]["n"], 400);
    assert_eq!(c("empty", 1, 5)["ok"], true);
    assert_eq!(c("large.py", 900, 1000)["lines"], json!([]));
    for path in [
        "../large.py",
        "/etc/passwd",
        "a/../large.py",
        "missing",
        "binary",
        "./large.py",
        "large.py\0",
    ] {
        assert_eq!(c(path, 1, 2)["ok"], false);
    }
    assert_eq!(
        diff::file_context(&t.0, "-x", "large.py", 1, 2, None)["ok"],
        false
    );
}
#[test]
fn test_submodule_changes_without_external_access() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    let first = r.commit(&[("a.txt", Some(b"a\n"))], "Commit");
    let second = r.commit(&[("a.txt", Some(b"b\n"))], "Commit");
    r.git(&[
        "update-index",
        "--add",
        "--cacheinfo",
        &format!("160000,{first},vendor/module"),
    ]);
    let env = r.dated(None);
    r.command(&["commit", "-m", "Add submodule"], None, &env);
    let d = head(&r);
    assert_eq!(d["files"][0]["path"], "vendor/module");
    assert_eq!(d["files"][0]["mode_change"], json!(["000000", "160000"]));
    r.git(&[
        "update-index",
        "--cacheinfo",
        &format!("160000,{second},vendor/module"),
    ]);
    let env = r.dated(None);
    r.command(&["commit", "-m", "Update submodule"], None, &env);
    let d = head(&r);
    assert_eq!(d["files"][0]["status"], "modified");
    assert_eq!(d["files"][0]["additions"], 1);
    assert_eq!(d["files"][0]["deletions"], 1);
    assert_eq!(
        diff::file_context(&t.0, "HEAD", "vendor/module", 1, 2, None)["ok"],
        false
    );
}
struct Unavailable;
impl Highlighter for Unavailable {
    fn language(&self, _: &str) -> Option<String> {
        None
    }
    fn classify(&self, _: &str, _: &str) -> Result<Vec<(usize, usize, String)>, String> {
        panic!("unavailable highlighter called")
    }
}
#[test]
fn test_missing_highlight_import_and_available_flag() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    r.commit(
        &[("a.py", Some(b"def hello():\n    return \"world\"\n"))],
        "Commit",
    );
    for h in [None, Some(&Unavailable as &dyn Highlighter)] {
        for _ in 0..2 {
            let d = diff::commit_detail(&t.0, "HEAD", Limits::default(), h);
            assert!(d["files"][0]["language"].is_null());
            assert!(d["files"][0]["hunks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|h| h["lines"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|l| l["k"] == json!([]))));
        }
    }
}
/// A deterministic test adapter exercises multiline classification and Unicode offsets.
struct Syntax;
impl Highlighter for Syntax {
    fn language(&self, _: &str) -> Option<String> {
        Some("Python".into())
    }
    fn classify(&self, _: &str, text: &str) -> Result<Vec<(usize, usize, String)>, String> {
        let mut runs = Vec::new();
        let mut offset = 0;
        let mut doc = false;
        for line in text.split('\n') {
            let length = line.chars().count();
            if line.contains("\"\"\"") {
                let was = doc;
                doc = !doc;
                if was || doc {
                    runs.push((offset, offset + length, "doc".into()));
                }
            } else if doc {
                runs.push((offset, offset + length, "doc".into()));
            } else {
                for word in ["def", "return"] {
                    if let Some(i) = line.find(word) {
                        runs.push((
                            offset + line[..i].chars().count(),
                            offset + line[..i].chars().count() + word.len(),
                            "keyword".into(),
                        ));
                    }
                }
                if let Some(i) = line.find('"') {
                    runs.push((
                        offset + line[..i].chars().count(),
                        offset + length,
                        "string".into(),
                    ));
                }
            }
            offset += length + 1;
        }
        Ok(runs)
    }
}
#[test]
fn test_syntax_runs_per_hunk_per_side_and_context_new_side() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    r.commit(&[("a.py",Some(b"\"\"\"old opening\ncontext inside\nold ending\"\"\"\ndef hello():\n    return \"world\"\n"))],"Commit");
    r.commit(&[("a.py",Some(b"# new opening\ncontext inside\n# new ending\ndef hello():\n    return \"world\"\n"))],"Commit");
    let d = diff::commit_detail(&t.0, "HEAD", Limits::default(), Some(&Syntax));
    let f = &d["files"][0];
    assert_eq!(f["language"], "Python");
    let lines = f["hunks"][0]["lines"].as_array().unwrap();
    let context = lines
        .iter()
        .find(|l| l["text"] == "context inside" && l["t"] == " ")
        .unwrap();
    assert!(!context["k"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k[2] == "doc"));
    let removed = lines
        .iter()
        .find(|l| l["t"] == "-" && l["text"] == "old ending\"\"\"")
        .unwrap();
    assert!(removed["k"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k[2] == "doc"));
    let mut kinds = std::collections::BTreeSet::new();
    for line in lines {
        for k in line["k"].as_array().unwrap() {
            let a = k[0].as_u64().unwrap();
            let b = k[1].as_u64().unwrap();
            assert!(a < b && b <= line["text"].as_str().unwrap().chars().count() as u64);
            assert!(["doc", "keyword", "string"].contains(&k[2].as_str().unwrap()));
            kinds.insert(k[2].as_str().unwrap());
        }
    }
    assert!(kinds.contains("keyword") && kinds.contains("string"));
    let c = diff::file_context(&t.0, "HEAD", "a.py", 4, 5, Some(&Syntax));
    assert!(c["lines"].as_array().unwrap().iter().any(|l| l["k"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k[2] == "string")));
}
#[test]
fn test_twenty_file_and_three_thousand_line_performance() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    let content = (0..3000)
        .map(|i| format!("value_{i} = \"item {i}\"\n"))
        .collect::<String>();
    let names: Vec<_> = (0..19).map(|i| format!("module_{i}.py")).collect();
    let mut files: Vec<_> = names
        .iter()
        .map(|n| {
            (
                n.as_str(),
                Some(b"def value():\n    return \"ready\"\n".as_slice()),
            )
        })
        .collect();
    files.push(("large.py", Some(content.as_bytes())));
    r.commit(&files, "Commit");
    let start = Instant::now();
    let d = diff::commit_detail(&r.path, "HEAD", Limits::default(), Some(&Syntax));
    assert_eq!(d["stats"]["files"], 20);
    assert_eq!(d["truncated"], false);
    assert!(start.elapsed() < Duration::from_millis(300));
}
#[test]
fn test_context_output_cap_and_invalid_utf8() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    r.commit(
        &[
            ("invalid.txt", Some(b"bad\xff\n")),
            ("huge", Some(&vec![b'x'; 8 * 1024 * 1024 + 1])),
        ],
        "Commit",
    );
    assert_eq!(
        diff::file_context(&t.0, "HEAD", "invalid.txt", 1, 2, None)["lines"][0]["text"],
        "bad�"
    );
    assert_eq!(
        diff::file_context(&t.0, "HEAD", "huge", 1, 2, None)["error"],
        "output-too-large"
    );
    assert_eq!(log::timezone_minutes("invalid"), 0);
}
/// `IMPRO_GIT_DEBUG=1` prints one stderr line per failed git run; unset it stays silent.
#[test]
fn test_git_debug_reports_failed_invocations_only_when_enabled() {
    const MARK: &str = "IMPRO_TEST_GIT_DEBUG_ROOT";
    if let Some(root) = std::env::var_os(MARK) {
        let r = GitRepo::new(std::path::Path::new(&root));
        let d = diff::commit_detail(&r.path, "HEAD", Limits::default(), None);
        assert_eq!(d["error"], "unknown-revision");
        return;
    }
    let run = |debug: Option<&str>| {
        let t = Temp::new();
        let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "test_git_debug_reports_failed_invocations_only_when_enabled",
            "--nocapture",
        ])
        .env(MARK, &t.0)
        .env_remove("IMPRO_GIT_DEBUG");
        if let Some(value) = debug {
            cmd.env("IMPRO_GIT_DEBUG", value);
        }
        let out = cmd.output().unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    let on = run(Some("1"));
    let line = on
        .lines()
        .find(|l| l.starts_with("impro: git failed: "))
        .unwrap_or_else(|| panic!("no diagnostic line in {on:?}"));
    assert!(line.contains("rev-parse"), "{line}");
    assert!(line.contains(" status=128 "), "{line}");
    assert!(line.contains(" stderr=fatal: "), "{line}");
    assert!(line.len() < 1000, "{line}");
    assert!(!run(None).contains("impro: git failed"));
    assert!(!run(Some("0")).contains("impro: git failed"));
}
