//! Turns reference text into project files. Matching is case-insensitive and
//! every strategy returns the first hit in its documented probe order.

use std::collections::HashMap;

use super::entry::{
    FileEntry, FileId, RefMode, Resolved, NOTE_EXTENSIONS,
};
use crate::parse::text::{basename, dirname, extension, is_external, stem};

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

pub fn lower(s: &str) -> String {
    if s.is_ascii() {
        s.to_ascii_lowercase()
    } else {
        s.to_lowercase()
    }
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

const SCRIPT_EXTENSIONS: [&str; 10] = [
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".json", ".css", ".vue", ".cjs", ".svelte",
];

pub struct Resolver<'a> {
    pub files: &'a [Option<FileEntry>],
    pub lookup: &'a Lookup,
}

impl<'a> Resolver<'a> {
    fn path_of(&self, id: FileId) -> &str {
        self.files[id.index()]
            .as_ref()
            .map_or("", |f| &f.path)
    }

    /// Exact (case-insensitive) project path; `joined` is already normalised.
    fn exact(&self, joined: &str) -> Option<FileId> {
        if joined.is_empty() {
            return None;
        }
        self.lookup.by_path.get(lower(joined).as_str()).copied()
    }

    fn exact_join(&self, scratch: &mut String, base: &str, rel: &str) -> Option<FileId> {
        if !join_norm(scratch, base, rel) {
            return None;
        }
        self.exact(scratch)
    }

    /// First probe `base + suffix` that exists.
    fn with_suffixes(&self, scratch: &mut String, base: &str, rel: &str, suffixes: &[&str]) -> Option<FileId> {
        if !join_norm(scratch, base, rel) {
            return None;
        }
        let len = scratch.len();
        for s in suffixes {
            scratch.truncate(len);
            scratch.push_str(s);
            if let Some(id) = self.exact(scratch) {
                return Some(id);
            }
        }
        None
    }

    /// Resolves `target` written in `source`; also reports how many files
    /// matched a bare wiki name (0 when another strategy applied).
    pub fn resolve(&self, source: &str, target: &str, mode: RefMode) -> Resolved {
        let target = target
            .split(['#', '^', '?'])
            .next()
            .unwrap_or("");
        let web = matches!(
            mode,
            RefMode::Wiki | RefMode::Markdown | RefMode::Html | RefMode::Style
        );
        if target.is_empty() || (web && is_external(target)) {
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
            RefMode::Script => self.script(folder, target, &mut scratch),
            RefMode::Style => self.style(source, folder, target, &mut scratch),
            RefMode::Html => self.from_folder_or_root(folder, target, &mut scratch),
            RefMode::RustMod => self.rust_mod(source, target, &mut scratch),
            RefMode::RustUse => self.rust_use(source, target, &mut scratch),
            RefMode::Shell | RefMode::Include => {
                self.exact_join(&mut scratch, folder, target)
                    .or_else(|| self.exact_join(&mut scratch, "", target))
            }
            RefMode::Ruby => {
                let sfx: &[&str] = if extension(target).is_empty() {
                    &[".rb", ""]
                } else {
                    &[""]
                };
                self.with_suffixes(&mut scratch, folder, target, sfx)
            }
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
            (dirname(p) != folder, p.chars().count(), lower(p), p.to_string())
        });
        found.dedup();
        found
    }

    fn wiki(&self, folder: &str, target: &str, scratch: &mut String) -> (Option<FileId>, bool) {
        if target.contains('/') {
            let suffixes: &[&str] = if extension(target).is_empty() {
                &[".md", ".markdown"]
            } else {
                &[""]
            };
            for base in ["", folder] {
                if let Some(id) = self.with_suffixes(scratch, base, target, suffixes) {
                    return (Some(id), false);
                }
            }
            return (None, false);
        }
        let found = self.wiki_candidates(folder, target);
        (found.first().copied(), found.len() > 1)
    }

    fn markdown(&self, folder: &str, target: &str, scratch: &mut String) -> Option<FileId> {
        let rel = target.trim_start_matches('/');
        self.exact_join(scratch, folder, rel)
            .or_else(|| self.exact_join(scratch, "", rel))
    }

    fn from_folder_or_root(&self, folder: &str, target: &str, scratch: &mut String) -> Option<FileId> {
        match target.strip_prefix('/') {
            Some(rest) => self.exact_join(scratch, "", rest),
            None => self.exact_join(scratch, folder, target),
        }
    }

    fn python(&self, folder: &str, spec: &str, scratch: &mut String) -> Option<FileId> {
        let level = spec.len() - spec.trim_start_matches('.').len();
        let module = spec[level..].replace('.', "/");
        if module.is_empty() {
            return None;
        }
        let try_base = |this: &Self, base: &str, scratch: &mut String| {
            this.with_suffixes(scratch, base, &module, &[".py", "/__init__.py", ".pyi"])
        };
        if level > 0 {
            let mut base = folder.to_string();
            for _ in 1..level {
                let mut up = String::new();
                if !join_norm(&mut up, &base, "..") {
                    return None;
                }
                base = up;
            }
            return try_base(self, &base, scratch);
        }
        ["", folder, "src"]
            .into_iter()
            .find_map(|b| try_base(self, b, scratch))
    }

    fn script(&self, folder: &str, spec: &str, scratch: &mut String) -> Option<FileId> {
        if !spec.starts_with('.') {
            return None;
        }
        if let Some(id) = self.exact_join(scratch, folder, spec) {
            return Some(id);
        }
        // ESM style `./x.js` that is really `x.ts`
        for (from, to) in [(".js", ".ts"), (".jsx", ".tsx"), (".mjs", ".mts"), (".cjs", ".cts")] {
            if let Some(stem) = spec.strip_suffix(from) {
                let swapped = format!("{stem}{to}");
                if let Some(id) = self.exact_join(scratch, folder, &swapped) {
                    return Some(id);
                }
                if from == ".js" {
                    let tsx = format!("{stem}.tsx");
                    if let Some(id) = self.exact_join(scratch, folder, &tsx) {
                        return Some(id);
                    }
                }
            }
        }
        if let Some(id) = self.with_suffixes(scratch, folder, spec, &SCRIPT_EXTENSIONS) {
            return Some(id);
        }
        let dir_index: Vec<String> = SCRIPT_EXTENSIONS.iter().map(|e| format!("/index{e}")).collect();
        let refs: Vec<&str> = dir_index.iter().map(String::as_str).collect();
        self.with_suffixes(scratch, folder, spec, &refs)
    }

    fn style(&self, source: &str, folder: &str, spec: &str, scratch: &mut String) -> Option<FileId> {
        let (base_dir, rel) = match spec.strip_prefix('/') {
            Some(r) => ("", r),
            None => (folder, spec),
        };
        if let Some(id) = self.exact_join(scratch, base_dir, rel) {
            return Some(id);
        }
        let sass = matches!(extension(source).as_str(), ".scss" | ".sass");
        if !sass {
            return None;
        }
        // Sass: optional extension, `_partial` names and directory indexes
        let mut full = String::new();
        if !join_norm(&mut full, base_dir, rel) {
            return None;
        }
        let (dir, name) = match full.rsplit_once('/') {
            Some((d, n)) => (d.to_string(), n.to_string()),
            None => (String::new(), full.clone()),
        };
        let has_ext = !extension(&name).is_empty();
        let exts: &[&str] = if has_ext { &[""] } else { &[".scss", ".sass", ".css"] };
        for e in exts {
            let candidates = [
                format!("{name}{e}"),
                format!("_{name}{e}"),
            ];
            for c in candidates {
                if let Some(id) = self.exact_join(scratch, &dir, &c) {
                    return Some(id);
                }
            }
        }
        for idx in ["_index.scss", "index.scss", "_index.sass", "index.sass"] {
            let rel = format!("{name}/{idx}");
            if let Some(id) = self.exact_join(scratch, &dir, &rel) {
                return Some(id);
            }
        }
        None
    }

    /// Directory in which `mod x;` declarations of `source` live.
    fn module_dir(source: &str) -> String {
        let dir = dirname(source);
        match stem(source) {
            "mod" | "lib" | "main" => dir.to_string(),
            s => {
                if dir.is_empty() {
                    s.to_string()
                } else {
                    format!("{dir}/{s}")
                }
            }
        }
    }

    fn rust_module_file(&self, scratch: &mut String, dir: &str, name: &str) -> Option<FileId> {
        let a = format!("{name}.rs");
        let b = format!("{name}/mod.rs");
        let rel_a = a.as_str();
        let rel_b = b.as_str();
        self.exact_join(scratch, dir, rel_a)
            .or_else(|| self.exact_join(scratch, dir, rel_b))
    }

    fn rust_mod(&self, source: &str, name: &str, scratch: &mut String) -> Option<FileId> {
        let dir = Self::module_dir(source);
        self.rust_module_file(scratch, &dir, name)
    }

    /// Directory of the crate root: the nearest `src` ancestor, preferring one
    /// that holds `lib.rs` or `main.rs`.
    fn crate_root(&self, source: &str, scratch: &mut String) -> Option<String> {
        let mut dir = dirname(source);
        let mut first_src = None;
        loop {
            if basename(dir) == "src" {
                first_src.get_or_insert_with(|| dir.to_string());
                if ["lib.rs", "main.rs"]
                    .iter()
                    .any(|f| self.exact_join(scratch, dir, f).is_some())
                {
                    return Some(dir.to_string());
                }
            }
            if dir.is_empty() {
                break;
            }
            dir = dirname(dir);
        }
        first_src
    }

    fn rust_dir_file(&self, scratch: &mut String, dir: &str) -> Option<FileId> {
        for f in ["mod.rs", "lib.rs", "main.rs"] {
            if let Some(id) = self.exact_join(scratch, dir, f) {
                return Some(id);
            }
        }
        let name = format!("{}.rs", basename(dir));
        self.exact_join(scratch, dirname(dir), &name)
    }

    fn rust_use(&self, source: &str, spec: &str, scratch: &mut String) -> Option<FileId> {
        let mut segs: Vec<&str> = spec.split("::").filter(|s| !s.is_empty()).collect();
        if segs.is_empty() {
            return None;
        }
        let head = segs.remove(0);
        let mut dir = match head {
            "crate" => self.crate_root(source, scratch)?,
            "self" => Self::module_dir(source),
            "super" => {
                let mut d = Self::module_dir(source);
                d = dirname(&d).to_string();
                while segs.first() == Some(&"super") {
                    segs.remove(0);
                    d = dirname(&d).to_string();
                }
                d
            }
            _ => return None,
        };
        let base_dir = dir.clone();
        let mut best = None;
        for seg in &segs {
            match self.rust_module_file(scratch, &dir, seg) {
                Some(id) => {
                    best = Some(id);
                    dir = if dir.is_empty() {
                        seg.to_string()
                    } else {
                        format!("{dir}/{seg}")
                    };
                }
                None => break,
            }
        }
        let found = best.or_else(|| self.rust_dir_file(scratch, &base_dir));
        found.filter(|id| self.path_of(*id) != source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::entry::parse_entry;
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
            lookup.by_name.entry(lower(basename(p)).into()).or_default().push(id);
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
            ("alias.md", "---\naliases: [Nickname, \"Other Name\"]\n---\n"),
        ]);
        let w = RefMode::Wiki;
        let m = RefMode::Markdown;
        assert_eq!(f.r("a/Source.md", "note", w), Some("a/Note.md"));
        assert_eq!(f.r("a/Source.md", "some/x", w), Some("some/X.md"));
        assert_eq!(f.r("a/Source.md", "space name.md", m), Some("a/space name.md"));
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
        assert_eq!(f.r("z/deep/er/x.md", "t", RefMode::Wiki), Some("z/deep/er/T.md"));
    }

    #[test]
    fn filenames_precede_aliases() {
        let f = fixture(&[
            ("Real.md", ""),
            ("other.md", "---\nalias: Real\n---\n"),
        ]);
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
        ]);
        let p = RefMode::Python;
        assert_eq!(f.r("pkg/main.py", "root", p), Some("root.py"));
        // root before source folder
        assert_eq!(f.r("pkg/main.py", "util", p), Some("util.py"));
        assert_eq!(f.r("pkg/main.py", "module", p), Some("src/module/__init__.py"));
        assert_eq!(f.r("pkg/main.py", ".sibling", p), Some("pkg/sibling.py"));
        assert_eq!(f.r("pkg/main.py", "..outer", p), Some("outer.py"));
        assert_eq!(f.r("pkg/main.py", "external", p), None);
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
            ("web/s/theme/_index.scss", ""),
        ]);
        assert_eq!(f.r("web/main.ts", "./mod", RefMode::Script), Some("web/mod.ts"));
        assert_eq!(f.r("web/main.ts", "./sub", RefMode::Script), Some("web/sub/index.js"));
        assert_eq!(f.r("web/main.ts", "./style.css", RefMode::Script), Some("web/style.css"));
        assert_eq!(f.r("web/main.ts", "./util.js", RefMode::Script), Some("web/util.ts"));
        assert_eq!(f.r("web/main.ts", "external", RefMode::Script), None);
        assert_eq!(f.r("web/style.css", "other.css", RefMode::Style), Some("web/other.css"));
        assert_eq!(f.r("web/style.css", "img.png", RefMode::Style), Some("web/img.png"));
        assert_eq!(f.r("web/s/main.scss", "part", RefMode::Style), Some("web/s/_part.scss"));
        assert_eq!(f.r("web/s/main.scss", "theme", RefMode::Style), Some("web/s/theme/_index.scss"));
        assert_eq!(f.r("web/page.html", "../A.md", RefMode::Html), Some("A.md"));
        assert_eq!(f.r("web/page.html", "/A.md", RefMode::Html), Some("A.md"));
    }

    #[test]
    fn rust_module_paths() {
        let f = fixture(&[
            ("crates/x/src/lib.rs", ""),
            ("crates/x/src/a.rs", ""),
            ("crates/x/src/a/b.rs", ""),
            ("crates/x/src/c/mod.rs", ""),
            ("crates/x/src/c/d.rs", ""),
        ]);
        let m = RefMode::RustMod;
        let u = RefMode::RustUse;
        assert_eq!(f.r("crates/x/src/lib.rs", "a", m), Some("crates/x/src/a.rs"));
        assert_eq!(f.r("crates/x/src/lib.rs", "c", m), Some("crates/x/src/c/mod.rs"));
        assert_eq!(f.r("crates/x/src/a.rs", "b", m), Some("crates/x/src/a/b.rs"));
        assert_eq!(f.r("crates/x/src/c/mod.rs", "d", m), Some("crates/x/src/c/d.rs"));
        assert_eq!(f.r("crates/x/src/c/d.rs", "crate::a::b::Thing", u), Some("crates/x/src/a/b.rs"));
        assert_eq!(f.r("crates/x/src/c/d.rs", "super::Item", u), Some("crates/x/src/c/mod.rs"));
        assert_eq!(f.r("crates/x/src/c/d.rs", "crate::Item", u), Some("crates/x/src/lib.rs"));
        assert_eq!(f.r("crates/x/src/a.rs", "self::b", u), Some("crates/x/src/a/b.rs"));
        assert_eq!(f.r("crates/x/src/lib.rs", "crate::Item", u), None);
        assert_eq!(f.r("crates/x/src/a.rs", "std::io", u), None);
    }

    #[test]
    fn shell_include_ruby() {
        let f = fixture(&[
            ("bin/run.sh", ""),
            ("bin/lib.sh", ""),
            ("conf/env.sh", ""),
            ("c/a.c", ""),
            ("c/a.h", ""),
            ("rb/x.rb", ""),
            ("rb/y.rb", ""),
        ]);
        assert_eq!(f.r("bin/run.sh", "./lib.sh", RefMode::Shell), Some("bin/lib.sh"));
        assert_eq!(f.r("bin/run.sh", "conf/env.sh", RefMode::Shell), Some("conf/env.sh"));
        assert_eq!(f.r("c/a.c", "a.h", RefMode::Include), Some("c/a.h"));
        assert_eq!(f.r("rb/x.rb", "y", RefMode::Ruby), Some("rb/y.rb"));
    }
}
