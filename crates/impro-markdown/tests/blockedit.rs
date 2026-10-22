use impro_markdown::blockedit::*;
fn check(source: &str, action: &str, expected: &str, position: Option<usize>, width: usize) {
    let offset = source[..source.find('|').unwrap()].chars().count();
    let source = source.replacen('|', "", 1);
    let edit = transform(&source, offset, action, width, false)
        .unwrap_or_else(|| panic!("unhandled {source:?} {action}"));
    assert_eq!(edit.apply(&source), expected, "{action} {source:?}");
    if let Some(p) = position {
        assert_eq!(edit.cursor, p, "{source:?}");
    }
}
#[test]
fn test_lists_split_increment_and_renumber_siblings() {
    for (s, e, p) in [
        (
            "1. on|e\n2. two\n3. three",
            "1. on\n2. e\n3. two\n4. three",
            Some(9),
        ),
        ("  - [x] do|ne", "  - [x] do\n  - [ ] ne", None),
        (
            "1. one\n  1. child|\n2. two",
            "1. one\n  1. child\n  2. \n2. two",
            None,
        ),
        ("1. a\n2. |\n3. c\n\n1. new", "1. a\n\n1. c\n\n1. new", None),
        ("- one|\n\n1. a\n1. b", "- one\n- \n\n1. a\n1. b", None),
    ] {
        check(s, "enter", e, p, 4);
    }
}
#[test]
fn test_empty_nested_outdent_and_parent_with_children() {
    for (s, a, e, p, w) in [
        ("  - |", "enter", "- ", Some(2), 2),
        ("- |", "enter", "", Some(0), 4),
        ("    - |item", "backspace", "  - item", Some(4), 2),
        (
            "- pa|rent\n  - child\n    - grandchild\n- sibling",
            "tab",
            "  - parent\n    - child\n      - grandchild\n- sibling",
            None,
            2,
        ),
        (
            "  - pa|rent\n    - child\n- sibling",
            "outdent",
            "- parent\n  - child\n- sibling",
            None,
            2,
        ),
        (
            "1. parent\n2. ch|ild\n3. sibling",
            "tab",
            "1. parent\n  1. child\n2. sibling",
            None,
            2,
        ),
        (
            "- root\n  - parent\n    - |child\n      - grandchild",
            "backspace",
            "- root\n  - parent\n  - child\n    - grandchild",
            None,
            4,
        ),
        ("- |item", "backspace", "item", Some(0), 4),
    ] {
        check(s, a, e, p, w);
    }
}
#[test]
fn test_heading_quote_tasks_and_shortcuts() {
    for (s, a, e, p) in [
        ("## Title|", "enter", "## Title\n", Some(9)),
        ("## |Title", "backspace", "Title", Some(0)),
        ("> quote|", "enter", "> quote\n> ", None),
        ("> |", "enter", "", Some(0)),
        ("> |quote", "backspace", "quote", Some(0)),
        ("[]|", "space", "- [ ] ", Some(6)),
        ("- [x] do|ne", "toggle", "- [ ] done", Some(8)),
        ("```python|", "enter", "```python\n\n```", Some(10)),
        ("---|", "enter", "---\n", Some(4)),
    ] {
        check(s, a, e, p, 4);
    }
}
#[test]
fn test_code_indent_exit_and_empty_removal() {
    for (s, a, e, p, w) in [
        (
            "```python\n    hello|\n```",
            "enter",
            "```python\n    hello\n    \n```",
            None,
            4,
        ),
        ("```\nx|\n```", "tab", "```\nx  \n```", None, 2),
        ("```\n  x|\n```", "outdent", "```\nx\n```", Some(5), 2),
        ("```\n|\n```", "backspace", "", Some(0), 4),
        ("```\nx|\n```", "down", "```\nx\n```\n", Some(10), 4),
        (
            "before\n```\n|x\n```\nafter",
            "up",
            "before\n```\nx\n```\nafter",
            Some(6),
            4,
        ),
        (
            "```\nx\n\n\n|\n```",
            "enter",
            "```\nx\n\n\n\n```\n",
            None,
            4,
        ),
    ] {
        check(s, a, e, p, w);
    }
}
#[test]
fn test_dividers_remove_as_a_block() {
    for (s, a, e, p) in [
        ("---|\ntext", "backspace", "text", 0),
        ("|---\ntext", "delete", "text", 0),
        ("---\n|text", "backspace", "text", 0),
        ("before|\n---\nafter", "delete", "before\nafter", 6),
    ] {
        check(s, a, e, Some(p), 4);
    }
}
#[test]
fn test_unicode_offsets_tabs_and_prefix() {
    check(
        "é\n1. 雪|花\n2. 尾",
        "enter",
        "é\n1. 雪\n2. 花\n3. 尾",
        Some(10),
        4,
    );
    let text = "```\n雪\n```";
    assert_eq!(code_at(text, 5).unwrap().closing, 6);
    let edit = transform(text, 5, "tab", 4, true).unwrap();
    assert_eq!(edit.apply(text), "```\n雪\t\n```");
    assert_eq!(edit.cursor, 6);
    assert_eq!(prefix("- [x] 雪").unwrap().end, 6);
    assert!(transform("plain", 2, "enter", 4, false).is_none());
}
