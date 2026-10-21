//! Directory walking and parallel file reading with cancellation.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use super::entry::{kind_of, parse_entry, FileEntry, FileKind, Resolved};
use super::resolve::{Lookup, Resolver};
use crate::parse::text::basename;

/// Largest text file that is indexed.
pub const MAX_FILE_SIZE: u64 = 1024 * 1024;

/// Directories that never contribute files.
pub const SKIP_DIRS: [&str; 11] = [
    ".git",
    ".hg",
    ".svn",
    "__pycache__",
    "node_modules",
    ".venv",
    "venv",
    ".mypy_cache",
    ".pytest_cache",
    ".tox",
    ".idea",
];

#[derive(Clone, Debug)]
pub struct ScanConfig {
    ignore: HashSet<String>,
    /// Text sources larger than this are skipped.
    pub max_file_size: u64,
    /// Worker threads for reading and resolving; 0 picks a bounded default.
    pub threads: usize,
}

impl Default for ScanConfig {
    fn default() -> Self {
        let mut ignore: HashSet<String> = SKIP_DIRS.iter().map(|s| s.to_string()).collect();
        ignore.insert(".ceo".to_string());
        ScanConfig {
            ignore,
            max_file_size: MAX_FILE_SIZE,
            threads: 0,
        }
    }
}

impl ScanConfig {
    pub fn ignores(&self, dir_name: &str) -> bool {
        self.ignore.contains(dir_name)
    }

    pub fn with_ignored(mut self, names: impl IntoIterator<Item = String>) -> Self {
        self.ignore = names.into_iter().collect();
        self
    }

    fn worker_count(&self, jobs: usize) -> usize {
        let auto = std::thread::available_parallelism()
            .map_or(2, |n| n.get())
            .min(8);
        let n = if self.threads == 0 {
            auto
        } else {
            self.threads
        };
        n.clamp(1, 32).min(jobs.max(1))
    }
}

/// Cooperative cancellation shared between the UI and workers.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("scan cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// Relative paths of all regular files below `dir`, which sits at `rel_prefix`
/// inside the project ("" for the root). Symlinks are never followed.
pub fn list_files(
    dir: &Path,
    rel_prefix: &str,
    config: &ScanConfig,
    cancel: &CancelToken,
) -> Result<Vec<String>, Cancelled> {
    let mut out = Vec::new();
    if !rel_prefix.is_empty() && config.ignores(basename(rel_prefix)) {
        return Ok(out);
    }
    let mut stack: Vec<(PathBuf, String)> = vec![(dir.to_path_buf(), rel_prefix.to_string())];
    while let Some((path, rel)) = stack.pop() {
        if cancel.is_cancelled() {
            return Err(Cancelled);
        }
        let rd = match fs::read_dir(&path) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for entry in rd.flatten() {
            let name = match entry.file_name().into_string() {
                Ok(n) => n,
                Err(_) => continue,
            };
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if ft.is_dir() {
                if !config.ignores(&name) {
                    stack.push((entry.path(), child));
                }
            } else if ft.is_file() {
                out.push(child);
            }
        }
    }
    Ok(out)
}

pub fn ctime_of(path: &Path) -> Option<f64> {
    fs::symlink_metadata(path).ok().and_then(|m| times(&m))
}

fn times(m: &fs::Metadata) -> Option<f64> {
    let secs = |t: std::time::SystemTime| {
        t.duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    };
    if let Ok(created) = m.created() {
        return Some(secs(created));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let c = m.ctime() as f64 + m.ctime_nsec() as f64 / 1e9;
        let w = m.mtime() as f64 + m.mtime_nsec() as f64 / 1e9;
        Some(c.min(w))
    }
    #[cfg(not(unix))]
    {
        m.modified().ok().map(secs)
    }
}

fn read_checked(
    root: &Path,
    rel: &str,
    config: &ScanConfig,
    check_ancestors: bool,
) -> Option<FileEntry> {
    let abs = root.join(rel);
    if check_ancestors {
        let mut cur = root.to_path_buf();
        let parts: Vec<&str> = rel.split('/').collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            cur.push(part);
            if fs::symlink_metadata(&cur).ok()?.file_type().is_symlink() {
                return None;
            }
        }
    }
    let meta = fs::symlink_metadata(&abs).ok()?;
    if !meta.is_file() {
        return None;
    }
    let ctime = times(&meta);
    let path: Arc<str> = Arc::from(rel);
    if kind_of(rel) == FileKind::Attachment {
        // resolvable by name; contents are never needed
        return Some(parse_entry(path, String::new(), ctime));
    }
    if meta.len() > config.max_file_size {
        return None;
    }
    let mut data = Vec::with_capacity(meta.len() as usize + 1);
    fs::File::open(&abs)
        .ok()?
        .take(config.max_file_size + 1)
        .read_to_end(&mut data)
        .ok()?;
    if data.len() as u64 > config.max_file_size || data.contains(&0) {
        return None;
    }
    let text = String::from_utf8(data).ok()?;
    Some(parse_entry(path, text, ctime))
}

/// Reads one project file from disk (symlinks, directories, oversized,
/// binary and non-UTF-8 text files yield `None`).
pub fn read_entry(root: &Path, rel: &str, config: &ScanConfig) -> Option<FileEntry> {
    read_checked(root, rel, config, true)
}

/// Walks and reads the whole project on a bounded set of threads. Entries come
/// back sorted by path so file ids are deterministic.
pub fn read_all(
    root: &Path,
    config: &ScanConfig,
    cancel: &CancelToken,
) -> Result<Vec<FileEntry>, Cancelled> {
    let mut paths = list_files(root, "", config, cancel)?;
    paths.sort_unstable();
    let cursor = AtomicUsize::new(0);
    const CHUNK: usize = 32;
    let workers = config.worker_count(paths.len() / CHUNK + 1);
    let mut parts: Vec<Vec<(usize, FileEntry)>> = Vec::new();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        if cancel.is_cancelled() {
                            break;
                        }
                        let start = cursor.fetch_add(CHUNK, Ordering::Relaxed);
                        if start >= paths.len() {
                            break;
                        }
                        for (i, rel) in paths.iter().enumerate().skip(start).take(CHUNK) {
                            if let Some(e) = read_checked(root, rel, config, false) {
                                mine.push((i, e));
                            }
                        }
                    }
                    mine
                })
            })
            .collect();
        for h in handles {
            if let Ok(v) = h.join() {
                parts.push(v);
            }
        }
    });
    if cancel.is_cancelled() {
        return Err(Cancelled);
    }
    let mut all: Vec<(usize, FileEntry)> = parts.into_iter().flatten().collect();
    all.sort_unstable_by_key(|(i, _)| *i);
    Ok(all.into_iter().map(|(_, e)| e).collect())
}

/// Resolves every reference of every file on a bounded set of threads.
pub fn resolve_all(
    files: &[Option<FileEntry>],
    lookup: &Lookup,
    threads: usize,
    cancel: &CancelToken,
) -> Result<Vec<Vec<Resolved>>, Cancelled> {
    let config = ScanConfig {
        threads,
        ..ScanConfig::default()
    };
    const CHUNK: usize = 64;
    let cursor = AtomicUsize::new(0);
    let workers = config.worker_count(files.len() / CHUNK + 1);
    let resolver = Resolver { files, lookup };
    let mut parts: Vec<Vec<(usize, Vec<Resolved>)>> = Vec::new();
    std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        if cancel.is_cancelled() {
                            break;
                        }
                        let start = cursor.fetch_add(CHUNK, Ordering::Relaxed);
                        if start >= files.len() {
                            break;
                        }
                        for (i, f) in files.iter().enumerate().skip(start).take(CHUNK) {
                            if let Some(f) = f {
                                let r = f
                                    .refs
                                    .iter()
                                    .map(|r| resolver.resolve(&f.path, &r.target, r.mode))
                                    .collect();
                                mine.push((i, r));
                            }
                        }
                    }
                    mine
                })
            })
            .collect();
        for h in handles {
            if let Ok(v) = h.join() {
                parts.push(v);
            }
        }
    });
    if cancel.is_cancelled() {
        return Err(Cancelled);
    }
    let mut out: Vec<Vec<Resolved>> = vec![Vec::new(); files.len()];
    for (i, r) in parts.into_iter().flatten() {
        out[i] = r;
    }
    Ok(out)
}
