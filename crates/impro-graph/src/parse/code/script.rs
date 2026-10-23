//! `import` / `export … from` / `require()` specifiers of script files.
//!
//! Like the reference, this reads the text as written: a specifier inside a
//! comment or a template string counts too.

use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};

use super::{local_spec, CodeKind, CodeRef};
use crate::parse::text::LineTracker;

fn pattern() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        RegexBuilder::new(
            r#"(?:\b(?:import|export)\s+(?:[^;\n]*?\s+from\s*)?|\brequire\s*\(\s*)["']([^"']+)["']"#,
        )
        .case_insensitive(true)
        .build()
        .expect("script import pattern is valid")
    })
}

pub fn imports(text: &str) -> Vec<CodeRef> {
    let mut lines = LineTracker::new(text);
    let mut out = Vec::new();
    for caps in pattern().captures_iter(text) {
        let (Some(whole), Some(spec)) = (caps.get(0), caps.get(1)) else {
            continue;
        };
        if local_spec(spec.as_str()) {
            out.push(CodeRef {
                spec: spec.as_str().to_string(),
                kind: CodeKind::Script,
                line: lines.line_at(whole.start()),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(text: &str) -> Vec<(String, u32)> {
        imports(text)
            .into_iter()
            .map(|r| (r.spec, r.line))
            .collect()
    }

    #[test]
    fn import_export_and_require_forms() {
        let text = "import a from './a';\nimport './b.css';\nexport {s} from \"./s\";\nconst q = require ( '../q' );\nimport w from 'lib';";
        assert_eq!(
            specs(text),
            [
                ("./a".to_string(), 1),
                ("./b.css".to_string(), 2),
                ("./s".to_string(), 3),
                ("../q".to_string(), 4),
                ("lib".to_string(), 5),
            ]
        );
    }

    #[test]
    fn external_and_anchor_specifiers_are_skipped() {
        assert!(specs(
            "import a from 'http://x/y.js';\nimport b from '#local';\nimport c from '//cdn/x.js'"
        )
        .is_empty());
    }

    #[test]
    fn comments_count_like_code() {
        assert_eq!(specs("// import './c'"), [("./c".to_string(), 1)]);
    }

    #[test]
    fn dynamic_import_is_not_a_declaration() {
        assert!(specs("import('./dyn');").is_empty());
    }
}
