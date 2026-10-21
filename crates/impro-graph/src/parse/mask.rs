//! Hides regions that must not produce links or tags (comments, fenced code
//! and inline code). Masked bytes become spaces, so byte offsets, line breaks
//! and therefore line numbers are preserved exactly.

use std::borrow::Cow;
use std::collections::HashMap;

fn blank(buf: &mut [u8], from: usize, to: usize) {
    for b in &mut buf[from..to] {
        if *b != b'\n' {
            *b = b' ';
        }
    }
}

/// Returns the text with comments and code blanked out. Borrows when there is
/// nothing to hide.
pub fn mask(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let maybe = memchr::memmem::find(bytes, b"<!--").is_some()
        || memchr::memchr2(b'`', b'~', bytes).is_some();
    if !maybe {
        return Cow::Borrowed(text);
    }
    let mut buf = bytes.to_vec();
    blank_comments(&mut buf);
    blank_fences(&mut buf);
    blank_inline_code(&mut buf);
    match String::from_utf8(buf) {
        Ok(s) => Cow::Owned(s),
        // unreachable in practice: only whole ASCII-delimited ranges are blanked
        Err(e) => Cow::Owned(String::from_utf8_lossy(e.as_bytes()).into_owned()),
    }
}

fn blank_comments(buf: &mut [u8]) {
    let mut at = 0;
    while let Some(i) = memchr::memmem::find(&buf[at..], b"<!--") {
        let start = at + i;
        let end = match memchr::memmem::find(&buf[start + 4..], b"-->") {
            Some(j) => start + 4 + j + 3,
            None => buf.len(),
        };
        blank(buf, start, end);
        at = end;
    }
}

/// Length of a fence run at the start of `line` (after up to three spaces),
/// with its marker byte.
fn fence_of(line: &[u8]) -> Option<(u8, usize, &[u8])> {
    let mut i = 0;
    while i < line.len() && i < 3 && line[i] == b' ' {
        i += 1;
    }
    let marker = *line.get(i)?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let mut n = 0;
    while line.get(i + n) == Some(&marker) {
        n += 1;
    }
    (n >= 3).then(|| (marker, n, &line[i + n..]))
}

fn blank_fences(buf: &mut [u8]) {
    let mut open: Option<(u8, usize)> = None;
    let mut pos = 0;
    while pos < buf.len() {
        let end = memchr::memchr(b'\n', &buf[pos..]).map_or(buf.len(), |i| pos + i + 1);
        let fence = fence_of(&buf[pos..end]).map(|(m, n, rest)| {
            (
                m,
                n,
                rest.iter().all(|b| b.is_ascii_whitespace() || *b == 0x0b),
            )
        });
        match (open, fence) {
            (Some((m, n)), f) => {
                if let Some((fm, fnn, only_ws)) = f {
                    if fm == m && fnn >= n && only_ws {
                        open = None;
                    }
                }
                blank(buf, pos, end);
            }
            (None, Some((m, n, _))) => {
                open = Some((m, n));
                blank(buf, pos, end);
            }
            (None, None) => {}
        }
        pos = end;
    }
}

/// A code span opens with a backtick run and closes at the next run of the
/// same length; if a run finds no partner, shorter tails of it are tried.
fn blank_inline_code(buf: &mut [u8]) {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while let Some(off) = memchr::memchr(b'`', &buf[i..]) {
        let start = i + off;
        let mut len = 1;
        while buf.get(start + len) == Some(&b'`') {
            len += 1;
        }
        runs.push((start, len));
        i = start + len;
    }
    if runs.len() < 2 {
        return;
    }
    let mut by_len: HashMap<usize, Vec<usize>> = HashMap::new();
    for (idx, (_, len)) in runs.iter().enumerate() {
        by_len.entry(*len).or_default().push(idx);
    }
    let mut r = 0;
    while r < runs.len() {
        let (start, len) = runs[r];
        let mut found = None;
        for k in (1..=len).rev() {
            if let Some(list) = by_len.get(&k) {
                let at = list.partition_point(|&j| j <= r);
                if let Some(&j) = list.get(at) {
                    found = Some((start + (len - k), j));
                    break;
                }
            }
        }
        match found {
            Some((from, j)) => {
                let (cs, cl) = runs[j];
                blank(buf, from, cs + cl);
                r = j + 1;
            }
            None => r += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_borrowed() {
        assert!(matches!(mask("[[a]] text"), Cow::Borrowed(_)));
    }

    #[test]
    fn hides_fences_comments_and_inline_code() {
        let text = "```md\n[[bad]]\n```\n~~~\n[[bad]]\n~~~\n`[[bad]]` ``a ` [[bad]]`` <!-- [[bad]] -->\n[[Good]]";
        let m = mask(text);
        assert_eq!(m.len(), text.len());
        assert_eq!(m.matches("[[").count(), 1);
        assert!(m.contains("[[Good]]"));
        assert_eq!(m.matches('\n').count(), text.matches('\n').count());
    }

    #[test]
    fn unclosed_fence_and_comment_run_to_the_end() {
        assert!(!mask("```\n[[unclosed]]").contains("[["));
        assert!(!mask("a <!-- [[x]]").contains("[["));
    }

    #[test]
    fn fence_needs_matching_marker_and_length() {
        let t = "````\n```\n[[in]]\n````\n[[out]]";
        let m = mask(t);
        assert!(!m.contains("[[in]]"));
        assert!(m.contains("[[out]]"));
        // a longer info string line does not close
        let t = "```\n```js\n[[in]]\n```\n[[out]]";
        let m = mask(t);
        assert!(!m.contains("[[in]]"));
        assert!(m.contains("[[out]]"));
    }

    #[test]
    fn unpaired_backticks_stay_visible_and_multibyte_survives() {
        let m = mask("`[[a]] é`x `[[b]]");
        assert!(!m.contains("[[a]]"));
        assert!(m.contains("[[b]]"));
        assert_eq!(mask("é `é` [[é]]").as_ref().len(), "é `é` [[é]]".len());
    }

    #[test]
    fn spans_may_cross_lines() {
        let m = mask("`one\n[[x]]` [[y]]");
        assert!(!m.contains("[[x]]"));
        assert!(m.contains("[[y]]"));
    }
}
