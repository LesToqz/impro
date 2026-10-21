//! Import extraction for source files. Each language gets a small scanner that
//! understands comments and string literals; none of them need a full parser.

mod clike;
mod js;
mod python;
mod rust;
mod shell;
mod style;

use super::text::extension;

/// How a reference must be resolved against the project tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodeKind {
    /// Dotted module path, possibly with leading dots for relative imports.
    Python,
    /// JavaScript, TypeScript, Vue, Svelte: relative specifiers only.
    Script,
    /// CSS/Sass `@import`, `@use`, `url()`.
    Style,
    /// HTML `src`/`href` attributes.
    Html,
    /// `mod name;`
    RustMod,
    /// `use crate::a::b;` (segments joined with `::`).
    RustUse,
    /// `source file` / `. file`
    Shell,
    /// `#include "file"`
    Include,
    /// `require_relative 'file'`
    Ruby,
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
        ".py", ".pyi", ".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".mts", ".cts", ".vue",
        ".svelte", ".css", ".scss", ".sass", ".html", ".htm", ".rs", ".sh", ".bash", ".c", ".h",
        ".cc", ".cpp", ".hpp", ".rb",
    ]
}

/// Imports of the source file at `path` (extension decides the language).
pub fn extract_imports(path: &str, text: &str) -> Vec<CodeRef> {
    let ext = extension(path);
    match ext.as_str() {
        ".py" | ".pyi" => python::imports(text),
        ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" | ".mts" | ".cts" | ".vue"
        | ".svelte" => js::imports(text),
        ".css" => style::css_imports(text, false),
        ".scss" | ".sass" => style::css_imports(text, true),
        ".html" | ".htm" => style::html_refs(text),
        ".rs" => rust::imports(text),
        ".sh" | ".bash" => shell::imports(text),
        ".c" | ".h" | ".cc" | ".cpp" | ".hpp" => clike::includes(text),
        ".rb" => clike::require_relative(text),
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
        assert_eq!(specs("a.RS", "mod x;"), ["x"]);
    }

    #[test]
    fn supported_list_matches_dispatch() {
        for ext in supported_extensions() {
            // every advertised extension must dispatch to some scanner
            let probe = match *ext {
                ".py" | ".pyi" => "import zz",
                ".js" | ".jsx" | ".ts" | ".tsx" | ".mjs" | ".cjs" | ".mts" | ".cts" | ".vue"
                | ".svelte" => "import \"./zz\"",
                ".css" | ".scss" | ".sass" => "@import \"zz\";",
                ".html" | ".htm" => "<img src=zz>",
                ".rs" => "mod zz;",
                ".sh" | ".bash" => "source zz",
                ".rb" => "require_relative 'zz'",
                _ => "#include \"zz\"",
            };
            assert!(
                !extract_imports(&format!("f{ext}"), probe).is_empty(),
                "{ext}"
            );
        }
    }
}
