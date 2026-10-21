//! `#include "file"` (C family) and `require_relative` (Ruby).

use super::{CodeKind, CodeRef};

pub fn includes(text: &str) -> Vec<CodeRef> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let rest = match line.trim_start().strip_prefix('#') {
            Some(r) => r.trim_start(),
            None => continue,
        };
        let rest = match rest.strip_prefix("include") {
            Some(r) => r.trim_start(),
            None => continue,
        };
        if let Some(body) = rest.strip_prefix('"') {
            if let Some(end) = body.find('"') {
                out.push(CodeRef {
                    spec: body[..end].to_string(),
                    kind: CodeKind::Include,
                    line: n as u32 + 1,
                });
            }
        }
    }
    out
}

pub fn require_relative(text: &str) -> Vec<CodeRef> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let rest = match line.trim_start().strip_prefix("require_relative") {
            Some(r) => r.trim_start().trim_start_matches('(').trim_start(),
            None => continue,
        };
        let q = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => q,
            _ => continue,
        };
        if let Some(end) = rest[1..].find(q) {
            out.push(CodeRef {
                spec: rest[1..1 + end].to_string(),
                kind: CodeKind::Ruby,
                line: n as u32 + 1,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_includes_ignore_system_headers() {
        let t = "#include <stdio.h>\n  #  include \"util.h\"\n// #include \"no.h\"\n#include \"dir/a.h\" // c\n";
        let r: Vec<_> = includes(t).into_iter().map(|r| r.spec).collect();
        // line comments are not tracked for includes; a commented-out include
        // does not start with '#', so it is skipped naturally
        assert_eq!(r, ["util.h", "dir/a.h"]);
    }

    #[test]
    fn ruby_relative_requires() {
        let t = "require 'json'\nrequire_relative 'lib/x'\nrequire_relative(\"y\")\n";
        let r: Vec<_> = require_relative(t).into_iter().map(|r| r.spec).collect();
        assert_eq!(r, ["lib/x", "y"]);
    }
}
