//! The project index: every note, source file and (possibly referenced)
//! attachment under a root, with parsed and resolved references.
//!
//! Files live in a slab addressed by [`FileId`]. Updates re-parse only the
//! changed file and re-resolve only the references that could be affected by
//! it (found through a key table), and report what changed.

mod entry;
mod resolve;
mod scan;

pub use entry::{
    kind_of, parse_entry, EdgeKind, FileEntry, FileId, FileKind, Ref, RefMode, Resolved,
    CODE_EXTENSIONS, NOTE_EXTENSIONS,
};
pub use resolve::Resolver;
pub use scan::{CancelToken, Cancelled, ScanConfig, MAX_FILE_SIZE, SKIP_DIRS};

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use resolve::{lower, Lookup};

use crate::parse::text::basename;

/// What a mutation of the index did, by project-relative path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexChange {
    pub added: Vec<Arc<str>>,
    pub removed: Vec<Arc<str>>,
    /// Content (text, tags, aliases or references) changed.
    pub modified: Vec<Arc<str>>,
    /// Content unchanged, but some references now resolve differently
    /// because another file appeared, vanished or changed its aliases.
    pub rerouted: Vec<Arc<str>>,
}

impl IndexChange {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.modified.is_empty()
            && self.rerouted.is_empty()
    }

    pub fn merge(&mut self, other: IndexChange) {
        self.added.extend(other.added);
        self.removed.extend(other.removed);
        self.modified.extend(other.modified);
        self.rerouted.extend(other.rerouted);
    }
}

#[derive(Default, Debug)]
struct Watchers {
    by_key: HashMap<Box<str>, Vec<FileId>>,
    /// Sources with a reference that can match anything (`..`, `.`).
    wild: Vec<FileId>,
}

pub struct ProjectIndex {
    root: PathBuf,
    config: ScanConfig,
    files: Vec<Option<FileEntry>>,
    free: Vec<u32>,
    live: usize,
    /// Case-sensitive truth: relative path -> file.
    exact: HashMap<Arc<str>, FileId>,
    lookup: Lookup,
    watchers: Watchers,
    /// Unsaved buffer texts that shadow the files on disk.
    overrides: HashMap<String, String>,
}

impl ProjectIndex {
    /// An empty index for `root`; use [`ProjectIndex::scan`] to fill it.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_config(root, ScanConfig::default())
    }

    pub fn with_config(root: impl Into<PathBuf>, config: ScanConfig) -> Self {
        let root = root.into();
        let root = std::path::absolute(&root).unwrap_or(root);
        ProjectIndex {
            root,
            config,
            files: Vec::new(),
            free: Vec::new(),
            live: 0,
            exact: HashMap::new(),
            lookup: Lookup::default(),
            watchers: Watchers::default(),
            overrides: HashMap::new(),
        }
    }

    /// Scans `root` on up to `config.threads` threads; stops early (with
    /// `Err(Cancelled)`) once `cancel` fires.
    pub fn scan(
        root: impl Into<PathBuf>,
        config: ScanConfig,
        cancel: &CancelToken,
    ) -> Result<Self, Cancelled> {
        let mut index = Self::with_config(root, config);
        index.fill(cancel)?;
        Ok(index)
    }

    /// Rebuilds everything from disk, keeping unsaved buffer overrides.
    pub fn rescan(&mut self, cancel: &CancelToken) -> Result<(), Cancelled> {
        let mut fresh = Self::with_config(self.root.clone(), self.config.clone());
        fresh.fill(cancel)?;
        let overrides = std::mem::take(&mut self.overrides);
        *self = fresh;
        for (path, text) in overrides {
            self.update_file(Path::new(&path), Some(&text));
        }
        Ok(())
    }

    fn fill(&mut self, cancel: &CancelToken) -> Result<(), Cancelled> {
        let entries = scan::read_all(&self.root, &self.config, cancel)?;
        self.files.reserve(entries.len());
        for e in entries {
            self.insert_slot(e);
        }
        let resolved = scan::resolve_all(&self.files, &self.lookup, self.config.threads, cancel)?;
        for (slot, res) in self.files.iter_mut().zip(resolved) {
            if let Some(f) = slot {
                f.resolved = res;
            }
        }
        Ok(())
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &ScanConfig {
        &self.config
    }

    /// Number of indexed files.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    pub fn get(&self, id: FileId) -> Option<&FileEntry> {
        self.files.get(id.index()).and_then(Option::as_ref)
    }

    /// Slab length; valid [`FileId`]s are below this.
    pub fn slots(&self) -> usize {
        self.files.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (FileId, &FileEntry)> {
        self.files
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.as_ref().map(|f| (FileId(i as u32), f)))
    }

    /// The file at exactly this project-relative path.
    pub fn id_of(&self, rel: &str) -> Option<FileId> {
        self.exact.get(rel).copied()
    }

    /// Case-insensitive lookup, as references see paths.
    pub fn id_of_folded(&self, rel: &str) -> Option<FileId> {
        self.lookup.by_path.get(lower(rel).as_str()).copied()
    }

    pub fn entry_by_path(&self, rel: &str) -> Option<&FileEntry> {
        self.id_of(rel).and_then(|id| self.get(id))
    }

    /// Resolves a reference as the graph would. Returns the file path.
    pub fn resolve_ref(&self, source: &str, target: &str, mode: RefMode) -> Option<&str> {
        let r = Resolver {
            files: &self.files,
            lookup: &self.lookup,
        };
        let id = r.resolve(source, target, mode).dest?;
        self.get(id).map(|f| &*f.path)
    }

    /// All files that match a bare wiki name from `source`, best first.
    pub fn wiki_candidates(&self, source: &str, target: &str) -> Vec<FileId> {
        Resolver {
            files: &self.files,
            lookup: &self.lookup,
        }
        .wiki_candidates(crate::parse::text::dirname(source), target)
    }

    /// Project-relative `/` path for `path`, or `None` when it is outside the
    /// root or below an ignored directory.
    pub fn rel_path(&self, path: &Path) -> Option<String> {
        let joined;
        let p = if path.is_absolute() {
            path
        } else {
            joined = self.root.join(path);
            &joined
        };
        let mut parts: Vec<&str> = Vec::new();
        let rel = lexical_strip(p, &self.root)?;
        for c in rel.components() {
            match c {
                Component::Normal(s) => parts.push(s.to_str()?),
                Component::CurDir => {}
                Component::ParentDir => {
                    parts.pop()?;
                }
                _ => return None,
            }
        }
        if parts.is_empty() {
            return None;
        }
        let dirs = &parts[..parts.len() - 1];
        if dirs.iter().any(|d| self.config.ignores(d)) {
            return None;
        }
        Some(parts.join("/"))
    }

    // ---- registration -------------------------------------------------

    fn insert_slot(&mut self, entry: FileEntry) -> FileId {
        let id = match self.free.pop() {
            Some(i) => {
                self.files[i as usize] = Some(entry);
                FileId(i)
            }
            None => {
                self.files.push(Some(entry));
                FileId(self.files.len() as u32 - 1)
            }
        };
        self.live += 1;
        self.register(id);
        id
    }

    fn take_slot(&mut self, id: FileId) -> Option<FileEntry> {
        self.deregister(id);
        let e = self.files[id.index()].take()?;
        self.free.push(id.0);
        self.live -= 1;
        Some(e)
    }

    fn register(&mut self, id: FileId) {
        let (path, name, aliases, keys, wild) = {
            let e = match self.get(id) {
                Some(e) => e,
                None => return,
            };
            let mut keys = Vec::new();
            let mut wild = false;
            for r in &e.refs {
                if !entry::ref_keys(r, &mut keys) {
                    wild = true;
                }
            }
            keys.sort_unstable();
            keys.dedup();
            (
                e.path.clone(),
                lower(basename(&e.path)),
                e.aliases.iter().map(|a| lower(a)).collect::<Vec<_>>(),
                keys,
                wild,
            )
        };
        self.exact.insert(path.clone(), id);
        self.fold_insert(id, &path);
        self.lookup.by_name.entry(name.into()).or_default().push(id);
        for a in aliases {
            let bucket = self.lookup.by_alias.entry(a.into()).or_default();
            if !bucket.contains(&id) {
                bucket.push(id);
            }
        }
        for k in keys {
            self.watchers.by_key.entry(k.into()).or_default().push(id);
        }
        if wild {
            self.watchers.wild.push(id);
        }
    }

    /// Case-folded path table: when two paths fold together the smaller wins.
    fn fold_insert(&mut self, id: FileId, path: &str) {
        let key: Box<str> = lower(path).into();
        match self.lookup.by_path.get(&key).copied() {
            Some(other) if other != id => {
                let theirs = self.get(other).map(|f| f.path.clone());
                let replace = match theirs {
                    Some(t) => *t > *path,
                    None => true,
                };
                if replace {
                    self.lookup.by_path.insert(key, id);
                }
            }
            _ => {
                self.lookup.by_path.insert(key, id);
            }
        }
    }

    fn deregister(&mut self, id: FileId) {
        let (path, name, aliases, keys, wild) = {
            let e = match self.get(id) {
                Some(e) => e,
                None => return,
            };
            let mut keys = Vec::new();
            let mut wild = false;
            for r in &e.refs {
                if !entry::ref_keys(r, &mut keys) {
                    wild = true;
                }
            }
            keys.sort_unstable();
            keys.dedup();
            (
                e.path.clone(),
                lower(basename(&e.path)),
                e.aliases.iter().map(|a| lower(a)).collect::<Vec<_>>(),
                keys,
                wild,
            )
        };
        self.exact.remove(&path);
        let folded = lower(&path);
        if self.lookup.by_path.get(folded.as_str()) == Some(&id) {
            self.lookup.by_path.remove(folded.as_str());
            // another path may fold to the same key (case-only difference)
            let rival = self
                .lookup
                .by_name
                .get(name.as_str())
                .into_iter()
                .flatten()
                .copied()
                .filter(|o| *o != id)
                .filter_map(|o| self.get(o).map(|f| (o, f.path.clone())))
                .filter(|(_, p)| lower(p) == folded)
                .min_by(|a, b| a.1.cmp(&b.1));
            if let Some((o, _)) = rival {
                self.lookup.by_path.insert(folded.into(), o);
            }
        }
        remove_from(&mut self.lookup.by_name, &name, id);
        for a in aliases {
            remove_from(&mut self.lookup.by_alias, &a, id);
        }
        for k in keys {
            remove_from(&mut self.watchers.by_key, &k, id);
        }
        if wild {
            self.watchers.wild.retain(|x| *x != id);
        }
    }

    // ---- resolution ---------------------------------------------------

    fn resolve_entry(&self, id: FileId) -> Vec<Resolved> {
        let entry = match self.get(id) {
            Some(e) => e,
            None => return Vec::new(),
        };
        let r = Resolver {
            files: &self.files,
            lookup: &self.lookup,
        };
        entry
            .refs
            .iter()
            .map(|rf| r.resolve(&entry.path, &rf.target, rf.mode))
            .collect()
    }

    /// Re-resolves `id`; true when the outcome differs from before.
    fn refresh(&mut self, id: FileId) -> bool {
        let fresh = self.resolve_entry(id);
        match self.files[id.index()].as_mut() {
            Some(e) if e.resolved != fresh => {
                e.resolved = fresh;
                true
            }
            _ => false,
        }
    }

    /// Files whose references might point at (or away from) `entry`.
    fn sources_affected_by(&self, entry: &FileEntry, out: &mut Vec<FileId>) {
        let mut keys = Vec::new();
        entry::file_keys(entry, &mut keys);
        for k in &keys {
            if let Some(v) = self.watchers.by_key.get(k.as_str()) {
                out.extend(v);
            }
        }
        out.extend(&self.watchers.wild);
    }

    // ---- mutation -----------------------------------------------------

    fn commit(&mut self, rel: &str, new: Option<FileEntry>, change: &mut IndexChange) {
        let old_id = self.id_of(rel);
        let mut affected: Vec<FileId> = Vec::new();
        let path: Arc<str>;
        let mut subject: Option<FileId> = None;
        match (old_id, new) {
            (None, None) => return,
            (Some(id), None) => {
                if let Some(e) = self.get(id) {
                    self.sources_affected_by(e, &mut affected);
                }
                let gone = self.take_slot(id);
                path = gone.map(|e| e.path).unwrap_or_else(|| Arc::from(rel));
                change.removed.push(path.clone());
            }
            (None, Some(e)) => {
                path = e.path.clone();
                let id = self.insert_slot(e);
                if let Some(e) = self.get(id) {
                    self.sources_affected_by(e, &mut affected);
                }
                subject = Some(id);
                change.added.push(path.clone());
            }
            (Some(id), Some(mut e)) => {
                let old = match self.get(id) {
                    Some(o) => o,
                    None => return,
                };
                if old.same_content(&e) {
                    if let Some(slot) = self.files[id.index()].as_mut() {
                        slot.ctime = e.ctime;
                    }
                    return;
                }
                path = e.path.clone();
                let aliases_changed = old.aliases != e.aliases;
                if aliases_changed {
                    self.sources_affected_by(old, &mut affected);
                }
                e.resolved = old.resolved.clone();
                e.resolved.resize(e.refs.len(), Resolved::default());
                self.deregister(id);
                self.files[id.index()] = Some(e);
                self.register(id);
                if aliases_changed {
                    if let Some(e) = self.get(id) {
                        self.sources_affected_by(e, &mut affected);
                    }
                }
                subject = Some(id);
                change.modified.push(path.clone());
            }
        }
        if let Some(id) = subject {
            self.refresh(id);
        }
        affected.sort_unstable();
        affected.dedup();
        for id in affected {
            if Some(id) == subject {
                continue;
            }
            if self.get(id).is_some() && self.refresh(id) {
                if let Some(e) = self.get(id) {
                    let p = e.path.clone();
                    if !change.modified.contains(&p)
                        && !change.added.contains(&p)
                        && !change.rerouted.contains(&p)
                    {
                        change.rerouted.push(p);
                    }
                }
            }
        }
    }

    fn load(&self, rel: &str) -> Option<FileEntry> {
        if let Some(text) = self.overrides.get(rel) {
            let ctime = scan::ctime_of(&self.root.join(rel));
            return Some(parse_entry(Arc::from(rel), text.clone(), ctime));
        }
        scan::read_entry(&self.root, rel, &self.config)
    }

    /// Re-reads `path`. With `Some(text)` the text replaces the on-disk
    /// content (an unsaved buffer); with `None` any buffer is dropped and the
    /// file is read from disk again (removed if it no longer exists).
    pub fn update_file(&mut self, path: &Path, text: Option<&str>) -> IndexChange {
        let mut change = IndexChange::default();
        let rel = match self.rel_path(path) {
            Some(r) => r,
            None => return change,
        };
        match text {
            Some(t) => {
                self.overrides.insert(rel.clone(), t.to_string());
            }
            None => {
                self.overrides.remove(&rel);
            }
        }
        let new = self.load(&rel);
        self.commit(&rel, new, &mut change);
        change
    }

    /// Forgets a file, or every file below a directory path.
    pub fn remove(&mut self, path: &Path) -> IndexChange {
        let mut change = IndexChange::default();
        let rel = match self.rel_path(path) {
            Some(r) => r,
            None => return change,
        };
        self.remove_rel(&rel, &mut change);
        change
    }

    fn remove_rel(&mut self, rel: &str, change: &mut IndexChange) {
        let prefix = format!("{rel}/");
        let mut doomed: Vec<Arc<str>> = self
            .exact
            .keys()
            .filter(|p| {
                let s: &str = p;
                s == rel || s.starts_with(&prefix)
            })
            .cloned()
            .collect();
        doomed.sort();
        self.overrides
            .retain(|k, _| k != rel && !k.starts_with(&prefix));
        for p in doomed {
            self.commit(&p, None, change);
        }
    }

    /// Applies a file or directory rename that already happened on disk.
    pub fn rename(&mut self, from: &Path, to: &Path) -> IndexChange {
        let mut change = IndexChange::default();
        let from_rel = self.rel_path(from);
        let to_rel = self.rel_path(to);
        if let Some(f) = &from_rel {
            self.move_overrides(f, to_rel.as_deref());
            self.remove_rel(f, &mut change);
        }
        if let Some(t) = &to_rel {
            let abs = self.root.join(t);
            let md = std::fs::symlink_metadata(&abs);
            match md {
                Ok(m) if m.is_dir() => {
                    let cancel = CancelToken::default();
                    if let Ok(found) = scan::list_files(&abs, t, &self.config, &cancel) {
                        for rel in found {
                            let new = self.load(&rel);
                            self.commit(&rel, new, &mut change);
                        }
                    }
                }
                Ok(_) => {
                    let new = self.load(t);
                    self.commit(t, new, &mut change);
                }
                Err(_) => {
                    if self.overrides.contains_key(t.as_str()) {
                        let new = self.load(t);
                        self.commit(t, new, &mut change);
                    }
                }
            }
        }
        change
    }

    fn move_overrides(&mut self, from: &str, to: Option<&str>) {
        let prefix = format!("{from}/");
        let moved: Vec<String> = self
            .overrides
            .keys()
            .filter(|k| *k == from || k.starts_with(&prefix))
            .cloned()
            .collect();
        for k in moved {
            if let Some(text) = self.overrides.remove(&k) {
                if let Some(t) = to {
                    let new_key = format!("{t}{}", &k[from.len()..]);
                    self.overrides.insert(new_key, text);
                }
            }
        }
    }

    /// Paths of all indexed files, sorted (for diagnostics and tests).
    pub fn paths(&self) -> Vec<Arc<str>> {
        let mut v: Vec<_> = self.iter().map(|(_, f)| f.path.clone()).collect();
        v.sort();
        v
    }

    /// True if the resolved state equals a from-scratch resolution (used by
    /// tests to prove that incremental updates stay exact).
    pub fn resolution_is_consistent(&self) -> bool {
        self.iter()
            .all(|(id, f)| f.resolved == self.resolve_entry(id))
    }
}

fn remove_from(map: &mut HashMap<Box<str>, Vec<FileId>>, key: &str, id: FileId) {
    if let Some(v) = map.get_mut(key) {
        if let Some(pos) = v.iter().position(|x| *x == id) {
            v.swap_remove(pos);
        }
        if v.is_empty() {
            map.remove(key);
        }
    }
}

/// `path` relative to `root`, comparing components lexically after removing
/// `.` and `..` from `path`.
fn lexical_strip(path: &Path, root: &Path) -> Option<PathBuf> {
    let mut norm = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                norm.pop();
            }
            Component::CurDir => {}
            other => norm.push(other.as_os_str()),
        }
    }
    let mut root_norm = PathBuf::new();
    for c in root.components() {
        match c {
            Component::ParentDir => {
                root_norm.pop();
            }
            Component::CurDir => {}
            other => root_norm.push(other.as_os_str()),
        }
    }
    norm.strip_prefix(&root_norm).ok().map(Path::to_path_buf)
}
