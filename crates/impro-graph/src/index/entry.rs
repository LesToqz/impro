//! One indexed file: classification, parsed references and derived keys.

use std::sync::Arc;

use crate::parse::{
    self,
    code::CodeKind,
    links::{LinkKind, LinkSyntax},
    text::{basename, dirname, extension, stem},
};

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct FileId(pub u32);

impl FileId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileKind {
    Note,
    Code,
    Attachment,
}

pub const NOTE_EXTENSIONS: [&str; 2] = [".md", ".markdown"];

/// Extensions treated as source files (they become `code` nodes).
pub const CODE_EXTENSIONS: [&str; 35] = [
    ".py", ".pyi", ".js", ".ts", ".tsx", ".jsx", ".mjs", ".cjs", ".css", ".scss", ".sass", ".html",
    ".htm", ".json", ".sh", ".bash", ".rs", ".go", ".c", ".h", ".cpp", ".hpp", ".cc", ".java",
    ".kt", ".swift", ".rb", ".php", ".vue", ".svelte", ".sql", ".xml", ".yaml", ".yml", ".toml",
];

pub fn kind_of(path: &str) -> FileKind {
    let ext = extension(path);
    if NOTE_EXTENSIONS.contains(&ext.as_str()) {
        FileKind::Note
    } else if CODE_EXTENSIONS.contains(&ext.as_str()) {
        FileKind::Code
    } else {
        FileKind::Attachment
    }
}

/// How a reference is resolved to a file.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefMode {
    /// `[[name]]`: project root, then the source folder; bare names by search.
    Wiki,
    /// Relative path: source folder, then project root.
    Markdown,
    Python,
    Script,
    Style,
    Html,
    RustMod,
    RustUse,
    Shell,
    Include,
    Ruby,
}

/// Edge kinds, ordered like their names so sorted output is stable.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum EdgeKind {
    Embed,
    Import,
    Link,
    Tag,
}

impl EdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeKind::Embed => "embed",
            EdgeKind::Import => "import",
            EdgeKind::Link => "link",
            EdgeKind::Tag => "tag",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Ref {
    pub target: Box<str>,
    pub mode: RefMode,
    pub edge: EdgeKind,
    pub line: u32,
}

/// Outcome of resolving one [`Ref`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Resolved {
    pub dest: Option<FileId>,
    /// More than one file matched the bare name.
    pub ambiguous: bool,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: Arc<str>,
    pub kind: FileKind,
    /// Empty for attachments.
    pub text: Box<str>,
    pub tags: Vec<Box<str>>,
    pub aliases: Vec<Box<str>>,
    pub refs: Vec<Ref>,
    /// Parallel to `refs`.
    pub resolved: Vec<Resolved>,
    /// Creation time (seconds since the epoch) when the file system knows it.
    pub ctime: Option<f64>,
}

impl FileEntry {
    /// File name without extension; this is the node label.
    pub fn label(&self) -> &str {
        stem(&self.path)
    }

    pub fn folder(&self) -> &str {
        dirname(&self.path)
    }

    /// Same content for the purposes of change detection (ignores resolution
    /// results and timestamps).
    pub fn same_content(&self, other: &FileEntry) -> bool {
        self.path == other.path
            && self.kind == other.kind
            && self.text == other.text
            && self.tags == other.tags
            && self.aliases == other.aliases
            && self.refs == other.refs
    }
}

fn code_mode(kind: CodeKind) -> RefMode {
    match kind {
        CodeKind::Python => RefMode::Python,
        CodeKind::Script => RefMode::Script,
        CodeKind::Style => RefMode::Style,
        CodeKind::Html => RefMode::Html,
        CodeKind::RustMod => RefMode::RustMod,
        CodeKind::RustUse => RefMode::RustUse,
        CodeKind::Shell => RefMode::Shell,
        CodeKind::Include => RefMode::Include,
        CodeKind::Ruby => RefMode::Ruby,
    }
}

/// Parses `text` according to the kind implied by `path`.
pub fn parse_entry(path: Arc<str>, text: String, ctime: Option<f64>) -> FileEntry {
    let kind = kind_of(&path);
    let mut entry = FileEntry {
        path,
        kind,
        text: String::new().into_boxed_str(),
        tags: Vec::new(),
        aliases: Vec::new(),
        refs: Vec::new(),
        resolved: Vec::new(),
        ctime,
    };
    match kind {
        FileKind::Note => {
            let info = parse::parse_note(&text);
            entry.refs = info
                .links
                .into_iter()
                .map(|l| {
                    let mode = match l.syntax {
                        LinkSyntax::Wiki
                            if l.kind == LinkKind::Embed && l.target.starts_with(['.', '/']) =>
                        {
                            RefMode::Markdown
                        }
                        LinkSyntax::Wiki => RefMode::Wiki,
                        LinkSyntax::Markdown => RefMode::Markdown,
                    };
                    Ref {
                        target: l.target.into_boxed_str(),
                        mode,
                        edge: if l.kind == LinkKind::Embed {
                            EdgeKind::Embed
                        } else {
                            EdgeKind::Link
                        },
                        line: l.line,
                    }
                })
                .collect();
            entry.tags = info.tags.into_iter().map(String::into_boxed_str).collect();
            entry.aliases = info
                .aliases
                .into_iter()
                .map(String::into_boxed_str)
                .collect();
            entry.text = text.into_boxed_str();
        }
        FileKind::Code => {
            entry.refs = parse::extract_imports(&entry.path, &text)
                .into_iter()
                .map(|r| Ref {
                    target: r.spec.into_boxed_str(),
                    mode: code_mode(r.kind),
                    edge: EdgeKind::Import,
                    line: r.line,
                })
                .collect();
            entry.text = text.into_boxed_str();
        }
        FileKind::Attachment => {}
    }
    entry.resolved = vec![Resolved::default(); entry.refs.len()];
    entry
}

/// Keys that identify a file to the watcher table (see `ProjectIndex`):
/// lowercase base name, name without extension, directory name for
/// `index`-style files, and aliases.
pub fn file_keys(entry: &FileEntry, out: &mut Vec<String>) {
    let name = basename(&entry.path);
    let lower = name.to_lowercase();
    let s = stem(&entry.path).to_lowercase();
    if matches!(s.as_str(), "index" | "__init__" | "mod" | "_index") {
        let dir = dirname(&entry.path);
        if !dir.is_empty() {
            out.push(basename(dir).to_lowercase());
        }
    }
    let trimmed = s.trim_start_matches('_');
    if trimmed != s && !trimmed.is_empty() {
        out.push(trimmed.to_string());
    }
    out.push(s);
    out.push(lower);
    out.extend(entry.aliases.iter().map(|a| a.to_lowercase()));
}

/// Keys of a reference; `None` means "could be anything" (wildcard).
pub fn ref_keys(r: &Ref, out: &mut Vec<String>) -> bool {
    let target = r.target.split(['#', '?', '^']).next().unwrap_or("");
    match r.mode {
        RefMode::Python => {
            let last = target.rsplit('.').next().unwrap_or("");
            if last.is_empty() {
                return false;
            }
            out.push(last.to_lowercase());
            true
        }
        RefMode::RustUse => {
            for seg in target.split("::") {
                if !matches!(seg, "crate" | "self" | "super" | "") {
                    out.push(seg.to_lowercase());
                }
            }
            // crate-root fallbacks
            out.push("lib".into());
            out.push("main".into());
            out.push("mod".into());
            true
        }
        RefMode::RustMod => {
            out.push(target.to_lowercase());
            true
        }
        _ => {
            let comp = target
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("");
            if comp.is_empty() || comp == "." || comp == ".." {
                return false;
            }
            let lower = comp.to_lowercase();
            if let Some(i) = lower.rfind('.') {
                if i > 0 {
                    out.push(lower[..i].to_string());
                }
            }
            out.push(lower);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, text: &str) -> FileEntry {
        parse_entry(Arc::from(path), text.to_string(), None)
    }

    #[test]
    fn classification() {
        assert_eq!(kind_of("a/B.MD"), FileKind::Note);
        assert_eq!(kind_of("x.markdown"), FileKind::Note);
        assert_eq!(kind_of("x.py"), FileKind::Code);
        assert_eq!(kind_of("x.png"), FileKind::Attachment);
        assert_eq!(kind_of("Makefile"), FileKind::Attachment);
        assert_eq!(kind_of("x.txt"), FileKind::Attachment);
    }

    #[test]
    fn notes_get_refs_tags_and_aliases() {
        let e = entry(
            "n/A.md",
            "---\naliases: [Nick]\n---\n[[B]] ![[p.png]] ![i](q.png) ![[./r.png]] #t",
        );
        let modes: Vec<_> = e.refs.iter().map(|r| (r.mode, r.edge)).collect();
        assert_eq!(
            modes,
            [
                (RefMode::Wiki, EdgeKind::Link),
                (RefMode::Wiki, EdgeKind::Embed),
                (RefMode::Markdown, EdgeKind::Embed),
                (RefMode::Markdown, EdgeKind::Embed),
            ]
        );
        assert_eq!(&*e.aliases[0], "Nick");
        assert_eq!(&*e.tags[0], "t");
        assert_eq!(e.label(), "A");
        assert_eq!(e.folder(), "n");
    }

    #[test]
    fn code_has_no_tags_and_attachments_no_text() {
        let c = entry("a.py", "import b\n# #tag");
        assert!(c.tags.is_empty());
        assert_eq!(c.refs[0].edge, EdgeKind::Import);
        let a = entry("p.png", "ignored");
        assert!(a.refs.is_empty() && a.text.is_empty());
    }

    #[test]
    fn change_detection_ignores_resolution() {
        let a = entry("a.md", "[[b]]");
        let mut b = entry("a.md", "[[b]]");
        b.resolved[0].dest = Some(FileId(3));
        b.ctime = Some(1.0);
        assert!(a.same_content(&b));
        assert!(!a.same_content(&entry("a.md", "[[c]]")));
    }

    #[test]
    fn watcher_keys_connect_names_and_references() {
        let mut keys = Vec::new();
        file_keys(&entry("web/sub/index.js", ""), &mut keys);
        assert!(keys.contains(&"sub".to_string()));
        keys.clear();
        file_keys(&entry("s/_part.scss", ""), &mut keys);
        assert!(keys.contains(&"part".to_string()));
        let r = Ref {
            target: "../Note.md#h".into(),
            mode: RefMode::Markdown,
            edge: EdgeKind::Link,
            line: 1,
        };
        keys.clear();
        assert!(ref_keys(&r, &mut keys));
        assert!(keys.contains(&"note".to_string()));
        let dots = Ref {
            target: "..".into(),
            mode: RefMode::Script,
            edge: EdgeKind::Import,
            line: 1,
        };
        assert!(!ref_keys(&dots, &mut Vec::new()));
    }
}
