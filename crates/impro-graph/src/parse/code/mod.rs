//! Import extraction for source files. Python gets a small tokenizer that
//! understands comments and string literals; the other families are read with
//! patterns, like the reference does.

mod python;
mod script;
mod style;

use super::text::extension;

/// How a reference must be resolved against the project tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeKind {
    /// Dotted module path, possibly with leading dots for relative imports.
    Python,
    /// JavaScript, TypeScript, Vue: relative specifiers only.
    Script,
    /// CSS/Sass `@import` and `url()`.
    Style,
    /// HTML `src`/`href` attributes.
    Html,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRef {
    pub spec: String,
    pub kind: CodeKind,
    /// One-based line of the statement.
    pub line: u32,
}

/// Extensions (lowercase, with dot) for which imports are understood.
pub fn supported_extensions() -> &'static [&'static str] {
    &[
        ".py", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".vue", ".css", ".scss", ".sass",
        ".html", ".htm",
    ]
}

/// True for a specifier that can name a project file: not blank, not a URL
/// with a scheme, not protocol-relative and not an in-page anchor.
pub(crate) fn local_spec(spec: &str) -> bool {
    !spec.trim().is_empty() && !super::text::is_external(spec)
}

/// Imports of the source file at `path` (extension decides the language).
pub fn extract_imports(path: &str, text: &str) -> Vec<CodeRef> {
    let ext = extension(path);
    match ext.as_str() {
        ".py" => python::imports(text),
        ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" | ".vue" => script::imports(text),
        ".css" | ".scss" | ".sass" => style::css_imports(text),
        ".html" | ".htm" => style::html_refs(text),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs(path: &str, text: &str) -> Vec<String> {
        extract_imports(path, text)
            .into_iter()
            .map(|r| r.spec)
            .collect()
    }

    #[test]
    fn unsupported_files_have_no_imports() {
        assert!(extract_imports("a.md", "[[Note]]").is_empty());
        assert!(extract_imports("a.json", "{\"import\": \"x\"}").is_empty());
        assert!(extract_imports("a.rs", "mod x;").is_empty());
        assert!(extract_imports("a.sh", "source x").is_empty());
        assert!(extract_imports("a.pyi", "import x").is_empty());
        assert_eq!(specs("a.PY", "import x"), ["x"]);
    }

    #[test]
    fn supported_list_matches_dispatch() {
        for ext in supported_extensions() {
            // every advertised extension must dispatch to some scanner
            let probe = match *ext {
                ".py" => "import zz",
                ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" | ".vue" => "import \"./zz\"",
                ".css" | ".scss" | ".sass" => "@import \"zz\";",
                _ => "<img src=zz>",
            };
            assert!(
                !extract_imports(&format!("f{ext}"), probe).is_empty(),
                "{ext}"
            );
        }
    }
}
