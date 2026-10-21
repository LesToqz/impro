//! Read-only commit changes and bounded file context.
use crate::log::{
    clean, read_refs, run, timezone_minutes, valid_hash, GitError, MAX_OUTPUT, TIMEOUT,
};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Instant;
const METADATA: &str =
    "%H%x00%h%x00%P%x00%s%x00%b%x00%an%x00%ae%x00%at%x00%ai%x00%cn%x00%ce%x00%ct%x00%ci";
const OPTIONS: &[&str] = &[
    "-r",
    "--no-commit-id",
    "--root",
    "-M",
    "-C",
    "--find-copies-harder",
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--ignore-submodules=none",
    "--submodule=short",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--no-relative",
    "--inter-hunk-context=0",
];
/// Optional syntax adapter. Offsets are Unicode character indices, not UTF-8 bytes.
/// The default (`None`) produces null language and empty token runs. Return `Err`
/// to discard all runs for that side; adapters need no GTK dependencies here.
pub trait Highlighter {
    fn language(&self, path: &str) -> Option<String>;
    fn classify(&self, path: &str, text: &str) -> Result<Vec<(usize, usize, String)>, String>;
}
fn paint(lines: &mut [Value], path: &str, highlighter: Option<&dyn Highlighter>) {
    let Some(h) = highlighter else { return };
    if lines.is_empty() || h.language(path).is_none() {
        return;
    }
    let text = lines
        .iter()
        .map(|l| l["text"].as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let Ok(runs) = h.classify(path, &text) else {
        for l in lines {
            l["k"] = json!([]);
        }
        return;
    };
    let mut starts = Vec::with_capacity(lines.len());
    let mut lengths = Vec::with_capacity(lines.len());
    let mut position = 0;
    for line in lines.iter_mut() {
        starts.push(position);
        let length = line["text"].as_str().unwrap_or_default().chars().count();
        lengths.push(length);
        line["k"] = json!([]);
        position += length + 1;
    }
    for (start, end, kind) in runs {
        let mut index = starts
            .partition_point(|offset| *offset <= start)
            .saturating_sub(1);
        while index < lines.len() && starts[index] < end {
            let lo = start.saturating_sub(starts[index]);
            let hi = end.saturating_sub(starts[index]).min(lengths[index]);
            if lo < hi {
                if let Some(tokens) = lines[index]["k"].as_array_mut() {
                    tokens.push(json!([lo, hi, kind]));
                }
            }
            index += 1;
        }
    }
}
pub fn highlight_hunks(file: &mut Value, highlighter: Option<&dyn Highlighter>) {
    let path = file["path"].as_str().unwrap_or_default().to_owned();
    file["language"] = json!(highlighter.and_then(|h| h.language(&path)));
    if let Some(hunks) = file["hunks"].as_array_mut() {
        for hunk in hunks {
            if let Some(lines) = hunk["lines"].as_array_mut() {
                let mut old: Vec<_> = lines
                    .iter()
                    .filter(|l| l["t"] == " " || l["t"] == "-")
                    .cloned()
                    .collect();
                paint(&mut old, &path, highlighter);
                let mut removed = old.iter().filter(|l| l["t"] == "-");
                for l in lines.iter_mut().filter(|l| l["t"] == "-") {
                    if let Some(o) = removed.next() {
                        l["k"] = o["k"].clone();
                    }
                }
                let indices: Vec<_> = lines
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l["t"] == " " || l["t"] == "+")
                    .map(|(i, _)| i)
                    .collect();
                let mut new: Vec<_> = indices.iter().map(|i| lines[*i].clone()).collect();
                paint(&mut new, &path, highlighter);
                for (i, l) in indices.into_iter().zip(new) {
                    lines[i]["k"] = l["k"].clone();
                }
            }
        }
    }
}
fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}
fn number(b: &[u8]) -> i64 {
    text(b).parse().unwrap_or(0)
}
/// Parse machine paths exclusively from raw/numstat NUL records.
pub fn parse_changes(data: &[u8]) -> Vec<Value> {
    let records: Vec<_> = data.split(|b| *b == 0).collect();
    let mut i = 0;
    let mut files = Vec::new();
    while i < records.len() && records[i].starts_with(b":") {
        let h: Vec<_> = records[i][1..]
            .split(u8::is_ascii_whitespace)
            .filter(|s| !s.is_empty())
            .collect();
        i += 1;
        if h.len() != 5 || i >= records.len() {
            break;
        }
        let mut path = records[i];
        i += 1;
        let mut old = None;
        if matches!(h[4].first(), Some(b'R' | b'C')) {
            old = Some(text(path));
            let Some(p) = records.get(i) else { break };
            path = p;
            i += 1;
        }
        let status = match h[4].first() {
            Some(b'A') => "added",
            Some(b'D') => "deleted",
            Some(b'R') => "renamed",
            Some(b'C') => "copied",
            Some(b'T') => "typechange",
            _ => "modified",
        };
        files.push(json!({"path":text(path),"old_path":old,"status":status,"similarity":if h[4].len()>1{Some(number(&h[4][1..]))}else{None},"mode_change":if h[0]!=h[1]{Some([text(h[0]),text(h[1])])}else{None},"binary":false,"additions":0,"deletions":0,"too_large":false,"language":null,"hunks":[]}));
    }
    let by_path: std::collections::HashMap<_, _> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f["path"].as_str().unwrap_or_default().to_owned(), i))
        .collect();
    while i < records.len() {
        let row = records[i];
        i += 1;
        let counts: Vec<_> = row.splitn(3, |b| *b == b'\t').collect();
        if counts.len() != 3 {
            continue;
        }
        let mut path = counts[2];
        if path.is_empty() {
            i += 1;
            let Some(p) = records.get(i) else { break };
            path = p;
            i += 1;
        }
        if let Some(index) = by_path.get(&text(path)) {
            let f = &mut files[*index];
            f["binary"] = json!(counts[0] == b"-" || counts[1] == b"-");
            f["additions"] = json!(number(counts[0]));
            f["deletions"] = json!(number(counts[1]));
        }
    }
    files
}
fn range(s: &str) -> Option<(i64, i64)> {
    let s = s.get(1..)?;
    if let Some((a, b)) = s.split_once(',') {
        Some((a.parse().ok()?, b.parse().ok()?))
    } else {
        Some((s.parse().ok()?, 1))
    }
}
/// Unified hunks also carry both line numbers for split rendering.
/// Returns (hunks, too_large, clipped_long_line).
pub fn parse_patch(data: &[u8], max_lines: usize) -> (Vec<Value>, bool, bool) {
    let mut hunks: Vec<Value> = Vec::new();
    let (mut active, mut old, mut new, mut count, mut clipped) = (false, 0, 0, 0, false);
    for row in data.split(|b| *b == b'\n') {
        if row.starts_with(b"diff --git ") {
            active = false;
            continue;
        }
        if let Some(rest) = row.strip_prefix(b"@@ ") {
            if let Some(end) = rest.windows(3).position(|w| w == b" @@") {
                let header = text(&rest[..end]);
                if let Some((a, b)) = header.split_once(' ') {
                    if let (Some((o, ol)), Some((n, nl))) = (range(a), range(b)) {
                        old = o;
                        new = n;
                        let section = rest[end + 3..]
                            .strip_prefix(b" ")
                            .unwrap_or(&rest[end + 3..]);
                        hunks.push(json!({"old_start":o,"old_lines":ol,"new_start":n,"new_lines":nl,"section":clean(section,None,false,false),"lines":[]}));
                        active = true;
                        continue;
                    }
                }
            }
        }
        let Some(kind) = row.first().copied() else {
            continue;
        };
        if !active || !matches!(kind, b' ' | b'+' | b'-' | b'\\') {
            continue;
        }
        if kind == b'\\' && !row.starts_with(b"\\ No newline at end of file") {
            continue;
        }
        count += 1;
        if count > max_lines {
            return (vec![], true, clipped);
        }
        let full = clean(
            if kind == b'\\' { row } else { &row[1..] },
            None,
            kind != b'\\',
            false,
        );
        let cut = full.chars().count() > 2000;
        let mut line = json!({"t":(kind as char).to_string(),"o":if matches!(kind,b' '|b'-'){Some(old)}else{None},"n":if matches!(kind,b' '|b'+'){Some(new)}else{None},"text":if cut{clean(full.as_bytes(),Some(2000),true,false)}else{full},"k":[]});
        if cut {
            line["cut"] = json!(true);
            clipped = true;
        }
        if let Some(lines) = hunks.last_mut().and_then(|h| h["lines"].as_array_mut()) {
            lines.push(line);
        }
        old += i64::from(matches!(kind, b' ' | b'-'));
        new += i64::from(matches!(kind, b' ' | b'+'));
    }
    (hunks, false, clipped)
}
fn check_repo(root: &Path, deadline: Instant) -> Result<(), GitError> {
    run(root, &["rev-parse", "--git-dir"], deadline, 65536, false)
        .map(|_| ())
        .map_err(|e| {
            if e.0 == "git-failed" {
                GitError("not-a-repo")
            } else {
                e
            }
        })
}
fn resolve(root: &Path, rev: &str, deadline: Instant) -> Result<String, GitError> {
    if rev.is_empty() || rev.starts_with('-') || rev.contains(['\0', '\n', '\r']) {
        return Err(GitError("unknown-revision"));
    }
    let (data, _) = run(
        root,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{rev}^{{commit}}"),
        ],
        deadline,
        256,
        false,
    )
    .map_err(|e| {
        if e.0 == "git-failed" {
            GitError("unknown-revision")
        } else {
            e
        }
    })?;
    let sha = text(&data).trim().to_owned();
    if !valid_hash(sha.as_bytes()) {
        return Err(GitError("unknown-revision"));
    }
    Ok(sha)
}
/// Display limits; counts always describe the complete raw diff.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_files: i64,
    pub max_lines_per_file: i64,
    pub max_bytes: i64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 300,
            max_lines_per_file: 3000,
            max_bytes: 8 * 1024 * 1024,
        }
    }
}
/// Describe a commit relative to its first parent, or its empty root tree.
pub fn commit_detail(
    root: &Path,
    rev: &str,
    limits: Limits,
    highlighter: Option<&dyn Highlighter>,
) -> Value {
    let deadline = Instant::now() + TIMEOUT;
    let work = || -> Result<Value, GitError> {
        check_repo(root, deadline)?;
        let sha = resolve(root, rev, deadline)?;
        let (data, _) = run(
            root,
            &[
                "show",
                "--no-patch",
                "--encoding=UTF-8",
                &format!("--format={METADATA}"),
                &sha,
                "--",
            ],
            deadline,
            MAX_OUTPUT,
            false,
        )?;
        let data = data.strip_suffix(b"\n").unwrap_or(&data);
        let f: Vec<_> = data.split(|b| *b == 0).collect();
        if f.len() != 13 {
            return Err(GitError("git-failed"));
        }
        let parents: Vec<_> = text(f[2]).split_whitespace().map(str::to_owned).collect();
        let (refs, _) = read_refs(root, deadline, None)?;
        let identity = |i: usize| json!({"name":clean(f[i],Some(120),false,false),"email":clean(f[i+1],Some(120),false,false),"ts":number(f[i+2]),"tz":timezone_minutes(&text(f[i+3]))});
        let mut result = json!({"ok":true,"error":null,"hash":text(f[0]),"short":text(f[1]),"parents":parents,"subject":clean(f[3],Some(300),false,false),"body":clean(f[4],None,true,false).trim(),"author":identity(5),"committer":identity(9),"refs":refs.get(&sha).cloned().unwrap_or_default(),"merge":parents.len()>=2,"diff_base":parents.first(),"stats":{},"truncated":false,"files":[]});
        let mut args = vec!["diff-tree"];
        args.extend_from_slice(OPTIONS);
        args.extend(["--raw", "--numstat", "-z"]);
        if let Some(p) = parents.first() {
            args.push(p);
        }
        args.extend([&sha, "--"]);
        let (raw, _) = run(root, &args, deadline, MAX_OUTPUT, false)?;
        let mut files = parse_changes(&raw);
        result["stats"] = json!({"files":files.len(),"additions":files.iter().map(|f|f["additions"].as_i64().unwrap_or(0)).sum::<i64>(),"deletions":files.iter().map(|f|f["deletions"].as_i64().unwrap_or(0)).sum::<i64>()});
        let max_files = limits.max_files.clamp(0, 10000) as usize;
        let mut truncated = files.len() > max_files;
        files.truncate(max_files);
        if !files.is_empty() {
            let mut args = vec!["diff-tree"];
            args.extend_from_slice(OPTIONS);
            args.extend(["-p", "-U3", "-z"]);
            if let Some(p) = parents.first() {
                args.push(p);
            }
            args.extend([&sha, "--"]);
            let (patch, clipped) = run(
                root,
                &args,
                deadline,
                limits.max_bytes.clamp(0, MAX_OUTPUT as i64) as usize,
                true,
            )?;
            let mut starts = Vec::new();
            let mut offset = 0;
            for row in patch.split_inclusive(|b| *b == b'\n') {
                if row.starts_with(b"diff --git ") {
                    starts.push(offset);
                }
                offset += row.len();
            }
            starts.push(patch.len());
            let sections = starts.len() - 1;
            let mut index = 0;
            for file in &mut files {
                let width = if file["status"] == "typechange" { 2 } else { 1 };
                let end = index + width;
                let incomplete = end > sections || (clipped && end >= sections);
                if file["binary"] != true {
                    if incomplete {
                        file["too_large"] = json!(true);
                    } else {
                        let (hunks, large, cut) = parse_patch(
                            &patch[starts[index]..starts[end]],
                            limits.max_lines_per_file.clamp(0, 100000) as usize,
                        );
                        file["hunks"] = json!(hunks);
                        file["too_large"] = json!(large);
                        truncated |= cut;
                    }
                }
                truncated |= file["too_large"] == true;
                highlight_hunks(file, highlighter);
                index = end;
            }
            truncated |= clipped;
        }
        result["files"] = json!(files);
        result["truncated"] = json!(truncated);
        Ok(result)
    };
    match work() {
        Ok(r) => r,
        Err(e) => json!({"ok":false,"error":e.0}),
    }
}
/// Return at most 200 new-side lines from a validated tree blob.
pub fn file_context(
    root: &Path,
    rev: &str,
    path: &str,
    start: i64,
    end: i64,
    highlighter: Option<&dyn Highlighter>,
) -> Value {
    let start = start.max(1);
    let end = end.min(start.saturating_add(199)).max(start);
    let mut result = json!({"ok":false,"path":path,"start":start,"end":end,"lines":[]});
    let deadline = Instant::now() + TIMEOUT;
    let work = || -> Result<(Vec<Value>, i64), GitError> {
        if path.is_empty()
            || path.contains('\0')
            || Path::new(path).is_absolute()
            || path.split('/').any(|p| matches!(p, ".." | "." | ""))
        {
            return Err(GitError("missing-path"));
        }
        check_repo(root, deadline)?;
        let sha = resolve(root, rev, deadline)?;
        let (tree, _) = run(
            root,
            &[
                "ls-tree",
                "-z",
                "--full-tree",
                &sha,
                "--",
                &format!(":(literal){path}"),
            ],
            deadline,
            MAX_OUTPUT,
            false,
        )?;
        let entry = tree
            .split(|b| *b == 0)
            .find_map(|r| {
                let i = r.iter().position(|b| *b == b'\t')?;
                if &r[i + 1..] == path.as_bytes() {
                    Some(&r[..i])
                } else {
                    None
                }
            })
            .ok_or(GitError("missing-path"))?;
        let parts: Vec<_> = entry.split(u8::is_ascii_whitespace).collect();
        if parts.len() != 3 || parts[1] != b"blob" {
            return Err(GitError("missing-path"));
        }
        let (blob, _) = run(
            root,
            &["cat-file", "blob", &text(parts[2])],
            deadline,
            8 * 1024 * 1024,
            false,
        )?;
        if blob.contains(&0) {
            return Err(GitError("binary"));
        }
        let mut lines: Vec<_> = blob.split(|b| *b == b'\n').collect();
        if lines.last() == Some(&b"".as_slice()) {
            lines.pop();
        }
        let last = end.min(lines.len() as i64);
        let mut lines: Vec<_> = lines
            .iter()
            .enumerate()
            .skip((start - 1) as usize)
            .take((last - start + 1).max(0) as usize)
            .map(|(i, l)| json!({"n":i+1,"text":clean(l,Some(2000),true,false),"k":[]}))
            .collect();
        paint(&mut lines, path, highlighter);
        Ok((lines, last))
    };
    match work() {
        Ok((lines, end)) => {
            result["ok"] = json!(true);
            result["end"] = json!(end);
            result["lines"] = json!(lines);
        }
        Err(e) => result["error"] = json!(e.0),
    }
    result
}
