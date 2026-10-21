//! Python imports via a tiny tokenizer (no parser): handles comments, string
//! literals (including triple quoted), brackets and line continuations.

use super::{CodeKind, CodeRef};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tok<'a> {
    Name(&'a str),
    Dot,
    Comma,
    Star,
    LParen,
    Semi,
    Colon,
    Newline,
    Other,
}

struct Lexer<'a> {
    text: &'a str,
    i: usize,
    line: u32,
    depth: i32,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        Lexer {
            text,
            i: 0,
            line: 1,
            depth: 0,
        }
    }

    fn skip_string(&mut self, quote: u8) {
        let b = self.text.as_bytes();
        let triple = b.get(self.i + 1) == Some(&quote) && b.get(self.i + 2) == Some(&quote);
        self.i += if triple { 3 } else { 1 };
        while self.i < b.len() {
            match b[self.i] {
                b'\\' => {
                    if b.get(self.i + 1) == Some(&b'\n') {
                        self.line += 1;
                    }
                    self.i += 2;
                    continue;
                }
                b'\n' => {
                    if !triple {
                        // unterminated single-quoted string ends at the line
                        return;
                    }
                    self.line += 1;
                }
                c if c == quote => {
                    if !triple {
                        self.i += 1;
                        return;
                    }
                    if b.get(self.i + 1) == Some(&quote) && b.get(self.i + 2) == Some(&quote) {
                        self.i += 3;
                        return;
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
    }

    /// Next token and the line it starts on.
    fn next(&mut self) -> Option<(Tok<'a>, u32)> {
        let b = self.text.as_bytes();
        loop {
            let c = *b.get(self.i)?;
            match c {
                b' ' | b'\t' | b'\r' | 0x0c => self.i += 1,
                b'\n' => {
                    let line = self.line;
                    self.line += 1;
                    self.i += 1;
                    if self.depth <= 0 {
                        return Some((Tok::Newline, line));
                    }
                }
                b'\\' if matches!(b.get(self.i + 1), Some(b'\n') | Some(b'\r')) => {
                    self.i += 1;
                    if b[self.i] == b'\r' {
                        self.i += 1;
                    }
                    if b.get(self.i) == Some(&b'\n') {
                        self.i += 1;
                        self.line += 1;
                    }
                }
                b'#' => {
                    while self.i < b.len() && b[self.i] != b'\n' {
                        self.i += 1;
                    }
                }
                b'\'' | b'"' => {
                    let line = self.line;
                    self.skip_string(c);
                    return Some((Tok::Other, line));
                }
                _ if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => {
                    let start = self.i;
                    while self.i < b.len()
                        && (b[self.i].is_ascii_alphanumeric()
                            || b[self.i] == b'_'
                            || b[self.i] >= 0x80)
                    {
                        self.i += 1;
                    }
                    // `i` always lands on a char boundary: continuation bytes
                    // are >= 0x80 and consumed with their lead byte
                    return Some((Tok::Name(&self.text[start..self.i]), self.line));
                }
                _ if c.is_ascii_digit() => {
                    while self.i < b.len() && (b[self.i].is_ascii_alphanumeric() || b[self.i] == b'_')
                    {
                        self.i += 1;
                    }
                    return Some((Tok::Other, self.line));
                }
                _ => {
                    self.i += 1;
                    let tok = match c {
                        b'.' => Tok::Dot,
                        b',' => Tok::Comma,
                        b'*' => Tok::Star,
                        b'(' => {
                            self.depth += 1;
                            Tok::LParen
                        }
                        b'[' | b'{' => {
                            self.depth += 1;
                            Tok::Other
                        }
                        b')' | b']' | b'}' => {
                            self.depth = (self.depth - 1).max(0);
                            Tok::Other
                        }
                        b';' => Tok::Semi,
                        b':' => Tok::Colon,
                        _ => Tok::Other,
                    };
                    return Some((tok, self.line));
                }
            }
        }
    }
}

/// Parses `a.b.c` and returns it joined with dots.
fn dotted<'a>(lx: &mut Lexer<'a>, first: Option<(Tok<'a>, u32)>) -> (String, Option<(Tok<'a>, u32)>) {
    let mut name = String::new();
    let mut tok = first;
    loop {
        match tok {
            Some((Tok::Name(n), _)) if n != "import" => name.push_str(n),
            _ => return (name, tok),
        }
        tok = lx.next();
        if let Some((Tok::Dot, _)) = tok {
            name.push('.');
            tok = lx.next();
        } else {
            return (name, tok);
        }
    }
}

pub fn imports(text: &str) -> Vec<CodeRef> {
    let mut out = Vec::new();
    let mut lx = Lexer::new(text);
    let mut statement_start = true;
    let mut pending = lx.next();
    while let Some((tok, line)) = pending {
        let at_start = statement_start;
        statement_start = matches!(tok, Tok::Newline | Tok::Semi | Tok::Colon);
        match tok {
            Tok::Name("import") => {
                let mut cur = lx.next();
                loop {
                    let (name, after) = dotted(&mut lx, cur);
                    if !name.is_empty() && !name.ends_with('.') {
                        out.push(CodeRef {
                            spec: name,
                            kind: CodeKind::Python,
                            line,
                        });
                    }
                    cur = after;
                    if let Some((Tok::Name("as"), _)) = cur {
                        lx.next(); // alias
                        cur = lx.next();
                    }
                    if let Some((Tok::Comma, _)) = cur {
                        cur = lx.next();
                        continue;
                    }
                    break;
                }
                pending = cur;
                continue;
            }
            Tok::Name("from") if at_start => {
                let mut level = String::new();
                let mut cur = lx.next();
                while let Some((Tok::Dot, _)) = cur {
                    level.push('.');
                    cur = lx.next();
                }
                let (module, after) = dotted(&mut lx, cur);
                cur = after;
                if !matches!(cur, Some((Tok::Name("import"), _))) || (level.is_empty() && module.is_empty()) {
                    pending = cur;
                    continue;
                }
                let base = format!("{level}{module}");
                if !module.is_empty() {
                    out.push(CodeRef {
                        spec: base.clone(),
                        kind: CodeKind::Python,
                        line,
                    });
                }
                cur = lx.next();
                if let Some((Tok::LParen, _)) = cur {
                    cur = lx.next();
                }
                loop {
                    match cur {
                        Some((Tok::Star, _)) => cur = lx.next(),
                        Some((Tok::Name(n), _)) if n != "as" => {
                            // `from pkg import mod` may name a submodule
                            let spec = if module.is_empty() {
                                format!("{base}{n}")
                            } else {
                                format!("{base}.{n}")
                            };
                            out.push(CodeRef {
                                spec,
                                kind: CodeKind::Python,
                                line,
                            });
                            cur = lx.next();
                            if let Some((Tok::Name("as"), _)) = cur {
                                lx.next();
                                cur = lx.next();
                            }
                        }
                        Some((Tok::Comma, _)) => cur = lx.next(),
                        _ => break,
                    }
                }
                pending = cur;
                continue;
            }
            _ => {}
        }
        pending = lx.next();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(text: &str) -> Vec<(String, u32)> {
        imports(text).into_iter().map(|r| (r.spec, r.line)).collect()
    }

    #[test]
    fn plain_and_from_imports() {
        let refs = specs(
            "import a.b, c as d\nfrom a.b import c\nfrom . import x\nfrom ..m import y\n# import nope\ns=\"import nope\"",
        );
        let names: Vec<_> = refs.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(
            names,
            ["a.b", "c", "a.b", "a.b.c", ".x", "..m", "..m.y"]
        );
        assert_eq!(refs[0].1, 1);
        assert_eq!(refs.last().unwrap().1, 4);
    }

    #[test]
    fn strings_comments_and_docstrings_hide_imports() {
        let text = "\"\"\"\nimport hidden\n\"\"\"\nx = 'import nope'\n# from a import b\nimport real\n";
        assert_eq!(specs(text), [("real".to_string(), 6)]);
    }

    #[test]
    fn nested_statements_and_one_liners() {
        let text = "def f():\n    import inner\n    if x: import one_liner\ntry:\n    from pkg import (a,\n        b as c)\nexcept ImportError:\n    pass\n";
        let names: Vec<_> = specs(text).into_iter().map(|r| r.0).collect();
        assert_eq!(
            names,
            ["inner", "one_liner", "pkg", "pkg.a", "pkg.b"]
        );
    }

    #[test]
    fn star_imports_and_broken_code() {
        assert_eq!(specs("from m import *"), [("m".to_string(), 1)]);
        assert!(specs("import (").is_empty());
        assert!(specs("import").is_empty());
        assert!(specs("from").is_empty());
        assert!(specs("from . import *").is_empty());
        // `yield from` and `raise ... from` are not imports
        assert!(specs("def g():\n    yield from other()\nraise A from B\n").is_empty());
    }

    #[test]
    fn continuation_lines_and_semicolons() {
        let text = "x = 1; import a\nfrom b import \\\n    c\n";
        let r = specs(text);
        assert_eq!(r[0], ("a".to_string(), 1));
        assert_eq!(r[1], ("b".to_string(), 2));
        assert_eq!(r[2], ("b.c".to_string(), 2));
    }

    #[test]
    fn non_ascii_and_crlf() {
        let r = specs("# é\r\nimport módulo\r\nimport b\r\n");
        assert_eq!(r, [("módulo".to_string(), 2), ("b".to_string(), 3)]);
    }
}
