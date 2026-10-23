//! Turns reference text into project files. Matching is case-insensitive and
//! every strategy returns the first hit in its documented probe order.

use std::collections::HashMap;

use super::entry::{FileEntry, FileId, RefMode, Resolved, NOTE_EXTENSIONS};
use crate::parse::text::{dirname, extension, has_scheme, percent_decode};

/// Case-folded lookup tables, maintained incrementally by the index.
#[derive(Default, Debug)]
pub struct Lookup {
    /// lowercase relative path -> file
    pub by_path: HashMap<Box<str>, FileId>,
    /// lowercase file name (with extension) -> files
    pub by_name: HashMap<Box<str>, Vec<FileId>>,
    /// lowercase alias -> files
    pub by_alias: HashMap<Box<str>, Vec<FileId>>,
}

/// Case-insensitive key of a path or name (the full case fold).
pub fn lower(s: &str) -> String {
    crate::parse::text::casefold(s)
}

/// Appends `rel` to `base` resolving `.` and `..`; false if it leaves the root
/// or `rel` is absolute.
fn join_norm(out: &mut String, base: &str, rel: &str) -> bool {
    out.clear();
    if rel.starts_with('/') || base.starts_with('/') {
        return false;
    }
    for comp in base.split('/').chain(rel.split('/')) {
        match comp {
            "" | "." => {}
            ".." => {
                if out.is_empty() {
                    return false;
                }
                match out.rfind('/') {
                    Some(i) => out.truncate(i),
                    None => out.clear(),
                }
            }
            c => {
                if !out.is_empty() {
                    out.push('/');
                }
                out.push_str(c);
            }
        }
    }
    true
}

/// Suffixes tried after a script specifier: the path itself, then each known
/// extension, then a directory index with each extension.
const SCRIPT_PROBES: [&str; 17] = [
    "",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".mjs",
    ".json",
    ".css",
    ".vue",
    "/index.ts",
    "/index.tsx",
    "/index.js",
    "/index.jsx",
    "/index.mjs",
    "/index.json",
    "/index.css",
    "/index.vue",
];

pub struct Resolver<'a> {
    pub files: &'a [Option<FileEntry>],
    pub lookup: &'a Lookup,
}

impl<'a> Resolver<'a> {
    fn path_of(&self, id: FileId) -> &str {
        self.files[id.index()].as_ref().map_or("", |f| &f.path)
    }

    /// The file at `base/rel` + `suffix`, matched case-insensitively. The
    /// suffix is appended before dots are resolved, so `..` + `.ts` names a
    /// file called `...ts`; paths leaving the project or absolute ones match
    /// nothing.
    fn probe(&self, scratch: &mut String, base: &str, rel: &str, suffix: &str) -> Option<FileId> {
        let mut raw = String::with_capacity(base.len() + rel.len() + suffix.len() + 1);
        if rel.starts_with('/') || base.is_empty() {
            raw.push_str(rel);
        } else {
            raw.push_str(base);
            raw.push('/');
            raw.push_str(rel);
        }
        raw.push_str(suffix);
        if !join_norm(scratch, "", &raw) || scratch.is_empty() {
            return None;
        }
        self.lookup.by_path.get(lower(scratch).as_str()).copied()
    }

    fn probe_any(
        &self,
        scratch: &mut String,
        base: &str,
        rel: &str,
        suffixes: &[&str],
    ) -> Option<FileId> {
        suffixes
            .iter()
            .find_map(|s| self.probe(scratch, base, rel, s))
    }

    /// Resolves `target` written in `source`; also reports whether a bare
    /// wiki name matched several files.
    pub fn resolve(&self, source: &str, target: &str, mode: RefMode) -> Resolved {
        let decoded = percent_decode(target);
        let target = decoded.split(['#', '^', '?']).next().unwrap_or("");
        if target.is_empty() || has_scheme(target) {
            return Resolved::default();
        }
        let mut scratch = String::new();
        let folder = dirname(source);
        let mut ambiguous = false;
        let dest = match mode {
            RefMode::Wiki => {
                let (d, a) = self.wiki(folder, target, &mut scratch);
                ambiguous = a;
                d
            }
            RefMode::Markdown => self.markdown(folder, target, &mut scratch),
            RefMode::Python => self.python(folder, target, &mut scratch),
            RefMode::Script => {
                if target.starts_with('.') {
                    self.probe_any(&mut scratch, folder, target, &SCRIPT_PROBES)
                } else {
                    None
                }
            }
            RefMode::Plain => self.probe(&mut scratch, folder, target, ""),
        };
        Resolved { dest, ambiguous }
    }

    /// Candidate files for a bare wiki name, best first.
    pub fn wiki_candidates(&self, folder: &str, target: &str) -> Vec<FileId> {
        let mut found: Vec<FileId> = Vec::new();
        let has_ext = !extension(target).is_empty();
        if !has_ext {
            for ext in NOTE_EXTENSIONS {
                let key = lower(&format!("{target}{ext}"));
                if let Some(v) = self.lookup.by_name.get(key.as_str()) {
                    found.extend(v);
                }
            }
        }
        if found.is_empty() {
            if let Some(v) = self.lookup.by_name.get(lower(target).as_str()) {
                found.extend(v);
            }
        }
        if found.is_empty() {
            if let Some(v) = self.lookup.by_alias.get(lower(target).as_str()) {
                found.extend(v);
            }
        }
        found.sort_by_cached_key(|id| {
            let p = self.path_of(*id);
            (
                dirname(p) != folder,
                p.chars().count(),
                lower(p),
                p.to_string(),
            )
        });
        found.dedup();
        found
    }

    fn wiki(&self, folder: &str, target: &str, scratch: &mut String) -> (Option<FileId>, bool) {
        if target.contains('/') {
            let suffix = if extension(target).is_empty() {
                ".md"
            } else {
                ""
            };
            let found = ["", folder]
                .into_iter()
                .find_map(|base| self.probe(scratch, base, target, suffix));
            return (found, false);
        }
        let found = self.wiki_candidates(folder, target);
        (found.first().copied(), found.len() > 1)
    }

    fn markdown(&self, folder: &str, target: &str, scratch: &mut String) -> Option<FileId> {
        let rel = target.trim_start_matches('/');
        self.probe(scratch, folder, rel, "")
            .or_else(|| self.probe(scratch, "", rel, ""))
    }

    /// `pkg.mod` / `.sibling` / `..up.mod`: leading dots climb from the source
    /// folder; absolute names try the project root, the source folder, then `src`.
    fn python(&self, folder: &str, spec: &str, scratch: &mut String) -> Option<FileId> {
        let level = spec.len() - spec.trim_start_matches('.').len();
        let module = spec[level..].replace('.', "/");
        let suffixes = [".py", "/__init__.py"];
        if level > 0 {
            let mut base = folder.to_string();
            for _ in 1..level {
                if !base.is_empty() {
                    base.push('/');
                }
                base.push_str("..");
            }
            return self.probe_any(scratch, &base, &module, &suffixes);
        }
        ["", folder, "src"]
            .into_iter()
            .find_map(|b| self.probe_any(scratch, b, &module, &suffixes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::entry::parse_entry;
    use crate::parse::text::basename;
    use std::sync::Arc;

    struct Fixture {
        files: Vec<Option<FileEntry>>,
        lookup: Lookup,
    }

    fn fixture(spec: &[(&str, &str)]) -> Fixture {
        let mut files = Vec::new();
        let mut lookup = Lookup::default();
        for (i, (p, text)) in spec.iter().enumerate() {
            let e = parse_entry(Arc::from(*p), text.to_string(), None);
            let id = FileId(i as u32);
            lookup.by_path.insert(lower(p).into(), id);
            lookup
                .by_name
                .entry(lower(basename(p)).into())
                .or_default()
                .push(id);
            for a in &e.aliases {
                lookup.by_alias.entry(lower(a).into()).or_default().push(id);
            }
            files.push(Some(e));
        }
        Fixture { files, lookup }
    }

    impl Fixture {
        fn r(&self, source: &str, target: &str, mode: RefMode) -> Option<&str> {
            let res = Resolver {
                files: &self.files,
                lookup: &self.lookup,
            };
            res.resolve(source, target, mode)
                .dest
                .map(|id| &*self.files[id.index()].as_ref().unwrap().path)
        }
    }

    #[test]
    fn join_normalises_and_rejects_escapes() {
        let mut s = String::new();
        assert!(join_norm(&mut s, "a/b", "../c/./d"));
        assert_eq!(s, "a/c/d");
        assert!(join_norm(&mut s, "", "x//y"));
        assert_eq!(s, "x/y");
        assert!(!join_norm(&mut s, "a", "../../x"));
        assert!(!join_norm(&mut s, "a", "/abs"));
        assert!(join_norm(&mut s, "a", ".."));
        assert_eq!(s, "");
    }

    #[test]
    fn wiki_and_markdown_rules() {
        let f = fixture(&[
            ("a/Source.md", ""),
            ("a/Note.md", ""),
            ("Note.md", ""),
            ("long/path/Note.md", ""),
            ("some/X.md", ""),
            ("a/space name.md", ""),
            ("plain", ""),
            ("a/other.markdown", ""),
            (
                "alias.md",
                "---\naliases: [Nickname, \"Other Name\"]\n---\n",
            ),
        ]);
        let w = RefMode::Wiki;
        let m = RefMode::Markdown;
        assert_eq!(f.r("a/Source.md", "note", w), Some("a/Note.md"));
        assert_eq!(f.r("a/Source.md", "some/x", w), Some("some/X.md"));
        assert_eq!(
            f.r("a/Source.md", "space name.md", m),
            Some("a/space name.md")
        );
        assert_eq!(f.r("a/Source.md", "../Note.md", m), Some("Note.md"));
        assert_eq!(f.r("a/Source.md", "nickname", w), Some("alias.md"));
        assert_eq!(f.r("a/Source.md", "other name", w), Some("alias.md"));
        assert_eq!(f.r("a/Source.md", "plain", w), Some("plain"));
        assert_eq!(f.r("a/Source.md", "other", w), Some("a/other.markdown"));
        assert_eq!(f.r("a/Source.md", "../../../escape.md", m), None);
        assert_eq!(f.r("a/Source.md", "https://x", w), None);
        assert_eq!(f.r("else/Source.md", "NOTE", w), Some("Note.md"));
        // markdown prefers the source folder, then the root
        assert_eq!(f.r("a/Source.md", "Note.md", m), Some("a/Note.md"));
        assert_eq!(f.r("z/Source.md", "Note.md", m), Some("Note.md"));
        assert_eq!(f.r("z/Source.md", "/Note.md", m), Some("Note.md"));
        // a wiki path without extension only tries `.md`
        assert_eq!(f.r("z/Source.md", "a/other", w), None);
    }

    #[test]
    fn ambiguity_prefers_same_folder_then_shortest_then_lexical() {
        let f = fixture(&[
            ("z/deep/er/T.md", ""),
            ("b/T.md", ""),
            ("a/T.md", ""),
            ("src.md", ""),
        ]);
        let res = Resolver {
            files: &f.files,
            lookup: &f.lookup,
        };
        let r = res.resolve("src.md", "T", RefMode::Wiki);
        assert!(r.ambiguous);
        assert_eq!(f.r("src.md", "T", RefMode::Wiki), Some("a/T.md"));
        assert_eq!(f.r("b/x.md", "T", RefMode::Wiki), Some("b/T.md"));
        assert_eq!(
            f.r("z/deep/er/x.md", "t", RefMode::Wiki),
            Some("z/deep/er/T.md")
        );
    }

    #[test]
    fn filenames_precede_aliases() {
        let f = fixture(&[("Real.md", ""), ("other.md", "---\nalias: Real\n---\n")]);
        assert_eq!(f.r("x.md", "Real", RefMode::Wiki), Some("Real.md"));
    }

    #[test]
    fn python_search_order() {
        let f = fixture(&[
            ("pkg/main.py", ""),
            ("root.py", ""),
            ("pkg/util.py", ""),
            ("util.py", ""),
            ("src/module/__init__.py", ""),
            ("pkg/sibling.py", ""),
            ("outer.py", ""),
            ("pkg/stub.pyi", ""),
        ]);
        let p = RefMode::Python;
        assert_eq!(f.r("pkg/main.py", "root", p), Some("root.py"));
        // root before source folder
        assert_eq!(f.r("pkg/main.py", "util", p), Some("util.py"));
        assert_eq!(
            f.r("pkg/main.py", "module", p),
            Some("src/module/__init__.py")
        );
        assert_eq!(f.r("pkg/main.py", ".sibling", p), Some("pkg/sibling.py"));
        assert_eq!(f.r("pkg/main.py", "..outer", p), Some("outer.py"));
        assert_eq!(f.r("pkg/main.py", "external", p), None);
        assert_eq!(f.r("pkg/main.py", ".stub", p), None);
        assert_eq!(f.r("main.py", "....x", p), None);
    }

    #[test]
    fn script_style_and_html_paths() {
        let f = fixture(&[
            ("web/main.ts", ""),
            ("web/mod.ts", ""),
            ("web/sub/index.js", ""),
            ("web/style.css", ""),
            ("web/other.css", ""),
            ("web/img.png", ""),
            ("web/util.ts", ""),
            ("A.md", ""),
            ("web/s/_part.scss", ""),
            ("web/s/main.scss", ""),
            ("index.js", ""),
        ]);
        let s = RefMode::Script;
        assert_eq!(f.r("web/main.ts", "./mod", s), Some("web/mod.ts"));
        assert_eq!(f.r("web/main.ts", "./sub", s), Some("web/sub/index.js"));
        assert_eq!(f.r("web/main.ts", "./style.css", s), Some("web/style.css"));
        assert_eq!(f.r("web/main.ts", "external", s), None);
        assert_eq!(f.r("web/main.ts", "/web/mod", s), None);
        // `.js` is not rewritten to `.ts`
        assert_eq!(f.r("web/main.ts", "./util.js", s), None);
        assert_eq!(f.r("sub.js", ".", s), Some("index.js"));
        assert_eq!(
            f.r("web/style.css", "other.css", RefMode::Plain),
            Some("web/other.css")
        );
        assert_eq!(
            f.r("web/style.css", "img.png", RefMode::Plain),
            Some("web/img.png")
        );
        // Sass partials and extensionless names are not searched
        assert_eq!(f.r("web/s/main.scss", "part", RefMode::Plain), None);
        assert_eq!(
            f.r("web/page.html", "../A.md", RefMode::Plain),
            Some("A.md")
        );
        assert_eq!(f.r("web/page.html", "/A.md", RefMode::Plain), None);
    }

    #[test]
    fn targets_are_decoded_and_cut_at_anchors() {
        let f = fixture(&[("a b.md", ""), ("c.md", "")]);
        assert_eq!(
            f.r("x.md", "a%20b.md#top", RefMode::Markdown),
            Some("a b.md")
        );
        assert_eq!(f.r("x.md", "c.md?x=1", RefMode::Markdown), Some("c.md"));
        assert_eq!(f.r("x.md", "c%23h", RefMode::Wiki), Some("c.md"));
    }
}
