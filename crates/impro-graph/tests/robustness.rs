//! Malformed input must never panic: random soups of syntax fragments are fed
//! to every parser, the query language and the index.

mod common;

use common::*;
use impro_graph::filter::GraphOptions;
use impro_graph::layout::Rng;
use impro_graph::parse::{self, supported_extensions};
use impro_graph::query::{Doc, Query, Tags};

const FRAGMENTS: &[&str] = &[
    "[[",
    "]]",
    "[",
    "]",
    "(",
    ")",
    "<",
    ">",
    "!",
    "#",
    "`",
    "```",
    "~~~",
    "<!--",
    "-->",
    "---",
    "...",
    "\n",
    "\r\n",
    "\t",
    " ",
    "|",
    "^",
    "\\",
    "\"",
    "'",
    "/",
    "//",
    "/*",
    "*/",
    ":",
    ",",
    ";",
    "{",
    "}",
    "é",
    "€",
    "日本",
    "😀",
    "\u{feff}",
    "\u{0}",
    "%",
    "%2",
    "%20",
    "%zz",
    "?",
    "&",
    "=",
    "$",
    "*",
    "-",
    "import ",
    "from ",
    "export ",
    "require(",
    "as ",
    "mod ",
    "use ",
    "crate::",
    "self::",
    "super::",
    "@import ",
    "@use ",
    "url(",
    "src=",
    "href=",
    "source ",
    ". ",
    "#include ",
    "require_relative ",
    "r#\"",
    "\"#",
    "'a",
    "b'",
    "tags: [",
    "aliases:",
    "- ",
    "OR ",
    "-",
    "path:",
    "file:",
    "tag:",
    "content:",
    "note",
    "a.md",
    "../",
    "./",
    "x/y",
    "Name",
    "ünï",
    "0",
    "1e999",
];

/// Rounds per test; raise with `GRAPH_FUZZ_ROUNDS` for a longer soak.
fn rounds() -> usize {
    std::env::var("GRAPH_FUZZ_ROUNDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4000)
}

fn soup(rng: &mut Rng, pieces: usize) -> String {
    let mut s = String::new();
    for _ in 0..pieces {
        s.push_str(FRAGMENTS[(rng.next_u64() % FRAGMENTS.len() as u64) as usize]);
    }
    s
}

#[test]
fn parsers_survive_random_syntax_soup() {
    let mut rng = Rng::new(1);
    let exts: Vec<&str> = supported_extensions().to_vec();
    for round in 0..rounds() {
        let text = soup(&mut rng, 1 + (round % 60));
        let _ = parse::extract_links(&text);
        let _ = parse::extract_tags(&text);
        let _ = parse::parse_note(&text);
        let _ = parse::frontmatter::parse(&text);
        let _ = parse::mask::mask(&text);
        for ext in &exts {
            let _ = parse::extract_imports(&format!("f{ext}"), &text);
        }
        // every prefix at a char boundary, for the first few rounds
        if round < 40 {
            for (i, _) in text.char_indices() {
                let cut = &text[..i];
                let _ = parse::extract_links(cut);
                let _ = parse::extract_tags(cut);
                for ext in [".py", ".ts", ".css", ".html", ".rs", ".sh"] {
                    let _ = parse::extract_imports(&format!("f{ext}"), cut);
                }
            }
        }
    }
}

#[test]
fn queries_survive_random_input() {
    let mut rng = Rng::new(2);
    let tags: Vec<Box<str>> = vec!["a/b".into(), "é".into()];
    for round in 0..rounds() {
        let q = soup(&mut rng, 1 + (round % 12));
        let query = Query::parse(&q);
        let doc = Doc {
            path: "dir/Ünï.md",
            label: "Ünï",
            content: "some content é€ [[x]]",
            tags: Tags::Many(&tags),
        };
        let _ = query.matches(&doc);
        let _ = query.matches(&Doc {
            path: "",
            label: "",
            content: "",
            tags: Tags::None,
        });
    }
}

#[test]
fn index_and_graph_survive_random_files() {
    let p = Project::new();
    let mut rng = Rng::new(3);
    let exts = [
        ".md", ".py", ".ts", ".css", ".html", ".rs", ".sh", ".c", ".rb", ".vue", ".png",
    ];
    for i in 0..200 {
        let ext = exts[i % exts.len()];
        let name = soup(&mut rng, 2).replace(['/', '\0', '\n', '\r', '\\'], "_");
        let name = if name.len() > 40 {
            name.chars().take(10).collect()
        } else {
            name
        };
        p.write(&format!("d{}/{i}-{name}{ext}", i % 5), &soup(&mut rng, 30));
    }
    let mut idx = p.scan();
    for round in 0..60 {
        let rel = format!(
            "d{}/buffer{}{}",
            round % 5,
            round % 7,
            exts[round % exts.len()]
        );
        idx.update_file(&p.path(&rel), Some(&soup(&mut rng, 25)));
        assert!(idx.resolution_is_consistent());
    }
    for strict in [false, true] {
        for q in ["", "a", "/(/", "tag:a OR -x", "path:d1/ -file:.md"] {
            let g = idx.build_graph(&GraphOptions {
                query: q.into(),
                strict_filters: strict,
                existing_only: false,
                groups: vec!["a".into(), String::new(), "/[/".into()],
                ..GraphOptions::default()
            });
            assert!(g
                .pairs
                .iter()
                .all(|&(s, t)| (s as usize) < g.nodes.len() && (t as usize) < g.nodes.len()));
            let mut layout = impro_graph::layout::Layout::new();
            layout.set_graph(&g);
            layout.settle(50);
            assert!(layout.x().iter().chain(layout.y()).all(|v| v.is_finite()));
        }
    }
    // weird paths
    for odd in [
        "",
        ".",
        "..",
        "/",
        "/etc/passwd",
        "../x.md",
        "a/../../b.md",
        "a//b.md",
    ] {
        let _ = idx.update_file(std::path::Path::new(odd), Some("x"));
        let _ = idx.remove(std::path::Path::new(odd));
        let _ = idx.rename(std::path::Path::new(odd), std::path::Path::new(odd));
    }
}

/// Megabyte-sized adversarial inputs must stay (close to) linear.
#[test]
fn adversarial_inputs_stay_fast() {
    use std::time::Instant;
    let cases: Vec<(&str, String)> = vec![
        ("brackets", "[".repeat(300_000)),
        ("open links", "[a](".repeat(100_000)),
        ("angle links", "[a](<".repeat(100_000)),
        ("half wiki", "[[a]".repeat(100_000)),
        ("backticks", "`a".repeat(200_000)),
        (
            "backtick runs",
            (1..700).map(|n| "`".repeat(n) + "x").collect::<String>(),
        ),
        ("angles", "<a".repeat(200_000)),
        ("hashes", "#a/b ".repeat(100_000)),
        ("urls", "http://x#".repeat(60_000)),
        ("comments", "<!-- ".repeat(100_000)),
        ("fences", "```\n".repeat(100_000)),
        ("python from", "from . import (".repeat(50_000)),
        ("js import brace", "import {".repeat(100_000)),
        ("js import words", "import ".repeat(100_000)),
        ("css at-rules", "@import ".repeat(100_000)),
        ("html attrs", "src=".repeat(100_000)),
        ("rust use", "use {".repeat(50_000)),
        ("rust raw", "r#\"".repeat(100_000)),
    ];
    for (name, text) in cases {
        let start = Instant::now();
        let _ = parse::extract_links(&text);
        let _ = parse::extract_tags(&text);
        for ext in [".py", ".ts", ".css", ".html", ".rs", ".sh"] {
            let _ = parse::extract_imports(&format!("f{ext}"), &text);
        }
        let elapsed = start.elapsed().as_secs_f64();
        assert!(elapsed < 5.0, "{name}: {elapsed:.2}s");
        eprintln!("{name}: {elapsed:.3}s");
    }
}
