//! Pure parts of the live renderer: block scanning, per-line paint plans and
//! the vertical-rhythm constants. Nothing here touches GTK, so the code is
//! cheap to test and can run on any thread.
//!
//! Offsets are Unicode character offsets, the unit of `GtkTextBuffer`.
use crate::{blockedit, mdview};
use regex::Regex;
use std::sync::LazyLock;

/// Space a block leaves after itself (see [`Decoration::below`](super::Decoration));
/// the next block adds it to its own space above.
pub const BELOW_BODY: i32 = 6;
pub const BELOW_HEADING: i32 = 10;
pub const BELOW_QUOTE: i32 = 10;
pub const BELOW_CODE_LINE: i32 = 3;
pub const BELOW_CODE_END: i32 = 8;
pub const BELOW_REPLACEMENT: i32 = 8;
pub const BOTTOM_MARGIN: i32 = 28;
/// Longest line the inline analysis looks at; the rest is shown as plain text.
pub const INLINE_LIMIT: usize = 8192;

macro_rules! pattern {
    ($name:ident, $value:expr) => {
        pub static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($value).expect("constant live pattern"));
    };
}
pattern!(LIST, r"^(\s*)([-+*]|\d+[.)])\s+(?:\[([ xX])\]\s+)?");
pattern!(QUOTE, r"^(\s*(?:> ?)+)");
pattern!(WIKI, r"\[\[([^\]\n|]+)(?:\|([^\]\n]*))?\]\]");
pattern!(URL, r"https?://[^\s<>]+");
pattern!(TAIL_HASHES, r"\s+#+\s*$");

/// Structural kind of one block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Line,
    Codeblock,
    Table,
}
impl Kind {
    /// Name used by tests and diagnostics (`line`, `codeblock`, `table`).
    pub fn name(self) -> &'static str {
        match self {
            Kind::Line => "line",
            Kind::Codeblock => "codeblock",
            Kind::Table => "table",
        }
    }
}

/// One block as found by [`Scanner`]: `[start, end)` in characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scanned {
    pub start: i32,
    pub end: i32,
    pub kind: Kind,
    pub text: String,
}
/// Result of one [`Scanner::next`] step.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Block(Scanned),
    /// The block starting here ran off the end of the text; refetch from `from`
    /// with a longer chunk.
    More { from: i32 },
    /// Nothing is left in this chunk.
    Done,
}

/// Splits a chunk of the document into blocks the way the original did: one
/// per line, fenced code up to its closing fence, and pipe tables.
///
/// `complete` says that the chunk reaches the end of the document. A partial
/// chunk reports [`Step::More`] instead of guessing when a block or its
/// look-ahead line is cut off.
pub struct Scanner<'a> {
    text: &'a str,
    byte: usize,
    chars: i32,
    complete: bool,
}
impl<'a> Scanner<'a> {
    /// `base` is the character offset of `text[0]`.
    pub fn new(text: &'a str, base: i32, complete: bool) -> Self {
        Self {
            text,
            byte: 0,
            chars: base,
            complete,
        }
    }
    /// Character offset of the next unread line.
    pub fn position(&self) -> i32 {
        self.chars
    }
    /// Byte index of the next unread line within the chunk.
    pub fn byte(&self) -> usize {
        self.byte
    }
    fn line(&self, at: usize) -> Option<(&'a str, usize)> {
        let rest = &self.text[at..];
        if rest.is_empty() {
            return None;
        }
        match rest.find('\n') {
            Some(i) => Some((&rest[..=i], at + i + 1)),
            None if self.complete => Some((rest, self.text.len())),
            None => None,
        }
    }
    pub fn next_step(&mut self) -> Step {
        let start_byte = self.byte;
        let start = self.chars;
        let Some((first, mut at)) = self.line(start_byte) else {
            return if start_byte < self.text.len() {
                Step::More { from: start }
            } else {
                Step::Done
            };
        };
        let mut length = first.chars().count() as i32;
        let trimmed = first.strip_suffix('\n').unwrap_or(first);
        let mut kind = Kind::Line;
        if let Some(fence) = mdview::_FENCE.captures(trimmed) {
            kind = Kind::Codeblock;
            let token = fence[1].to_string();
            let mut closed = false;
            while at < self.text.len() {
                let Some((current, next)) = self.line(at) else {
                    return Step::More { from: start };
                };
                length += current.chars().count() as i32;
                at = next;
                if mdview::closes_fence(current.strip_suffix('\n').unwrap_or(current), &token) {
                    closed = true;
                    break;
                }
            }
            if !closed && !self.complete {
                return Step::More { from: start };
            }
        } else if first.contains('|') && at < self.text.len() {
            let Some((rule, after)) = self.line(at) else {
                return Step::More { from: start };
            };
            let rule_text = rule.strip_suffix('\n').unwrap_or(rule);
            if mdview::_TABLE_RULE.is_match(rule_text)
                && mdview::_cells(first).len() == mdview::_cells(rule).len()
            {
                kind = Kind::Table;
                length += rule.chars().count() as i32;
                at = after;
                while at < self.text.len() {
                    let Some((current, next)) = self.line(at) else {
                        return Step::More { from: start };
                    };
                    if !current.contains('|') || current.trim().is_empty() {
                        break;
                    }
                    length += current.chars().count() as i32;
                    at = next;
                }
                if at >= self.text.len() && !self.complete {
                    return Step::More { from: start };
                }
            }
        } else if first.contains('|') && !self.complete {
            // The look-ahead line is not in the chunk yet.
            return Step::More { from: start };
        }
        let text = self.text[start_byte..at].to_string();
        self.byte = at;
        self.chars += length;
        Step::Block(Scanned {
            start,
            end: start + length,
            kind,
            text,
        })
    }
}

/// Split a whole string into blocks (used by tests and small documents).
pub fn split_blocks(text: &str) -> Vec<Scanned> {
    let mut scanner = Scanner::new(text, 0, true);
    let mut out = Vec::new();
    while let Step::Block(block) = scanner.next_step() {
        out.push(block);
    }
    out
}

/// Maps byte positions inside a string to character positions.
pub struct CharMap {
    table: Option<Vec<u32>>,
}
impl CharMap {
    pub fn new(text: &str) -> Self {
        if text.is_ascii() {
            return Self { table: None };
        }
        let mut table = Vec::with_capacity(text.len() + 1);
        let mut chars = 0u32;
        for (_, c) in text.char_indices() {
            for _ in 0..c.len_utf8() {
                table.push(chars);
            }
            chars += 1;
        }
        table.push(chars);
        Self { table: Some(table) }
    }
    pub fn at(&self, byte: usize) -> i32 {
        match &self.table {
            None => byte as i32,
            Some(t) => t[byte.min(t.len() - 1)] as i32,
        }
    }
}

/// One tag application of a paint plan, in absolute characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Tag(&'static str, i32, i32),
    Hidden(i32, i32),
}
/// A clickable range relative to the block start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkSpan {
    pub a: i32,
    pub b: i32,
    pub target: String,
    pub revealed: bool,
}
/// A widget a block wants, positioned at a block-relative offset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Want {
    Task { checked: bool, toggle: i32, x: i32 },
    Bullet { text: String, x: i32 },
    Quote,
    Rule,
    Image { a: i32, b: i32, target: String },
    Code,
    Table,
}
/// Result of planning one `line` block.
#[derive(Clone, Debug, Default)]
pub struct LinePlan {
    pub ops: Vec<Op>,
    /// Tag name the first line takes its space above from.
    pub spacing: Option<&'static str>,
    pub below: i32,
    /// `(inset, hanging)` for the per-block indent tag.
    pub indent: Option<(i32, i32)>,
    pub links: Vec<LinkSpan>,
    pub pills: Vec<(i32, i32)>,
    pub wants: Vec<(i32, Want)>,
    pub replacement: Option<i32>,
    pub list_level: usize,
    pub number: usize,
}

fn expand_len(text: &str) -> usize {
    mdview::expand_tabs(text).chars().count()
}

/// Nesting level of a list item of indent `depth` from the blocks before it.
pub fn list_level<'a>(previous: impl Iterator<Item = (&'a str, usize, usize)>, depth: usize) -> usize {
    if depth == 0 {
        return 0;
    }
    for (text, level, _) in previous {
        let Some(parent) = LIST.captures(text) else {
            break;
        };
        let parent_depth = expand_len(&parent[1]);
        if parent_depth <= depth {
            return level + usize::from(parent_depth < depth);
        }
    }
    1
}
/// Number shown for an ordered item: continue the sibling above or start at `initial`.
pub fn list_number<'a>(
    previous: impl Iterator<Item = (&'a str, usize, usize)>,
    depth: usize,
    initial: usize,
) -> usize {
    for (text, _, number) in previous {
        let Some(item) = LIST.captures(text) else {
            break;
        };
        let other = expand_len(&item[1]);
        if other < depth {
            break;
        }
        if other == depth {
            return if number > 0 { number + 1 } else { initial };
        }
    }
    initial
}

/// Lowercase letters for odd levels (`a.`, `b.` ... `aa.`), the plain token otherwise.
pub fn number_label(token: &str, level: usize) -> String {
    let digits = &token[..token.len() - 1];
    let mut number: usize = digits.parse().unwrap_or(0);
    if level % 2 == 1 {
        let mut label = String::new();
        while number > 0 {
            let remainder = (number - 1) % 26;
            number = (number - 1) / 26;
            label.insert(0, (b'a' + remainder as u8) as char);
        }
        label.push('.');
        return label;
    }
    token.to_string()
}

fn bullet_glyph(level: usize) -> &'static str {
    ["•", "◦", "▪"][level.min(2)]
}

/// Inputs shared by the inline planner.
pub struct InlineEnv {
    /// Insert-mark offset when the view is editable, else `None`.
    pub caret: Option<i32>,
    /// Absolute offset of the block start.
    pub block: i32,
}

/// Plan the inline spans of `text`, which starts at absolute offset `base`.
pub fn plan_inline(plan: &mut LinePlan, env: &InlineEnv, text: &str, base: i32, depth: usize) {
    if depth > 12 {
        return;
    }
    let text = if text.len() > INLINE_LIMIT {
        let mut cut = INLINE_LIMIT;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        &text[..cut]
    } else {
        text
    };
    let map = CharMap::new(text);
    let relative = |n: i32| n - env.block;
    let revealed = |a: i32, b: i32| env.caret.is_some_and(|c| a < c && c < b);
    let urls: Vec<(usize, usize)> = URL.find_iter(text).map(|m| (m.start(), m.end())).collect();
    let mut covered: Vec<(usize, usize)> = Vec::new();
    for found in mdview::_INLINE.captures_iter(text) {
        let Ok(m) = found else { break };
        let whole = m.get(0).expect("whole match");
        let a = base + map.at(whole.start());
        let b = base + map.at(whole.end());
        let active = revealed(a, b);
        let delimiter = m.get(8);
        if delimiter.is_some() && urls.iter().any(|&(f, l)| f <= whole.start() && whole.start() < l) {
            continue;
        }
        covered.push((whole.start(), whole.end()));
        if m.get(1).is_some() {
            if !active {
                plan.ops.push(Op::Hidden(a, a + 1));
            }
        } else if let Some(ticks) = m.get(2).filter(|t| !t.as_str().is_empty()) {
            let n = ticks.as_str().chars().count() as i32;
            plan.ops.push(Op::Tag("code", a, b));
            if !active {
                plan.pills.push((relative(a) + n, relative(b) - n));
                plan.ops.push(Op::Hidden(a, a + n));
                plan.ops.push(Op::Hidden(b - n, b));
            }
        } else if let Some(label) = m.get(5) {
            let image = m.get(4).is_some_and(|v| !v.as_str().is_empty());
            let target = m.get(6).map_or("", |t| t.as_str());
            let target = target.trim_matches(['<', '>']);
            let start = base + map.at(label.start());
            let end = base + map.at(label.end());
            plan.ops.push(Op::Tag(if image { "image" } else { "link" }, start, end));
            if !image {
                plan.links.push(LinkSpan {
                    a: relative(a),
                    b: relative(b),
                    target: target.into(),
                    revealed: active,
                });
            }
            if !active {
                plan.ops.push(Op::Hidden(a, start));
                plan.ops.push(Op::Hidden(end, b));
                if image {
                    plan.wants.push((
                        relative(a),
                        Want::Image {
                            a,
                            b,
                            target: target.into(),
                        },
                    ));
                }
            }
            if !image {
                plan_inline(plan, env, label.as_str(), start, depth + 1);
            }
        } else if let Some(auto) = m.get(7) {
            plan.ops.push(Op::Tag("link", a + 1, b - 1));
            plan.links.push(LinkSpan {
                a: relative(a),
                b: relative(b),
                target: auto.as_str().into(),
                revealed: active,
            });
            if !active {
                plan.ops.push(Op::Hidden(a, a + 1));
                plan.ops.push(Op::Hidden(b - 1, b));
            }
        } else if let Some(delimiter) = delimiter {
            let token = delimiter.as_str();
            let kinds: &[&'static str] = match token {
                "***" | "___" => &["strong", "emphasis"],
                "**" | "__" => &["strong"],
                "*" | "_" => &["emphasis"],
                _ => &["strike"],
            };
            for name in kinds {
                plan.ops.push(Op::Tag(name, a, b));
            }
            let n = token.chars().count() as i32;
            if !active {
                plan.ops.push(Op::Hidden(a, a + n));
                plan.ops.push(Op::Hidden(b - n, b));
            }
            if let Some(body) = m.get(9) {
                plan_inline(plan, env, body.as_str(), base + map.at(body.start()), depth + 1);
            }
        }
    }
    for (regex, wiki) in [(&*WIKI, true), (&*URL, false)] {
        for m in regex.captures_iter(text) {
            let whole = m.get(0).expect("whole match");
            if covered.iter().any(|&(s, e)| s <= whole.start() && whole.start() < e) {
                continue;
            }
            let a = base + map.at(whole.start());
            let b = base + map.at(whole.end());
            let active = revealed(a, b);
            let target = if wiki {
                format!("[[{}]]", &m[1])
            } else {
                whole.as_str().trim_end_matches(['.', ',', ';', '!', '?', ')']).to_string()
            };
            plan.ops.push(Op::Tag("link", a, b));
            plan.links.push(LinkSpan {
                a: relative(a),
                b: relative(b),
                target,
                revealed: active,
            });
            if wiki && !active {
                let shown = m.get(2).unwrap_or_else(|| m.get(1).expect("wiki name"));
                let start = base + map.at(shown.start());
                let end = base + map.at(shown.end());
                plan.ops.push(Op::Hidden(a, start));
                plan.ops.push(Op::Hidden(end, b));
            }
        }
    }
}

/// Everything `_paint_block` decided for a `line` block.
///
/// `active` is true when the caret sits in the block. `previous` yields
/// `(text, level, number)` of earlier blocks, nearest first (only consumed for
/// nested or ordered list items).
pub fn plan_line<'a, I>(
    text: &str,
    start: i32,
    active: bool,
    caret: Option<i32>,
    previous: impl Fn() -> I,
) -> LinePlan
where
    I: Iterator<Item = (&'a str, usize, usize)>,
{
    let end = start + text.chars().count() as i32;
    let line = text.strip_suffix('\n').unwrap_or(text);
    let line_chars = line.chars().count() as i32;
    let mut plan = LinePlan {
        spacing: Some("body"),
        below: BELOW_BODY,
        ..Default::default()
    };
    plan.ops.push(Op::Tag("body", start, end));
    if line.trim().is_empty() {
        if !active {
            plan.ops.push(Op::Tag("blank", start, end));
            plan.spacing = Some("blank");
            plan.below = 0;
        }
        return plan;
    }
    let heading = if blockedit::HEADING.is_match(line) {
        mdview::_HEADING.captures(line)
    } else {
        None
    };
    let env = InlineEnv {
        caret,
        block: start,
    };
    let content: usize;
    if let Some(heading) = heading {
        let level = heading[1].len();
        let name = ["h1", "h2", "h3", "h4", "h5", "h6"][level - 1];
        plan.ops.push(Op::Tag(name, start, end));
        plan.spacing = Some(name);
        plan.below = BELOW_HEADING;
        let content_bytes = heading.get(2).expect("heading body").start();
        content = content_bytes;
        let skipped = line[..content_bytes].chars().count() as i32;
        plan.ops.push(Op::Hidden(start, start + skipped));
        if let Some(tail) = TAIL_HASHES.find(line) {
            let first = line[..tail.start()].chars().count() as i32;
            plan.ops.push(Op::Hidden(start + first, start + line_chars));
        }
    } else if mdview::_RULE.is_match(line) {
        plan.wants.push((0, Want::Rule));
        plan.replacement = Some(24);
        plan.spacing = None;
        plan.below = BELOW_REPLACEMENT;
        return plan;
    } else if let Some(quote) = QUOTE.captures(line) {
        let prefix = quote.get(1).expect("quote prefix");
        content = prefix.end();
        let depth = prefix.as_str().matches('>').count() as i32;
        plan.indent = Some((22 * depth, 0));
        plan.ops.push(Op::Tag("quote", start, end));
        plan.spacing = Some("quote");
        plan.below = BELOW_QUOTE;
        let skipped = line[..content].chars().count() as i32;
        plan.ops.push(Op::Hidden(start, start + skipped));
        plan.wants.push((0, Want::Quote));
    } else if let Some(item) = LIST.captures(line) {
        content = item.get(0).expect("list prefix").end();
        let depth = expand_len(&item[1]);
        let level = list_level(previous(), depth);
        plan.list_level = level;
        plan.indent = Some((depth as i32 * 12 + 24, 0));
        let skipped = line[..content].chars().count() as i32;
        let content_start = start + skipped;
        let task = item.get(3);
        if task.is_some_and(|t| t.as_str().eq_ignore_ascii_case("x")) {
            plan.ops.push(Op::Tag("checked", content_start, start + line_chars));
        }
        plan.ops.push(Op::Hidden(start, content_start));
        let x = depth as i32 * 12;
        if let Some(task) = task {
            let toggle = line[..task.start()].chars().count() as i32;
            plan.wants.push((
                0,
                Want::Task {
                    checked: task.as_str().eq_ignore_ascii_case("x"),
                    toggle,
                    x,
                },
            ));
        } else {
            let marker = &item[2];
            if marker.starts_with(|c: char| c.is_ascii_digit()) {
                let initial: usize = marker[..marker.len() - 1].parse().unwrap_or(0);
                plan.number = list_number(previous(), depth, initial);
            }
            let text = if "-+*".contains(marker) {
                bullet_glyph(level).to_string()
            } else {
                let token = if plan.number > 0 {
                    format!("{}{}", plan.number, &marker[marker.len() - 1..])
                } else {
                    marker.to_string()
                };
                number_label(&token, level)
            };
            plan.wants.push((0, Want::Bullet { text, x }));
        }
    } else {
        content = 0;
    }
    let skipped = line[..content].chars().count() as i32;
    plan_inline(&mut plan, &env, &line[content..], start + skipped, 0);
    plan
}

/// Whether the first line of fenced code `lines` closes at the last line.
pub fn fence_closed(first: &str, last: &str, lines: usize) -> bool {
    let Some(fence) = mdview::_FENCE.captures(first.strip_suffix('\n').unwrap_or(first)) else {
        return false;
    };
    lines > 1 && mdview::closes_fence(last.strip_suffix('\n').unwrap_or(last), &fence[1])
}

/// Language word after the opening fence (empty when absent).
pub fn fence_language(first: &str) -> String {
    mdview::_FENCE
        .captures(first.strip_suffix('\n').unwrap_or(first))
        .and_then(|f| f[2].split_whitespace().next().map(str::to_string))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(Kind, String)> {
        split_blocks(text).into_iter().map(|b| (b.kind, b.text)).collect()
    }

    #[test]
    fn scanner_splits_lines_fences_and_tables() {
        let source = "# T\n\n```py\nx\n```\n| a | b |\n| --- | --- |\n| 1 | 2 |\n\nend";
        let blocks = split_blocks(source);
        let joined: String = blocks.iter().map(|b| b.text.as_str()).collect();
        assert_eq!(joined, source);
        assert_eq!(blocks[2].kind, Kind::Codeblock);
        assert_eq!(blocks[2].text, "```py\nx\n```\n");
        assert_eq!(blocks[3].kind, Kind::Table);
        assert_eq!(blocks.last().unwrap().text, "end");
        assert_eq!(blocks.last().unwrap().end, source.chars().count() as i32);
    }

    #[test]
    fn unclosed_fence_runs_to_the_end_and_offsets_count_characters() {
        let blocks = kinds("é\n```\nø\n");
        assert_eq!(blocks[1].0, Kind::Codeblock);
        assert_eq!(blocks[1].1, "```\nø\n");
        let blocks = split_blocks("é\nø\n");
        assert_eq!((blocks[1].start, blocks[1].end), (2, 4));
    }

    #[test]
    fn partial_chunks_ask_for_more_text() {
        let source = "```\nabc\n```\nnext\n";
        let mut cut = Scanner::new(&source[..9], 0, false);
        assert_eq!(cut.next_step(), Step::More { from: 0 });
        let mut whole = Scanner::new(source, 0, true);
        assert!(matches!(whole.next_step(), Step::Block(b) if b.kind == Kind::Codeblock));
        let mut line = Scanner::new("one\ntw", 0, false);
        assert!(matches!(line.next_step(), Step::Block(_)));
        assert_eq!(line.next_step(), Step::More { from: 4 });
        let mut table = Scanner::new("| a | b |\n| --- | --- |\n", 0, false);
        assert_eq!(table.next_step(), Step::More { from: 0 });
    }

    #[test]
    fn headings_hide_prefix_and_closing_hashes() {
        let plan = plan_line("## Title ##\n", 0, false, None, std::iter::empty);
        assert!(plan.ops.contains(&Op::Hidden(0, 3)));
        assert!(plan.ops.contains(&Op::Hidden(8, 11)));
        assert_eq!(plan.spacing, Some("h2"));
    }

    #[test]
    fn inline_spans_reveal_only_around_the_caret() {
        let text = "**bold** `code`\n";
        let hidden = plan_line(text, 0, false, Some(100), std::iter::empty);
        assert!(hidden.ops.contains(&Op::Hidden(0, 2)));
        assert_eq!(hidden.pills, vec![(10, 14)]);
        let shown = plan_line(text, 0, true, Some(4), std::iter::empty);
        assert!(!shown.ops.contains(&Op::Hidden(0, 2)));
        assert!(shown.ops.contains(&Op::Hidden(9, 10)));
    }

    #[test]
    fn markers_levels_and_letters() {
        assert_eq!(number_label("1.", 0), "1.");
        assert_eq!(number_label("1.", 1), "a.");
        assert_eq!(number_label("27.", 1), "aa.");
        let previous = [("- a\n", 0usize, 0usize)];
        assert_eq!(list_level(previous.iter().copied(), 2), 1);
        let ordered = [("1. a\n", 0, 1)];
        assert_eq!(list_number(ordered.iter().copied(), 0, 1), 2);
        assert_eq!(list_number(std::iter::empty(), 0, 5), 5);
    }

    #[test]
    fn links_images_and_wiki_names() {
        let plan = plan_line(
            "[web](https://example.org) ![pic](a.png) [[note|alias]] https://x.org.\n",
            0,
            false,
            None,
            std::iter::empty,
        );
        let targets: Vec<_> = plan.links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(targets, ["https://example.org", "[[note]]", "https://x.org"]);
        assert!(plan.wants.iter().any(|(_, w)| matches!(w, Want::Image { target, .. } if target == "a.png")));
    }

    #[test]
    fn long_lines_are_cut_and_never_panic() {
        let text = format!("{}\n", "ø*a*".repeat(4000));
        let plan = plan_line(&text, 0, false, None, std::iter::empty);
        assert!(plan.ops.len() > 10);
    }
}
