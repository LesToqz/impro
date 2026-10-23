//! References of stylesheets (`@import`, `url()`) and HTML (`src`, `href`).
//!
//! Like the reference, this reads the text as written: a reference inside a
//! comment counts too.

use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};

use super::{local_spec, CodeKind, CodeRef};
use crate::parse::text::LineTracker;

fn css_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        RegexBuilder::new(r#"@import\s+["']([^"']+)["']|url\(\s*["']?([^\s)'";]+)"#)
            .case_insensitive(true)
            .build()
            .expect("style import pattern is valid")
    })
}

fn html_pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        RegexBuilder::new(r#"\b(?:src|href)\s*=\s*(?:["']([^"']+)["']|([^\s>]+))"#)
            .case_insensitive(true)
            .build()
            .expect("html reference pattern is valid")
    })
}

/// Every local specifier `re` finds, first capturing group that matched wins.
fn scan(re: &Regex, text: &str, kind: CodeKind) -> Vec<CodeRef> {
    let mut lines = LineTracker::new(text);
    let mut out = Vec::new();
    for caps in re.captures_iter(text) {
        let Some(whole) = caps.get(0) else { continue };
        let Some(spec) = caps.iter().skip(1).flatten().next() else {
            continue;
        };
        if local_spec(spec.as_str()) {
            out.push(CodeRef {
                spec: spec.as_str().to_string(),
                kind,
                line: lines.line_at(whole.start()),
            });
        }
    }
    out
}

pub fn css_imports(text: &str) -> Vec<CodeRef> {
    scan(css_pattern(), text, CodeKind::Style)
}

pub fn html_refs(text: &str) -> Vec<CodeRef> {
    scan(html_pattern(), text, CodeKind::Html)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(refs: Vec<CodeRef>) -> Vec<String> {
        refs.into_iter().map(|r| r.spec).collect()
    }

    #[test]
    fn css_imports_and_urls() {
        let text = "@import 'base.css';\n.a { background: url( 'img/a.png' ) }\n.b { background: url(b.png) }\n@import url(x.css);";
        assert_eq!(
            specs(css_imports(text)),
            ["base.css", "img/a.png", "b.png", "x.css"]
        );
        assert!(css_imports("url(data:image/png;base64,AA) url(http://x/y) url(#f)").is_empty());
    }

    #[test]
    fn html_attributes() {
        let text = "<link href=\"a.css\"><script src=b.js></script>\n<a href='c.html#x'>c</a><a href=\"https://x\">";
        let refs = html_refs(text);
        assert_eq!(specs(refs.clone()), ["a.css", "b.js", "c.html#x"]);
        assert_eq!(refs[2].line, 2);
    }
}
