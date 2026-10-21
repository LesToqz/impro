//! Bounded, display-independent Git decorations for a filesystem explorer.
//!
//! `git status --porcelain=v1 -z` is read incrementally from a pipe, so even pathological
//! output stays bounded: at most [`MAX_ENTRIES`] records, [`MAX_OUTPUT`] bytes and
//! [`TIMEOUT`] of wall-clock time. The module never touches GTK and is meant to run on a
//! worker thread.
use crate::pool::CancelToken;
use impro_core::fsops::absolute;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{self, Read};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub const MAX_ENTRIES: usize = 20_000;
pub const MAX_OUTPUT: usize = 16 * 1024 * 1024;
pub const TIMEOUT: Duration = Duration::from_secs(5);
const CONFLICTS: [&str; 7] = ["UU", "AA", "DD", "AU", "UA", "DU", "UD"];

/// Decoration class of a path; also the suffix of the theme role `git_<kind>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    New,
    Modified,
    Conflict,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::New => "new",
            Kind::Modified => "modified",
            Kind::Conflict => "conflict",
        }
    }
    /// A folder shows the strongest status among its descendants.
    fn priority(self) -> u8 {
        match self {
            Kind::New => 1,
            Kind::Modified => 2,
            Kind::Conflict => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Status {
    pub kind: Kind,
}

impl Status {
    pub const NEW: Status = Status { kind: Kind::New };
    pub const MODIFIED: Status = Status {
        kind: Kind::Modified,
    };
    pub const CONFLICT: Status = Status {
        kind: Kind::Conflict,
    };
}

/// Decorations for every changed file and (aggregated) folder under the explorer root.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub files: HashMap<PathBuf, Status>,
    pub folders: HashMap<PathBuf, Status>,
    pub git_dir: Option<PathBuf>,
    pub truncated: bool,
}

impl Snapshot {
    /// Paths whose decoration differs between `self` and `newer` (added, removed or changed).
    pub fn diff<'a>(&'a self, newer: &'a Snapshot) -> Vec<&'a Path> {
        fn changes<'a>(
            old: &'a HashMap<PathBuf, Status>,
            new: &'a HashMap<PathBuf, Status>,
            out: &mut Vec<&'a Path>,
        ) {
            out.extend(
                new.iter()
                    .filter(|(path, status)| old.get(*path) != Some(*status))
                    .map(|(path, _)| path.as_path()),
            );
            out.extend(
                old.keys()
                    .filter(|path| !new.contains_key(*path))
                    .map(PathBuf::as_path),
            );
        }
        let mut out = Vec::new();
        changes(&self.files, &newer.files, &mut out);
        changes(&self.folders, &newer.folders, &mut out);
        out
    }
}

/// Bounds of one status collection; the defaults are the module constants.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_entries: usize,
    pub max_output: usize,
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: MAX_ENTRIES,
            max_output: MAX_OUTPUT,
            timeout: TIMEOUT,
        }
    }
}

/// Output of one `git` invocation; `truncated` when a bound stopped the read early.
#[derive(Debug)]
pub struct Output {
    pub data: Vec<u8>,
    pub truncated: bool,
}

fn finish(mut child: Child, deadline: Instant) -> io::Result<std::process::ExitStatus> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(io::ErrorKind::TimedOut, "git did not exit"));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Read a pipe incrementally so even pathological status output is bounded.
///
/// `record_limit` counts NUL-terminated records; reaching it (or `max_output` bytes)
/// kills the process and reports a truncated read instead of an error.
pub fn run(
    root: &Path,
    args: &[&str],
    deadline: Instant,
    record_limit: Option<usize>,
    max_output: usize,
    cancel: Option<&CancelToken>,
) -> io::Result<Output> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("git stdout unavailable"))?;
    let fd = stdout.as_raw_fd();
    let mut data = Vec::new();
    let mut records = 0usize;
    let mut truncated = false;
    let mut buffer = vec![0u8; 65536];
    let failure = loop {
        if cancel.is_some_and(CancelToken::is_cancelled) {
            break Some(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break Some(io::Error::new(io::ErrorKind::TimedOut, "git timed out"));
        }
        let mut poll = libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        };
        let wait = remaining.min(Duration::from_millis(25)).as_millis() as i32;
        // SAFETY: `poll` is a valid pollfd array of length one for the open pipe `fd`.
        let ready = unsafe { libc::poll(&mut poll, 1, wait.max(1)) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            break Some(error);
        }
        if ready == 0 {
            continue;
        }
        let want = 65536.min(max_output.saturating_sub(data.len()));
        if want == 0 {
            truncated = true;
            break None;
        }
        match stdout.read(&mut buffer[..want]) {
            Ok(0) => break None,
            Ok(count) => {
                data.extend_from_slice(&buffer[..count]);
                records += buffer[..count].iter().filter(|b| **b == 0).count();
                if data.len() >= max_output || record_limit.is_some_and(|limit| records >= limit) {
                    truncated = true;
                    break None;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => break Some(error),
        }
    };
    if failure.is_some() || truncated {
        let _ = child.kill();
    }
    drop(stdout);
    let status = finish(
        child,
        deadline.max(Instant::now() + Duration::from_millis(1)),
    );
    if let Some(error) = failure {
        return Err(error);
    }
    let status = status?;
    if !status.success() && !truncated {
        return Err(io::Error::other(format!("git exited with {status}")));
    }
    Ok(Output { data, truncated })
}

/// Parse v1 `-z` output (rename destination first), including unloaded descendants.
pub fn parse_porcelain(data: &[u8], repo: &Path, root: &Path, max_entries: usize) -> Snapshot {
    let mut result = Snapshot::default();
    // A byte cap may end halfway through a filename; never invent that row.
    let mut records: Vec<&[u8]> = data.split(|b| *b == 0).collect();
    records.pop();
    let resolve = |bytes: &[u8]| absolute(repo.join(OsStr::from_bytes(bytes)));
    let mut count = 0usize;
    let mut index = 0usize;
    while index < records.len() {
        let record = records[index];
        index += 1;
        if record.is_empty() {
            continue;
        }
        if count >= max_entries {
            result.truncated = true;
            break;
        }
        count += 1;
        if record.len() < 4 || record[2] != b' ' {
            continue;
        }
        let code = String::from_utf8_lossy(&record[..2]).into_owned();
        let path = resolve(&record[3..]);
        let renamed = code.contains('R') || code.contains('C');
        let source = if renamed {
            let source = records.get(index).copied().unwrap_or(b"");
            index += usize::from(index < records.len());
            Some(resolve(source))
        } else {
            None
        };
        let status = if CONFLICTS.contains(&code.as_str()) {
            Status::CONFLICT
        } else if code == "??" {
            Status::NEW
        } else if code.contains('D') || renamed || code.contains('M') || code.contains('T') {
            Status::MODIFIED
        } else if code.contains('A') {
            Status::NEW
        } else {
            continue;
        };
        // Deleted paths contribute to folders, but have no file decoration.
        if path.starts_with(root) {
            if !code.contains('D') || CONFLICTS.contains(&code.as_str()) {
                result.files.insert(path.clone(), status);
            }
            aggregate(&mut result, &path, root, status.kind);
        }
        // A rename also modifies its old parent, including moves out of root.
        if let Some(source) = source {
            if code.contains('R') && source.starts_with(root) {
                aggregate(&mut result, &source, root, Kind::Modified);
            }
        }
    }
    result
}

fn aggregate(result: &mut Snapshot, path: &Path, root: &Path, kind: Kind) {
    let mut parent = path.parent();
    while let Some(folder) = parent.filter(|p| p.starts_with(root)) {
        let stronger = result
            .folders
            .get(folder)
            .is_none_or(|previous| kind.priority() > previous.kind.priority());
        if stronger {
            result.folders.insert(folder.to_path_buf(), Status { kind });
        }
        if folder == root {
            break;
        }
        parent = folder.parent();
    }
}

/// Collect decorations for `root`: empty on a missing Git or repository, errors or timeout.
pub fn collect_status(root: &Path, max_entries: usize) -> Snapshot {
    let limits = Limits {
        max_entries,
        ..Limits::default()
    };
    collect_status_with(root, &limits, None)
}

/// [`collect_status`] with explicit bounds and an optional cancellation token.
pub fn collect_status_with(root: &Path, limits: &Limits, cancel: Option<&CancelToken>) -> Snapshot {
    let root = absolute(root);
    let deadline = Instant::now() + limits.timeout;
    let attempt = || -> io::Result<Snapshot> {
        let top = run(
            &root,
            &["rev-parse", "--show-toplevel"],
            deadline,
            None,
            limits.max_output,
            cancel,
        )?;
        let repo = PathBuf::from(OsStr::from_bytes(trim_newline(&top.data)));
        let git_dir = run(
            &root,
            &["rev-parse", "--absolute-git-dir"],
            deadline,
            None,
            limits.max_output,
            cancel,
        )?;
        let output = run(
            &root,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
                "--ignored=no",
            ],
            deadline,
            Some(limits.max_entries.max(1) * 2 + 1),
            limits.max_output,
            cancel,
        )?;
        let mut result = parse_porcelain(&output.data, &repo, &root, limits.max_entries);
        result.git_dir = Some(PathBuf::from(OsStr::from_bytes(trim_newline(
            &git_dir.data,
        ))));
        result.truncated |= output.truncated;
        Ok(result)
    };
    attempt().unwrap_or_default()
}

fn trim_newline(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().rposition(|b| *b != b'\n').map_or(0, |i| i + 1);
    &bytes[..end]
}
