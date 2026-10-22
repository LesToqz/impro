use impro_markdown::mdview::*;
use std::path::Path;
fn blocks(s: &str) -> Vec<Block> {
    parse_blocks(s).collect()
}
fn joined(s: &str) -> String {
    parse_inline(s).iter().map(|s| s.text.as_str()).collect()
}
#[test]
fn test_headings_and_rules() {
    let b = blocks("# One\n\nTwo\n===\n\nThree\n---\n\n###### Six\n\n---");
    assert_eq!(
        b.iter()
            .map(|b| (b.kind.as_str(), b.level))
            .collect::<Vec<_>>(),
        [
            ("heading", 1),
            ("heading", 1),
            ("heading", 2),
            ("heading", 6),
            ("rule", 0)
        ]
    );
    assert_eq!(b[0].text, "One");
}
#[test]
fn test_inline_styles_and_nesting() {
    let spans = parse_inline("**bold _nested_** *italic* ~~gone~~ `a * b` ***both***");
    for (text, styles) in [
        ("nested", vec!["strong", "emphasis"]),
        ("a * b", vec!["code"]),
        ("gone", vec!["strike"]),
        ("both", vec!["strong", "emphasis"]),
    ] {
        assert!(spans.iter().any(|s| s.text == text && s.styles == styles));
    }
}
#[test]
fn test_escapes_hard_and_soft_breaks() {
    let spans = parse_inline("\\*literal\\* soft\nline  \nhard\\\nbreak");
    assert_eq!(
        spans.iter().map(|s| s.text.as_str()).collect::<String>(),
        "*literal* soft line\nhard\nbreak"
    );
    assert!(spans.iter().all(|s| s.styles.is_empty()));
}
#[test]
fn test_nested_quotes_lists_tasks() {
    let b = blocks("> outer\n>> inner\n\n- parent\n  1. child\n    - [x] done\n- [ ] todo");
    assert_eq!(b[..2].iter().map(|b| b.quote).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(
        b[2..]
            .iter()
            .map(|b| (b.indent, b.marker.as_str()))
            .collect::<Vec<_>>(),
        [(0, "•"), (2, "1."), (4, "☑"), (0, "☐")]
    );
}
#[test]
fn test_fenced_and_indented_code() {
    let b = blocks("```python\ndef f():\n    pass\n```\n\n    *literal*\n\n~~~js\nlet x = 1;\n~~~");
    assert!(b.iter().all(|b| b.kind == "codeblock"));
    assert_eq!(b.len(), 3);
    assert_eq!(b[0].text, "def f():\n    pass\n");
    assert_eq!(b[0].language, "python");
    assert_eq!(b[1].text, "*literal*\n");
    assert_eq!(b[2].language, "js");
}
#[test]
fn test_code_preserves_quote_characters_and_list_continuations() {
    let b = blocks("```\n> literal\n```\n\n> ```\n> > literal\n> ```");
    assert_eq!(
        b.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(),
        ["> literal\n", "> literal\n"]
    );
    let b = blocks("- first\n  continuation\n  - child");
    assert_eq!(b[0].text, "first\ncontinuation");
    assert_eq!(b[1].indent, 2);
}
#[test]
fn test_tables_preserve_cells_headers_and_alignment() {
    let b = blocks("| A | Bee | C |\n| :--- | :---: | ---: |\n| longer | x | 1 |\n| end | y | 2 |");
    assert!(b.iter().all(|b| b.kind == "table"));
    assert_eq!(
        b.iter().map(|b| b.header).collect::<Vec<_>>(),
        [true, false, false]
    );
    assert_eq!(b[0].cells, ["A", "Bee", "C"]);
    assert_eq!(b[1].cells, ["longer", "x", "1"]);
    assert_eq!(b[2].cells, ["end", "y", "2"]);
    assert!(b
        .iter()
        .all(|b| b.alignments == ["left", "center", "right"]));
    let b = blocks(&format!(
        "| A | B |\n| --- | --- |\n{}",
        "| row | cell |\n".repeat(300)
    ));
    assert_eq!(b.len(), 301);
    assert!(b[1..].iter().all(|b| b.cells == ["row", "cell"]));
    let b = blocks(&format!(
        "| A | B |\n| --- | --- |\n| {} | tail |",
        "x".repeat(1000)
    ));
    assert_eq!(b[1].cells, ["x".repeat(1000), "tail".into()]);
}
#[test]
fn test_table_escaped_pipes_and_optional_outer_separators() {
    let b = blocks("A | B\n--- | ---:\nleft \\| middle | `right \\| end`\nonly |");
    assert_eq!(b[1].cells, [r"left \| middle", r"`right \| end`"]);
    assert_eq!(joined(&b[1].cells[0]), "left | middle");
    assert_eq!(b[2].cells, ["only", ""]);
    assert_eq!(_cells(r"| a\\ | b |"), [r"a\\", "b"]);
    assert!(!blocks("| A | B | C |\n| --- | --- |")
        .iter()
        .any(|b| b.kind == "table"));
}
#[test]
fn test_raw_html_and_images_remain_inert() {
    let text = "<script>alert(1)</script> ![a cat](https://invalid/image.png)";
    assert_eq!(joined(text), "<script>alert(1)</script> [image: a cat]");
    assert!(parse_inline(text).iter().all(|s| s.link.is_none()));
}
#[test]
fn test_links_autolinks_and_safe_targets() {
    let spans =
        parse_inline("[web](https://example.org/a_(b)) <https://example.org> [mail](mailto:x@y.z)");
    assert_eq!(
        spans
            .iter()
            .filter_map(|s| s.link.as_deref())
            .collect::<Vec<_>>(),
        [
            "https://example.org/a_(b)",
            "https://example.org",
            "mailto:x@y.z"
        ]
    );
    for value in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "ftp://example.org",
        "//example.org/x",
        "/etc/passwd",
        "#part",
        "https:\n//x",
        "https://[bad",
    ] {
        assert_eq!(link_target(value, Path::new("/tmp/a.md")), None, "{value}");
    }
    assert_eq!(
        link_target("https://example.org", Path::new("/tmp/a.md")),
        Some(LinkTarget::Uri("https://example.org".into()))
    );
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("some file.md");
    std::fs::write(&p, "").unwrap();
    assert_eq!(
        link_target("some%20file.md#heading", &d.path().join("source.md")),
        Some(LinkTarget::File(p.clone()))
    );
    assert_eq!(link_target("missing.md", &p), None);
}
#[test]
fn test_large_blocks_are_bounded() {
    for text in [
        "a".repeat(200000),
        format!("```\n{}\n```", "x".repeat(200000)),
        format!("    {}", "x".repeat(200000)),
    ] {
        let b = blocks(&text);
        assert!(b.len() > 10);
        assert!(b.iter().all(|b| b.text.chars().count() <= CHUNK * 2));
    }
}
#[test]
fn test_wiki_note_resolution_and_alias() {
    let spans = parse_inline("[[note]] [[note|alias]] https://example.org/a_b_c.");
    assert_eq!(
        spans
            .iter()
            .filter(|s| s.link.is_some())
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>(),
        ["note", "alias", "https://example.org/a_b_c"]
    );
    assert_eq!(
        spans
            .iter()
            .filter_map(|s| s.link.as_deref())
            .collect::<Vec<_>>(),
        ["[[note]]", "[[note]]", "https://example.org/a_b_c"]
    );
    let d = tempfile::tempdir().unwrap();
    let notes = d.path().join(".impro/Notes");
    std::fs::create_dir_all(&notes).unwrap();
    let target = notes.join("other.md");
    std::fs::write(&target, "").unwrap();
    for value in ["[[other]]", "[[other|alias]]"] {
        assert_eq!(
            link_target(value, &notes.join("source.md")),
            Some(LinkTarget::File(target.clone()))
        );
    }
    for value in ["[[missing]]", "[[/etc/passwd]]"] {
        assert_eq!(link_target(value, &notes.join("source.md")), None);
    }
}
#[test]
fn test_private_matcher_groups_and_unicode() {
    let text = "é **two** and [label](target)";
    let m = _INLINE.captures_iter(text).next().unwrap().unwrap();
    assert_eq!(m.get(8).unwrap().as_str(), "**");
    assert_eq!(m.get(9).unwrap().as_str(), "two");
    assert_eq!(_cells("| 雪 | é \\| z |"), ["雪", "é \\| z"]);
    assert_eq!(
        parse_inline_with("x", &["strong".into()], 13)[0].styles,
        ["strong"]
    );
}
#[test]
#[ignore]
fn parsing_performance() {
    let text="# Header\n\nA paragraph of ordinary **Markdown** text.\n\n| A | B |\n| --- | --- |\n| x | y |\n\n".repeat(12000);
    let started = std::time::Instant::now();
    let mut max = std::time::Duration::ZERO;
    let mut count = 0;
    let mut blocks = parse_blocks(&text);
    loop {
        let at = std::time::Instant::now();
        let next = blocks.next();
        max = max.max(at.elapsed());
        if next.is_none() {
            break;
        }
        count += 1;
    }
    println!(
        "parse bytes={} blocks={} total_ms={:.3} max_chunk_ms={:.3}",
        text.len(),
        count,
        started.elapsed().as_secs_f64() * 1000.,
        max.as_secs_f64() * 1000.
    );
}
