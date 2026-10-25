//! Project search core, independent of GTK.
//!
//! Match coordinates are zero-based *character* offsets (what `GtkTextBuffer` counts), not
//! bytes. A match's preview string carries the character offset of its shortened display
//! line. Everything here runs on worker threads; the panel in `searchpanel.rs` only
//! renders what this module streams.
use crate::pool::CancelToken;
use impro_core::{fsops::absolute, text::casefold};
use regex::{Regex, RegexBuilder};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Directories that are never searched or listed by the quick switcher.
pub const SKIP_DIRS: [&str; 12] = [
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
    ".ceo",
];
pub const MAX_FILE_SIZE: usize = 1024 * 1024;
/// Longest preview line shown for a match, in characters.
pub const MAX_PREVIEW: usize = 200;

pub fn is_skip_dir(name: &str) -> bool {
    #[cfg(unix)]
    return SKIP_DIRS.contains(&name);
    #[cfg(windows)]
    return SKIP_DIRS.iter().any(|skip| skip.eq_ignore_ascii_case(name));
}

/// [`SKIP_DIRS`] in alphabetical order (the settings page lists them that way).
pub fn skip_dirs_sorted() -> Vec<&'static str> {
    let mut names = SKIP_DIRS.to_vec();
    names.sort_unstable();
    names
}

/// A query the user can correct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchError(pub String);

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SearchError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub column: usize,
    pub length: usize,
    pub line_text: String,
    /// Character offset of `line_text` within the original line.
    pub preview_offset: usize,
}

/// Content hits for one absolute path, in source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileGroup {
    pub path: PathBuf,
    pub relative_path: String,
    pub matches: Vec<Match>,
    pub is_current: bool,
}

/// Content groups followed by separate absolute basename hits.
///
/// Totals describe retained content matches only. Cancelled scans return their partial
/// results; `truncated` indicates an actual match omitted by a limit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub groups: Vec<FileGroup>,
    pub file_names: Vec<PathBuf>,
    pub truncated: bool,
    pub total_matches: usize,
    pub total_files: usize,
}

#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub regex: bool,
    pub current_path: Option<PathBuf>,
    pub max_per_file: usize,
    pub max_total: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: false,
            regex: false,
            current_path: None,
            max_per_file: 200,
            max_total: 1000,
        }
    }
}

/// Live text for paths: `Some` overrides the disk (unsaved editor buffers).
pub type TextProvider<'a> = &'a dyn Fn(&Path) -> Option<String>;

/// Live-text lookup for a list of paths: one main-thread round trip per call.
pub type Overlay<'a> = &'a mut dyn FnMut(&[PathBuf]) -> Vec<Option<String>>;

/// Read a file as search text: regular, at most 1 MiB, no NUL in its first 8 KiB, UTF-8.
pub fn read_text(path: &Path) -> Option<String> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_SIZE as u64 {
        return None;
    }
    let mut file = std::fs::File::open(path).ok()?;
    // Binary files are rejected after their first block instead of being read whole.
    let mut data = vec![0u8; (meta.len() as usize).min(8192)];
    file.read_exact(&mut data).ok()?;
    if data.contains(&0) {
        return None;
    }
    data.reserve((meta.len() as usize).saturating_sub(data.len()) + 1);
    file.take(MAX_FILE_SIZE as u64 + 1 - data.len() as u64)
        .read_to_end(&mut data)
        .ok()?;
    if data.len() > MAX_FILE_SIZE {
        return None;
    }
    String::from_utf8(data).ok()
}

type WalkHook = Arc<dyn Fn(&CancelToken) + Send + Sync>;

fn walk_hooks() -> &'static Mutex<HashMap<PathBuf, WalkHook>> {
    static HOOKS: OnceLock<Mutex<HashMap<PathBuf, WalkHook>>> = OnceLock::new();
    HOOKS.get_or_init(Default::default)
}

/// Test seam: run `hook` whenever a search enumerates `root` (e.g. to make a scan slow).
#[doc(hidden)]
pub fn set_walk_hook(root: &Path, hook: Option<WalkHook>) {
    let mut hooks = walk_hooks().lock().unwrap_or_else(|e| e.into_inner());
    match hook {
        Some(hook) => hooks.insert(root.to_path_buf(), hook),
        None => hooks.remove(root),
    };
}

pub(crate) fn fold(text: &str) -> String {
    if text.is_ascii() {
        text.to_ascii_lowercase()
    } else {
        casefold(text)
    }
}

/// Path of `path` relative to `root`, with `..` components when it lies outside.
pub fn relative_to(path: &Path, root: &Path) -> String {
    if let Ok(rest) = path.strip_prefix(root) {
        return rest.to_string_lossy().into_owned();
    }
    let a: Vec<_> = path.components().collect();
    let b: Vec<_> = root.components().collect();
    let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let mut out = PathBuf::new();
    for _ in common..b.len() {
        out.push("..");
    }
    for part in &a[common..] {
        out.push(part.as_os_str());
    }
    if out.as_os_str().is_empty() {
        ".".into()
    } else {
        out.to_string_lossy().into_owned()
    }
}

/// Every regular, non-symlink file below `root` outside [`SKIP_DIRS`], sorted by
/// case-folded relative path. Files are not opened here.
pub fn project_files(root: &Path, cancel: Option<&CancelToken>) -> Vec<PathBuf> {
    let root = absolute(root);
    if let Some(hook) = walk_hooks()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&root)
        .cloned()
    {
        hook(cancel.unwrap_or(&CancelToken::new()));
    }
    let mut paths = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(directory) = stack.pop() {
        if cancel.is_some_and(CancelToken::is_cancelled) {
            return Vec::new();
        }
        let Ok(listing) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in listing.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !is_skip_dir(&entry.file_name().to_string_lossy()) {
                    stack.push(entry.path());
                }
            } else if kind.is_file() {
                paths.push(entry.path());
            }
        }
    }
    if cancel.is_some_and(CancelToken::is_cancelled) {
        return Vec::new();
    }
    paths.sort_by_cached_key(|path| {
        let relative = relative_to(path, &root);
        (fold(&relative), relative)
    });
    paths
}

/// Eligible absolute text-file paths in stable relative-path order.
pub fn iter_project_files(root: &Path) -> impl Iterator<Item = PathBuf> {
    project_files(root, None)
        .into_iter()
        .filter(|path| read_text(path).is_some())
}

/// Files for the quick switcher: [`SKIP_DIRS`] and (unless `show_hidden`) dot-entries are
/// left out; directories are visited depth-first in name order, at most `limit` files.
pub fn list_project_files(root: &Path, show_hidden: bool, limit: usize) -> Vec<PathBuf> {
    fn visit(dir: &Path, show_hidden: bool, limit: usize, out: &mut Vec<PathBuf>) {
        if out.len() >= limit {
            return;
        }
        let Ok(listing) = std::fs::read_dir(dir) else {
            return;
        };
        let (mut dirs, mut files) = (Vec::new(), Vec::new());
        for entry in listing.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_dir = entry
                .file_type()
                .map(|kind| kind.is_dir() || (kind.is_symlink() && entry.path().is_dir()))
                .unwrap_or(false);
            let symlink = entry.file_type().is_ok_and(|kind| kind.is_symlink());
            if is_dir {
                if !is_skip_dir(&name) && (show_hidden || !name.starts_with('.')) && !symlink {
                    dirs.push((name, entry.path()));
                }
            } else if show_hidden || !name.starts_with('.') {
                files.push((name, entry.path()));
            }
        }
        files.sort();
        dirs.sort();
        for (_, path) in files {
            out.push(path);
            if out.len() >= limit {
                return;
            }
        }
        for (_, path) in dirs {
            visit(&path, show_hidden, limit, out);
            if out.len() >= limit {
                return;
            }
        }
    }
    let mut out = Vec::new();
    visit(&absolute(root), show_hidden, limit, &mut out);
    out
}

fn describe(error: &regex::Error) -> String {
    match error {
        regex::Error::Syntax(message) => message
            .lines()
            .rev()
            .find_map(|line| line.strip_prefix("error: "))
            .unwrap_or(message)
            .to_owned(),
        regex::Error::CompiledTooBig(_) => "pattern too large".to_owned(),
        other => other.to_string(),
    }
}

fn pattern(query: &str, case_sensitive: bool, regex: bool) -> Result<Regex, SearchError> {
    let source = if regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    RegexBuilder::new(&source)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|error| SearchError(format!("Invalid regular expression: {}", describe(&error))))
}

/// `str.isspace()` of the original implementation (adds the ASCII separators U+001C..1F).
fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Lines split at every Unicode line boundary; a trailing separator adds no empty line.
pub fn splitlines(text: &str) -> impl Iterator<Item = &str> {
    let bytes = text.as_bytes();
    let (mut start, mut scan) = (0usize, 0usize);
    std::iter::from_fn(move || {
        if start >= bytes.len() {
            return None;
        }
        while scan < bytes.len() {
            // Separators start on a character boundary: ASCII, U+0085 or U+2028/2029.
            let width = match bytes[scan] {
                b'\n' | 0x0b | 0x0c | 0x1c | 0x1d | 0x1e => 1,
                b'\r' if bytes.get(scan + 1) == Some(&b'\n') => 2,
                b'\r' => 1,
                0xc2 if bytes.get(scan + 1) == Some(&0x85) => 2,
                0xe2 if bytes.get(scan + 1) == Some(&0x80)
                    && matches!(bytes.get(scan + 2), Some(0xa8 | 0xa9)) =>
                {
                    3
                }
                _ => 0,
            };
            if width > 0 {
                let line = &text[start..scan];
                scan += width;
                start = scan;
                return Some(line);
            }
            scan += 1;
        }
        let line = &text[start..];
        start = bytes.len();
        Some(line)
    })
}

/// Call `accept` for every non-empty match, line by line; stop when it returns `false`.
fn scan_text(text: &str, pattern: &Regex, mut accept: impl FnMut(Match) -> bool) {
    for (line_no, line) in splitlines(text).enumerate() {
        let (mut byte_pos, mut char_pos) = (0usize, 0usize);
        for found in pattern.find_iter(line) {
            if found.is_empty() {
                continue;
            }
            char_pos += line[byte_pos..found.start()].chars().count();
            byte_pos = found.start();
            let length = line[found.range()].chars().count();
            if !accept(preview(
                line_no,
                line,
                char_pos,
                found.start(),
                found.end(),
                length,
            )) {
                return;
            }
        }
    }
}

/// Shorten a long line around the match, keeping a little leading context.
fn preview(
    line_no: usize,
    line: &str,
    start_c: usize,
    start_b: usize,
    end_b: usize,
    length: usize,
) -> Match {
    // Retain whitespace that is itself part of the match.
    let stripped = line.trim_end_matches(is_space).len();
    let display = &line[..stripped.max(end_b)];
    // Leave room for the highlighted match in a narrow sidebar. Only the characters just
    // before the match matter, so long lines stay linear.
    let indent = display
        .chars()
        .take(start_c)
        .take_while(|c| is_space(*c))
        .count();
    let mut offset_c = (indent.min(start_c) as isize).max(start_c as isize - 4) as usize;
    let back = start_c - offset_c;
    let mut offset_b = if back == 0 {
        start_b
    } else {
        line[..start_b]
            .char_indices()
            .rev()
            .nth(back - 1)
            .map_or(0, |(i, _)| i)
    };
    for c in line[offset_b..start_b].chars() {
        if is_space(c) {
            offset_c += 1;
            offset_b += c.len_utf8();
        } else {
            break;
        }
    }
    let tail = &display[offset_b..];
    let cut = tail
        .char_indices()
        .nth(MAX_PREVIEW)
        .map_or(tail.len(), |(i, _)| i);
    Match {
        line: line_no,
        column: start_c,
        length,
        line_text: tail[..cut].to_owned(),
        preview_offset: offset_c,
    }
}

/// All matches of `query` in `text`; `preview_offset` locates the shortened line text.
pub fn find_in_text(
    text: &str,
    query: &str,
    case_sensitive: bool,
    regex: bool,
) -> Result<Vec<Match>, SearchError> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = pattern(query, case_sensitive, regex)?;
    let mut found = Vec::new();
    scan_text(text, &pattern, |m| {
        found.push(m);
        true
    });
    Ok(found)
}

/// Search streaming: the current file first, then sorted project contents and basenames.
///
/// `progress` runs after the current file and after every project file with the results so
/// far (cheap to ignore, or to flush on a timer). Cancelling returns the partial results.
/// Totals count displayed content matches; `truncated` means at least one actual content
/// match was omitted.
pub fn search_stream(
    root: &Path,
    query: &str,
    options: &SearchOptions,
    mut overlay: Option<Overlay<'_>>,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(&SearchResults),
) -> Result<SearchResults, SearchError> {
    let mut result = SearchResults::default();
    if query.is_empty() {
        progress(&result);
        return Ok(result);
    }
    let pattern = pattern(query, options.case_sensitive, options.regex)?;
    let root = if root.as_os_str().is_empty() {
        PathBuf::new()
    } else {
        absolute(root)
    };
    let current = options.current_path.as_deref().map(absolute);
    let (max_per_file, max_total) = (options.max_per_file, options.max_total);
    // A literal query that is absent from the whole text cannot match on any line.
    let prefilter = !options.regex;

    let scan = |result: &mut SearchResults, path: &Path, text: &str| {
        if prefilter && !pattern.is_match(text) {
            return;
        }
        let mut matches = Vec::new();
        let mut truncated = false;
        let mut total = result.total_matches;
        scan_text(text, &pattern, |m| {
            if matches.len() >= max_per_file || total >= max_total {
                truncated = true;
                return false;
            }
            matches.push(m);
            total += 1;
            true
        });
        result.truncated |= truncated;
        result.total_matches = total;
        if !matches.is_empty() {
            let relative_path = if root.as_os_str().is_empty() {
                path.to_string_lossy().into_owned()
            } else {
                relative_to(path, &root)
            };
            result.groups.push(FileGroup {
                path: path.to_path_buf(),
                relative_path,
                matches,
                is_current: current.as_deref() == Some(path),
            });
            result.total_files += 1;
        }
    };

    if let Some(current) = current.as_deref().filter(|_| !cancel.is_cancelled()) {
        let live = overlay.as_mut().and_then(|overlay| {
            overlay(&[current.to_path_buf()])
                .into_iter()
                .next()
                .flatten()
        });
        if let Some(text) = live.or_else(|| read_text(current)) {
            scan(&mut result, current, &text);
        }
    }
    // The first update precedes even enumerating the project.
    progress(&result);
    if root.as_os_str().is_empty() {
        return Ok(result);
    }
    let paths = project_files(&root, Some(cancel));
    let needle = fold(query);
    // One round trip for every path: the provider only looks open buffers up, and a single
    // snapshot keeps the scan consistent (and fast while the main thread repaints).
    let live = overlay
        .as_mut()
        .map(|overlay| overlay(&paths))
        .unwrap_or_default();
    for (index, path) in paths.iter().enumerate() {
        if cancel.is_cancelled() {
            return Ok(result);
        }
        // Eligibility comes from the disk, as for the quick listing of text files.
        let Some(disk) = read_text(path) else {
            continue;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        if fold(&name).contains(&needle) {
            result.file_names.push(path.clone());
        }
        if current.as_deref() != Some(path.as_path()) {
            let text = live.get(index).and_then(|t| t.as_deref()).unwrap_or(&disk);
            scan(&mut result, path, text);
        }
        progress(&result);
    }
    Ok(result)
}

/// Blocking search with an optional per-path live-text provider (any thread).
pub fn search(
    root: &Path,
    query: &str,
    options: &SearchOptions,
    provider: Option<TextProvider<'_>>,
    cancel: Option<&CancelToken>,
) -> Result<SearchResults, SearchError> {
    let mut chunked = |paths: &[PathBuf]| -> Vec<Option<String>> {
        paths
            .iter()
            .map(|path| provider.and_then(|provider| provider(path)))
            .collect()
    };
    let overlay: Option<Overlay<'_>> = provider.map(|_| &mut chunked as Overlay<'_>);
    let none = CancelToken::new();
    search_stream(
        root,
        query,
        options,
        overlay,
        cancel.unwrap_or(&none),
        &mut |_| {},
    )
}
