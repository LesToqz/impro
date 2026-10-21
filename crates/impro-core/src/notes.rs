//! Lazy project notes and pure Markdown edits. All offsets count Unicode characters.
use crate::fsops::{self, FsOpError};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
};
pub const NOTES_DIR: &str = crate::identity::NOTES_DIR;
pub const NOTE_SUFFIX: &str = ".md";
pub const TABLE: &str = "| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n|  |  |  |\n";
pub const SLASH_TABLE: &str =
    "| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n|  |  |  |\n|  |  |  |\n";
pub type NoteNameError = FsOpError;
pub fn notes_dir(root: impl AsRef<Path>) -> PathBuf {
    fsops::absolute(root).join(NOTES_DIR)
}
pub fn note_filename(name: &str) -> Result<String, NoteNameError> {
    let mut name = name.trim();
    if name.to_lowercase().ends_with(NOTE_SUFFIX) {
        name = name[..name.len() - 3].trim_end();
    }
    if name.is_empty() {
        return Err(FsOpError::Invalid("Enter a note name.".into()));
    }
    if name.contains(['/', '\0']) || [".", ".."].contains(&name) {
        return Err(FsOpError::Invalid("Note names cannot contain '/'.".into()));
    }
    Ok(format!("{name}.md"))
}
pub fn note_path(root: impl AsRef<Path>, name: &str) -> Result<PathBuf, NoteNameError> {
    let name = note_filename(name)?;
    let folder = notes_dir(root);
    let stem = &name[..name.len() - 3];
    let mut path = folder.join(&name);
    let mut number = 2;
    while fs::symlink_metadata(&path).is_ok() {
        path = folder.join(format!("{stem} {number}.md"));
        number += 1;
    }
    Ok(path)
}
pub fn create_note(root: impl AsRef<Path>, name: &str) -> Result<PathBuf, NoteNameError> {
    let filename = note_filename(name)?;
    let title = &filename[..filename.len() - 3];
    let folder = notes_dir(&root);
    fs::create_dir_all(&folder)
        .map_err(|e| FsOpError::Io(format!("Cannot create the notes folder: {e}")))?;
    loop {
        let path = note_path(&root, title)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                write!(file, "# {title}\n\n")
                    .map_err(|e| FsOpError::Io(format!("Cannot create {filename}: {e}")))?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(FsOpError::Io(format!("Cannot create {filename}: {e}"))),
        }
    }
}
pub fn list_notes(root: impl AsRef<Path>, sort: &str) -> Result<Vec<PathBuf>, NoteNameError> {
    if !["name", "modified"].contains(&sort) {
        return Err(FsOpError::Invalid(
            "Notes sort must be 'name' or 'modified'.".into(),
        ));
    }
    let Ok(entries) = fs::read_dir(notes_dir(root)) else {
        return Ok(Vec::new());
    };
    let mut entries: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| is_markdown(p) && p.is_file())
        .map(|p| {
            let modified = fs::metadata(&p).ok().and_then(|m| m.modified().ok());
            let name = p
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .replace('ß', "ss");
            (p, name, modified)
        })
        .collect();
    entries.sort_by(|a, b| {
        let by_name = || a.1.cmp(&b.1).then(a.0.cmp(&b.0));
        if sort == "modified" {
            b.2.cmp(&a.2).then_with(by_name)
        } else {
            by_name()
        }
    });
    Ok(entries.into_iter().map(|e| e.0).collect())
}
fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("md"))
}
pub fn is_note(root: impl AsRef<Path>, path: Option<&Path>) -> bool {
    path.filter(|p| !p.as_os_str().is_empty()).is_some_and(|p| {
        is_markdown(p) && fsops::absolute(p).parent() == Some(notes_dir(root).as_path())
    })
}
pub fn is_note_path(path: Option<&Path>) -> bool {
    path.is_some_and(|p| {
        if !is_markdown(p) {
            return false;
        }
        let absolute = fsops::absolute(p);
        let parts: Vec<_> = absolute
            .parent()
            .unwrap_or(Path::new("/"))
            .components()
            .collect();
        parts
            .windows(2)
            .any(|w| w[0].as_os_str() == ".impro" && w[1].as_os_str() == "Notes")
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub select_start: usize,
    pub select_end: usize,
}
impl Edit {
    pub fn apply(&self, document: &str) -> String {
        format!(
            "{}{}{}",
            slice(document, 0, self.start),
            self.text,
            slice(document, self.end, len(document))
        )
    }
}
fn len(s: &str) -> usize {
    s.chars().count()
}
fn byte(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map(|p| p.0).unwrap_or(s.len())
}
fn slice(s: &str, a: usize, b: usize) -> &str {
    &s[byte(s, a)..byte(s, b.max(a))]
}
fn bounds(d: &str, start: isize, end: Option<isize>) -> (usize, usize) {
    let n = len(d) as isize;
    let (a, b) = (
        start.clamp(0, n) as usize,
        end.unwrap_or(start).clamp(0, n) as usize,
    );
    (a.min(b), a.max(b))
}
fn line_bounds(d: &str, s: usize, mut e: usize) -> (usize, usize) {
    let first = slice(d, 0, s)
        .rfind('\n')
        .map(|i| len(&d[..i]) + 1)
        .unwrap_or(0);
    if e > s && slice(d, e - 1, e) == "\n" {
        e -= 1;
    }
    let last = slice(d, e, len(d))
        .find('\n')
        .map(|i| e + len(&slice(d, e, len(d))[..i]))
        .unwrap_or(len(d));
    (first, last)
}
fn edit(s: usize, e: usize, text: String, a: usize, b: usize) -> Edit {
    Edit {
        start: s,
        end: e,
        text,
        select_start: a,
        select_end: b,
    }
}
fn prefix_lines(d: &str, s: usize, e: usize, f: impl Fn(usize, &str) -> String) -> Edit {
    let (a, b) = line_bounds(d, s, e);
    let lines: Vec<_> = slice(d, a, b).split('\n').collect();
    let edited: Vec<_> = lines.iter().enumerate().map(|(i, l)| f(i, l)).collect();
    let text = edited.join("\n");
    let (x, y) = if s == e {
        let cursor =
            (s as isize + len(&edited[0]) as isize - len(lines[0]) as isize).max(0) as usize;
        (cursor, cursor)
    } else {
        (a, a + len(&text))
    };
    edit(a, b, text, x, y)
}
fn heading(line: &str) -> String {
    let level = line.bytes().take_while(|b| *b == b'#').count();
    if level > 0 && level < 6 && (line.len() == level || line.as_bytes()[level] == b' ') {
        format!("#{line}")
    } else if level == 0 {
        format!("# {line}")
    } else {
        line.into()
    }
}
fn list_marker() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"^(?:[-*+] \[[ xX]\] |[-*+] |\d+[.)] )").expect("static regex"))
}
fn block_marker() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"^(?:#{1,6}(?: +|$)|> ?|[-*+] \[[ xX]\] |[-*+] |\d+[.)] )")
            .expect("static regex")
    })
}
fn list_line(line: &str, marker: &str) -> String {
    let body = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - body.len()];
    let stripped = list_marker()
        .find(body)
        .map(|m| &body[m.end()..])
        .unwrap_or(body);
    format!("{indent}{marker}{stripped}")
}
fn wrap(d: &str, s: usize, e: usize, before: &str, after: &str, placeholder: &str) -> Edit {
    let selected = if s == e { placeholder } else { slice(d, s, e) };
    edit(
        s,
        e,
        format!("{before}{selected}{after}"),
        s + len(before),
        s + len(before) + len(selected),
    )
}
fn block(d: &str, s: usize, e: usize, body: &str, selection: (usize, usize)) -> Edit {
    let before =
        if s == 0 || slice(d, s - 1, s) == "\n" && (s < 2 || slice(d, s - 2, s - 1) == "\n") {
            ""
        } else if slice(d, s - 1, s) == "\n" {
            "\n"
        } else {
            "\n\n"
        };
    let after = if e < len(d) && slice(d, e, e + 1) != "\n" {
        "\n"
    } else {
        ""
    };
    let offset = s + len(before);
    edit(
        s,
        e,
        format!("{before}{body}{after}"),
        offset + selection.0,
        offset + selection.1,
    )
}
fn link(d: &str, s: usize, e: usize) -> Edit {
    if s < e {
        let selected = slice(d, s, e);
        let url = s + len(selected) + 3;
        edit(s, e, format!("[{selected}](url)"), url, url + 3)
    } else {
        wrap(d, s, e, "[", "](url)", "link text")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NoteCommand {
    pub id: &'static str,
    pub label: &'static str,
    pub icons: &'static [&'static str],
    pub glyph: &'static str,
}
impl NoteCommand {
    pub fn edit(&self, d: &str, s: isize, e: Option<isize>) -> Result<Edit, NoteNameError> {
        apply_command(self.id, d, s, e)
    }
}
pub const NOTE_COMMANDS: [NoteCommand; 10] = [
    NoteCommand {
        id: "heading",
        label: "Heading",
        icons: &[],
        glyph: "H",
    },
    NoteCommand {
        id: "bold",
        label: "Bold",
        icons: &["format-text-bold-symbolic"],
        glyph: "B",
    },
    NoteCommand {
        id: "italic",
        label: "Italic",
        icons: &["format-text-italic-symbolic"],
        glyph: "I",
    },
    NoteCommand {
        id: "bullet",
        label: "Bullet list",
        icons: &["view-list-bullet-symbolic"],
        glyph: "•",
    },
    NoteCommand {
        id: "numbered",
        label: "Numbered list",
        icons: &["view-list-ordered-symbolic"],
        glyph: "1.",
    },
    NoteCommand {
        id: "checklist",
        label: "Checklist",
        icons: &["checkbox-checked-symbolic"],
        glyph: "☐",
    },
    NoteCommand {
        id: "table",
        label: "Table",
        icons: &["x-office-spreadsheet-symbolic", "view-grid-symbolic"],
        glyph: "▦",
    },
    NoteCommand {
        id: "code",
        label: "Code block",
        icons: &[],
        glyph: "</>",
    },
    NoteCommand {
        id: "link",
        label: "Link",
        icons: &["insert-link-symbolic"],
        glyph: "Link",
    },
    NoteCommand {
        id: "rule",
        label: "Horizontal rule",
        icons: &[],
        glyph: "―",
    },
];
pub fn apply_command(
    id: &str,
    d: &str,
    start: isize,
    end: Option<isize>,
) -> Result<Edit, NoteNameError> {
    let (s, e) = bounds(d, start, end);
    Ok(match id {
        "heading" => prefix_lines(d, s, e, |_, l| heading(l)),
        "bold" => wrap(d, s, e, "**", "**", "bold text"),
        "italic" => wrap(d, s, e, "*", "*", "italic text"),
        "bullet" => prefix_lines(d, s, e, |_, l| list_line(l, "- ")),
        "numbered" => prefix_lines(d, s, e, |i, l| list_line(l, &format!("{}. ", i + 1))),
        "checklist" => prefix_lines(d, s, e, |_, l| list_line(l, "- [ ] ")),
        "table" => block(d, e, e, TABLE, (2, 10)),
        "rule" => block(d, e, e, "---\n", (4, 4)),
        "link" => link(d, s, e),
        "code" => {
            let selected = slice(d, s, e).trim_end_matches('\n');
            block(
                d,
                s,
                e,
                &format!("```\n{selected}\n```\n"),
                (4, 4 + len(selected)),
            )
        }
        _ => return Err(FsOpError::Invalid(format!("Unknown note command: {id}"))),
    })
}
fn turn_line(
    d: &str,
    s: usize,
    e: usize,
    prefix: &str,
    suffix: &str,
    selection: Option<(usize, usize)>,
) -> Edit {
    let (a, b) = line_bounds(d, s, e);
    let line = slice(d, a, b);
    let body = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - body.len()];
    let mut body = body;
    while let Some(m) = block_marker().find(body) {
        body = &body[m.end()..];
    }
    let text = format!("{indent}{prefix}{body}{suffix}");
    let offset = a + len(indent);
    let (x, y) = selection.unwrap_or((len(prefix) + len(body), len(prefix) + len(body)));
    edit(a, b, text, offset + x, offset + y)
}
#[derive(Clone, Copy, Debug)]
pub struct SlashCommand {
    pub id: &'static str,
    pub label: &'static str,
    pub keywords: &'static [&'static str],
    pub icons: &'static [&'static str],
    pub glyph: &'static str,
    pub hint: &'static str,
    pub group: &'static str,
    pub secondary: &'static str,
}
impl SlashCommand {
    pub fn edit(&self, d: &str, s: isize, e: Option<isize>) -> Result<Edit, NoteNameError> {
        apply_slash_command(self.id, d, s, e)
    }
}
pub fn apply_slash_command(
    id: &str,
    d: &str,
    start: isize,
    end: Option<isize>,
) -> Result<Edit, NoteNameError> {
    let (s, e) = bounds(d, start, end);
    Ok(match id {
        "text" => turn_line(d, s, e, "", "", None),
        "h1" => turn_line(d, s, e, "# ", "", None),
        "h2" => turn_line(d, s, e, "## ", "", None),
        "h3" => turn_line(d, s, e, "### ", "", None),
        "bullet" => turn_line(d, s, e, "- ", "", None),
        "numbered" => turn_line(d, s, e, "1. ", "", None),
        "todo" => turn_line(d, s, e, "- [ ] ", "", None),
        "quote" => turn_line(d, s, e, "> ", "", None),
        "code" => turn_line(d, s, e, "```\n", "\n```\n", Some((4, 4))),
        "mermaid" => turn_line(d, s, e, "```mermaid\n", "\n```\n", Some((11, 11))),
        "callout" => turn_line(d, s, e, "> **Note**\n> ", "", Some((4, 8))),
        "link" => link(d, s, e),
        "image" => edit(s, e, "![alt](path)".into(), s + 7, s + 11),
        "date" => {
            let date = fsops::local_date("%Y-%m-%d");
            edit(s, e, date, s + 10, s + 10)
        }
        "table" => {
            let (a, b) = line_bounds(d, s, e);
            let stripped = turn_line(d, s, e, "", "", None).text;
            let body = stripped.trim();
            let table = if body.is_empty() {
                SLASH_TABLE.into()
            } else {
                SLASH_TABLE.replacen("Column 1", body, 1)
            };
            edit(
                a,
                b,
                table,
                a + 2,
                a + 2 + if body.is_empty() { 8 } else { len(body) },
            )
        }
        "divider" => {
            let old = turn_line(d, s, e, "", "", None);
            let text = if old.text.trim().is_empty() {
                "---\n".into()
            } else {
                format!("{}\n\n---\n", old.text)
            };
            let end = old.start + len(&text);
            edit(old.start, old.end, text, end, end)
        }
        _ => return Err(FsOpError::Invalid(format!("Unknown slash command: {id}"))),
    })
}
pub const INLINE_MARKERS: [(&str, &str); 4] = [
    ("bold", "**"),
    ("italic", "*"),
    ("strike", "~~"),
    ("inline_code", "`"),
];
pub const TURN_COMMANDS: [&str; 8] = [
    "text", "h1", "h2", "h3", "bullet", "numbered", "todo", "quote",
];
pub type FormatBounds = (usize, usize, usize, usize);
pub fn selection_formats(
    d: &str,
    mut s: usize,
    mut e: usize,
) -> BTreeMap<&'static str, FormatBounds> {
    let mut found = BTreeMap::new();
    if s >= e || e > len(d) {
        return found;
    }
    loop {
        let marker = INLINE_MARKERS.iter().map(|(_, m)| *m).find(|m| {
            e - s > 2 * len(m) && slice(d, s, e).starts_with(m) && slice(d, s, e).ends_with(m)
        });
        if let Some(m) = marker {
            s += len(m);
            e -= len(m);
        } else {
            break;
        }
    }
    while s < e {
        let mut wrapper = None;
        for (command, marker) in INLINE_MARKERS {
            let n = len(marker);
            if s >= n && slice(d, s - n, s) == marker && slice(d, e, e + n) == marker {
                wrapper = Some((command, s - n, e + n, s, e));
                break;
            }
        }
        if wrapper.is_none()
            && s > 0
            && slice(d, s - 1, s) == "["
            && (s < 2 || slice(d, s - 2, s - 1) != "!")
        {
            let after = slice(d, e, len(d));
            if after.starts_with("](") {
                if let Some(i) = after.find(')') {
                    if !after[..i].contains('\n') {
                        wrapper = Some(("link", s - 1, e + len(&after[..i + 1]), s, e));
                    }
                }
            }
        }
        if let Some((command, a, b, x, y)) = wrapper {
            found.insert(command, (a, b, x, y));
            s = a;
            e = b;
        } else {
            break;
        }
    }
    found
}
pub fn apply_selection_command(
    id: &str,
    d: &str,
    start: isize,
    end: isize,
) -> Result<Edit, NoteNameError> {
    let (s, e) = bounds(d, start, Some(end));
    let active = selection_formats(d, s, e);
    if let Some(&(a, b, x, y)) = active.get(id) {
        let removed = x - a;
        return Ok(edit(
            a,
            b,
            slice(d, x, y).into(),
            a.max(s.saturating_sub(removed)),
            (y - removed).min(e.saturating_sub(removed)),
        ));
    }
    if let Some((_, m)) = INLINE_MARKERS.iter().find(|(c, _)| *c == id) {
        return Ok(wrap(d, s, e, m, m, ""));
    }
    if id == "link" {
        let mut edit = link(d, s, e);
        edit.select_start = s + 1;
        edit.select_end = e + 1;
        return Ok(edit);
    }
    if !TURN_COMMANDS.contains(&id) {
        return Err(FsOpError::Invalid(format!(
            "Unknown selection command: {id}"
        )));
    }
    let (a, b) = line_bounds(d, s, e);
    let mut old_offset = a;
    let mut new_offset = a;
    let mut positions = BTreeMap::new();
    let mut edited = Vec::new();
    for (i, line) in slice(d, a, b).split('\n').enumerate() {
        let mut text = apply_slash_command(id, line, 0, Some(len(line) as isize))?.text;
        if id == "numbered" {
            text = text.replacen("1. ", &format!("{}. ", i + 1), 1);
        }
        let delta = len(&text) as isize - len(line) as isize;
        for offset in [s, e] {
            if old_offset <= offset && offset <= old_offset + len(line) {
                positions.insert(
                    offset,
                    new_offset + (offset as isize - old_offset as isize + delta).max(0) as usize,
                );
            }
        }
        old_offset += len(line) + 1;
        new_offset += len(&text) + 1;
        edited.push(text);
    }
    let text = edited.join("\n");
    let x = positions.get(&s).copied().unwrap_or(a);
    let y = positions
        .get(&e)
        .copied()
        .unwrap_or(a + len(&text) + usize::from(e > b));
    Ok(edit(a, b, text, x, y))
}

pub const SLASH_COMMANDS: [SlashCommand; 16] = [
    SlashCommand {
        id: "text",
        label: "Text",
        keywords: &["plain", "paragraph"],
        icons: &["format-text-symbolic"],
        glyph: "T",
        hint: "",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "h1",
        label: "Heading 1",
        keywords: &["h1", "title 1"],
        icons: &[],
        glyph: "H1",
        hint: "#",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "h2",
        label: "Heading 2",
        keywords: &["h2", "title 2"],
        icons: &[],
        glyph: "H2",
        hint: "##",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "h3",
        label: "Heading 3",
        keywords: &["h3", "title 3"],
        icons: &[],
        glyph: "H3",
        hint: "###",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "bullet",
        label: "Bullet list",
        keywords: &["ul", "unordered"],
        icons: &["view-list-bullet-symbolic"],
        glyph: "•",
        hint: "-",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "numbered",
        label: "Numbered list",
        keywords: &["ol", "ordered"],
        icons: &["view-list-ordered-symbolic"],
        glyph: "1.",
        hint: "1.",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "todo",
        label: "To-do list",
        keywords: &["todo", "check", "checklist", "task"],
        icons: &["checkbox-checked-symbolic"],
        glyph: "☐",
        hint: "[ ]",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "quote",
        label: "Quote",
        keywords: &["blockquote"],
        icons: &["format-indent-more-symbolic"],
        glyph: "❯",
        hint: ">",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "divider",
        label: "Divider",
        keywords: &["hr", "rule", "separator"],
        icons: &[],
        glyph: "―",
        hint: "---",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "code",
        label: "Code",
        keywords: &["code", "fence", "snippet"],
        icons: &["text-x-script-symbolic"],
        glyph: "</>",
        hint: "```",
        group: "Basic blocks",
        secondary: "Block",
    },
    SlashCommand {
        id: "mermaid",
        label: "Code · Mermaid",
        keywords: &["code mermaid", "diagram"],
        icons: &[],
        glyph: "◇",
        hint: "```mermaid",
        group: "Basic blocks",
        secondary: "",
    },
    SlashCommand {
        id: "table",
        label: "Table",
        keywords: &["table", "grid"],
        icons: &["view-grid-symbolic"],
        glyph: "▦",
        hint: "3 × 3",
        group: "Insert",
        secondary: "",
    },
    SlashCommand {
        id: "link",
        label: "Link",
        keywords: &["url", "hyperlink"],
        icons: &["insert-link-symbolic"],
        glyph: "↗",
        hint: "[]()",
        group: "Insert",
        secondary: "",
    },
    SlashCommand {
        id: "date",
        label: "Date",
        keywords: &["today", "date"],
        icons: &["x-office-calendar-symbolic"],
        glyph: "▣",
        hint: "ISO",
        group: "Insert",
        secondary: "",
    },
    SlashCommand {
        id: "callout",
        label: "Callout",
        keywords: &["aside", "notice"],
        icons: &["dialog-information-symbolic"],
        glyph: "ⓘ",
        hint: "> **",
        group: "Insert",
        secondary: "",
    },
    SlashCommand {
        id: "image",
        label: "Image",
        keywords: &["picture", "photo"],
        icons: &["insert-image-symbolic"],
        glyph: "▧",
        hint: "![]()",
        group: "Insert",
        secondary: "",
    },
];
