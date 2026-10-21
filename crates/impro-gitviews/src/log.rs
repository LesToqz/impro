//! Bounded, read-only commit graph data, independent of a display.
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

pub const TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_OUTPUT: usize = 16 * 1024 * 1024;
pub const LOG_FORMAT: &str = "%H%x00%h%x00%P%x00%s%x00%an%x00%ae%x00%at%x00%ai%x00%ct";
/// Locale-independent public Git error code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitError(pub &'static str);
impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for GitError {}

/// Drain incrementally with a shared deadline; always kill and reap the child.
pub fn run(
    root: &Path,
    args: &[&str],
    deadline: Instant,
    cap: usize,
    partial: bool,
) -> Result<(Vec<u8>, bool), GitError> {
    let (mut bytes, clipped) = run_with(root, args, deadline, cap, partial, move |reader| {
        let mut bytes = Vec::with_capacity(cap.min(65536));
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    })?;
    bytes.truncate(cap);
    Ok((bytes, clipped))
}
fn run_with<T: Send + 'static>(
    root: &Path,
    args: &[&str],
    deadline: Instant,
    cap: usize,
    partial: bool,
    parse: impl FnOnce(&mut dyn Read) -> std::io::Result<T> + Send + 'static,
) -> Result<(T, bool), GitError> {
    let mut child = ChildGuard(
        Command::new("git")
            .args(["--no-pager", "-c", "core.quotepath=off", "-C"])
            .arg(root)
            .args(args)
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("LC_ALL", "C")
            .env("GIT_PAGER", "cat")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                GitError(if e.kind() == std::io::ErrorKind::NotFound {
                    "git-missing"
                } else {
                    "not-a-repo"
                })
            })?,
    );
    let stdout = child.stdout.take().ok_or(GitError("git-failed"))?;
    let (tx, rx) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let limit = cap.saturating_add(1) as u64;
        let mut source = stdout.take(limit);
        let result = parse(&mut source);
        let clipped = limit - source.limit() > cap as u64;
        let _ = tx.send(result.map(|data| (data, clipped)));
    });
    let received = rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    // A reader finishing doesn't imply process exit (a child may close stdout then hang).
    let mut result = match received {
        Ok(Ok((data, clipped))) => {
            if clipped {
                let _ = child.kill();
            }
            if clipped && !partial {
                Err(GitError("output-too-large"))
            } else {
                Ok((data, clipped))
            }
        }
        Ok(Err(_)) => Err(GitError("git-failed")),
        Err(_) => Err(GitError("timeout")),
    };
    if result.is_err() {
        let _ = child.kill();
    }
    // A sleeping watchdog also covers processes that close stdout before hanging.
    let signal = std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let wake = signal.clone();
    let pid = child.id().to_string();
    let watchdog = std::thread::spawn(move || {
        let (lock, condition) = &*wake;
        let finished = lock.lock().unwrap_or_else(|e| e.into_inner());
        let (finished, _) = condition
            .wait_timeout_while(
                finished,
                deadline.saturating_duration_since(Instant::now()),
                |done| !*done,
            )
            .unwrap_or_else(|e| e.into_inner());
        if !*finished {
            // std has no timed wait or clonable Child kill handle. On Unix, kill(1)
            // interrupts wait without polling or unsafe platform calls.
            let _ = Command::new("/bin/kill")
                .args(["-KILL", &pid])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    });
    let status = child.wait();
    let timed_out = Instant::now() >= deadline;
    let (lock, condition) = &*signal;
    *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
    condition.notify_one();
    let _ = watchdog.join();
    if timed_out {
        result = Err(GitError("timeout"));
    } else if !status.is_ok_and(|s| s.success())
        && result.as_ref().is_ok_and(|(_, clipped)| !clipped)
    {
        result = Err(GitError("git-failed"));
    }
    let _ = reader.join();
    result
}
struct ChildGuard(std::process::Child);
impl std::ops::Deref for ChildGuard {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Replace invalid UTF-8 and remove controls; path controls are escaped to retain identity.
pub fn clean(value: &[u8], cap: Option<usize>, layout: bool, path: bool) -> String {
    let mut output = String::with_capacity(value.len().min(cap.unwrap_or(value.len())));
    for c in String::from_utf8_lossy(value).chars() {
        let control = c.is_control()
            || matches!(c as u32, 0xad | 0x600..=0x605 | 0x61c | 0x6dd | 0x70f | 0x890..=0x891 | 0x8e2 | 0x180e | 0x200b..=0x200f | 0x202a..=0x202e | 0x2060..=0x2064 | 0x2066..=0x206f | 0xfeff | 0xfff9..=0xfffb | 0x110bd | 0x110cd | 0x13430..=0x1343f | 0x1bca0..=0x1bca3 | 0x1d173..=0x1d17a | 0xe0001 | 0xe0020..=0xe007f);
        if control && !(layout && matches!(c, '\n' | '\r' | '\t')) {
            if path {
                use std::fmt::Write;
                let _ = write!(output, "\\u{:04x}", c as u32);
            }
        } else {
            output.push(c);
        }
    }
    if let Some(cap) = cap {
        if output.chars().count() > cap {
            output = output.chars().take(cap.saturating_sub(1)).collect();
            output.push('…');
        }
    }
    output
}
pub fn timezone_minutes(date: &str) -> i64 {
    let s = date.rsplit(' ').next().unwrap_or("").as_bytes();
    if s.len() == 5 && matches!(s[0], b'+' | b'-') && s[1..].iter().all(u8::is_ascii_digit) {
        let n = ((s[1] - b'0') as i64 * 10 + (s[2] - b'0') as i64) * 60
            + (s[3] - b'0') as i64 * 10
            + (s[4] - b'0') as i64;
        if s[0] == b'-' {
            -n
        } else {
            n
        }
    } else {
        0
    }
}
pub fn infer_merged_name(subject: &[u8]) -> Option<String> {
    let s = clean(subject, Some(4096), false, false);
    let name = if let Some(rest) = s.strip_prefix("Merge pull request #") {
        let (number, rest) = rest.split_once(" from ")?;
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let (owner, branch) = rest.split_once('/')?;
        if owner.is_empty() || owner.chars().any(char::is_whitespace) {
            return None;
        }
        branch.split_whitespace().next()?
    } else if let Some(rest) = s.strip_prefix("Merge ceo/") {
        let (name, _) = rest.split_once(':')?;
        if name.is_empty() || name.chars().any(char::is_whitespace) {
            return None;
        }
        return Some(clean(
            format!("ceo/{name}").as_bytes(),
            Some(300),
            false,
            false,
        ));
    } else {
        let (rest, remote) = if let Some(r) = s.strip_prefix("Merge remote-tracking branch ") {
            (r, true)
        } else {
            let r = s
                .strip_prefix("Merge branch ")
                .or_else(|| s.strip_prefix("Merge tag "))?;
            (r, false)
        };
        let quote = rest.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        let rest = &rest[1..];
        let end = rest.char_indices().find_map(|(i, c)| {
            let tail = &rest[i + c.len_utf8()..];
            (c == quote && (tail.is_empty() || tail.starts_with(" into "))).then_some(i)
        })?;
        let name = &rest[..end];
        if remote {
            name.split_once('/')
                .filter(|(_, n)| !n.is_empty())
                .map_or(name, |(_, n)| n)
        } else {
            name
        }
    };
    if name.is_empty() {
        None
    } else {
        Some(clean(name.as_bytes(), Some(300), false, false))
    }
}
fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
fn number(bytes: &[u8]) -> i64 {
    String::from_utf8_lossy(bytes).parse().unwrap_or(0)
}
pub(crate) fn valid_hash(s: &[u8]) -> bool {
    s.len() == 40
        && s.iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}
/// Parse fixed-width NUL fields without copying the complete output or field list.
pub fn parse_log(data: &[u8]) -> Vec<Value> {
    parse_log_stream(&mut std::io::Cursor::new(data)).unwrap_or_default()
}
fn parse_log_stream(source: &mut dyn Read) -> std::io::Result<Vec<Value>> {
    let mut source = BufReader::with_capacity(65536, source);
    let mut row: [Vec<u8>; 9] = std::array::from_fn(|_| Vec::with_capacity(128));
    let mut commits = Vec::with_capacity(1500);
    loop {
        for field in &mut row {
            field.clear();
            if source.read_until(0, field)? == 0 {
                return Ok(commits);
            }
            if field.last() == Some(&0) {
                field.pop();
            }
        }
        if !valid_hash(&row[0]) {
            continue;
        }
        let parents: Vec<_> = String::from_utf8_lossy(&row[2])
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        commits.push(json!({"hash":text(&row[0]),"short":text(&row[1]),"merged_name":if parents.len()>=2 {infer_merged_name(&row[3])} else {None},"parents":parents,"subject":clean(&row[3],Some(300),false,false),"author":clean(&row[4],Some(120),false,false),"email":clean(&row[5],Some(120),false,false),"ts":number(&row[6]),"tz":timezone_minutes(&text(&row[7])),"cts":number(&row[8]),"refs":[]}));
    }
}

pub type Refs = HashMap<String, Vec<Value>>;
pub fn parse_refs(data: &[u8], branch: Option<&str>) -> (Refs, Option<String>) {
    let mut refs = Refs::new();
    let mut default = None;
    for row in data.split(|b| *b == b'\n') {
        let f: Vec<_> = row.split(|b| *b == 0).map(text).collect();
        if f.len() != 5 {
            continue;
        }
        if f[0] == "refs/remotes/origin/HEAD" && f[4].starts_with("refs/remotes/") {
            default = Some(clean(&f[4].as_bytes()[13..], None, false, false));
        }
        let (name, kind) = if let Some(n) = f[0].strip_prefix("refs/heads/") {
            (n, "branch")
        } else if let Some(n) = f[0]
            .strip_prefix("refs/remotes/")
            .filter(|_| !f[0].ends_with("/HEAD"))
        {
            (n, "remote")
        } else if let Some(n) = f[0].strip_prefix("refs/tags/") {
            (n, "tag")
        } else {
            continue;
        };
        if f[3] != "commit" && f[2].is_empty() {
            continue;
        }
        let mut r = json!({"name":clean(name.as_bytes(),None,false,false),"kind":kind});
        if kind == "branch" && Some(name) == branch {
            r["current"] = json!(true);
        }
        refs.entry(if f[2].is_empty() {
            f[1].clone()
        } else {
            f[2].clone()
        })
        .or_default()
        .push(r);
    }
    if default.is_none() {
        default = [Some("main"), Some("master"), branch]
            .into_iter()
            .flatten()
            .find(|n| {
                refs.values()
                    .flatten()
                    .any(|r| r["kind"] == "branch" && r["name"] == *n)
            })
            .map(str::to_owned)
            .or_else(|| branch.map(str::to_owned));
    }
    (refs, default)
}
pub fn read_refs(
    root: &Path,
    deadline: Instant,
    branch: Option<&str>,
) -> Result<(Refs, Option<String>), GitError> {
    let (data, _) = run(
        root,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(objecttype)%00%(symref)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
        deadline,
        MAX_OUTPUT,
        false,
    )?;
    Ok(parse_refs(&data, branch))
}
/// Graph JSON in topological order; default limit is 1,500 when `None`.
pub fn collect_graph(root: &Path, limit: Option<i64>) -> Value {
    let deadline = Instant::now() + TIMEOUT;
    let joined = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(root)
    };
    let mut absolute = std::path::PathBuf::new();
    for part in joined.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                absolute.pop();
            }
            part => absolute.push(part.as_os_str()),
        }
    }
    let mut result = json!({"ok":false,"error":null,"repo":{"name":clean(absolute.file_name().unwrap_or_default().as_encoded_bytes(),None,false,false),"root":clean(absolute.as_os_str().as_encoded_bytes(),None,false,true),"default_branch":null},"head":{"hash":null,"branch":null,"detached":false},"dirty":null,"total":0,"truncated":false,"commits":[]});
    let mut work = || -> Result<(), GitError> {
        let r = &mut result;
        let (top, _) = run(
            root,
            &["rev-parse", "--show-toplevel"],
            deadline,
            MAX_OUTPUT,
            false,
        )
        .map_err(|e| {
            if e.0 == "git-failed" {
                GitError("not-a-repo")
            } else {
                e
            }
        })?;
        let top = text(&top).trim_end_matches('\n').to_owned();
        r["repo"]["name"] = json!(clean(
            Path::new(&top)
                .file_name()
                .unwrap_or_default()
                .as_encoded_bytes(),
            None,
            false,
            false
        ));
        r["repo"]["root"] = json!(clean(top.as_bytes(), None, false, true));
        let branch = match run(
            root,
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
            deadline,
            MAX_OUTPUT,
            false,
        ) {
            Ok((b, _)) => Some(text(&b).trim_end_matches('\n').to_owned()),
            Err(e) if e.0 == "git-failed" => None,
            Err(e) => return Err(e),
        };
        r["head"]["branch"] =
            json!(branch
                .as_ref()
                .map(|b| clean(b.as_bytes(), None, false, false)));
        r["head"]["detached"] = json!(branch.is_none());
        // Preserve discovered HEAD/repository information even for an unborn repository.
        let (refs, default) = read_refs(root, deadline, branch.as_deref())?;
        r["repo"]["default_branch"] = json!(default);
        let sha = match run(
            root,
            &["rev-parse", "--verify", "HEAD^{commit}"],
            deadline,
            MAX_OUTPUT,
            false,
        ) {
            Ok((s, _)) => s,
            Err(e) => {
                r["error"] = json!(if e.0 == "git-failed" { "empty" } else { e.0 });
                return Ok(());
            }
        };
        r["head"]["hash"] = json!(text(&sha).trim());
        let limit = limit.unwrap_or(1500).clamp(1, 10000).to_string();
        let (mut commits, _) = run_with(
            root,
            &[
                "log",
                "--topo-order",
                "-n",
                &limit,
                "--branches",
                "--remotes",
                "--tags",
                "HEAD",
                "-z",
                "--encoding=UTF-8",
                &format!("--format={LOG_FORMAT}"),
                "--",
            ],
            deadline,
            MAX_OUTPUT,
            false,
            parse_log_stream,
        )?;
        for c in &mut commits {
            c["refs"] = Value::Array(
                refs.get(c["hash"].as_str().unwrap_or_default())
                    .cloned()
                    .unwrap_or_default(),
            );
        }
        let total = match run(
            root,
            &[
                "rev-list",
                "--count",
                "--branches",
                "--remotes",
                "--tags",
                "HEAD",
                "--",
            ],
            deadline.min(Instant::now() + Duration::from_millis(750)),
            128,
            false,
        ) {
            Ok((n, _)) => commits.len().max(text(&n).trim().parse().unwrap_or(0)),
            Err(_) => {
                let shown: HashSet<_> = commits.iter().filter_map(|c| c["hash"].as_str()).collect();
                commits.len()
                    + usize::from(commits.iter().any(|c| {
                        c["parents"].as_array().is_some_and(|ps| {
                            ps.iter()
                                .any(|p| !shown.contains(p.as_str().unwrap_or_default()))
                        })
                    }))
            }
        };
        if let Ok((data, false)) = run(
            root,
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
                "--ignored=no",
            ],
            deadline,
            2 * 1024 * 1024,
            true,
        ) {
            let mut records = data.split(|b| *b == 0);
            let mut count = 0;
            while let Some(row) = records.next() {
                if row.is_empty() {
                    continue;
                }
                count += 1;
                if row.iter().take(2).any(|b| matches!(b, b'R' | b'C')) {
                    records.next();
                }
            }
            r["dirty"] = json!({"changed":count});
        }
        r["truncated"] = json!(total > commits.len());
        r["total"] = json!(total);
        r["commits"] = Value::Array(commits);
        r["ok"] = json!(true);
        Ok(())
    };
    if let Err(e) = work() {
        result["error"] = json!(e.0);
    }
    result
}
