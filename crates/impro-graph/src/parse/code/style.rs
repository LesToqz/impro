//! CSS/Sass `@import`, `@use`, `@forward`, `url()` and HTML `src`/`href`.

use super::{CodeKind, CodeRef};
use crate::parse::text::{is_external, LineTracker};

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn skip_ws(s: &[u8], mut i: usize) -> usize {
    while s.get(i).is_some_and(|b| b.is_ascii_whitespace()) {
        i += 1;
    }
    i
}

fn quoted(text: &str, i: usize) -> Option<(&str, usize)> {
    let s = text.as_bytes();
    let q = *s.get(i)?;
    if q != b'"' && q != b'\'' {
        return None;
    }
    let end = memchr::memchr2(q, b'\n', &s[i + 1..])?;
    (s[i + 1 + end] == q).then(|| (&text[i + 1..i + 1 + end], i + end + 2))
}

/// `url(` argument starting right after the parenthesis.
fn url_arg(text: &str, i: usize) -> Option<(&str, usize)> {
    let s = text.as_bytes();
    let i = skip_ws(s, i);
    if let Some(r) = quoted(text, i) {
        return Some(r);
    }
    let mut j = i;
    while j < s.len() && !s[j].is_ascii_whitespace() && !matches!(s[j], b')' | b'\'' | b'"' | b';')
    {
        j += 1;
    }
    (j > i).then(|| (&text[i..j], j))
}

pub fn css_imports(text: &str, line_comments: bool) -> Vec<CodeRef> {
    let s = text.as_bytes();
    let mut out = Vec::new();
    let mut lines = LineTracker::new(text);
    let mut push = |spec: &str, at: usize, out: &mut Vec<CodeRef>| {
        if !is_external(spec) {
            out.push(CodeRef {
                spec: spec.trim().to_string(),
                kind: CodeKind::Style,
                line: lines.line_at(at),
            });
        }
    };
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b'/' if s.get(i + 1) == Some(&b'*') => {
                i = memchr::memmem::find(&s[i + 2..], b"*/").map_or(s.len(), |n| i + 2 + n + 2);
            }
            b'/' if line_comments && s.get(i + 1) == Some(&b'/') => {
                i = memchr::memchr(b'\n', &s[i..]).map_or(s.len(), |n| i + n);
            }
            b'@' => {
                let start = i;
                let mut j = i + 1;
                while j < s.len() && (is_word(s[j]) || s[j] == b'-') {
                    j += 1;
                }
                let name = text[i + 1..j].to_ascii_lowercase();
                i = j;
                if !matches!(name.as_str(), "import" | "use" | "forward") {
                    continue;
                }
                let mut k = skip_ws(s, j);
                loop {
                    if let Some((spec, end)) = quoted(text, k) {
                        push(spec, start, &mut out);
                        k = skip_ws(s, end);
                    } else if s.len() >= k + 4 && s[k..k + 4].eq_ignore_ascii_case(b"url(") {
                        if let Some((spec, end)) = url_arg(text, k + 4) {
                            push(spec, start, &mut out);
                            k = end;
                        }
                        break;
                    } else {
                        break;
                    }
                    if s.get(k) == Some(&b',') && name == "import" {
                        k = skip_ws(s, k + 1);
                    } else {
                        break;
                    }
                }
                i = k.max(i);
            }
            b'u' | b'U'
                if s.len() >= i + 4
                    && s[i..i + 4].eq_ignore_ascii_case(b"url(")
                    && (i == 0 || !is_word(s[i - 1])) =>
            {
                if let Some((spec, end)) = url_arg(text, i + 4) {
                    push(spec, i, &mut out);
                    i = end;
                } else {
                    i += 4;
                }
            }
            _ => i += 1,
        }
    }
    out
}

pub fn html_refs(text: &str) -> Vec<CodeRef> {
    let s = text.as_bytes();
    let mut out = Vec::new();
    let mut lines = LineTracker::new(text);
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'<' && s[i..].starts_with(b"<!--") {
            i = memchr::memmem::find(&s[i + 4..], b"-->").map_or(s.len(), |n| i + 4 + n + 3);
            continue;
        }
        let rest = &s[i..];
        let attr = if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case(b"src") {
            3
        } else if rest.len() >= 4 && rest[..4].eq_ignore_ascii_case(b"href") {
            4
        } else {
            0
        };
        if attr == 0 || (i > 0 && is_word(s[i - 1])) {
            i += 1;
            continue;
        }
        let eq = skip_ws(s, i + attr);
        if s.get(eq) != Some(&b'=') {
            i += attr;
            continue;
        }
        let v = skip_ws(s, eq + 1);
        let found = match quoted(text, v) {
            Some(r) => Some(r),
            None => {
                let mut j = v;
                while j < s.len() && !s[j].is_ascii_whitespace() && s[j] != b'>' {
                    j += 1;
                }
                (j > v).then(|| (&text[v..j], j))
            }
        };
        match found {
            Some((spec, end)) => {
                if !is_external(spec) {
                    out.push(CodeRef {
                        spec: spec.trim().to_string(),
                        kind: CodeKind::Html,
                        line: lines.line_at(i),
                    });
                }
                i = end;
            }
            None => i = v.max(i + 1),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(refs: Vec<CodeRef>) -> Vec<String> {
        refs.into_iter().map(|r| r.spec).collect()
    }

    #[test]
    fn css_imports_and_urls() {
        let t =
            "@import \"other.css\"; a {x:url(img.png)} b {x:url(\"data:x\")} @import url('u.css');";
        assert_eq!(
            specs(css_imports(t, false)),
            ["other.css", "img.png", "u.css"]
        );
    }

    #[test]
    fn sass_directives_and_comments() {
        let t = "// @import 'no'\n/* @import 'no2' */\n@use \"sass:math\";\n@use './theme';\n@forward 'base';\n@import 'a', 'b';\na {background:url(./img.png)}";
        assert_eq!(
            specs(css_imports(t, true)),
            ["./theme", "base", "a", "b", "./img.png"]
        );
        let p = css_imports("a{}\n\n@import 'x';", false);
        assert_eq!(p[0].line, 3);
    }

    #[test]
    fn html_attributes() {
        let t = "<a href=\"page.html\"><img src=\"img.png\"><a href=\"https://x\"><a href=\"#id\">";
        assert_eq!(specs(html_refs(t)), ["page.html", "img.png"]);
        assert_eq!(specs(html_refs("<img src=photo.png>")), ["photo.png"]);
        assert_eq!(
            specs(html_refs(
                "<!-- <img src=hidden.png> --><SCRIPT SRC = 'a.js'></SCRIPT>"
            )),
            ["a.js"]
        );
        assert!(html_refs("<p>resource = 3</p>").is_empty());
    }
}
