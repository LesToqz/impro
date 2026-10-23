//! Inline `#tag` and frontmatter `tags:` extraction.

use super::frontmatter::{Frontmatter, PropValue};
use super::links::scan;
use super::mask::mask;
use super::text::{is_space, is_tag_char};

fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.chars().all(is_tag_char)
        && tag
            .chars()
            .any(|c| c.is_alphabetic() || c == '_' || c == '-' || c == '/')
}

fn push_unique(out: &mut Vec<String>, tag: &str) {
    let tag = tag.trim_start_matches('#');
    if valid_tag(tag) && !out.iter().any(|t| t == tag) {
        out.push(tag.to_string());
    }
}

/// Tags listed in the `tags:` property (a list or a comma/space separated string).
pub fn frontmatter_tags(fm: &Frontmatter) -> Vec<String> {
    let mut out = Vec::new();
    match fm.get("tags") {
        Some(PropValue::List(items)) => items.iter().for_each(|t| push_unique(&mut out, t)),
        Some(PropValue::Text(s)) => s
            .split(|c: char| c == ',' || c.is_whitespace())
            .for_each(|t| push_unique(&mut out, t)),
        None => {}
    }
    out
}

/// Blanks every `scheme://…` and `mailto:…` URL (up to the next blank) so a
/// `#fragment` inside stays untouched. A prefix with nothing after it is no URL.
fn blank_urls(buf: &mut [u8]) {
    let Ok(text) = std::str::from_utf8(buf) else {
        return;
    };
    let b = text.as_bytes();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let hit = if b[i] == b':' && b[i + 1..].starts_with(b"//") && i > 0 {
            // scheme letters directly before the colon
            let mut s = i;
            while s > 0 && b[s - 1].is_ascii_alphabetic() {
                s -= 1;
            }
            (s < i).then_some((s, i + 3))
        } else if b[i..].len() >= 7 && b[i..i + 7].eq_ignore_ascii_case(b"mailto:") {
            Some((i, i + 7))
        } else {
            None
        };
        i = match hit {
            Some((start, prefix_end)) => {
                let len: usize = text[prefix_end..]
                    .chars()
                    .take_while(|c| !is_space(*c))
                    .map(char::len_utf8)
                    .sum();
                if len > 0 {
                    ranges.push((start, prefix_end + len));
                    prefix_end + len
                } else {
                    i + 1
                }
            }
            None => i + 1,
        };
    }
    for (start, end) in ranges {
        buf[start..end].fill(b' ');
    }
}

/// Inline tags of a note body. `body` is the text after the frontmatter.
pub fn inline_tags(body: &str, out: &mut Vec<String>) {
    if memchr::memchr(b'#', body.as_bytes()).is_none() {
        return;
    }
    let masked = mask(body);
    let mut buf = masked.as_bytes().to_vec();
    // links hide their anchors (`[[x#heading]]`, `[a](b#c)`)
    let mut spans = Vec::new();
    scan(&masked, |raw| spans.push((raw.start, raw.end)));
    for (s, e) in spans {
        buf[s..e].fill(b' ');
    }
    blank_urls(&mut buf);
    let text = match std::str::from_utf8(&buf) {
        Ok(t) => t,
        Err(_) => return,
    };
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(off) = memchr::memchr(b'#', &bytes[at..]) {
        let hash = at + off;
        at = hash + 1;
        let prev = text[..hash].chars().next_back();
        if let Some(p) = prev {
            if super::text::is_word_char(p) || p == '/' || p == '#' {
                continue;
            }
        }
        let mut end = hash + 1;
        for c in text[hash + 1..].chars() {
            if !is_tag_char(c) {
                break;
            }
            end += c.len_utf8();
        }
        if end > hash + 1 {
            push_unique(out, &text[hash + 1..end]);
            at = end;
        }
    }
}

/// All tags of a note: frontmatter first, then inline ones.
pub fn extract_tags(text: &str) -> Vec<String> {
    let fm = super::frontmatter::parse(text);
    let mut tags = frontmatter_tags(&fm);
    inline_tags(&text[fm.body_start..], &mut tags);
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_tag_sources() {
        let text = "---\ntags: \"#front, second third\"\n---\n# Heading\n#front #a/b/c #123 #a1 #_ #hy-phen #кириллица `#code` [#label](x#anchor) [[x#heading]] https://x/#url abc#inside #end";
        assert_eq!(
            extract_tags(text),
            [
                "front",
                "second",
                "third",
                "a/b/c",
                "a1",
                "_",
                "hy-phen",
                "кириллица",
                "end"
            ]
        );
        assert_eq!(
            extract_tags("---\ntags:\n - \"#one\"\n - two\n---\n#two"),
            ["one", "two"]
        );
    }

    #[test]
    fn masked_regions_have_no_tags() {
        let text =
            "```md\n[[bad]] #bad\n```\n~~~\n[[bad]]\n~~~\n`#x` <!-- #y --> mailto:a@b#c ftp://h/#z";
        assert!(extract_tags(text).is_empty());
    }

    #[test]
    fn heading_markers_are_not_tags() {
        assert!(extract_tags("# One\n## Two\n### Three #\n").is_empty());
        assert_eq!(extract_tags("text #ok# #next"), ["ok", "next"]);
    }
}
