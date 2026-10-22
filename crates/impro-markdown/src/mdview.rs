//! Pure, deliberately limited Markdown parsing. HTML and images stay inert.
//!
//! Blocks and offsets use Unicode characters. Parsing owns no GUI objects and
//! can run on a worker. Consume [`parse_blocks`] incrementally on the main loop.
use regex::Regex;
use std::{
    borrow::Cow,
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::LazyLock,
};

/// Target character count for paragraph and code chunks.
pub const CHUNK: usize = 8192;
/// Literal text with semantic styles and an optional unresolved link.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub styles: Vec<String>,
    pub link: Option<String>,
}
/// Flat structural block; container nesting is retained in indent and quote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    pub kind: String,
    pub text: String,
    pub level: usize,
    pub indent: usize,
    pub quote: usize,
    pub marker: String,
    pub language: String,
    pub cells: Vec<String>,
    pub alignments: Vec<String>,
    pub header: bool,
    pub continuation: bool,
}
macro_rules! pattern {
    ($name:ident, $value:expr) => {
        pub static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($value).expect("constant Markdown pattern"));
    };
}
pattern!(_LIST, r"^( *)([-+*]|\d+[.)])\s+(.*)$");
pattern!(_FENCE, r"^ {0,3}(`{3,}|~{3,})(.*)$");
pattern!(_HEADING, r"^ {0,3}(#{1,6})(?:\s+|$)(.*)$");
pattern!(
    _RULE,
    r"^ {0,3}(?:(?:\*\s*){3,}|(?:-\s*){3,}|(?:_\s*){3,})$"
);
pattern!(
    _TABLE_RULE,
    r"^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*$"
);
pattern!(
    _TEXT_LINK,
    r"\[\[([^\]\n|]+)(?:\|([^\]\n]*))?\]\]|https?://[^\s<>]+"
);
pattern!(SETEXT, r"^ {0,3}(=+|-+)\s*$");
pattern!(TRAILING_HASHES, r"\s+#+\s*$");
pattern!(TASK, r"^\[([ xX])\]\s*(.*)$");
/// Original inline capture groups: escape, ticks, code, image, label, target,
/// autolink, delimiter, body. Backreferences are evaluated by a bounded engine.
/// Consumers must handle the engine's fallible match iterator.
pub static _INLINE: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    fancy_regex::Regex::new(
    r##"(?s)\\([!"#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~]|\n)|(`+)([^`]*?)\2|(!?)\[([^\]\n]*)\]\((<[^>\n]*>|[^\s()]*(?:\([^()]*\)[^\s()]*)*)(?:\s+"[^"\n]*")?\)|<((?:https?://|mailto:)[^<>\s]+)>|(\*\*\*|___|\*\*|__|~~|\*|_)(?=\S)(.+?)\8| {2,}\n"##
).expect("constant inline pattern")
});

fn span(text: impl Into<String>, styles: &[String], link: Option<String>) -> Span {
    Span {
        text: text.into(),
        styles: styles.to_vec(),
        link,
    }
}
fn plain(text: &str, styles: &[String], result: &mut Vec<Span>) {
    let mut pos = 0;
    for m in _TEXT_LINK.captures_iter(text) {
        let all = m.get(0).expect("complete match");
        if all.start() > pos {
            result.push(span(
                text[pos..all.start()].replace('\n', " "),
                styles,
                None,
            ));
        }
        let mut linked = styles.to_vec();
        linked.push("link".into());
        if let Some(name) = m.get(1) {
            result.push(span(
                m.get(2).unwrap_or(name).as_str(),
                &linked,
                Some(format!("[[{}]]", name.as_str())),
            ));
        } else {
            let target = all
                .as_str()
                .trim_end_matches(['.', ',', ';', '!', '?', ')']);
            result.push(span(target, &linked, Some(target.into())));
            if target.len() < all.len() {
                result.push(span(&all.as_str()[target.len()..], styles, None));
            }
        }
        pos = all.end();
    }
    if pos < text.len() {
        result.push(span(text[pos..].replace('\n', " "), styles, None));
    }
}
/// Parse inline Markdown with no inherited styles.
pub fn parse_inline(text: &str) -> Vec<Span> {
    parse_inline_with(text, &[], 0)
}
/// Parse inline Markdown with inherited styles and a recursion guard (12).
pub fn parse_inline_with(text: &str, styles: &[String], depth: usize) -> Vec<Span> {
    if depth > 12 {
        return vec![span(text, styles, None)];
    }
    let links: Vec<_> = _TEXT_LINK
        .find_iter(text)
        .map(|m| (m.start(), m.end()))
        .collect();
    let mut result = Vec::new();
    let mut pos = 0;
    for item in _INLINE.captures_iter(text) {
        let Ok(m) = item else { break };
        let all = m.get(0).expect("complete match");
        if m.get(8).is_some()
            && links
                .iter()
                .any(|&(a, b)| a <= all.start() && all.start() < b)
        {
            continue;
        }
        if all.start() > pos {
            plain(&text[pos..all.start()], styles, &mut result);
        }
        if let Some(escape) = m.get(1) {
            result.push(span(escape.as_str(), styles, None));
        } else if let Some(code) = m.get(3) {
            let mut added = styles.to_vec();
            added.push("code".into());
            result.push(span(code.as_str().replace('\n', " "), &added, None));
        } else if let Some(label) = m.get(5) {
            if m.get(4).is_some_and(|v| !v.as_str().is_empty()) {
                let mut added = styles.to_vec();
                added.push("image".into());
                result.push(span(format!("[image: {}]", label.as_str()), &added, None));
            } else {
                let target = m
                    .get(6)
                    .map(|v| v.as_str().trim_matches(['<', '>']))
                    .unwrap_or_default();
                for mut value in parse_inline_with(label.as_str(), styles, depth + 1) {
                    value.styles.push("link".into());
                    value.link = Some(target.into());
                    result.push(value);
                }
            }
        } else if let Some(auto) = m.get(7) {
            let mut added = styles.to_vec();
            added.push("link".into());
            result.push(span(auto.as_str(), &added, Some(auto.as_str().into())));
        } else if let Some(delimiter) = m.get(8) {
            let mut added = styles.to_vec();
            for name in match delimiter.as_str() {
                "***" | "___" => &["strong", "emphasis"][..],
                "**" | "__" => &["strong"][..],
                "~~" => &["strike"][..],
                _ => &["emphasis"][..],
            } {
                added.push((*name).into());
            }
            result.extend(parse_inline_with(
                m.get(9).map(|v| v.as_str()).unwrap_or_default(),
                &added,
                depth + 1,
            ));
        } else {
            result.push(span("\n", styles, None));
        }
        pos = all.end();
    }
    if pos < text.len() {
        plain(&text[pos..], styles, &mut result);
    }
    result
}
/// Strip up to `limit` quote prefixes, retaining literal `>` in code content.
pub fn _quote(mut line: &str, limit: usize) -> (usize, &str) {
    let mut depth = 0;
    while depth < limit {
        let spaces = line.bytes().take_while(|&b| b == b' ').count();
        if spaces > 3 || line.as_bytes().get(spaces) != Some(&b'>') {
            break;
        }
        line = &line[spaces + 1..];
        if line.starts_with(' ') {
            line = &line[1..];
        }
        depth += 1;
    }
    (depth, line)
}
/// Byte positions of pipe separators preceded by an even number of slashes.
pub fn separators(line: &str) -> impl Iterator<Item = usize> + '_ {
    let mut slashes = 0;
    line.bytes().enumerate().filter_map(move |(i, b)| {
        let pipe = b == b'|' && slashes % 2 == 0;
        slashes = if b == b'\\' { slashes + 1 } else { 0 };
        pipe.then_some(i)
    })
}
/// Split unescaped pipes, retain escape text, and cap the intermediate cells at 32.
pub fn _cells(line: &str) -> Vec<String> {
    let body = line.trim();
    let mut cells = vec![String::new()];
    let mut last = 0;
    for at in separators(body) {
        cells
            .last_mut()
            .expect("one cell")
            .push_str(&body[last..at]);
        if cells.len() < 32 {
            cells.push(String::new());
        } else {
            cells.last_mut().expect("one cell").push('|');
        }
        last = at + 1;
    }
    cells.last_mut().expect("one cell").push_str(&body[last..]);
    if line.trim_start().starts_with('|') {
        cells.remove(0);
    }
    if cells.len() > 1 && cells.last().is_some_and(|v| v.trim().is_empty()) {
        cells.pop();
    }
    cells.into_iter().map(|v| v.trim().into()).collect()
}
/// Expand tab stops to four characters, as the structural parser does.
pub fn expand_tabs(line: &str) -> Cow<'_, str> {
    if !line.contains('\t') {
        return Cow::Borrowed(line);
    }
    let mut out = String::new();
    let mut column = 0;
    for c in line.chars() {
        if c == '\t' {
            let n = 4 - column % 4;
            out.extend(std::iter::repeat_n(' ', n));
            column += n;
        } else {
            out.push(c);
            column = if c == '\r' || c == '\n' {
                0
            } else {
                column + 1
            };
        }
    }
    Cow::Owned(out)
}
/// Recognize a closing fence of the same character and at least the opening width.
pub fn closes_fence(line: &str, token: &str) -> bool {
    let spaces = line.bytes().take_while(|&b| b == b' ').count();
    if spaces > 3 {
        return false;
    }
    let body = &line[spaces..];
    let Some(first) = token.as_bytes().first() else {
        return false;
    };
    let n = body.bytes().take_while(|b| b == first).count();
    n >= token.len() && body[n..].trim().is_empty()
}
fn block(kind: &str, text: impl Into<String>, quote: usize) -> Block {
    Block {
        kind: kind.into(),
        text: text.into(),
        quote,
        ..Default::default()
    }
}
fn chunks(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    let mut empty = text.is_empty();
    std::iter::from_fn(move || {
        if empty {
            empty = false;
            return Some("");
        }
        if rest.is_empty() {
            return None;
        }
        let end = rest
            .char_indices()
            .nth(CHUNK)
            .map_or(rest.len(), |(i, _)| i);
        let (first, tail) = rest.split_at(end);
        rest = tail;
        Some(first)
    })
}
struct FenceState {
    token: String,
    language: String,
    quote: usize,
    continued: bool,
    content: String,
    size: usize,
}
/// Incremental flat block iterator. No inline parsing or file access occurs here.
pub struct Blocks<'a> {
    lines: Vec<&'a str>,
    at: usize,
    pending: VecDeque<Block>,
    fence: Option<FenceState>,
    table: Option<(usize, usize, Vec<String>)>,
}
/// Yield bounded paragraphs/code and individual table rows. CRLF is normalized
/// per line without copying the whole document.
pub fn parse_blocks(text: &str) -> Blocks<'_> {
    Blocks {
        lines: text
            .split_inclusive('\n')
            .map(|s| {
                if let Some(line) = s.strip_suffix('\n') {
                    line.strip_suffix('\r').unwrap_or(line)
                } else {
                    s
                }
            })
            .chain(text.ends_with('\n').then_some(""))
            .chain(text.is_empty().then_some(""))
            .collect(),
        at: 0,
        pending: VecDeque::new(),
        fence: None,
        table: None,
    }
}
impl Iterator for Blocks<'_> {
    type Item = Block;
    fn next(&mut self) -> Option<Block> {
        loop {
            if let Some(b) = self.pending.pop_front() {
                return Some(b);
            }
            if let Some(FenceState {
                token,
                language,
                quote,
                mut continued,
                mut content,
                mut size,
            }) = self.fence.take()
            {
                while self.at < self.lines.len() {
                    let (_, line) = _quote(self.lines[self.at], quote);
                    self.at += 1;
                    if closes_fence(line, &token) {
                        break;
                    }
                    let length = line.chars().count();
                    let mut offset = 0;
                    for part in chunks(line) {
                        let n = part.chars().count();
                        content.push_str(part);
                        if offset + CHUNK >= length {
                            content.push('\n');
                        }
                        offset += n;
                        size += n + 1;
                        if size >= CHUNK {
                            let mut b = block("codeblock", std::mem::take(&mut content), quote);
                            b.language = language.clone();
                            b.continuation = continued;
                            self.pending.push_back(b);
                            continued = true;
                            size = 0;
                        }
                    }
                    if !self.pending.is_empty() {
                        self.fence = Some(FenceState {
                            token: token.clone(),
                            language: language.clone(),
                            quote,
                            continued,
                            content: std::mem::take(&mut content),
                            size,
                        });
                        break;
                    }
                }
                if self.fence.is_none() && (!content.is_empty() || !continued) {
                    let mut b = block("codeblock", content, quote);
                    b.language = language;
                    b.continuation = continued;
                    self.pending.push_back(b);
                }
                continue;
            }
            if self.at >= self.lines.len() {
                return None;
            }
            if let Some((quote, count, alignments)) = self.table.take() {
                let (q, line) = _quote(self.lines[self.at], 32);
                if q == quote && !line.trim().is_empty() && line.contains('|') {
                    self.at += 1;
                    let mut b = block("table", line, quote);
                    b.cells = _cells(line);
                    b.cells.resize(count, String::new());
                    b.cells.truncate(count);
                    b.alignments = alignments.clone();
                    self.table = Some((quote, count, alignments));
                    return Some(b);
                }
            }
            let expanded = expand_tabs(self.lines[self.at]);
            let (quote, line) = _quote(&expanded, 32);
            if line.trim().is_empty() {
                self.at += 1;
                continue;
            }
            if let Some(m) = _FENCE.captures(line) {
                self.at += 1;
                self.fence = Some(FenceState {
                    token: m[1].into(),
                    language: m[2].split_whitespace().next().unwrap_or_default().into(),
                    quote,
                    continued: false,
                    content: String::new(),
                    size: 0,
                });
                continue;
            }
            if let Some(m) = _HEADING.captures(line) {
                self.at += 1;
                let mut b = block(
                    "heading",
                    TRAILING_HASHES.replace(&m[2], "").into_owned(),
                    quote,
                );
                b.level = m[1].len();
                return Some(b);
            }
            let following = self.lines.get(self.at + 1).map(|s| _quote(s, 32).1);
            if let Some(next) = following.filter(|v| SETEXT.is_match(v)) {
                self.at += 2;
                let mut b = block("heading", line.trim(), quote);
                b.level = if next.trim_start().starts_with('=') {
                    1
                } else {
                    2
                };
                return Some(b);
            }
            if _RULE.is_match(line) {
                self.at += 1;
                return Some(block("rule", "", quote));
            }
            if let Some(next) = following.filter(|v| line.contains('|') && _TABLE_RULE.is_match(v))
            {
                let cells = _cells(line);
                let rules = _cells(next);
                if cells.len() == rules.len() {
                    self.at += 2;
                    let alignments: Vec<String> = rules
                        .iter()
                        .map(|v| {
                            if v.starts_with(':') && v.ends_with(':') {
                                "center"
                            } else if v.ends_with(':') {
                                "right"
                            } else {
                                "left"
                            }
                            .into()
                        })
                        .collect();
                    let mut b = block("table", line, quote);
                    b.cells = cells;
                    b.header = true;
                    b.alignments = alignments.clone();
                    self.table = Some((quote, b.cells.len(), alignments));
                    return Some(b);
                }
            }
            if let Some(m) = _LIST.captures(line) {
                let mut marker = m[2].to_string();
                let mut body = m[3].to_string();
                if let Some(task) = TASK.captures(&body) {
                    marker = if &task[1] == " " { "☐" } else { "☑" }.into();
                    body = task[2].into();
                } else if ["-", "+", "*"].contains(&marker.as_str()) {
                    marker = "•".into();
                }
                let column = m.get(3).expect("body").start();
                let indent = m[1].len().min(64);
                self.at += 1;
                let mut size = body.chars().count();
                while self.at < self.lines.len() && size < CHUNK {
                    let (q, current) = _quote(self.lines[self.at], 32);
                    let width = current.bytes().take_while(|&b| b == b' ').count();
                    if q != quote
                        || width < column
                        || current.trim().is_empty()
                        || _LIST.is_match(current)
                        || _FENCE.is_match(current)
                        || _HEADING.is_match(current)
                    {
                        break;
                    }
                    body.push('\n');
                    body.push_str(&current[column..]);
                    size += current.chars().count();
                    self.at += 1;
                }
                let mut b = block("list", body, quote);
                b.indent = indent;
                b.marker = marker;
                return Some(b);
            }
            if let Some(code) = line.strip_prefix("    ") {
                self.at += 1;
                for part in chunks(code) {
                    self.pending
                        .push_back(block("codeblock", format!("{part}\n"), quote));
                }
                continue;
            }
            let mut content = String::new();
            let mut size = 0;
            while self.at < self.lines.len() {
                let (q, current) = _quote(self.lines[self.at], 32);
                if !content.is_empty()
                    && (q != quote
                        || _FENCE.is_match(current)
                        || _HEADING.is_match(current)
                        || _LIST.is_match(current)
                        || _RULE.is_match(current))
                {
                    break;
                }
                if current.trim().is_empty() {
                    break;
                }
                if !content.is_empty()
                    && self.lines.get(self.at + 1).is_some_and(|v| {
                        let s = _quote(v, 32).1;
                        _TABLE_RULE.is_match(s) || SETEXT.is_match(s)
                    })
                {
                    break;
                }
                for part in chunks(current) {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(part);
                    size += part.chars().count() + 1;
                    if size >= CHUNK {
                        self.pending.push_back(block(
                            "paragraph",
                            std::mem::take(&mut content),
                            quote,
                        ));
                        size = 0;
                    }
                }
                self.at += 1;
                if !self.pending.is_empty() {
                    break;
                }
            }
            if !content.is_empty() {
                self.pending.push_back(block("paragraph", content, quote));
            }
        }
    }
}
/// Safe external URI or existing local file, resolved without loading it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkTarget {
    Uri(String),
    File(PathBuf),
}
fn unquote(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
/// Resolve web/mail links and relative files, including workspace wiki notes.
/// Performs filesystem reads: invoke on a worker after startup.
pub fn link_target(target: &str, source_path: &Path) -> Option<LinkTarget> {
    if target.starts_with("[[") && target.ends_with("]]") {
        let name = unquote(
            target[2..target.len() - 2]
                .split('|')
                .next()
                .unwrap_or_default(),
        );
        let name = name.split('#').next().unwrap_or_default();
        if name.is_empty() || Path::new(name).is_absolute() || name.chars().any(|c| (c as u32) < 32)
        {
            return None;
        }
        let source = if source_path.is_absolute() {
            source_path.to_path_buf()
        } else {
            std::env::current_dir().ok()?.join(source_path)
        };
        let parent = source.parent()?;
        let root = parent
            .ancestors()
            .find(|p| p.join(".impro").is_dir() || p.join(".git").exists())
            .unwrap_or(parent);
        let variants = if Path::new(name).extension().is_some_and(|e| !e.is_empty()) {
            vec![name.to_string()]
        } else {
            vec![
                format!("{name}.md"),
                format!("{name}.markdown"),
                name.to_string(),
            ]
        };
        for folder in [
            root.to_path_buf(),
            parent.to_path_buf(),
            root.join(".impro/Notes"),
        ] {
            for name in &variants {
                let p = folder.join(name);
                if p.is_file() {
                    return p.canonicalize().ok().map(LinkTarget::File);
                }
            }
        }
        return None;
    }
    if target.chars().any(|c| (c as u32) < 32) {
        return None;
    }
    if let Some((scheme, rest)) = target.split_once(':') {
        if ["http", "https"].contains(&scheme.to_ascii_lowercase().as_str()) {
            let authority = rest.strip_prefix("//")?.split(['/', '?', '#']).next()?;
            if authority.is_empty() {
                return None;
            }
            if authority.contains('[') || authority.contains(']') {
                let (head, rest) = authority.split_once('[')?;
                if head.contains(']') {
                    return None;
                }
                let (host, tail) = rest.split_once(']')?;
                if tail.contains(['[', ']']) {
                    return None;
                }
                let valid = if let Some(host) = host.strip_prefix('v') {
                    host.split_once('.').is_some_and(|(version, body)| {
                        !version.is_empty()
                            && version.bytes().all(|b| b.is_ascii_hexdigit())
                            && !body.is_empty()
                    })
                } else {
                    host.split('%')
                        .next()?
                        .parse::<std::net::Ipv6Addr>()
                        .is_ok()
                };
                if !valid {
                    return None;
                }
            }
            return Some(LinkTarget::Uri(target.into()));
        }
        if scheme.eq_ignore_ascii_case("mailto") && !rest.split(['?', '#']).next()?.is_empty() {
            return Some(LinkTarget::Uri(target.into()));
        }
        return None;
    }
    if target.starts_with("//") {
        return None;
    }
    let path = unquote(target.split(['?', '#']).next()?);
    if path.is_empty() || Path::new(&path).is_absolute() {
        return None;
    }
    let path = source_path.parent()?.join(path);
    if !path.is_file() {
        return None;
    }
    path.canonicalize().ok().map(LinkTarget::File)
}
