use impro_core::notes::*;
use serde::Deserialize;
use std::{fs, path::Path};
#[derive(Deserialize, Debug)]
struct Case {
    mode: String,
    id: String,
    document: String,
    start: isize,
    end: isize,
    edit: Edit,
    result: String,
}
#[test]
fn test_exact_markdown_in_an_empty_note() {
    for (id, text) in [
        ("heading", "# "),
        ("bold", "**bold text**"),
        ("italic", "*italic text*"),
        ("bullet", "- "),
        ("numbered", "1. "),
        ("checklist", "- [ ] "),
        ("table", TABLE),
        ("code", "```\n\n```\n"),
        ("link", "[link text](url)"),
        ("rule", "---\n"),
    ] {
        assert_eq!(apply_command(id, "", 0, None).unwrap().apply(""), text);
    }
}
#[test]
fn test_exact_slash_edits_empty_and_populated_lines() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("note_edits.json")).unwrap();
    assert_eq!(cases.len(), 912);
    for c in cases {
        let edit = match c.mode.as_str() {
            "note" => apply_command(&c.id, &c.document, c.start, Some(c.end)),
            "slash" => apply_slash_command(&c.id, &c.document, c.start, Some(c.end)),
            _ => apply_selection_command(&c.id, &c.document, c.start, c.end),
        }
        .unwrap();
        assert_eq!(edit, c.edit, "{c:?}");
        assert_eq!(edit.apply(&c.document), c.result, "{c:?}");
    }
}
#[test]
fn test_notes_folder_is_a_hidden_project_folder() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(notes_dir(root.path()), root.path().join(".impro/Notes"));
    assert!(!notes_dir(root.path()).exists());
}
#[test]
fn test_name_maps_to_markdown_file_name_with_spaces_kept() {
    for name in [
        "coffi shop",
        "  coffi shop  ",
        "coffi shop.md",
        "coffi shop.MD",
    ] {
        assert_eq!(note_filename(name).unwrap(), "coffi shop.md");
    }
}
#[test]
fn test_invalid_names_are_refused() {
    for n in ["", "   ", ".md", "a/b", "/", "../x", ".", "..", "a\0b"] {
        assert!(note_filename(n).is_err(), "{n}");
    }
}
#[test]
fn test_folder_created_lazily_on_first_note() {
    let root = tempfile::tempdir().unwrap();
    assert!(list_notes(root.path(), "name").unwrap().is_empty());
    assert!(!notes_dir(root.path()).exists());
    assert!(create_note(root.path(), "a/b").is_err());
    let p = create_note(root.path(), "coffi shop").unwrap();
    assert_eq!(fs::read_to_string(p).unwrap(), "# coffi shop\n\n");
}
#[test]
fn test_existing_notes_are_never_overwritten() {
    let root = tempfile::tempdir().unwrap();
    for n in 1..=3 {
        let p = create_note(root.path(), "coffi shop").unwrap();
        assert_eq!(
            p.file_name().unwrap(),
            if n == 1 {
                "coffi shop.md".into()
            } else {
                std::ffi::OsString::from(format!("coffi shop {n}.md"))
            }
        );
    }
}
#[test]
fn test_list_shows_markdown_files_only_sorted_by_name() {
    let root = tempfile::tempdir().unwrap();
    let folder = notes_dir(root.path());
    fs::create_dir_all(&folder).unwrap();
    for n in [
        "b.md",
        "A.MD",
        "coffi shop 2.md",
        "coffi shop.md",
        "skip.txt",
        ".md.swp",
    ] {
        fs::write(folder.join(n), "").unwrap();
    }
    fs::create_dir(folder.join("folder.md")).unwrap();
    let paths = list_notes(root.path(), "name").unwrap();
    let names: Vec<_> = paths
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, ["A.MD", "b.md", "coffi shop.md", "coffi shop 2.md"]);
    assert!(list_notes(root.path(), "other").is_err());
}
#[test]
fn test_is_note() {
    assert!(is_note(
        "/project",
        Some(Path::new("/project/.impro/Notes/x.md"))
    ));
    assert!(!is_note(
        "/project",
        Some(Path::new("/project/.impro/Notes/sub/x.md"))
    ));
    assert!(!is_note("/project", None));
}
#[test]
fn test_note_path_without_root_includes_nested_notes_only() {
    for p in [
        "/project/.impro/Notes/a.md",
        "/project/.impro/Notes/sub/a.MD",
    ] {
        assert!(is_note_path(Some(Path::new(p))));
    }
    for p in [
        "/project/Notes/a.md",
        "/project/.impro/Other/a.md",
        "/project/.impro/Notes.md",
    ] {
        assert!(!is_note_path(Some(Path::new(p))));
    }
}
#[test]
fn test_selected_markers_unwrap_and_unicode_offsets_are_characters() {
    let e = apply_selection_command("bold", "hi café 🌿", 9, 3).unwrap();
    assert_eq!(e.apply("hi café 🌿"), "hi **café 🌿**");
    assert_eq!((e.select_start, e.select_end), (5, 11));
}
#[test]
fn test_inline_toggles_and_chained_formats_preserve_text_selection() {
    let f = selection_formats("***word***", 3, 7);
    assert!(f.contains_key("bold") && f.contains_key("italic"));
    let e = apply_selection_command("bold", "**`word`**", 3, 7).unwrap();
    assert_eq!(e.apply("**`word`**"), "`word`");
}
#[test]
fn test_every_requested_command_exists_in_order() {
    assert_eq!(
        NOTE_COMMANDS.map(|c| c.label),
        [
            "Heading",
            "Bold",
            "Italic",
            "Bullet list",
            "Numbered list",
            "Checklist",
            "Table",
            "Code block",
            "Link",
            "Horizontal rule"
        ]
    );
    assert_eq!(SLASH_COMMANDS.len(), 16);
}
#[test]
fn test_name_sort_uses_full_unicode_casefold() {
    let root = tempfile::tempdir().unwrap();
    create_note(root.path(), "ς").unwrap();
    create_note(root.path(), "σ").unwrap();
    create_note(root.path(), "Straße").unwrap();
    create_note(root.path(), "ﬃ").unwrap();
    assert_eq!(impro_core::text::casefold("Straße Σς ﬃ"), "strasse σσ ffi");
    let paths = list_notes(root.path(), "name").unwrap();
    assert_eq!(paths[0].file_stem().unwrap(), "ﬃ");
}
