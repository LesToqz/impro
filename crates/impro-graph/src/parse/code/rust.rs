//! Rust `mod name;` declarations and `use crate::…` paths.

use super::{CodeKind, CodeRef};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tok<'a> {
    Ident(&'a str),
    Path, // ::
    Open,
    Close,
    Comma,
    Semi,
    Star,
    Other,
}

struct Lexer<'a> {
    text: &'a str,
    i: usize,
    line: u32,
}

impl<'a> Lexer<'a> {
    fn skip_quoted(&mut self, quote: u8) {
        let b = self.text.as_bytes();
        self.i += 1;
        while self.i < b.len() {
            match b[self.i] {
                b'\\' => self.i += 1,
                b'\n' => self.line += 1,
                c if c == quote => {
                    self.i += 1;
                    return;
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    fn skip_raw(&mut self) -> bool {
        // at the first `#` or `"` after an `r` prefix
        let b = self.text.as_bytes();
        let mut hashes = 0;
        let mut j = self.i;
        while b.get(j) == Some(&b'#') {
            hashes += 1;
            j += 1;
        }
        if b.get(j) != Some(&b'"') {
            return false;
        }
        j += 1;
        while j < b.len() {
            if b[j] == b'\n' {
                self.line += 1;
            }
            if b[j] == b'"' && b[j + 1..].iter().take(hashes).filter(|c| **c == b'#').count() == hashes
                && b.len() >= j + 1 + hashes
            {
                self.i = j + 1 + hashes;
                return true;
            }
            j += 1;
        }
        self.i = b.len();
        true
    }

    fn next(&mut self) -> Option<(Tok<'a>, u32)> {
        let b = self.text.as_bytes();
        loop {
            let c = *b.get(self.i)?;
            match c {
                b'\n' => {
                    self.line += 1;
                    self.i += 1;
                }
                c if c.is_ascii_whitespace() => self.i += 1,
                b'/' if b.get(self.i + 1) == Some(&b'/') => {
                    while self.i < b.len() && b[self.i] != b'\n' {
                        self.i += 1;
                    }
                }
                b'/' if b.get(self.i + 1) == Some(&b'*') => {
                    let mut depth = 0;
                    while self.i < b.len() {
                        if b[self.i..].starts_with(b"/*") {
                            depth += 1;
                            self.i += 2;
                        } else if b[self.i..].starts_with(b"*/") {
                            depth -= 1;
                            self.i += 2;
                            if depth == 0 {
                                break;
                            }
                        } else {
                            if b[self.i] == b'\n' {
                                self.line += 1;
                            }
                            self.i += 1;
                        }
                    }
                }
                b'"' => self.skip_quoted(b'"'),
                b'\'' => {
                    // char literal or lifetime
                    let rest = &self.text[self.i + 1..];
                    let mut it = rest.chars();
                    match it.next() {
                        Some('\\') => self.skip_quoted(b'\''),
                        Some(ch) if it.next() == Some('\'') => self.i += 2 + ch.len_utf8(),
                        _ => self.i += 1,
                    }
                }
                c if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => {
                    let start = self.i;
                    while self.i < b.len()
                        && (b[self.i].is_ascii_alphanumeric() || b[self.i] == b'_' || b[self.i] >= 0x80)
                    {
                        self.i += 1;
                    }
                    let word = &self.text[start..self.i];
                    if matches!(word, "r" | "br" | "cr") && matches!(b.get(self.i), Some(b'#') | Some(b'"'))
                        && self.skip_raw()
                    {
                        continue;
                    }
                    return Some((Tok::Ident(word), self.line));
                }
                c if c.is_ascii_digit() => {
                    while self.i < b.len() && (b[self.i].is_ascii_alphanumeric() || b[self.i] == b'_') {
                        self.i += 1;
                    }
                }
                _ => {
                    let line = self.line;
                    self.i += 1;
                    let tok = match c {
                        b':' if b.get(self.i) == Some(&b':') => {
                            self.i += 1;
                            Tok::Path
                        }
                        b'{' => Tok::Open,
                        b'}' => Tok::Close,
                        b',' => Tok::Comma,
                        b';' => Tok::Semi,
                        b'*' => Tok::Star,
                        _ => Tok::Other,
                    };
                    return Some((tok, line));
                }
            }
        }
    }
}

const MAX_USE_PATHS: usize = 256;

/// Parses one use tree below `prefix`, pushing each full path found.
fn use_tree<'a>(
    lx: &mut Lexer<'a>,
    first: Option<(Tok<'a>, u32)>,
    prefix: &mut Vec<&'a str>,
    out: &mut Vec<String>,
) -> Option<(Tok<'a>, u32)> {
    let depth = prefix.len();
    let mut tok = first;
    // leading `::`
    if let Some((Tok::Path, _)) = tok {
        tok = lx.next();
    }
    loop {
        match tok {
            Some((Tok::Ident("as"), _)) => {
                lx.next(); // alias
                tok = lx.next();
            }
            Some((Tok::Ident(seg), _)) => {
                prefix.push(seg);
                tok = lx.next();
                if let Some((Tok::Path, _)) = tok {
                    tok = lx.next();
                    continue;
                }
            }
            Some((Tok::Open, _)) => {
                let mut inner = lx.next();
                loop {
                    if matches!(inner, Some((Tok::Close, _)) | None) {
                        break;
                    }
                    let before = prefix.len();
                    inner = use_tree(lx, inner, prefix, out);
                    prefix.truncate(before);
                    if let Some((Tok::Comma, _)) = inner {
                        inner = lx.next();
                    }
                }
                tok = lx.next();
                prefix.truncate(depth);
                return tok;
            }
            Some((Tok::Star, _)) => {
                tok = lx.next();
            }
            _ => break,
        }
    }
    if prefix.len() > depth || depth > 0 {
        let mut path = prefix.clone();
        if path.last() == Some(&"self") && path.len() > 1 {
            path.pop();
        }
        if out.len() < MAX_USE_PATHS && !path.is_empty() {
            out.push(path.join("::"));
        }
    }
    prefix.truncate(depth);
    tok
}

pub fn imports(text: &str) -> Vec<CodeRef> {
    let mut lx = Lexer { text, i: 0, line: 1 };
    let mut out = Vec::new();
    let mut tok = lx.next();
    while let Some((t, line)) = tok {
        match t {
            Tok::Ident("mod") => {
                let name = lx.next();
                if let Some((Tok::Ident(n), _)) = name {
                    let after = lx.next();
                    if let Some((Tok::Semi, _)) = after {
                        out.push(CodeRef {
                            spec: n.trim_start_matches("r#").to_string(),
                            kind: CodeKind::RustMod,
                            line,
                        });
                    }
                    tok = after;
                    continue;
                }
                tok = name;
                continue;
            }
            Tok::Ident("use") => {
                let first = lx.next();
                let mut paths = Vec::new();
                let mut prefix = Vec::new();
                let after = use_tree(&mut lx, first, &mut prefix, &mut paths);
                for p in paths {
                    let head = p.split("::").next().unwrap_or("");
                    if matches!(head, "crate" | "self" | "super") {
                        out.push(CodeRef {
                            spec: p,
                            kind: CodeKind::RustUse,
                            line,
                        });
                    }
                }
                tok = after;
                continue;
            }
            _ => {}
        }
        tok = lx.next();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(text: &str) -> Vec<(String, CodeKind)> {
        imports(text).into_iter().map(|r| (r.spec, r.kind)).collect()
    }

    #[test]
    fn modules_and_use_paths() {
        let t = "pub mod a;\nmod b { fn f() {} }\nuse crate::x::y;\nuse std::io;\nuse super::z as w;\nuse self::q::*;\n";
        assert_eq!(
            specs(t),
            [
                ("a".to_string(), CodeKind::RustMod),
                ("crate::x::y".to_string(), CodeKind::RustUse),
                ("super::z".to_string(), CodeKind::RustUse),
                ("self::q".to_string(), CodeKind::RustUse),
            ]
        );
    }

    #[test]
    fn nested_use_trees() {
        let t = "use crate::{a, b::{c, self}, d as e};";
        let s: Vec<_> = specs(t).into_iter().map(|r| r.0).collect();
        assert_eq!(s, ["crate::a", "crate::b::c", "crate::b", "crate::d"]);
    }

    #[test]
    fn comments_strings_and_lifetimes_are_skipped() {
        let t = "// mod no;\n/* /* nested */ mod no2; */\nfn f<'a>(x: &'a str) -> char { let _ = \"mod no3;\"; let _ = r#\"mod no4;\"#; 'x' }\nmod real;\n";
        assert_eq!(specs(t), [("real".to_string(), CodeKind::RustMod)]);
        assert_eq!(imports(t)[0].line, 4);
    }

    #[test]
    fn degenerate_input() {
        assert!(specs("mod").is_empty());
        assert!(specs("use").is_empty());
        assert!(specs("use crate::{").is_empty() || specs("use crate::{").len() <= 1);
        assert!(specs("use ;").is_empty());
        assert!(specs("fn f() { let c = '\\''; }").is_empty());
    }
}
