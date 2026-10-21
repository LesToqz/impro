mod support;
use impro_gitviews::{diff, log};
use serde_json::json;
use std::fs;
use std::time::{Duration, Instant};
use support::*;
#[test]
fn test_table() {
    for(subject,expected)in[
("Merge branch 'feature/ui'",Some("feature/ui")),("Merge branch 'feature/ui' into master",Some("feature/ui")),("Merge branch \"feature/quotes\"",Some("feature/quotes")),("Merge remote-tracking branch 'origin/feature/x'",Some("feature/x")),("Merge remote-tracking branch 'upstream/release' into master",Some("release")),("Merge tag 'v2.0'",Some("v2.0")),("Merge pull request #42 from acme/feature/auth",Some("feature/auth")),("Merge pull request #47 from acme/hotfix/critical\n",Some("hotfix/critical")),("Merge ceo/codex/pane-split: draggable dividers between the panes of both docks (terminal/terminal/graph), live resize, keyboard + double-click reset",Some("ceo/codex/pane-split")),("Merge ceo/codex/explorer-no-badges: git status shown by green/orange name colours only (U/M/A/R/! letters removed)",Some("ceo/codex/explorer-no-badges")),("Merge ceo/claude/graph-web: Graph view page (force engine, Canvas2D renderer, camera, interaction, controls panel) + node and WebKit tests",Some("ceo/claude/graph-web")),("Merge ceo/codex/terminal-startup: shells spawn on the final dock geometry (fixes the start-up cursor drift in UTF-8), early input buffered, UTF-8 start-up regression tests",Some("ceo/codex/terminal-startup")),("Ordinary change",None),("Merge something unknown",None),("Merge branch 'unterminated",None),("",None)]{assert_eq!(log::infer_merged_name(subject.as_bytes()).as_deref(),expected,"{subject}");}
    assert_eq!(log::infer_merged_name("x".repeat(20000).as_bytes()), None);
    assert_eq!(
        log::infer_merged_name(format!("Merge ceo/codex/large: {}", "x".repeat(20000)).as_bytes())
            .as_deref(),
        Some("ceo/codex/large")
    );
}
#[test]
fn test_sanitising_and_parser_without_git() {
    let fields = [
        "a".repeat(40),
        "aaaaaaa".into(),
        "".into(),
        format!("a\u{1f}\u{1b}{}", "🌍".repeat(400)),
        "name\u{7f}".repeat(40),
        "e\u{1}mail".repeat(40),
        "1234".into(),
        "2001-01-01 12:00:00 -0330".into(),
        "4567".into(),
    ];
    let c = &log::parse_log(format!("{}\0", fields.join("\0")).as_bytes())[0];
    assert_eq!(c["subject"].as_str().unwrap().chars().count(), 300);
    assert!(c["subject"].as_str().unwrap().ends_with('…'));
    assert!(!c["subject"].as_str().unwrap().contains('\u{1f}'));
    for k in ["author", "email"] {
        assert!(c[k].as_str().unwrap().chars().count() <= 120);
    }
    assert_eq!(c["tz"], -210);
    assert_eq!(log::clean(b"bad\xff", None, false, false), "bad�");
}
#[test]
fn test_linear_limit_refs_and_dirty() {
    let t = Temp::new();
    let mut r = linear(&t.0, 5);
    r.tag("lightweight", false);
    r.tag("annotated", true);
    fs::write(t.0.join("new file"), "hello").unwrap();
    fs::write(t.0.join("main.py"), "changed").unwrap();
    let g = log::collect_graph(&t.0, Some(2));
    assert_eq!(g["ok"], true, "{g}");
    assert_eq!(g["total"], 5);
    assert_eq!(g["commits"].as_array().unwrap().len(), 2);
    assert_eq!(g["truncated"], true);
    assert_eq!(g["dirty"], json!({"changed":2}));
    assert_eq!(g["repo"]["default_branch"], "master");
    assert_eq!(g["head"]["branch"], "master");
    assert_eq!(
        strings(&g["commits"][0]["refs"], "name"),
        ["master", "lightweight", "annotated"]
            .map(str::to_owned)
            .into()
    );
    topological(&g);
    assert!(
        !strings(&g["commits"], "hash").contains(g["commits"][1]["parents"][0].as_str().unwrap())
    );
}
#[test]
fn test_staged_rename_is_one_dirty_path() {
    let t = Temp::new();
    let r = linear(&t.0, 1);
    r.git(&["mv", "main.py", "renamed.py"]);
    assert_eq!(
        log::collect_graph(&t.0, None)["dirty"],
        json!({"changed":1})
    );
}
#[test]
fn test_octopus() {
    let t = Temp::new();
    octopus(&t.0);
    let g = log::collect_graph(&t.0, None);
    assert_eq!(g["total"], 5);
    assert_eq!(g["commits"][0]["parents"].as_array().unwrap().len(), 3);
    assert_eq!(g["commits"][0]["merged_name"], "feature/auth");
    topological(&g);
}
#[test]
fn test_deleted_branches() {
    let t = Temp::new();
    deleted_branches(&t.0);
    let g = log::collect_graph(&t.0, None);
    let commits = g["commits"].as_array().unwrap();
    let names: std::collections::BTreeSet<_> = commits
        .iter()
        .filter(|c| c["parents"].as_array().unwrap().len() > 1)
        .map(|c| c["merged_name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "ceo/codex/pane-split",
            "ceo/codex/explorer-no-badges",
            "ceo/codex/git-data"
        ]
        .into()
    );
    assert_eq!(
        commits
            .iter()
            .map(|c| c["refs"].as_array().unwrap().len())
            .sum::<usize>(),
        1
    );
    topological(&g);
}
#[test]
fn test_detached_head_only_commit_is_included() {
    let t = Temp::new();
    let r = detached(&t.0);
    let g = log::collect_graph(&t.0, None);
    assert_eq!(
        g["head"],
        json!({"hash":r.git(&["rev-parse","HEAD"]),"branch":null,"detached":true})
    );
    assert!(strings(&g["commits"], "hash").contains(g["head"]["hash"].as_str().unwrap()));
    assert_eq!(g["total"], 6);
    topological(&g);
}
#[test]
fn test_unborn_empty_not_repo_and_git_missing() {
    isolated("test_unborn_empty_not_repo_and_git_missing", |path| {
        assert_eq!(log::collect_graph(path, None)["error"], "not-a-repo");
        GitRepo::new(path);
        let g = log::collect_graph(path, None);
        assert_eq!(g["error"], "empty");
        assert_eq!(g["ok"], false);
        assert_eq!(g["head"]["branch"], "master");
        std::env::set_var("PATH", "");
        assert_eq!(log::collect_graph(path, None)["error"], "git-missing");
    });
}
#[test]
fn test_remote_default_and_current_ref() {
    let t = Temp::new();
    let r = clone_with_remote(&t.0);
    r.checkout("topic", true);
    let g = log::collect_graph(&r.path, None);
    assert_eq!(g["repo"]["default_branch"], "origin/master");
    let refs = g["commits"][0]["refs"].as_array().unwrap();
    assert!(!strings(&g["commits"][0]["refs"], "name").contains("origin/HEAD"));
    for r in [
        json!({"name":"origin/master","kind":"remote"}),
        json!({"name":"topic","kind":"branch","current":true}),
        json!({"name":"v1","kind":"tag"}),
    ] {
        assert!(refs.contains(&r));
    }
}
#[test]
fn test_default_precedence_and_subdirectory_root() {
    let t = Temp::new();
    let r = linear(&t.0, 1);
    r.branch("main");
    r.checkout("master", false);
    fs::create_dir(t.0.join("nested")).unwrap();
    let g = log::collect_graph(&t.0.join("nested"), None);
    assert_eq!(g["repo"]["root"], t.0.to_str().unwrap());
    assert_eq!(g["repo"]["default_branch"], "main");
    r.git(&["branch", "-m", "master", "topic"]);
    r.git(&["branch", "-D", "main"]);
    assert_eq!(
        log::collect_graph(&t.0, None)["repo"]["default_branch"],
        "topic"
    );
}
#[test]
fn test_multiple_roots_exclude_notes_and_stash() {
    let t = Temp::new();
    let mut r = linear(&t.0, 2);
    let original = r.git(&["rev-parse", "HEAD"]);
    r.git(&["checkout", "--orphan", "unrelated"]);
    r.git(&["rm", "-rf", "."]);
    r.commit(&[("other.txt", Some(b"second root"))], "Unrelated root");
    r.checkout("master", false);
    fs::write(t.0.join("main.py"), "stash me").unwrap();
    r.git(&["stash", "push", "-m", "not a graph commit"]);
    let env = r.dated(None);
    r.command(
        &["notes", "add", "-m", "not a graph commit", &original],
        None,
        &env,
    );
    let g = log::collect_graph(&t.0, None);
    assert_eq!(g["total"], 3);
    assert_eq!(
        g["commits"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["parents"].as_array().unwrap().is_empty())
            .count(),
        2
    );
    topological(&g);
}
#[test]
fn test_unicode_controls_and_author_dates() {
    let t = Temp::new();
    let mut r = GitRepo::new(&t.0);
    r.commit_as(
        &[("Привет мир.py", Some("print(\"🌍\")\n".as_bytes()))],
        &format!("Привет\u{1b} {}", "s".repeat(400)),
        Some("2025-02-01T12:00:00+05:30"),
        Some((
            &format!("名\u{1}{}", "a".repeat(140)),
            &format!("{}@example.test", "e".repeat(140)),
        )),
    );
    let g = log::collect_graph(&t.0, None);
    let c = &g["commits"][0];
    assert!(c["subject"].as_str().unwrap().starts_with("Привет "));
    for (k, n) in [("subject", 300), ("author", 120), ("email", 120)] {
        assert_eq!(c[k].as_str().unwrap().chars().count(), n);
    }
    assert_eq!(c["tz"], 330);
}
#[test]
fn test_three_thousand_commits_performance() {
    let t = Temp::new();
    fast_history(&t.0, 3000);
    let start = Instant::now();
    let g = log::collect_graph(&t.0, Some(1500));
    assert!(start.elapsed() < Duration::from_millis(1500));
    assert_eq!(g["ok"], true, "{g}");
    assert_eq!(g["total"], 3000);
    assert_eq!(g["commits"].as_array().unwrap().len(), 1500);
    assert_eq!(g["truncated"], true);
    topological(&g);
}
#[test]
fn test_bounded_runner_reaps_on_timeout_and_cap() {
    isolated("test_bounded_runner_reaps_on_timeout_and_cap", |path| {
        executable(&path.join("git"),"#!/usr/bin/python3\nimport os,sys,time\nopen(os.path.join(sys.argv[5], 'pid'), 'w').write(str(os.getpid()))\nassert all(os.environ[k]==v for k,v in [('GIT_OPTIONAL_LOCKS','0'),('GIT_TERMINAL_PROMPT','0'),('GIT_NO_LAZY_FETCH','1'),('LC_ALL','C'),('GIT_PAGER','cat')])\nif 'hang' in sys.argv: time.sleep(10)\nelif 'closed' in sys.argv: os.close(1); time.sleep(10)\nelse: sys.stdout.write('x'*1000000)\n");
        std::env::set_var("PATH", path);
        for args in [vec!["hang"], vec!["closed"]] {
            let start = Instant::now();
            assert_eq!(
                log::run(
                    path,
                    &args,
                    start + Duration::from_millis(100),
                    log::MAX_OUTPUT,
                    false
                )
                .unwrap_err()
                .0,
                "timeout"
            );
            assert!(start.elapsed() < Duration::from_secs(1));
            let pid = fs::read_to_string(path.join("pid")).unwrap();
            assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
        }
        let (data, clipped) = log::run(
            path,
            &[],
            Instant::now() + Duration::from_secs(2),
            100,
            true,
        )
        .unwrap();
        assert_eq!(data.len(), 100);
        assert!(clipped);
        assert_eq!(
            log::run(
                path,
                &[],
                Instant::now() + Duration::from_secs(2),
                100,
                false
            )
            .unwrap_err()
            .0,
            "output-too-large"
        );
        let pid = fs::read_to_string(path.join("pid")).unwrap();
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    });
}
#[test]
fn test_dirty_failure_and_count_fallback() {
    isolated("test_dirty_failure_and_count_fallback", |path| {
        linear(path, 4);
        let fake = path.join("fake");
        fs::create_dir(&fake).unwrap();
        executable(&fake.join("git"),"#!/bin/sh\ncase \" $* \" in *' status '*|*' rev-list '*) exit 1;; esac\nexec /usr/bin/git \"$@\"\n");
        std::env::set_var("PATH", &fake);
        let g = log::collect_graph(path, Some(2));
        assert_eq!(g["ok"], true);
        assert!(g["dirty"].is_null());
        assert!(g["total"].as_i64().unwrap() > 2);
        assert_eq!(g["truncated"], true);
    });
}
#[test]
fn test_import_and_call_with_gi_blocked() {
    let t = Temp::new();
    linear(&t.0, 1);
    assert_eq!(log::collect_graph(&t.0, None)["ok"], true);
    let d = diff::commit_detail(&t.0, "HEAD", diff::Limits::default(), None);
    assert_eq!(d["ok"], true, "{d}");
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
#[test]
fn test_fixture_is_deterministic_and_matches_topology() {
    let a = Temp::new();
    let b = Temp::new();
    let first = showcase(&a.0);
    let second = showcase(&b.0);
    assert_eq!(first.git(&["show-ref"]), second.git(&["show-ref"]));
    let g = log::collect_graph(&a.0, None);
    assert_eq!(g["dirty"], json!({"changed":2}));
    assert_eq!(g["head"]["branch"], "master");
    assert_eq!(
        first.git(&["rev-list", "--first-parent", "--count", "master"]),
        "14"
    );
    let mut parents: Vec<_> = g["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["parents"].as_array().unwrap().len())
        .filter(|n| *n > 1)
        .collect();
    parents.sort();
    assert_eq!(parents, vec![2, 2, 3]);
    assert_eq!(
        first
            .git(&["branch", "--format=%(refname:short)"])
            .lines()
            .collect::<std::collections::BTreeSet<_>>(),
        [
            "master",
            "feature/auth",
            "feature/api",
            "feature/ui",
            "hotfix/critical",
            "experiment/graph-polish"
        ]
        .into()
    );
    assert_eq!(
        first.git(&["rev-list", "--count", "master..experiment/graph-polish"]),
        "2"
    );
}
#[test]
#[ignore]
fn timing_impro_and_five_thousand_commits() {
    let t = Temp::new();
    fast_history(&t.0, 5000);
    let impro = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (root, name) in [(&impro, "impro"), (&t.0, "synthetic-5000")] {
        let before = Instant::now();
        let (data, _) = log::run(
            root,
            &[
                "log",
                "--topo-order",
                "-n",
                "1500",
                "--branches",
                "--remotes",
                "--tags",
                "HEAD",
                "-z",
                "--encoding=UTF-8",
                &format!("--format={}", log::LOG_FORMAT),
                "--",
            ],
            Instant::now() + log::TIMEOUT,
            log::MAX_OUTPUT,
            false,
        )
        .unwrap();
        let git = before.elapsed();
        let before = Instant::now();
        let commits = log::parse_log(&data);
        let parse = before.elapsed();
        eprintln!(
            "{name}: git={git:?}, parse={parse:?}, commits={}",
            commits.len()
        );
        assert!(parse < Duration::from_millis(40));
    }
}

#[test]
fn test_showcase_reference_json_parity() {
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("support/reference.json")).unwrap();
    let t = Temp::new();
    let r = showcase(&t.0);
    assert_eq!(r.git(&["show-ref"]), expected["refs"]);
    let mut graph = log::collect_graph(&t.0, None);
    graph["repo"]["root"] = json!("");
    graph["repo"]["name"] = json!("");
    assert_eq!(graph, expected["graph"]);
    for detail in expected["details"].as_array().unwrap() {
        let actual = diff::commit_detail(
            &t.0,
            detail["hash"].as_str().unwrap(),
            diff::Limits::default(),
            None,
        );
        assert_eq!(&actual, detail, "commit {}", detail["hash"]);
    }
    for context in expected["contexts"].as_array().unwrap() {
        let actual = diff::file_context(
            &t.0,
            context["rev"].as_str().unwrap(),
            context["path"].as_str().unwrap(),
            1,
            200,
            None,
        );
        assert_eq!(
            actual, context["result"],
            "context {}:{}",
            context["rev"], context["path"]
        );
    }
}
