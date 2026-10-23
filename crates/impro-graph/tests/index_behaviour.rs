mod common;

use std::path::Path;

use common::*;
use impro_graph::filter::{folder, local, GraphOptions};
use impro_graph::index::{CancelToken, EdgeKind, ProjectIndex, RefMode, ScanConfig};
use impro_graph::model::NodeKind;

#[test]
fn resolution_rules_end_to_end() {
    let p = Project::new();
    for f in [
        "a/Source.md",
        "a/Note.md",
        "Note.md",
        "long/path/Note.md",
        "some/X.md",
        "a/space name.md",
        "plain",
        "a/other.markdown",
    ] {
        p.write(f, "");
    }
    p.write(
        "alias.md",
        "---\naliases: [Nickname, \"Other Name\"]\n---\n",
    );
    let idx = p.scan();
    for (target, mode, expected) in [
        ("note", RefMode::Wiki, Some("a/Note.md")),
        ("some/x", RefMode::Wiki, Some("some/X.md")),
        ("space name.md", RefMode::Markdown, Some("a/space name.md")),
        ("../Note.md", RefMode::Markdown, Some("Note.md")),
        ("nickname", RefMode::Wiki, Some("alias.md")),
        ("plain", RefMode::Wiki, Some("plain")),
        ("other", RefMode::Wiki, Some("a/other.markdown")),
        ("../../../escape.md", RefMode::Markdown, None),
        ("https://x", RefMode::Wiki, None),
    ] {
        assert_eq!(
            idx.resolve_ref("a/Source.md", target, mode),
            expected,
            "{target}"
        );
    }
    assert_eq!(
        idx.resolve_ref("else/Source.md", "NOTE", RefMode::Wiki),
        Some("Note.md")
    );

    let mut idx = idx;
    p.write(
        "a/Source.md",
        "[[Note]] [[Note]] [[Source]] [percent](space%20name.md)",
    );
    idx.update_file(&p.path("a/Source.md"), None);
    let g = idx.build_graph(&GraphOptions::default());
    assert_eq!(g.stats.ambiguous.len(), 2);
    assert_eq!(g.stats.ambiguous[0].candidates.len(), 3);
    assert_eq!(&*g.stats.ambiguous[0].resolved, "a/Note.md");
    let e = g
        .edges
        .iter()
        .find(|e| &*g.nodes[e.dst as usize].id == "a/Note.md")
        .expect("edge");
    assert_eq!(e.count, 2);
    assert!(g.edges.iter().any(|e| e.is_self()), "self links are kept");
    assert!(ids(&g).contains("a/space name.md"));
}

#[test]
fn toggles_and_stats() {
    let p = Project::new();
    p.write("A.md", "[[B]] ![[pic.png]] [[Missing]] #a/b");
    p.write("B.md", "");
    p.write("orphan.md", "");
    p.write("code.py", "");
    p.write_bytes("pic.png", &[0x89, b'P', b'N', b'G', 0, 0xff]);
    p.write("unused.txt", "unused");
    let idx = p.scan();
    let g = idx.build_graph(&opts_all());
    let want: std::collections::BTreeSet<String> = [
        "A.md",
        "B.md",
        "orphan.md",
        "code.py",
        "pic.png",
        "?Missing",
        "#a/b",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(ids(&g), want);
    assert_eq!(g.stats.count(NodeKind::Note), 3);
    assert_eq!(
        g.stats
            .unresolved
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ["?Missing"]
    );
    assert_eq!(g.stats.orphans, 2);
    assert_eq!(g.stats.components, 3);

    let g = idx.build_graph(&GraphOptions {
        show_tags: false,
        show_attachments: false,
        show_code: false,
        show_orphans: false,
        ..GraphOptions::default()
    });
    assert_eq!(
        ids(&g),
        ["A.md", "B.md"].iter().map(|s| s.to_string()).collect()
    );
    assert_eq!(g.nodes[0].out_degree, 1);
    let none = idx.build_graph(&GraphOptions {
        query: "nonsense".into(),
        ..GraphOptions::default()
    });
    assert!(none.nodes.is_empty());
}

#[test]
fn query_and_colour_rules() {
    let p = Project::new();
    p.write(
        "folder/Alpha.md",
        "---\ntags: [planet/rocky]\n---\nbright red planet [[Beta]] [[pic.png]] [[Missing]]",
    );
    p.write("Beta.md", "blue planet");
    p.write("pic.png", "");
    let idx = p.scan();
    // data-client mode keeps non-file neighbours of matches
    let g = idx.build_graph(&GraphOptions {
        query: "file:alpha".into(),
        existing_only: false,
        groups: vec!["tag:planet".into(), String::new()],
        ..GraphOptions::default()
    });
    let want: std::collections::BTreeSet<String> =
        ["folder/Alpha.md", "#planet/rocky", "pic.png", "?Missing"]
            .iter()
            .map(|s| s.to_string())
            .collect();
    assert_eq!(ids(&g), want);
    let group = |id: &str| g.nodes[g.index_of(id).expect(id)].group;
    assert_eq!(group("folder/Alpha.md"), Some(0));
    assert_eq!(
        group("pic.png"),
        Some(1),
        "an empty rule matches everything in data mode"
    );

    // strict mode: every file and tag is tested, missing targets stay
    let g = idx.build_graph(&GraphOptions {
        query: "file:alpha".into(),
        existing_only: false,
        strict_filters: true,
        groups: vec!["tag:planet".into(), String::new(), "path:folder".into()],
        ..GraphOptions::default()
    });
    assert_eq!(
        ids(&g),
        ["?Missing", "folder/Alpha.md"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    assert_eq!(
        g.nodes[g.index_of("folder/Alpha.md").unwrap()].group,
        Some(0)
    );
    let g = idx.build_graph(&GraphOptions {
        strict_filters: true,
        groups: vec![String::new(), "blue".into()],
        existing_only: false,
        ..GraphOptions::default()
    });
    assert_eq!(
        g.nodes[g.index_of("Beta.md").unwrap()].group,
        Some(1),
        "empty rules are skipped"
    );
    assert_eq!(g.nodes[g.index_of("?Missing").unwrap()].group, None);
}

#[test]
fn code_imports_become_edges() {
    let p = Project::new();
    p.write(
        "pkg/main.py",
        "import root\nimport util\nimport module\nfrom . import sibling\nfrom .. import outer\nimport external",
    );
    for f in [
        "root.py",
        "pkg/util.py",
        "src/module/__init__.py",
        "pkg/sibling.py",
        "outer.py",
    ] {
        p.write(f, "");
    }
    p.write(
        "web/main.ts",
        "import x from \"./mod\"; import \"external\"; require(\"./sub\"); import \"./style.css\"",
    );
    p.write("web/mod.ts", "");
    p.write("web/sub/index.js", "");
    p.write("web/style.css", "@import \"other.css\"; a {x:url(img.png)}");
    p.write("web/other.css", "");
    p.write("web/img.png", "");
    p.write("web/page.html", "<img src=\"img.png\"><a href=\"../A.md\">");
    p.write("A.md", "");
    let idx = p.scan();
    let g = idx.build_graph(&GraphOptions::default());
    let targets = |src: &str| -> std::collections::BTreeSet<String> {
        g.edges
            .iter()
            .filter(|e| &*g.nodes[e.src as usize].id == src)
            .map(|e| g.nodes[e.dst as usize].id.to_string())
            .collect()
    };
    let set = |v: &[&str]| -> std::collections::BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    };
    // root beats the source folder for `util`: root.py exists, util.py is only in pkg
    assert_eq!(
        targets("pkg/main.py"),
        set(&[
            "root.py",
            "pkg/util.py",
            "src/module/__init__.py",
            "pkg/sibling.py",
            "outer.py"
        ])
    );
    assert_eq!(
        targets("web/main.ts"),
        set(&["web/mod.ts", "web/sub/index.js", "web/style.css"])
    );
    assert_eq!(
        targets("web/style.css"),
        set(&["web/other.css", "web/img.png"])
    );
    assert_eq!(targets("web/page.html"), set(&["web/img.png", "A.md"]));
    assert!(g
        .edges
        .iter()
        .filter(|e| &*g.nodes[e.src as usize].id != "A.md")
        .all(|e| e.kind == EdgeKind::Import));
    let g = idx.build_graph(&GraphOptions {
        show_code: false,
        ..GraphOptions::default()
    });
    assert_eq!(ids(&g), set(&["A.md"]));
}

#[test]
fn markdown_embeds_are_relative_to_the_note() {
    let p = Project::new();
    p.write("pic.png", "");
    p.write("notes/pic.png", "");
    p.write("notes/A.md", "![image](pic.png) ![[pic.png]] [B](../B.md)");
    p.write("B.md", "");
    let idx = p.scan();
    let g = idx.build_graph(&GraphOptions::default());
    assert_eq!(
        ids(&g),
        ["B.md", "notes/A.md", "notes/pic.png"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    let embed = g
        .edges
        .iter()
        .find(|e| e.kind == EdgeKind::Embed)
        .expect("embed");
    assert_eq!(embed.count, 2);
}

#[test]
fn local_graph_layers_and_directions() {
    let p = Project::new();
    for (path, text) in [
        ("A.md", "[[B]]"),
        ("B.md", "[[C]]"),
        ("C.md", ""),
        ("D.md", "[[A]]"),
    ] {
        p.write(path, text);
    }
    let idx = p.scan();
    let o = GraphOptions::default();
    let at = |depth, inc, out| local(&idx, Path::new("A.md"), depth, inc, out, &o);
    assert_eq!(
        ids(&at(1, true, true).graph),
        ["A.md", "B.md", "D.md"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    assert_eq!(
        ids(&at(2, true, true).graph),
        ["A.md", "B.md", "C.md", "D.md"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    assert_eq!(
        ids(&at(1, false, true).graph),
        ["A.md", "B.md"].iter().map(|s| s.to_string()).collect()
    );
    let l = at(1, true, false);
    assert_eq!(
        ids(&l.graph),
        ["A.md", "D.md"].iter().map(|s| s.to_string()).collect()
    );
    assert_eq!(l.focus, l.graph.index_of("A.md"));
    // layers and weights
    let l = at(2, true, true);
    let layer = |id: &str| l.layers[l.graph.index_of(id).unwrap()];
    assert_eq!(
        (layer("A.md"), layer("B.md"), layer("D.md"), layer("C.md")),
        (0, 1, 1, 2)
    );
    let w = l.weights();
    assert_eq!(w[l.graph.index_of("A.md").unwrap()], 12.0);
    assert_eq!(w[l.graph.index_of("C.md").unwrap()], 4.0);
    assert!(local(&idx, Path::new("missing.md"), 1, true, true, &o)
        .graph
        .nodes
        .is_empty());
    // absolute paths and case-insensitive spelling find the focus
    assert!(local(&idx, &p.path("a.md"), 1, true, true, &o)
        .focus
        .is_some());
}

#[test]
fn local_focus_survives_search() {
    let p = Project::new();
    p.write("A.md", "[[B]] [[C]]");
    p.write("B.md", "needle");
    p.write("C.md", "hay");
    let idx = p.scan();
    let o = GraphOptions {
        query: "needle".into(),
        strict_filters: true,
        ..GraphOptions::default()
    };
    let l = local(&idx, Path::new("A.md"), 1, true, true, &o);
    assert_eq!(
        ids(&l.graph),
        ["A.md", "B.md"].iter().map(|s| s.to_string()).collect()
    );
    assert_eq!(l.layers.len(), 2);
}

#[test]
fn folder_graph_keeps_only_files_below_the_prefix() {
    let p = Project::new();
    p.write(".impro/Notes/a.md", "[[b]] [[outside]] #tag");
    p.write(".impro/Notes/b.md", "");
    p.write("outside.md", "[[a]]");
    let idx = p.scan();
    let g = folder(&idx, ".impro/Notes", &GraphOptions::default());
    assert_eq!(
        ids(&g),
        [".impro/Notes/a.md", ".impro/Notes/b.md"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    assert_eq!(g.edges.len(), 1);
    assert_eq!(idx.build_graph(&GraphOptions::default()).nodes.len(), 4);
    assert!(folder(&idx, "missing", &GraphOptions::default())
        .nodes
        .is_empty());
}

#[test]
fn incremental_updates_and_buffers() {
    let p = Project::new();
    let a = p.write("A.md", "[[B]]");
    let mut idx = p.scan();
    let o = GraphOptions::default();
    assert!(
        idx.update_file(&a, None).is_empty(),
        "unchanged files report nothing"
    );

    p.write("B.md", "");
    let change = idx.update_file(&p.path("B.md"), None);
    assert_eq!(
        change
            .added
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ["B.md"]
    );
    assert_eq!(
        change
            .rerouted
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ["A.md"]
    );
    assert_eq!(idx.build_graph(&o).edges.len(), 1);
    assert!(idx.resolution_is_consistent());

    // unsaved text shadows the file
    let change = idx.update_file(&a, Some(""));
    assert_eq!(change.modified.len(), 1);
    assert!(ids(&idx.build_graph(&o)).contains("A.md"));
    assert!(idx.build_graph(&o).edges.is_empty());
    idx.update_file(&a, Some("#tag [[C]]"));
    assert!(ids(&idx.build_graph(&o)).contains("#tag"));
    // disk edits are not seen while a buffer is open
    std::fs::write(&a, "[[B]] #disk").unwrap();
    assert!(idx.update_file(&a, Some("#tag [[C]]")).is_empty());
    idx.update_file(&a, None);
    assert!(ids(&idx.build_graph(&o)).contains("#disk"));

    p.remove("A.md");
    let change = idx.update_file(&a, None);
    assert_eq!(change.removed.len(), 1);
    assert_eq!(
        ids(&idx.build_graph(&o)),
        ["B.md"].iter().map(|s| s.to_string()).collect()
    );

    // buffers for files not on disk
    idx.update_file(&p.path("New.md"), Some("[[B]]"));
    assert!(ids(&idx.build_graph(&o)).contains("New.md"));
    idx.rescan(&CancelToken::new()).expect("rescan");
    assert!(
        ids(&idx.build_graph(&o)).contains("New.md"),
        "buffers survive a rescan"
    );
    idx.update_file(&p.path("New.md"), None);
    assert!(!ids(&idx.build_graph(&o)).contains("New.md"));
    assert!(idx
        .update_file(Path::new("../outside.md"), Some("x"))
        .is_empty());
    assert!(idx.resolution_is_consistent());
}

#[test]
fn alias_changes_reroute_references() {
    let p = Project::new();
    p.write("A.md", "[[Old]] [[absent.png]]");
    let b = p.write("B.md", "---\nalias: Old\n---\n");
    let mut idx = p.scan();
    assert_eq!(idx.resolve_ref("A.md", "Old", RefMode::Wiki), Some("B.md"));
    std::fs::write(&b, "---\nalias: New\n---\n").unwrap();
    let change = idx.update_file(&b, None);
    assert_eq!(
        change
            .rerouted
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        ["A.md"]
    );
    assert_eq!(idx.resolve_ref("A.md", "Old", RefMode::Wiki), None);
    assert_eq!(idx.resolve_ref("A.md", "New", RefMode::Wiki), Some("B.md"));
    assert!(ids(&idx.build_graph(&opts_all())).contains("?absent.png"));
    assert!(!ids(&idx.build_graph(&GraphOptions {
        existing_only: false,
        show_attachments: false,
        ..GraphOptions::default()
    }))
    .contains("?absent.png"));
    assert!(idx.resolution_is_consistent());
}

#[test]
fn scan_skips_unsafe_and_ignored_files() {
    let p = Project::new();
    p.write(".git/ignored.md", "");
    p.write("node_modules/pkg/readme.md", "");
    p.write(".ceo/task.md", "");
    p.write_bytes("binary.md", b"a\0b");
    p.write_bytes("latin1.md", &[0x68, 0xe9, 0x6c, 0x6c, 0x6f]);
    p.write("large.md", &"a".repeat(1024 * 1024 + 1));
    p.write("exact.md", &"b".repeat(1024 * 1024));
    p.write("Good.md", "");
    // big attachments stay resolvable
    p.write_bytes("big.png", &vec![1u8; 2 * 1024 * 1024]);
    p.write("Ref.md", "![[big.png]]");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(p.root(), p.path("loop")).unwrap();
        std::os::unix::fs::symlink(p.path("Good.md"), p.path("link.md")).unwrap();
    }
    let idx = p.scan();
    let g = idx.build_graph(&GraphOptions::default());
    assert_eq!(
        ids(&g),
        ["Good.md", "Ref.md", "big.png", "exact.md"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
}

#[test]
fn node_cap_keeps_the_most_connected() {
    let p = Project::new();
    p.write("A.md", "[[B]] [[C]] [[D]]");
    for n in ["B", "C", "D"] {
        p.write(&format!("{n}.md"), "");
    }
    let idx = p.scan();
    let g = idx.build_graph(&GraphOptions {
        max_nodes: 2,
        ..GraphOptions::default()
    });
    assert!(g.truncated);
    assert_eq!(g.total_nodes, 4);
    assert_eq!(
        ids(&g),
        ["A.md", "B.md"].iter().map(|s| s.to_string()).collect()
    );
    assert_eq!(
        g.nodes[0].out_degree, 1,
        "degrees are recomputed after the cap"
    );
    let zero = idx.build_graph(&GraphOptions {
        max_nodes: 0,
        ..GraphOptions::default()
    });
    assert!(zero.truncated && zero.nodes.is_empty());
    assert!(!idx.build_graph(&GraphOptions::default()).truncated);
}

#[test]
fn cancellation_stops_a_scan() {
    let p = Project::new();
    for i in 0..50 {
        p.write(&format!("n{i}.md"), "x");
    }
    let token = CancelToken::new();
    token.cancel();
    assert!(ProjectIndex::scan(p.root(), ScanConfig::default(), &token).is_err());
    let ok =
        ProjectIndex::scan(p.root(), ScanConfig::default(), &CancelToken::new()).expect("scan");
    assert_eq!(ok.len(), 50);
}

#[test]
fn rename_moves_files_and_folders() {
    let p = Project::new();
    p.write("dir/A.md", "[[B]]");
    p.write("dir/B.md", "[[../top]]");
    p.write("top.md", "[[A]]");
    let mut idx = p.scan();
    let o = GraphOptions::default();
    let before = idx.build_graph(&o).edges.len();
    std::fs::rename(p.path("dir"), p.path("moved")).unwrap();
    let change = idx.rename(&p.path("dir"), &p.path("moved"));
    assert_eq!(change.removed.len(), 2);
    assert_eq!(change.added.len(), 2);
    let g = idx.build_graph(&o);
    assert_eq!(
        ids(&g),
        ["moved/A.md", "moved/B.md", "top.md"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    );
    assert_eq!(
        g.edges.len(),
        before,
        "relative links keep resolving after the move"
    );
    assert!(idx.resolution_is_consistent());
    // single file rename
    std::fs::rename(p.path("top.md"), p.path("renamed.md")).unwrap();
    idx.rename(&p.path("top.md"), &p.path("renamed.md"));
    assert!(ids(&idx.build_graph(&o)).contains("renamed.md"));
    assert!(!ids(&idx.build_graph(&o)).contains("top.md"));
    assert!(idx.resolution_is_consistent());
    // removing a folder
    let change = idx.remove(&p.path("moved"));
    assert_eq!(change.removed.len(), 2);
    assert_eq!(idx.len(), 1);
}

#[test]
fn file_time_is_the_older_of_change_and_modified_time() {
    let p = Project::new();
    let old = p.write("old.md", "");
    p.write("fresh.md", "");
    let long_ago = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
    std::fs::File::options()
        .write(true)
        .open(old)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();
    let g = p.scan().build_graph(&GraphOptions::default());
    let time = |id: &str| g.nodes[g.index_of(id).unwrap()].ctime.unwrap();
    assert_eq!(time("old.md"), 1_000_000.0, "an old mtime wins over a newer change time");
    assert!(time("fresh.md") > 1_000_000_000.0);
}
