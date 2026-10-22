//! Undoable Markdown block transforms, with GtkTextBuffer character offsets.
use regex::Regex;
use std::{collections::BTreeMap, sync::LazyLock};
macro_rules! pattern {
    ($name:ident,$value:expr) => {
        pub static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($value).expect("constant edit pattern"));
    };
}
pattern!(
    LIST,
    r"^([ \t]*)([-+*]|\d+[.)])[ \t]+(?:\[([ xX])\][ \t]+)?"
);
pattern!(HEADING, r"^ {0,3}#{1,6}[ \t]+");
pattern!(QUOTE, r"^[ \t]*(?:>[ \t]?)+");
pattern!(FENCE, r"^ {0,3}(`{3,}|~{3,})([^\n]*)$");
pattern!(RULE, r"^ {0,3}(?:(?:\*\s*){3,}|(?:-\s*){3,}|(?:_\s*){3,})$");
/// Smallest replacement and resulting caret, all in Unicode characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub cursor: usize,
}
impl Edit {
    /// Apply to an owned document; out-of-range offsets are clamped.
    pub fn apply(&self, source: &str) -> String {
        let a = byte_at(source, self.start);
        let b = byte_at(source, self.end.max(self.start));
        format!("{}{}{}", &source[..a], self.text, &source[b..])
    }
}
/// Fenced code extent, with character offsets for content and closing fence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Code {
    pub start: usize,
    pub content: usize,
    pub closing: usize,
    pub end: usize,
    pub fence: String,
    pub closed: bool,
}
pub(crate) fn byte_at(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map_or(s.len(), |(i, _)| i)
}
fn count(s: &str, n: usize) -> usize {
    s[..n].chars().count()
}
fn line_bytes(text: &str, offset: usize) -> (usize, usize, &str) {
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    (start, end, &text[start..end])
}
/// Start, end and text of the line containing a character offset.
pub fn line_at(text: &str, offset: usize) -> (usize, usize, &str) {
    let (a, b, line) = line_bytes(text, byte_at(text, offset));
    (count(text, a), count(text, b), line)
}
/// Match a list, heading, or quote prefix. Capture ranges are character offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefix {
    pub text: String,
    pub start: usize,
    pub end: usize,
    pub groups: Vec<Option<(usize, usize, String)>>,
}
/// Return the first structural prefix, in list/heading/quote order.
pub fn prefix(line: &str) -> Option<Prefix> {
    for re in [&*LIST, &*HEADING, &*QUOTE] {
        if let Some(m) = re.captures(line) {
            let whole = m.get(0)?;
            return Some(Prefix {
                text: whole.as_str().into(),
                start: count(line, whole.start()),
                end: count(line, whole.end()),
                groups: m
                    .iter()
                    .skip(1)
                    .map(|v| {
                        v.map(|v| {
                            (
                                count(line, v.start()),
                                count(line, v.end()),
                                v.as_str().into(),
                            )
                        })
                    })
                    .collect(),
            });
        }
    }
    None
}
fn code_bytes(text: &str, offset: usize) -> Option<Code> {
    let mut opened: Option<(usize, usize, String)> = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let value = line.trim_end_matches('\n');
        if let Some((start, content, fence)) = &opened {
            if crate::mdview::closes_fence(value, fence) && !value.contains('\r') {
                if *start <= offset && offset < at + line.len() {
                    return Some(Code {
                        start: *start,
                        content: *content,
                        closing: at,
                        end: at + line.len(),
                        fence: fence.clone(),
                        closed: true,
                    });
                }
                opened = None;
            }
        } else if let Some(m) = FENCE.captures(value) {
            opened = Some((at, at + line.len(), m[1].into()));
        }
        at += line.len();
    }
    opened
        .filter(|(a, _, _)| *a <= offset && offset <= text.len())
        .map(|(start, content, fence)| Code {
            start,
            content,
            closing: text.len(),
            end: text.len(),
            fence,
            closed: false,
        })
}
/// Find the fenced code block containing a character offset.
pub fn code_at(text: &str, offset: usize) -> Option<Code> {
    code_bytes(text, byte_at(text, offset)).map(|c| Code {
        start: count(text, c.start),
        content: count(text, c.content),
        closing: count(text, c.closing),
        end: count(text, c.end),
        ..c
    })
}
#[derive(Debug)]
struct RawEdit {
    start: usize,
    end: usize,
    text: String,
    cursor: usize,
}
fn edit(a: usize, b: usize, value: impl Into<String>, cursor: usize) -> RawEdit {
    RawEdit {
        start: a,
        end: b,
        text: value.into(),
        cursor,
    }
}
fn diff(before: &str, after: &str, cursor: usize) -> RawEdit {
    let start = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let tail = before[start..]
        .chars()
        .rev()
        .zip(after[start..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    edit(
        start,
        before.len() - tail,
        &after[start..after.len() - tail],
        cursor,
    )
}
fn number(token: &str) -> u128 {
    token[..token.len() - 1].parse().unwrap_or(0)
}
fn renumber(text: &str, mut cursor: isize) -> (String, isize) {
    let mut result = String::new();
    let mut counters = BTreeMap::<usize, u128>::new();
    let mut offset = 0;
    let mut fenced = None;
    for original in text.split_inclusive('\n') {
        let mut line = original.to_string();
        let fence = FENCE.captures(original.trim_end_matches('\n'));
        if let Some(m) = &fence {
            let c = m[1].as_bytes()[0];
            fenced = if fenced == Some(c) { None } else { Some(c) };
        }
        let item = if fenced.is_none() && fence.is_none() {
            LIST.captures(original)
        } else {
            None
        };
        if let Some(m) = item {
            let depth = crate::mdview::expand_tabs(&m[1]).chars().count();
            counters.retain(|d, _| *d <= depth);
            if m[2].starts_with(|c: char| c.is_ascii_digit()) {
                let n = *counters.entry(depth).or_insert_with(|| number(&m[2]));
                counters.insert(depth, n.saturating_add(1));
                let token = format!("{}{}", n, &m[2][m[2].len() - 1..]);
                let span = m.get(2).expect("list token");
                let delta = token.len() as isize - span.len() as isize;
                if (offset + span.end()) as isize <= cursor {
                    cursor += delta;
                }
                line.replace_range(span.range(), &token);
            } else {
                counters.remove(&depth);
            }
        } else {
            counters.clear();
        }
        offset += line.len();
        result.push_str(&line);
    }
    (result, cursor)
}
fn replace(
    text: &str,
    a: usize,
    b: usize,
    value: &str,
    cursor: usize,
    renum: bool,
    restart: bool,
) -> RawEdit {
    let mut after = format!("{}{}{}", &text[..a], value, &text[b..]);
    let mut cursor = cursor as isize;
    if restart {
        if let Some(following) = after[a + value.len()..]
            .find('\n')
            .map(|i| i + a + value.len())
        {
            let (start, _, line) = line_bytes(&after, following + 1);
            if let Some(m) = LIST.captures(line) {
                if m[2].starts_with(|c: char| c.is_ascii_digit()) {
                    let token = format!("1{}", &m[2][m[2].len() - 1..]);
                    let s = m.get(2).expect("list token");
                    let range = start + s.start()..start + s.end();
                    after.replace_range(range, &token);
                }
            }
        }
    }
    if renum {
        let (mut first, mut last, _) = line_bytes(&after, a.min(after.len()));
        while first > 0 {
            let (previous, _, line) = line_bytes(&after, first - 1);
            if !LIST.is_match(line) {
                break;
            }
            first = previous;
        }
        while last < after.len() {
            let (_, following, line) = line_bytes(&after, last + 1);
            if !LIST.is_match(line) {
                break;
            }
            last = following;
        }
        let (region, relative) = renumber(&after[first..last], cursor - first as isize);
        after.replace_range(first..last, &region);
        cursor = first as isize + relative;
    }
    diff(text, &after, cursor.max(0) as usize)
}
fn unindent(line: &str, width: usize) -> (&str, usize) {
    let amount = if line.starts_with('\t') {
        1
    } else {
        width.min(line.bytes().take_while(|&c| c == b' ').count())
    };
    (&line[amount..], amount)
}
fn outdent_width(text: &str, mut start: usize, depth: usize, width: usize) -> usize {
    while start > 0 {
        let (a, _, previous) = line_bytes(text, start - 1);
        start = a;
        let Some(m) = LIST.captures(previous) else {
            break;
        };
        let parent = crate::mdview::expand_tabs(&m[1]).chars().count();
        if parent < depth {
            return depth - parent;
        }
    }
    width.min(depth)
}
fn exit_bytes(text: &str, code: &Code, above: bool) -> RawEdit {
    if above {
        return if code.start > 0 {
            edit(code.start, code.start, "", code.start - 1)
        } else {
            edit(0, 0, "\n", 0)
        };
    }
    if code.closed {
        if code.end < text.len() {
            return edit(code.end, code.end, "", code.end);
        }
        let value = if text.ends_with('\n') { "" } else { "\n" };
        return edit(code.end, code.end, value, code.end + value.len());
    }
    let value = format!(
        "{}{}\n",
        if text.ends_with('\n') { "" } else { "\n" },
        code.fence
    );
    let cursor = text.len() + value.len();
    edit(text.len(), text.len(), value, cursor)
}
fn into_edit(text: &str, raw: RawEdit) -> Edit {
    let after = format!("{}{}{}", &text[..raw.start], raw.text, &text[raw.end..]);
    Edit {
        start: count(text, raw.start),
        end: count(text, raw.end),
        text: raw.text,
        cursor: count(&after, raw.cursor.min(after.len())),
    }
}
/// Move outside a code block, completing an unclosed fence when necessary.
pub fn exit_code(text: &str, code: &Code, above: bool) -> Edit {
    let bytes = Code {
        start: byte_at(text, code.start),
        content: byte_at(text, code.content),
        closing: byte_at(text, code.closing),
        end: byte_at(text, code.end),
        ..code.clone()
    };
    into_edit(text, exit_bytes(text, &bytes, above))
}
/// Transform a key action (`enter`, `tab`, `outdent`, `backspace`, `delete`,
/// `space`, `toggle`, `exit`, `up`, `down`), or let the text view handle it.
pub fn transform(
    text: &str,
    offset: usize,
    action: &str,
    width: usize,
    tabs: bool,
) -> Option<Edit> {
    raw_transform(text, byte_at(text, offset), action, width, tabs).map(|e| into_edit(text, e))
}
fn raw_transform(
    text: &str,
    offset: usize,
    action: &str,
    width: usize,
    tabs: bool,
) -> Option<RawEdit> {
    let (start, end, line) = line_bytes(text, offset);
    let indent = if tabs { "\t".into() } else { " ".repeat(width) };
    if let Some(code) = code_bytes(text, offset) {
        if code.closed && code.content == code.closing && action == "enter" {
            return Some(edit(code.content, code.content, "\n", code.content));
        }
        if start == code.start && action == "enter" {
            return Some(edit(end, end, format!("\n\n{}", code.fence), end + 1));
        }
        let first = code.content;
        let last = text[first
            ..text[..code.closing]
                .char_indices()
                .next_back()
                .map_or(0, |(i, _)| i)
                .max(first)]
            .rfind('\n')
            .map_or(first, |i| first + i + 1);
        if ["exit", "toggle"].contains(&action) || action == "down" && start >= last {
            return Some(exit_bytes(text, &code, false));
        }
        if action == "up" && start <= first {
            return Some(exit_bytes(text, &code, true));
        }
        if action == "backspace" && text[first..code.closing].trim().is_empty() {
            return Some(replace(
                text, code.start, code.end, "", code.start, false, false,
            ));
        }
        if action == "backspace" && offset == first
            || action == "delete" && code.closed && offset == code.closing.saturating_sub(1)
        {
            return Some(edit(offset, offset, "", offset));
        }
        if action == "enter" {
            if start >= last
                && start > first
                && line.trim().is_empty()
                && line_bytes(text, start - 1).2.trim().is_empty()
            {
                return Some(exit_bytes(text, &code, false));
            }
            let leading = text[start..offset]
                .bytes()
                .take_while(|c| b" \t".contains(c))
                .count();
            return Some(edit(
                offset,
                offset,
                format!("\n{}", &text[start..start + leading]),
                offset + 1 + leading,
            ));
        }
        if action == "tab" {
            return Some(edit(offset, offset, &indent, offset + indent.len()));
        }
        if action == "outdent" {
            let (_, amount) = unindent(line, width);
            return Some(edit(
                start,
                start + amount,
                "",
                offset.saturating_sub(amount).max(start),
            ));
        }
        return None;
    }
    let item = LIST.captures(line);
    let heading = HEADING.captures(line);
    let quote = QUOTE.captures(line);
    let matched = item.as_ref().or(heading.as_ref()).or(quote.as_ref());
    let content = start + matched.and_then(|m| m.get(0)).map_or(0, |m| m.end());
    if action == "space" && &line[..offset - start] == "[]" {
        return Some(replace(
            text,
            start,
            offset,
            "- [ ] ",
            start + 6,
            false,
            false,
        ));
    }
    if action == "toggle" {
        if let Some(task) = item.as_ref().and_then(|m| m.get(3)) {
            let a = start + task.start();
            return Some(edit(
                a,
                a + 1,
                if task.as_str() == " " { "x" } else { " " },
                offset,
            ));
        }
    }
    if action == "enter" {
        if let Some(m) = FENCE.captures(line).filter(|_| offset == end) {
            return Some(edit(end, end, format!("\n\n{}", &m[1]), end + 1));
        }
        if RULE.is_match(line) {
            return Some(edit(end, end, "\n", end + 1));
        }
        if item.is_some() || quote.is_some() {
            let m = matched.expect("prefix");
            let whole = m.get(0).expect("whole prefix");
            if line[whole.end()..].trim().is_empty() {
                if item.as_ref().is_some_and(|m| !m[1].is_empty()) {
                    return raw_transform(text, offset, "outdent", width, tabs);
                }
                return Some(replace(text, start, end, "", start, true, true));
            }
            let mut marker = whole.as_str().to_string();
            if let Some(m) = &item {
                let mut token = m[2].to_string();
                if token.starts_with(|c: char| c.is_ascii_digit()) {
                    token = format!(
                        "{}{}",
                        number(&token).saturating_add(1),
                        &token[token.len() - 1..]
                    );
                }
                marker = format!(
                    "{}{}{}",
                    &m[1],
                    token,
                    if m.get(3).is_some() { " [ ] " } else { " " }
                );
            }
            return Some(replace(
                text,
                offset,
                offset,
                &format!("\n{marker}"),
                offset + 1 + marker.len(),
                true,
                false,
            ));
        }
        if heading.is_some() {
            return Some(edit(offset, offset, "\n", offset + 1));
        }
    }
    if action == "backspace" && matched.is_some() && offset == content {
        if item.as_ref().is_some_and(|m| !m[1].is_empty()) {
            return raw_transform(text, offset, "outdent", width, tabs);
        }
        return Some(replace(text, start, content, "", start, true, true));
    }
    if let Some(item) = item.filter(|_| ["tab", "outdent"].contains(&action)) {
        let mut stop = end;
        let depth = crate::mdview::expand_tabs(&item[1]).chars().count();
        while stop < text.len() {
            let (_, child_end, child) = line_bytes(text, stop + 1);
            let Some(nested) = LIST.captures(child) else {
                break;
            };
            if crate::mdview::expand_tabs(&nested[1]).chars().count() <= depth {
                break;
            }
            stop = child_end;
        }
        let (mut replacement, mut delta) = if action == "outdent" {
            let amount = outdent_width(text, start, depth, width);
            (
                text[start..stop]
                    .split('\n')
                    .map(|v| unindent(v, amount).0)
                    .collect::<Vec<_>>()
                    .join("\n"),
                -(unindent(line, amount).1 as isize),
            )
        } else {
            (
                text[start..stop]
                    .split('\n')
                    .map(|v| format!("{indent}{v}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                indent.len() as isize,
            )
        };
        if action == "tab" && item[2].starts_with(|c: char| c.is_ascii_digit()) {
            let m = LIST.captures(&replacement).expect("indented item");
            let s = m.get(2).expect("token");
            let token = format!("1{}", &item[2][item[2].len() - 1..]);
            let range = s.range();
            delta += token.len() as isize - s.len() as isize;
            replacement.replace_range(range, &token);
        }
        return Some(replace(
            text,
            start,
            stop,
            &replacement,
            (offset as isize + delta).max(0) as usize,
            true,
            false,
        ));
    }
    if ["backspace", "delete"].contains(&action) {
        if RULE.is_match(line) && [start, end].contains(&offset) {
            return Some(edit(start, end + usize::from(end < text.len()), "", start));
        }
        if action == "backspace" && offset == start && start > 0 {
            let (a, _, previous) = line_bytes(text, start - 1);
            if RULE.is_match(previous) {
                return Some(edit(a, start, "", a));
            }
        }
        if action == "delete" && offset == end && end < text.len() {
            let (_, b, following) = line_bytes(text, end + 1);
            if RULE.is_match(following) {
                return Some(edit(end, b, "", end));
            }
        }
    }
    None
}
