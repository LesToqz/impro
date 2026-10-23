//! Wikilinks, embeds, Markdown links and bare `<path>` references.

use super::mask::mask;
use super::text::{is_external, percent_decode, PosTracker};

/// Reference style as written by the author.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// `[[Target]]`
    Wiki,
    /// `![[file]]` or `![alt](file)`
    Embed,
    /// `[text](path)` or `<path>`
    Markdown,
}

/// Which syntax produced the link; path rules differ between the two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkSyntax {
    Wiki,
    Markdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// Path or name without heading, block id or query.
    pub target: String,
    pub kind: LinkKind,
    pub alias: Option<String>,
    pub heading: Option<String>,
    pub block: Option<String>,
    /// One-based line of the first character of the link.
    pub line: u32,
    /// Character (not byte) offset of the first character of the link.
    pub start: usize,
    pub syntax: LinkSyntax,
}

/// One syntactic match in masked text, before target cleaning.
pub(crate) struct RawLink<'a> {
    pub start: usize,
    pub end: usize,
    pub embed: bool,
    pub syntax: LinkSyntax,
    /// `[[inner]]`, the destination of `(…)`, or the content of `<…>`.
    pub inner: &'a str,
    /// The `[text]` part of a Markdown link.
    pub label: Option<&'a str>,
    pub autolink: bool,
}

/// Longest destination or title considered; longer ones are not links. This
/// keeps adversarial input (a megabyte of `[a](`) linear.
const MAX_SPAN: usize = 2048;

/// Remembers the last "first byte matching `pred` at or after here" answer so
/// runs of `[` do not rescan the same text. Queries must not move backwards.
struct StopCache {
    pred: fn(u8) -> bool,
    from: usize,
    at: Option<usize>,
    primed: bool,
}

impl StopCache {
    fn new(pred: fn(u8) -> bool) -> Self {
        StopCache {
            pred,
            from: 0,
            at: None,
            primed: false,
        }
    }

    fn find(&mut self, bytes: &[u8], from: usize) -> Option<usize> {
        let valid = self.primed
            && from >= self.from
            && match self.at {
                Some(at) => from <= at,
                None => true,
            };
        if !valid {
            let rest = &bytes[from.min(bytes.len())..];
            self.at = rest.iter().position(|b| (self.pred)(*b)).map(|i| i + from);
            self.from = from;
            self.primed = true;
        }
        self.at
    }
}

/// The three searches a Markdown link needs.
struct Caches {
    /// `]` or a line break (end of link text).
    text_end: StopCache,
    /// ASCII whitespace or `)` (end of a bare destination).
    dest_end: StopCache,
    /// `>` (end of an angle destination).
    angle_end: StopCache,
}

impl Caches {
    fn new() -> Self {
        Caches {
            text_end: StopCache::new(|b| b == b']' || b == b'\n'),
            dest_end: StopCache::new(|b| b == b')' || b == b' ' || (b'\t'..=b'\r').contains(&b)),
            angle_end: StopCache::new(|b| b == b'>'),
        }
    }
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    let limit = i + MAX_SPAN;
    for c in s[i..].chars() {
        if !c.is_whitespace() || i >= limit {
            break;
        }
        i += c.len_utf8();
    }
    i
}

/// After a destination: optional `"title"`, optional space, then `)`.
fn close_paren(s: &str, dest_end: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let ws = skip_ws(s, dest_end);
    if ws > dest_end && matches!(bytes.get(ws), Some(b'"') | Some(b'\'')) {
        let window = &bytes[ws + 1..bytes.len().min(ws + 1 + MAX_SPAN)];
        let line_end = memchr::memchr(b'\n', window).map_or(ws + 1 + window.len(), |i| ws + 1 + i);
        let mut q = ws + 1;
        while q < line_end {
            if matches!(bytes[q], b'"' | b'\'') {
                let after = skip_ws(s, q + 1);
                if bytes.get(after) == Some(&b')') {
                    return Some(after + 1);
                }
            }
            q += 1;
        }
    }
    (bytes.get(ws) == Some(&b')')).then_some(ws + 1)
}

/// `[label](dest "title")` starting at the `[` at `open`.
fn markdown_link<'a>(
    s: &'a str,
    open: usize,
    caches: &mut Caches,
) -> Option<(usize, &'a str, &'a str)> {
    let bytes = s.as_bytes();
    let stop = caches.text_end.find(bytes, open + 1)?;
    if bytes[stop] != b']' || bytes.get(stop + 1) != Some(&b'(') {
        return None;
    }
    let label = &s[open + 1..stop];
    let dest_start = skip_ws(s, stop + 2);
    if bytes.get(dest_start) == Some(&b'<') {
        if let Some(gt) = caches.angle_end.find(bytes, dest_start + 1) {
            if gt > dest_start + 1 && gt - dest_start <= MAX_SPAN {
                if let Some(end) = close_paren(s, gt + 1) {
                    return Some((end, label, &s[dest_start..gt + 1]));
                }
            }
        }
    }
    let j = caches
        .dest_end
        .find(bytes, dest_start)
        .unwrap_or(bytes.len());
    if j - dest_start > MAX_SPAN {
        return None;
    }
    if j == dest_start {
        return None;
    }
    close_paren(s, j).map(|end| (end, label, &s[dest_start..j]))
}

/// Walks every link-shaped region of already masked text in order.
pub(crate) fn scan<'a>(masked: &'a str, mut emit: impl FnMut(RawLink<'a>)) {
    let bytes = masked.as_bytes();
    let mut pos = 0;
    let mut cursor = 0; // end of the previous match
    let mut caches = Caches::new();
    while let Some(off) = memchr::memchr2(b'[', b'<', &bytes[pos..]) {
        let at = pos + off;
        pos = at + 1;
        if bytes[at] == b'<' {
            let mut j = at + 1;
            for c in masked[at + 1..].chars() {
                if c == '<' || c == '>' || c.is_whitespace() {
                    break;
                }
                j += c.len_utf8();
            }
            if j > at + 1 && bytes.get(j) == Some(&b'>') {
                emit(RawLink {
                    start: at,
                    end: j + 1,
                    embed: false,
                    syntax: LinkSyntax::Markdown,
                    inner: &masked[at + 1..j],
                    label: None,
                    autolink: true,
                });
                pos = j + 1;
                cursor = pos;
            }
            continue;
        }
        let embed = at > cursor && bytes[at - 1] == b'!';
        let start = if embed { at - 1 } else { at };
        // [[wiki]]
        if bytes.get(at + 1) == Some(&b'[') {
            if let Some(q) = caches.text_end.find(bytes, at + 2) {
                if bytes[q] == b']' && q > at + 2 && bytes.get(q + 1) == Some(&b']') {
                    emit(RawLink {
                        start,
                        end: q + 2,
                        embed,
                        syntax: LinkSyntax::Wiki,
                        inner: &masked[at + 2..q],
                        label: None,
                        autolink: false,
                    });
                    pos = q + 2;
                    cursor = pos;
                    continue;
                }
            }
        }
        if let Some((end, label, dest)) = markdown_link(masked, at, &mut caches) {
            emit(RawLink {
                start,
                end,
                embed,
                syntax: LinkSyntax::Markdown,
                inner: dest,
                label: Some(label),
                autolink: false,
            });
            pos = end;
            cursor = end;
        }
    }
}

struct Cleaned {
    target: String,
    heading: Option<String>,
    block: Option<String>,
}

fn clean_target(raw: &str, decode: bool) -> Cleaned {
    let raw = raw.trim();
    let decoded = if decode {
        percent_decode(raw)
    } else {
        std::borrow::Cow::Borrowed(raw)
    };
    let no_query = decoded.split('?').next().unwrap_or("");
    let (rest, block) = match no_query.split_once('^') {
        Some((a, b)) => (a, Some(b.to_string())),
        None => (no_query, None),
    };
    let (target, heading) = match rest.split_once('#') {
        Some((a, b)) => (a, Some(b.to_string())),
        None => (rest, None),
    };
    Cleaned {
        target: target.trim().to_string(),
        heading,
        block,
    }
}

/// Every link in `text`, ignoring code, comments, external URLs and anchors.
pub fn extract_links(text: &str) -> Vec<Link> {
    let masked = mask(text);
    let mut tracker = PosTracker::new(text);
    let mut out = Vec::new();
    scan(&masked, |raw| {
        let (target_src, alias, kind, decode): (String, Option<String>, LinkKind, bool) =
            match raw.syntax {
                LinkSyntax::Wiki => {
                    let inner = raw.inner.replace("\\|", "|");
                    let (t, a) = match inner.split_once('|') {
                        Some((t, a)) => (t.to_string(), Some(a.to_string())),
                        None => (inner, None),
                    };
                    let kind = if raw.embed {
                        LinkKind::Embed
                    } else {
                        LinkKind::Wiki
                    };
                    (t, a, kind, true)
                }
                LinkSyntax::Markdown => {
                    if raw.autolink && !raw.inner.contains('.') {
                        // `<br>`, `<div>`: markup, not a file reference
                        return;
                    }
                    let kind = if raw.embed {
                        LinkKind::Embed
                    } else {
                        LinkKind::Markdown
                    };
                    let dest = raw.inner.trim_matches(|c| c == '<' || c == '>');
                    (dest.to_string(), raw.label.map(str::to_string), kind, true)
                }
            };
        if is_external(&target_src) {
            return;
        }
        let cleaned = clean_target(&target_src, decode);
        if cleaned.target.is_empty() {
            return;
        }
        let (start, line) = tracker.at(raw.start);
        out.push(Link {
            target: cleaned.target,
            kind,
            alias,
            heading: cleaned.heading,
            block: cleaned.block,
            line,
            start,
            syntax: raw.syntax,
        });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_shapes() {
        let text = "[[Target#Heading^block|Alias]] ![[pic.png]] [name](a%20b.md#h \"Title\") ![alt](p.png) <file.md> [[a\\|b]] [[ ]]";
        let links = extract_links(text);
        let targets: Vec<_> = links.iter().map(|l| l.target.as_str()).collect();
        assert_eq!(
            targets,
            ["Target", "pic.png", "a b.md", "p.png", "file.md", "a"]
        );
        assert_eq!(links[0].heading.as_deref(), Some("Heading"));
        assert_eq!(links[0].block.as_deref(), Some("block"));
        assert_eq!(links[0].alias.as_deref(), Some("Alias"));
        let kinds: Vec<_> = links.iter().map(|l| l.kind).collect();
        use LinkKind::*;
        assert_eq!(kinds, [Wiki, Embed, Markdown, Embed, Markdown, Wiki]);
        assert_eq!(links.last().unwrap().alias.as_deref(), Some("b"));
        assert_eq!(links[2].heading.as_deref(), Some("h"));
        assert_eq!(links[2].alias.as_deref(), Some("name"));
        assert_eq!(extract_links("[x](a.md?q=1)")[0].target, "a.md");
        assert_eq!(extract_links("[[Self]]")[0].target, "Self");
    }

    #[test]
    fn ignored_regions_and_positions() {
        let text = "```md\n[[bad]] #bad\n```\n~~~\n[[bad]]\n~~~\n`[[bad]]` ``a ` [[bad]]`` <!-- [[bad]] -->\n[[Good]]";
        let links = extract_links(text);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Good");
        assert_eq!(links[0].start, text.find("[[Good]]").unwrap());
        assert_eq!(links[0].line, 8);
        assert!(extract_links(
            "[x](https://x) [x](http://x) [x](mailto:x) [x](data:x) [x](#anchor) <https://x>"
        )
        .is_empty());
        assert!(extract_links("```\n[[unclosed]]").is_empty());
    }

    #[test]
    fn offsets_count_characters_and_embed_prefix() {
        let text = "é€ ![[pic.png]]";
        let l = &extract_links(text)[0];
        assert_eq!(l.start, 3);
        assert_eq!(l.kind, LinkKind::Embed);
        assert_eq!(l.line, 1);
    }

    #[test]
    fn frontmatter_properties_count() {
        let text = "---\nrelation: \"[[Other]]\"\n---\nBody";
        assert_eq!(extract_links(text)[0].target, "Other");
    }

    #[test]
    fn markdown_edge_cases() {
        // angle destination with spaces, quoted titles, empty/unclosed forms
        let l = extract_links("[a](<my file.md> 'T')");
        assert_eq!(l[0].target, "my file.md");
        assert!(extract_links("[a]()").is_empty());
        assert!(extract_links("[a](b.md").is_empty());
        assert!(extract_links("[a]\n(b.md)").is_empty());
        // nested brackets in the label are tolerated
        assert_eq!(extract_links("[[a]](b.md)")[0].target, "a");
        assert_eq!(extract_links("[x [y](b.md)")[0].target, "b.md");
        // markup that looks like an autolink is not a file reference
        assert!(extract_links("<br> <div class=a> </div> <br/>").is_empty());
        // path-like heading-only links stay local
        assert!(extract_links("[[#Heading]]").is_empty());
    }

    #[test]
    fn dotted_and_nested_paths_are_kept() {
        let l = extract_links("[[dir/Note.v2#h]] [[../up]]");
        assert_eq!(l[0].target, "dir/Note.v2");
        assert_eq!(l[1].target, "../up");
    }
}
