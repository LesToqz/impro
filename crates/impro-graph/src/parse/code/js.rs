//! ES module / CommonJS imports for JavaScript, TypeScript, Vue and Svelte.

use super::{CodeKind, CodeRef};
use crate::parse::text::LineTracker;

struct Cursor<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

impl<'a> Cursor<'a> {
    /// Skips whitespace and comments.
    fn skip_trivia(&mut self) {
        loop {
            match self.s.get(self.i) {
                Some(c) if c.is_ascii_whitespace() => self.i += 1,
                Some(b'/') if self.s.get(self.i + 1) == Some(&b'/') => {
                    self.i = memchr::memchr(b'\n', &self.s[self.i..]).map_or(self.s.len(), |n| self.i + n);
                }
                Some(b'/') if self.s.get(self.i + 1) == Some(&b'*') => {
                    self.i = memchr::memmem::find(&self.s[self.i + 2..], b"*/")
                        .map_or(self.s.len(), |n| self.i + 2 + n + 2);
                }
                _ => return,
            }
        }
    }

    fn ident(&mut self) -> Option<&'a str> {
        let start = self.i;
        while self.s.get(self.i).is_some_and(|b| is_ident(*b)) {
            self.i += 1;
        }
        (self.i > start).then(|| &self.text[start..self.i])
    }

    /// A quoted literal with no escapes or line breaks inside.
    fn plain_string(&mut self) -> Option<&'a str> {
        let q = *self.s.get(self.i)?;
        if q != b'"' && q != b'\'' && q != b'`' {
            return None;
        }
        let rest = &self.s[self.i + 1..];
        let end = memchr::memchr3(q, b'\n', b'\\', rest)?;
        if rest[end] != q {
            return None;
        }
        let value = &self.text[self.i + 1..self.i + 1 + end];
        self.i += end + 2;
        Some(value)
    }

    /// Skips any string/template literal, tolerating escapes.
    fn skip_literal(&mut self) {
        let q = self.s[self.i];
        self.i += 1;
        while let Some(&c) = self.s.get(self.i) {
            self.i += 1;
            if c == b'\\' {
                self.i += 1;
            } else if c == q || (c == b'\n' && q != b'`') {
                return;
            }
        }
    }
}

/// `from "x"` tail after the binding clause of an import/export.
fn from_clause(c: &mut Cursor<'_>) -> Option<String> {
    for _ in 0..4096 {
        c.skip_trivia();
        match c.s.get(c.i)? {
            b'{' => {
                let close = memchr::memchr(b'}', &c.s[c.i..])?;
                c.i += close + 1;
            }
            b'*' | b',' => c.i += 1,
            b if is_ident(*b) => {
                if c.ident()? == "from" {
                    c.skip_trivia();
                    return c.plain_string().map(str::to_string);
                }
            }
            _ => return None,
        }
    }
    None
}

pub fn imports(text: &str) -> Vec<CodeRef> {
    let mut out = Vec::new();
    let mut lines = LineTracker::new(text);
    let mut c = Cursor {
        s: text.as_bytes(),
        text,
        i: 0,
    };
    let mut push = |spec: String, at: usize, out: &mut Vec<CodeRef>| {
        out.push(CodeRef {
            spec,
            kind: CodeKind::Script,
            line: lines.line_at(at),
        });
    };
    while c.i < c.s.len() {
        let b = c.s[c.i];
        match b {
            b'/' if matches!(c.s.get(c.i + 1), Some(b'/') | Some(b'*')) => c.skip_trivia(),
            b'"' | b'\'' | b'`' => c.skip_literal(),
            _ if is_ident(b) => {
                let start = c.i;
                let word = match c.ident() {
                    Some(w) => w,
                    None => {
                        c.i += 1;
                        continue;
                    }
                };
                // a property access like `x.import` is not a keyword
                let prop = start > 0 && c.s[start - 1] == b'.';
                if prop {
                    continue;
                }
                match word {
                    "import" => {
                        let mut look = Cursor { ..c };
                        look.skip_trivia();
                        match look.s.get(look.i) {
                            Some(b'(') => {
                                look.i += 1;
                                look.skip_trivia();
                                if let Some(spec) = look.plain_string() {
                                    push(spec.to_string(), start, &mut out);
                                    c.i = look.i;
                                }
                            }
                            Some(b'"') | Some(b'\'') => {
                                if let Some(spec) = look.plain_string() {
                                    push(spec.to_string(), start, &mut out);
                                    c.i = look.i;
                                }
                            }
                            Some(b'.') => {}
                            _ => {
                                if let Some(spec) = from_clause(&mut look) {
                                    push(spec, start, &mut out);
                                    c.i = look.i;
                                }
                            }
                        }
                    }
                    "export" => {
                        let mut look = Cursor { ..c };
                        if let Some(spec) = from_clause(&mut look) {
                            push(spec, start, &mut out);
                            c.i = look.i;
                        }
                    }
                    "require" => {
                        let mut look = Cursor { ..c };
                        look.skip_trivia();
                        if look.s.get(look.i) == Some(&b'(') {
                            look.i += 1;
                            look.skip_trivia();
                            if let Some(spec) = look.plain_string() {
                                push(spec.to_string(), start, &mut out);
                                c.i = look.i;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => c.i += 1,
        }
    }
    out
}

impl<'a> Clone for Cursor<'a> {
    fn clone(&self) -> Self {
        Cursor {
            s: self.s,
            text: self.text,
            i: self.i,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(text: &str) -> Vec<(String, u32)> {
        imports(text).into_iter().map(|r| (r.spec, r.line)).collect()
    }

    #[test]
    fn static_and_commonjs_forms() {
        let r = specs("import x from \"./x\";\nimport \"./x.css\"; require(\"./r\"); export {x} from \"./e\";");
        let names: Vec<_> = r.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(names, ["./x", "./x.css", "./r", "./e"]);
        assert_eq!(r[1].1, 2);
    }

    #[test]
    fn multiline_clauses_and_star_exports() {
        let text = "import {\n  a,\n  b as c,\n} from './multi';\nexport * from './star';\nexport * as ns from './ns';\nimport def, * as all from './both';\nimport type { T } from './types';";
        let r: Vec<_> = specs(text).into_iter().map(|r| r.0).collect();
        assert_eq!(r, ["./multi", "./star", "./ns", "./both", "./types"]);
    }

    #[test]
    fn dynamic_import_and_ts_require() {
        let text = "const m = await import('./lazy');\nimport fs = require(\"./legacy\");\nimport.meta.url;";
        let r: Vec<_> = specs(text).into_iter().map(|r| r.0).collect();
        assert_eq!(r, ["./lazy", "./legacy"]);
    }

    #[test]
    fn comments_strings_and_declarations_are_not_imports() {
        let text = "// import a from './a'\n/* require('./b') */\nconst s = \"import c from './c'\";\nexport const x = 1;\nexport default function f() {}\nobj.import('./d');\nexport class A {}\n";
        assert!(specs(text).is_empty());
    }

    #[test]
    fn template_literals_and_unterminated_input() {
        assert!(specs("const t = `import x from './x'`;").is_empty());
        assert!(specs("import").is_empty());
        assert!(specs("import {").is_empty());
        assert!(specs("export * from").is_empty());
        assert!(specs("require(").is_empty());
        assert_eq!(specs("import a from './é';").len(), 1);
    }

    #[test]
    fn vue_blocks() {
        let text = "<template><div/></template>\n<script>\nimport Foo from './Foo.vue'\n</script>\n";
        assert_eq!(specs(text), [("./Foo.vue".to_string(), 3)]);
    }
}
