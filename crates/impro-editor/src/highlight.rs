//! Scope-based syntax classification and cancellable background buffer colouring.
use gtk::prelude::*;
use impro_core::themes::{SyntaxColors, KINDS};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::Path,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use syntect::{
    parsing::{ParseState, Scope, ScopeStack, SyntaxSet},
    util::LinesWithEndings,
};
pub const MAX_BYTES: usize = 1_500_000;
pub const MAX_LINE: usize = 20_000;
pub const DEBOUNCE_MS: u64 = 120;
pub type Run = (usize, usize, &'static str);
static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
/// Cheap language detection: does not load the syntax set or touch the filesystem.
pub fn language_name(path: &str, first_line: &str) -> &'static str {
    let name = Path::new(path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let ext = name.rsplit_once('.').map(|x| x.1).unwrap_or("");
    match name.as_str() {
        "dockerfile" | "containerfile" => return "Dockerfile",
        "makefile" | "gnumakefile" => return "Makefile",
        "cmakelists.txt" => return "CMake",
        ".gitignore" | ".gitattributes" | ".dockerignore" => return "Plain Text",
        ".bashrc" | ".bash_profile" | ".bash_aliases" | ".bash_logout" | ".zshrc" | ".zprofile"
        | ".zshenv" | ".zlogin" | ".profile" | ".env" => return "Shell Script",
        _ => {}
    }
    if name.starts_with(".env.") {
        return "Shell Script";
    }
    if name.starts_with("dockerfile.") {
        return "Dockerfile";
    }
    let lang = match ext {
        "py" | "pyi" | "pyw" => "Python",
        "rs" => "Rust",
        "md" | "markdown" => "Markdown",
        "json" | "jsonc" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "js" | "mjs" | "cjs" => "JavaScript",
        "ts" => "TypeScript",
        "tsx" => "TypeScript React",
        "jsx" => "JavaScript React",
        "html" | "htm" => "HTML",
        "xml" | "svg" => "XML",
        "css" => "CSS",
        "scss" => "SCSS",
        "sass" => "Sass",
        "sh" | "bash" | "zsh" => "Shell Script",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" | "cxx" => "C++",
        "go" => "Go",
        "sql" => "SQL",
        "rb" => "Ruby",
        "pl" | "pm" => "Perl",
        "lua" => "Lua",
        "java" => "Java",
        "php" => "PHP",
        "ini" | "cfg" | "conf" => "INI",
        "cmake" => "CMake",
        "txt" | "log" => return "Plain Text",
        _ => "",
    };
    if !lang.is_empty() {
        return lang;
    }
    // Explicit language aliases also serve diff callers without a filename.
    let direct = match name.as_str() {
        "python" | "python3" => "Python",
        "rust" => "Rust",
        "json" => "JSON",
        "yaml" => "YAML",
        "toml" => "TOML",
        "markdown" => "Markdown",
        "javascript" => "JavaScript",
        "typescript" => "TypeScript",
        "bash" | "shell script" => "Shell Script",
        "html" => "HTML",
        "xml" => "XML",
        "css" => "CSS",
        "scss" => "SCSS",
        "sass" => "Sass",
        "go" => "Go",
        "sql" => "SQL",
        "c" => "C",
        "c++" => "C++",
        "ruby" => "Ruby",
        "perl" => "Perl",
        "lua" => "Lua",
        "ini" => "INI",
        "cmake" => "CMake",
        "typescript react" => "TypeScript React",
        "javascript react" => "JavaScript React",
        _ => "",
    };
    if !direct.is_empty() {
        return direct;
    }
    if let Some(shebang) = first_line.strip_prefix("#!") {
        let mut words = shebang.split_whitespace();
        let mut program = words.next().unwrap_or("").rsplit('/').next().unwrap_or("");
        if program == "env" {
            program = words
                .find(|w| !w.starts_with('-') && !w.contains('='))
                .unwrap_or("");
        }
        let program = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        return match program {
            "python" => "Python",
            "sh" | "bash" | "zsh" | "dash" | "ash" | "ksh" => "Shell Script",
            "node" | "nodejs" | "bun" => "JavaScript",
            "deno" => "TypeScript",
            "ruby" => "Ruby",
            "perl" => "Perl",
            _ => "Plain Text",
        };
    }
    "Plain Text"
}
fn syntaxes() -> &'static SyntaxSet {
    SYNTAXES.get_or_init(|| {
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        for grammar in [
            include_str!("toml.sublime-syntax"),
            include_str!("docker.sublime-syntax"),
        ] {
            if let Ok(syntax) =
                syntect::parsing::SyntaxDefinition::load_from_str(grammar, true, None)
            {
                builder.add(syntax);
            }
        }
        builder.build()
    })
}
/// Pure, thread-safe classification into the 26 theme roles; offsets count characters.
/// This blocking API loads syntax data lazily. UI callers must use a worker.
pub fn tokenize(path_or_language: &str, text: &str) -> Vec<Run> {
    tokenize_cancel(path_or_language, text, &AtomicBool::new(false))
}
#[derive(Clone)]
struct Checkpoint {
    byte: usize,
    chars: usize,
    parser: ParseState,
    stack: ScopeStack,
    fence: Option<(ParseState, ScopeStack)>,
}
struct LexCache {
    language: String,
    text: String,
    runs: Vec<Run>,
    checkpoints: Vec<Checkpoint>,
}
#[derive(Default)]
struct Lexed {
    runs: Vec<Run>,
    checkpoints: Vec<Checkpoint>,
    dirty_start: usize,
    dirty_end: usize,
}
fn tokenize_cancel(path: &str, text: &str, cancel: &AtomicBool) -> Vec<Run> {
    lex_cached(path, text, cancel, None).runs
}
fn lex_cached(path: &str, text: &str, cancel: &AtomicBool, old: Option<&LexCache>) -> Lexed {
    if text.len() > MAX_BYTES || text.split('\n').any(|s| s.chars().count() > MAX_LINE) {
        return Lexed::default();
    }
    let language = language_name(path, text.lines().next().unwrap_or(""));
    if language == "Plain Text" || cancel.load(Ordering::Relaxed) {
        return Lexed::default();
    }
    let set = syntaxes();
    let syntax_name = match language {
        "Shell Script" => "Bourne Again Shell (bash)",
        "TypeScript" | "TypeScript React" | "JavaScript React" => "JavaScript",
        "INI" => "YAML",
        "SCSS" | "Sass" => "CSS",
        _ => language,
    };
    let syntax = set
        .find_syntax_by_name(syntax_name)
        .or_else(|| set.find_syntax_by_extension(Path::new(path).extension()?.to_str()?))
        .unwrap_or_else(|| set.find_syntax_plain_text());
    let old = old.filter(|c| c.language == path);
    let prefix = old
        .map(|c| {
            text.bytes()
                .zip(c.text.bytes())
                .take_while(|(a, b)| a == b)
                .count()
        })
        .unwrap_or(0);
    let suffix = old
        .map(|c| {
            text.bytes()
                .rev()
                .zip(c.text.bytes().rev())
                .take_while(|(a, b)| a == b)
                .count()
                .min(text.len().min(c.text.len()) - prefix)
        })
        .unwrap_or(0);
    let seed = old.and_then(|c| c.checkpoints.iter().rev().find(|p| p.byte <= prefix));
    let mut parser = seed
        .map(|p| p.parser.clone())
        .unwrap_or_else(|| ParseState::new(syntax));
    let mut stack = seed.map(|p| p.stack.clone()).unwrap_or_default();
    let mut fence = seed.and_then(|p| p.fence.clone());
    let mut offset = seed.map(|p| p.chars).unwrap_or(0);
    let mut absolute_byte = seed.map(|p| p.byte).unwrap_or(0);
    let dirty_start = offset;
    let mut dirty_end = text.chars().count();
    let mut result = old
        .map(|c| {
            c.runs
                .iter()
                .filter(|r| r.0 < offset)
                .map(|r| (r.0, r.1.min(offset), r.2))
                .collect()
        })
        .unwrap_or_default();
    let mut checkpoints: Vec<Checkpoint> = old
        .map(|c| {
            c.checkpoints
                .iter()
                .take_while(|p| p.byte < absolute_byte)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let delta_bytes = old
        .map(|c| text.len() as isize - c.text.len() as isize)
        .unwrap_or(0);
    let delta_chars = old
        .map(|c| text.chars().count() as isize - c.text.chars().count() as isize)
        .unwrap_or(0);
    let source_start = absolute_byte;
    for (line_number, line) in LinesWithEndings::from(&text[source_start..]).enumerate() {
        if absolute_byte > source_start && absolute_byte >= text.len() - suffix {
            if let Some(cache) = old {
                let old_byte = (absolute_byte as isize - delta_bytes) as usize;
                if let Ok(index) = cache
                    .checkpoints
                    .binary_search_by_key(&old_byte, |p| p.byte)
                {
                    let cp = &cache.checkpoints[index];
                    if cp.parser == parser && cp.stack == stack && cp.fence == fence {
                        for (a, b, k) in cache.runs.iter().filter(|r| r.1 > cp.chars) {
                            push_run(
                                &mut result,
                                (((*a).max(cp.chars)) as isize + delta_chars) as usize,
                                (*b as isize + delta_chars) as usize,
                                k,
                            );
                        }
                        checkpoints.extend(cache.checkpoints[index..].iter().map(|p| {
                            let mut p = p.clone();
                            p.byte = (p.byte as isize + delta_bytes) as usize;
                            p.chars = (p.chars as isize + delta_chars) as usize;
                            p
                        }));
                        dirty_end = offset;
                        break;
                    }
                }
            }
        }
        if line_number % 32 == 0 {
            checkpoints.push(Checkpoint {
                byte: absolute_byte,
                chars: offset,
                parser: parser.clone(),
                stack: stack.clone(),
                fence: fence.clone(),
            });
        }
        absolute_byte += line.len();
        if cancel.load(Ordering::Relaxed) {
            return Lexed::default();
        }
        if language == "Markdown" && line.trim_start().starts_with("```") {
            push_run(
                &mut result,
                offset,
                offset + line.trim_end_matches('\n').chars().count(),
                "string",
            );
            if fence.is_some() {
                fence = None;
            } else {
                let alias = line.trim().trim_start_matches('`').trim();
                let name = language_name(alias, "");
                let syntax = set
                    .find_syntax_by_name(name)
                    .unwrap_or_else(|| set.find_syntax_plain_text());
                fence = Some((ParseState::new(syntax), ScopeStack::new()));
            }
            offset += line.chars().count();
            continue;
        }
        let (current_parser, current_stack) = if let Some((p, s)) = fence.as_mut() {
            (p, s)
        } else {
            (&mut parser, &mut stack)
        };
        let ops = current_parser.parse_line(line, set).unwrap_or_default();
        let mut byte = 0;
        let mut ch = offset;
        for (end, op) in ops {
            if end > byte {
                classify_span(&mut result, language, line, byte, end, ch, current_stack);
                ch += line[byte..end].chars().count();
            }
            let _ = current_stack.apply(&op);
            byte = end;
        }
        if byte < line.len() {
            classify_span(
                &mut result,
                language,
                line,
                byte,
                line.len(),
                ch,
                current_stack,
            );
        }
        offset += line.chars().count();
    }
    Lexed {
        runs: result,
        checkpoints,
        dirty_start,
        dirty_end,
    }
}
/// Resolve one syntect scope stack. Public for lexer compatibility diagnostics.
pub fn map_scope(scope: &str) -> Option<&'static str> {
    if scope.contains("invalid") {
        Some("error")
    } else if scope.contains("character.escape") {
        Some("escape")
    } else if scope.contains("comment.block.documentation") {
        Some("doc")
    } else if scope.contains("comment") {
        Some("comment")
    } else if scope.contains("regexp") {
        Some("regex")
    } else if scope.contains("markup.heading") {
        Some("heading")
    } else if scope.contains("markup.bold") {
        Some("strong")
    } else if scope.contains("markup.italic") {
        Some("emphasis")
    } else if scope.contains("markup.underline.link") || scope.contains("markup.link") {
        Some("link")
    } else if scope.contains("markup.raw") {
        Some("string")
    } else if scope.contains("entity.name.tag") {
        Some("tag")
    } else if scope.contains("entity.other.attribute-name") {
        Some("attribute")
    } else if scope.starts_with("string.") {
        Some("string")
    } else if scope.contains("constant.numeric") {
        Some("number")
    } else if scope.contains("constant.language") || scope.contains("constant.character.entity") {
        Some("constant")
    } else if scope.contains("meta.annotation") || scope.contains("entity.name.function.decorator")
    {
        Some("decorator")
    } else if scope.contains("meta.preprocessor") {
        Some("preproc")
    } else if scope.contains("entity.name.function") || scope.contains("support.function") {
        Some("function")
    } else if scope.contains("entity.name.type")
        || scope.contains("support.type")
        || scope.contains("support.class")
    {
        Some("class")
    } else if scope.contains("keyword.operator") {
        Some("operator")
    } else if scope.contains("keyword") || scope.contains("storage") {
        Some("keyword")
    } else if scope.contains("variable") {
        Some("variable")
    } else {
        None
    }
}
fn push_run(out: &mut Vec<Run>, start: usize, end: usize, kind: &'static str) {
    let start = out.last().map(|r| start.max(r.1)).unwrap_or(start);
    if start >= end {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.2 == kind && last.1 == start {
            last.1 = end;
            return;
        }
    }
    out.push((start, end, kind));
}
struct ScopeInfo {
    language: Option<&'static str>,
    kind: Option<&'static str>,
    all: String,
}
thread_local! {static SCOPE_CACHE:RefCell<HashMap<Vec<Scope>,Rc<ScopeInfo>>>=RefCell::new(HashMap::new());}
fn scope_info(stack: &ScopeStack) -> Rc<ScopeInfo> {
    SCOPE_CACHE.with(|cache| {
        if let Some(info) = cache.borrow().get(stack.as_slice()).cloned() {
            return info;
        }
        let mut scopes: Vec<_> = stack.as_slice().iter().map(ToString::to_string).collect();
        if let Some(last) = scopes.iter().rposition(|s| s.starts_with("source.")) {
            if last > 0 {
                scopes.drain(..last);
            }
        }
        let language = match scopes.first().map(String::as_str) {
            Some(s) if s.starts_with("source.css") => Some("CSS"),
            Some(s) if s == "source.js" || s.starts_with("source.js.") => Some("JavaScript"),
            Some(s) if s.starts_with("source.python") => Some("Python"),
            _ => None,
        };
        let kind = scopes.iter().rev().find_map(|s| map_scope(s));
        let info = Rc::new(ScopeInfo {
            language,
            kind,
            all: scopes.join(" "),
        });
        cache
            .borrow_mut()
            .insert(stack.as_slice().to_vec(), info.clone());
        info
    })
}
fn classify_span(
    out: &mut Vec<Run>,
    lang: &str,
    line: &str,
    begin: usize,
    end: usize,
    offset: usize,
    stack: &ScopeStack,
) {
    let value = &line[begin..end];
    if value.trim().is_empty() {
        return;
    }
    let info = scope_info(stack);
    let lang = info.language.unwrap_or(lang);
    let kind = info.kind;
    let all = info.all.as_str();
    let mut kind = if all.contains("comment") {
        Some("comment")
    } else {
        kind
    };
    if all.contains("meta.interpolation") {
        kind = if all.contains("punctuation.section.interpolation") {
            Some("keyword")
        } else {
            None
        };
    }
    if all.contains("string")
        && !all.contains("escape")
        && !all.contains("interpolation")
        && !all.contains("embedded")
    {
        kind = Some("string");
    }
    if all.contains("comment.block.documentation") {
        kind = if line.matches("\"\"\"").count() >= 2 || line.matches("'''").count() >= 2 {
            Some("doc")
        } else {
            Some("string")
        };
    }
    if all.contains("regexp") {
        kind = Some("regex");
    }
    if all.contains("constant.character.entity") {
        kind = Some("constant");
    }
    if all.contains("meta.preprocessor") && !all.contains("string") {
        kind = Some("preproc");
    }
    if matches!(lang, "Rust" | "Go") && all.contains("storage.type") {
        kind = Some("class");
    }
    if lang == "Shell Script" && all.contains("variable") {
        kind = Some("variable");
    }
    if lang == "TOML" && all.contains("entity.other.attribute-name") {
        kind = Some("property");
    }
    if lang == "TOML" && all.contains("punctuation.definition.string") {
        kind = Some("string");
    }
    if matches!(lang, "TypeScript" | "JavaScript") && value.starts_with('@') {
        let n = line[begin..]
            .chars()
            .take_while(|c| *c == '@' || *c == '.' || *c == '_' || c.is_alphanumeric())
            .count();
        push_run(out, offset, offset + n, "decorator");
        return;
    }
    if (lang == "HTML" || lang == "XML") && all.contains("punctuation.definition.tag") {
        kind = Some("punctuation");
    }
    if lang == "JSON" && all.contains("meta.mapping.key") {
        kind = Some("property");
    }
    if lang == "JSON" && all.contains("meta.structure.dictionary.key") {
        kind = Some("property");
    }
    if lang == "CSS" {
        if all.contains("property-name") {
            kind = Some("property");
        } else if all.contains("selector") {
            kind = Some("selector");
        } else if all.contains("property-value") && kind.is_none() {
            kind = Some("string");
        }
        if all.contains("keyword.control.at-rule") {
            kind = Some("control");
        }
    }
    if lang == "CSS" && all.contains("custom-property") {
        kind = Some("variable");
    }
    if lang == "CSS" && all.contains("keyword.other.important") {
        kind = Some("keyword");
    }
    if lang == "Markdown" {
        if all.contains("markup.heading") {
            kind = Some("heading");
        }
        if all.contains("markup.bold") {
            kind = Some("strong");
        }
        if all.contains("markup.italic") {
            kind = Some("emphasis");
        }
        if all.contains("markup.raw") {
            kind = Some("string");
        }
        if all.contains("punctuation.definition.list") {
            kind = Some("keyword");
        }
    }
    if lang == "YAML" {
        if all.contains("entity.name.tag") {
            kind = Some("property");
        } else if all.contains("string.unquoted") {
            kind = Some("string");
        }
    }
    if (lang == "CSS" && all.contains("custom-property"))
        || (lang == "CSS" && all.contains("keyword.other.important"))
        || (lang == "Shell Script" && all.contains("variable"))
        || matches!(kind, Some("number" | "constant" | "punctuation"))
        || (kind == Some("keyword") && all.contains("interpolation"))
        || (lang == "Markdown" && all.contains("punctuation.definition.list"))
    {
        push_run(
            out,
            offset,
            offset + value.chars().count(),
            kind.unwrap_or("keyword"),
        );
        return;
    }
    if matches!(
        kind,
        Some(
            "string"
                | "comment"
                | "doc"
                | "regex"
                | "heading"
                | "strong"
                | "emphasis"
                | "link"
                | "escape"
                | "property"
                | "tag"
                | "attribute"
                | "selector"
                | "preproc"
                | "decorator"
        )
    ) {
        push_run(
            out,
            offset,
            offset + value.chars().count(),
            kind.unwrap_or("string"),
        );
        return;
    }
    let chars: Vec<_> = value.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (b, c) = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if c.is_alphanumeric() || c == '_' || c == '$' {
            i += 1;
            while i < chars.len() && (chars[i].1.is_alphanumeric() || chars[i].1 == '_') {
                i += 1;
            }
            let word_end = if i < chars.len() {
                chars[i].0
            } else {
                value.len()
            };
            let word = &value[b..word_end];
            let after = &line[begin + word_end..];
            let before = line[..begin + b].trim_end();
            let previous = before
                .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
                .next()
                .unwrap_or("");
            let k = if lang == "Shell Script"
                && matches!(word, "echo" | "cd" | "export" | "printf" | "read")
            {
                "builtin"
            } else if previous == "fn" || previous == "def" || previous == "func" {
                "function"
            } else if matches!(lang, "Rust" | "Go")
                && matches!(
                    word,
                    "let"
                        | "mut"
                        | "type"
                        | "func"
                        | "struct"
                        | "enum"
                        | "fn"
                        | "trait"
                        | "impl"
                        | "const"
                        | "static"
                )
            {
                "keyword"
            } else if kind == Some("class")
                && !matches!(
                    word,
                    "type" | "func" | "struct" | "enum" | "fn" | "trait" | "impl"
                )
            {
                "class"
            } else if c.is_ascii_digit() {
                "number"
            } else if matches!(
                word,
                "true"
                    | "false"
                    | "True"
                    | "False"
                    | "None"
                    | "null"
                    | "nil"
                    | "NULL"
                    | "undefined"
                    | "Ellipsis"
                    | "NotImplemented"
            ) {
                "constant"
            } else if matches!(
                word,
                "if" | "elif"
                    | "else"
                    | "elseif"
                    | "for"
                    | "foreach"
                    | "while"
                    | "do"
                    | "return"
                    | "try"
                    | "except"
                    | "catch"
                    | "finally"
                    | "throw"
                    | "raise"
                    | "yield"
                    | "await"
                    | "break"
                    | "continue"
                    | "pass"
                    | "switch"
                    | "case"
                    | "default"
                    | "goto"
                    | "with"
                    | "assert"
                    | "import"
                    | "from"
                    | "export"
                    | "as"
                    | "match"
                    | "when"
                    | "then"
                    | "fi"
                    | "done"
                    | "esac"
                    | "until"
                    | "unless"
                    | "rescue"
                    | "ensure"
                    | "defer"
                    | "loop"
                    | "select"
                    | "of"
                    | "end"
                    | "begin"
                    | "endif"
                    | "package"
            ) && !matches!(lang, "SQL" | "Dockerfile" | "Makefile" | "INI")
            {
                "control"
            } else if matches!(
                word,
                "class"
                    | "def"
                    | "fn"
                    | "func"
                    | "function"
                    | "struct"
                    | "enum"
                    | "trait"
                    | "impl"
                    | "interface"
                    | "extends"
                    | "new"
                    | "implements"
                    | "type"
                    | "let"
                    | "pub"
                    | "static"
                    | "const"
                    | "var"
                    | "mut"
                    | "use"
                    | "mod"
                    | "in"
                    | "and"
                    | "or"
                    | "not"
                    | "is"
                    | "async"
                    | "lambda"
                    | "del"
                    | "global"
                    | "nonlocal"
                    | "package"
            ) || matches!(kind, Some("keyword" | "operator"))
            {
                "keyword"
            } else if matches!(
                previous,
                "class"
                    | "extends"
                    | "new"
                    | "implements"
                    | "struct"
                    | "enum"
                    | "interface"
                    | "trait"
                    | "impl"
                    | "instanceof"
                    | "union"
                    | "namespace"
                    | "type"
            ) {
                "class"
            } else if after.starts_with('(')
                || after.starts_with("!(")
                || matches!(kind, Some("function"))
            {
                if lang == "Python"
                    && matches!(
                        word,
                        "print"
                            | "len"
                            | "range"
                            | "enumerate"
                            | "zip"
                            | "open"
                            | "sum"
                            | "min"
                            | "max"
                            | "isinstance"
                    )
                {
                    "builtin"
                } else {
                    "function"
                }
            } else if matches!(kind, Some("class"))
                || matches!(
                    word,
                    "int"
                        | "str"
                        | "float"
                        | "bool"
                        | "bytes"
                        | "bytearray"
                        | "list"
                        | "dict"
                        | "set"
                        | "tuple"
                        | "object"
                        | "type"
                        | "complex"
                        | "memoryview"
                        | "slice"
                        | "string"
                        | "number"
                )
                || (c.is_uppercase()
                    && word.chars().any(char::is_lowercase)
                    && !before.ends_with('.'))
            {
                "class"
            } else if matches!(lang, "JavaScript" | "TypeScript")
                && after.starts_with(':')
                && (before.ends_with('{') || before.ends_with(','))
            {
                "property"
            } else if lang == "Shell Script"
                && matches!(word, "echo" | "cd" | "export" | "printf" | "read")
            {
                "builtin"
            } else {
                kind.unwrap_or("variable")
            };
            push_run(out, offset + start, offset + i, k);
        } else {
            i += 1;
            let k = if matches!(
                c,
                '=' | '+' | '-' | '*' | '/' | '%' | '<' | '>' | '!' | '&' | '|' | '~' | '^'
            ) {
                Some("operator")
            } else if lang == "HTML" || lang == "XML" {
                Some("punctuation")
            } else {
                None
            };
            if let Some(k) = k {
                push_run(out, offset + start, offset + i, k);
            }
        }
    }
}

#[derive(Clone)]
pub struct Highlighter(Rc<Inner>);
struct Inner {
    buffer: glib::WeakRef<gtk::TextBuffer>,
    view: glib::WeakRef<gtk::TextView>,
    language: RefCell<String>,
    tags: Vec<gtk::TextTag>,
    source: RefCell<Option<glib::SourceId>>,
    receiver: RefCell<Option<glib::SourceId>>,
    cancel: RefCell<Arc<AtomicBool>>,
    handler: RefCell<Option<glib::SignalHandlerId>>,
    generation: Cell<u64>,
    pending: Cell<bool>,
    skipped: Cell<bool>,
    destroyed: Cell<bool>,
    active: Cell<bool>,
    painted: Cell<bool>,
    max_batch: Cell<Duration>,
    cache: Arc<Mutex<Option<LexCache>>>,
    ready: Arc<AtomicBool>,
}
impl Highlighter {
    pub fn new(view: &gtk::TextView, styles: &SyntaxColors, language: &str) -> Self {
        let buffer = view.buffer().expect("text view buffer");
        let table = buffer.tag_table().expect("buffer tag table");
        let tags = KINDS
            .iter()
            .map(|kind| {
                let name = format!("hl-{kind}");
                table.lookup(&name).unwrap_or_else(|| {
                    let tag = gtk::TextTag::new(Some(&name));
                    table.add(&tag);
                    tag
                })
            })
            .collect();
        let this = Self(Rc::new(Inner {
            buffer: buffer.downgrade(),
            view: view.downgrade(),
            language: RefCell::new(language.into()),
            tags,
            source: RefCell::new(None),
            receiver: RefCell::new(None),
            cancel: RefCell::new(Arc::new(AtomicBool::new(false))),
            handler: RefCell::new(None),
            generation: Cell::new(0),
            pending: Cell::new(false),
            skipped: Cell::new(false),
            destroyed: Cell::new(false),
            active: Cell::new(true),
            painted: Cell::new(false),
            max_batch: Cell::new(Duration::ZERO),
            cache: Arc::new(Mutex::new(None)),
            ready: Arc::new(AtomicBool::new(false)),
        }));
        this.set_style(styles);
        let weak = Rc::downgrade(&this.0);
        let id = buffer.connect_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).restart(DEBOUNCE_MS);
            }
        });
        this.0.handler.replace(Some(id));
        let weak = Rc::downgrade(&this.0);
        view.connect_draw(move |_, _| {
            if let Some(inner) = weak.upgrade() {
                if !inner.painted.replace(true)
                    && inner.pending.get()
                    && inner.active.get()
                    && !inner.destroyed.get()
                {
                    Self(inner).restart(0);
                }
            }
            glib::Propagation::Proceed
        });
        this.restart(0);
        this
    }
    pub fn pending(&self) -> bool {
        self.0.pending.get()
    }
    pub fn skipped(&self) -> bool {
        self.0.skipped.get()
    }
    pub fn max_batch(&self) -> Duration {
        self.0.max_batch.get()
    }
    pub fn set_style(&self, styles: &SyntaxColors) {
        for (tag, kind) in self.0.tags.iter().zip(KINDS) {
            let style = styles.get(kind);
            let color = style.and_then(|s| gdk::RGBA::parse(&s.foreground).ok());
            tag.set_foreground_rgba(color.as_ref());
            tag.set_foreground_set(color.is_some());
            tag.set_weight(if style.is_some_and(|s| s.bold) {
                700
            } else {
                400
            });
            tag.set_weight_set(style.is_some_and(|s| s.bold));
            tag.set_style(if style.is_some_and(|s| s.italic) {
                pango::Style::Italic
            } else {
                pango::Style::Normal
            });
            tag.set_style_set(style.is_some_and(|s| s.italic));
            tag.set_underline(if style.is_some_and(|s| s.underline) {
                pango::Underline::Single
            } else {
                pango::Underline::None
            });
            tag.set_underline_set(style.is_some_and(|s| s.underline));
        }
    }
    pub fn set_language(&self, language: &str) {
        self.0.ready.store(false, Ordering::Relaxed);
        self.0.language.replace(language.into());
        self.restart(0);
    }
    pub fn set_active(&self, active: bool) {
        if self.0.active.replace(active) != active {
            if active {
                self.restart(0);
            } else {
                self.cancel();
            }
        }
    }
    fn cancel(&self) {
        self.0.generation.set(self.0.generation.get() + 1);
        self.0.cancel.borrow().store(true, Ordering::Relaxed);
        let source = self.0.source.take();
        let receiver = self.0.receiver.take();
        if let Some(id) = source {
            id.remove();
        }
        if let Some(id) = receiver {
            id.remove();
        }
        self.0.pending.set(false);
    }
    pub fn destroy(&self) {
        if self.0.destroyed.replace(true) {
            return;
        }
        self.cancel();
        if let (Some(buffer), Some(id)) = (self.0.buffer.upgrade(), self.0.handler.take()) {
            buffer.disconnect(id);
        }
    }
    fn restart(&self, delay: u64) {
        self.cancel();
        if self.0.destroyed.get() {
            return;
        }
        if self.0.language.borrow().as_str() == "Plain Text" {
            if let Some(buffer) = self.0.buffer.upgrade() {
                let (a, b) = buffer.bounds();
                for tag in &self.0.tags {
                    buffer.remove_tag(tag, &a, &b);
                }
            }
            return;
        }
        if !self.0.active.get() {
            return;
        }
        self.0.pending.set(true);
        if !self.0.painted.get() {
            return;
        }
        let weak = Rc::downgrade(&self.0);
        let cb = move || {
            if let Some(inner) = weak.upgrade() {
                let h = Self(inner);
                h.0.source.take();
                h.begin();
            }
        };
        let id = if delay == 0 {
            glib::idle_add_local_once(cb)
        } else {
            glib::timeout_add_local_once(Duration::from_millis(delay), cb)
        };
        self.0.source.replace(Some(id));
    }
    #[allow(deprecated)]
    fn begin(&self) {
        let Some(buffer) = self.0.buffer.upgrade() else {
            self.0.pending.set(false);
            return;
        };
        let generation = self.0.generation.get();
        let mut offset = 0;
        let mut parts = Vec::new();
        let weak = Rc::downgrade(&self.0);
        let id = glib::idle_add_local(move || {
            let Some(inner) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let h = Self(inner);
            if h.0.generation.get() != generation || h.0.destroyed.get() {
                return glib::ControlFlow::Break;
            }
            let started = Instant::now();
            loop {
                let end = (offset + 4096).min(buffer.char_count());
                parts.push(
                    buffer
                        .text(
                            &buffer.iter_at_offset(offset),
                            &buffer.iter_at_offset(end),
                            true,
                        )
                        .unwrap_or_default()
                        .to_string(),
                );
                offset = end;
                if offset == buffer.char_count() {
                    h.0.source.take();
                    h.lex(std::mem::take(&mut parts));
                    return glib::ControlFlow::Break;
                }
                if started.elapsed() >= Duration::from_millis(2) {
                    return glib::ControlFlow::Continue;
                }
            }
        });
        self.0.source.replace(Some(id));
    }
    #[allow(deprecated)]
    fn lex(&self, parts: Vec<String>) {
        let visible = self
            .0
            .view
            .upgrade()
            .map(|v| v.line_at_y(v.visible_rect().y()).0.offset().max(0) as usize)
            .unwrap_or(0);
        let language = self.0.language.borrow().clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.0.cancel.replace(cancel.clone());
        let generation = self.0.generation.get();
        let cache = self.0.cache.clone();
        let reusable = self.0.ready.swap(false, Ordering::Relaxed);
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT_IDLE);
        std::thread::spawn(move || {
            let text = parts.concat();
            let skipped =
                text.len() > MAX_BYTES || text.split('\n').any(|s| s.chars().count() > MAX_LINE);
            let Ok(mut cache) = cache.lock() else {
                return;
            };
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let lexed = if skipped {
                Lexed {
                    dirty_end: text.chars().count(),
                    ..Default::default()
                }
            } else {
                lex_cached(
                    &language,
                    &text,
                    &cancel,
                    if reusable { cache.as_ref() } else { None },
                )
            };
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let start = lexed.dirty_start / 256 * 256;
            let end = lexed.dirty_end;
            let mut chunks: Vec<(usize, usize, Vec<Run>)> = (start..end)
                .step_by(256)
                .map(|a| (a, (a + 256).min(end), Vec::new()))
                .collect();
            for &(a, b, k) in &lexed.runs {
                if b <= start || a >= end {
                    continue;
                }
                let a = a.max(start);
                let b = b.min(end);
                for index in (a - start) / 256..=(b.saturating_sub(1) - start) / 256 {
                    if let Some(chunk) = chunks.get_mut(index) {
                        chunk.2.push((a.max(chunk.0), b.min(chunk.1), k));
                    }
                }
            }
            *cache = Some(LexCache {
                language,
                text,
                runs: lexed.runs,
                checkpoints: lexed.checkpoints,
            });
            let pivot = (visible.saturating_sub(start) / 256).min(chunks.len().saturating_sub(1));
            chunks.rotate_left(pivot);
            let _ = tx.send((skipped, chunks));
        });
        let weak = Rc::downgrade(&self.0);
        let id = rx.attach(None, move |(skipped, chunks)| {
            if let Some(inner) = weak.upgrade() {
                let h = Self(inner);
                h.0.receiver.take();
                if h.0.generation.get() == generation && !h.0.destroyed.get() {
                    h.0.skipped.set(skipped);
                    h.paint(chunks, generation);
                }
            }
            glib::ControlFlow::Break
        });
        self.0.receiver.replace(Some(id));
    }
    fn paint(&self, chunks: Vec<(usize, usize, Vec<Run>)>, generation: u64) {
        let mut chunks = chunks.into_iter();
        let weak = Rc::downgrade(&self.0);
        let id = glib::idle_add_local(move || {
            let Some(inner) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let h = Self(inner);
            if h.0.generation.get() != generation || h.0.destroyed.get() {
                return glib::ControlFlow::Break;
            }
            let Some(buffer) = h.0.buffer.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let started = Instant::now();
            loop {
                let Some((a, b, runs)) = chunks.next() else {
                    h.0.max_batch
                        .set(h.0.max_batch.get().max(started.elapsed()));
                    h.0.pending.set(false);
                    h.0.ready.store(true, Ordering::Relaxed);
                    h.0.source.take();
                    return glib::ControlFlow::Break;
                };
                let first = buffer.iter_at_offset(a as i32);
                let last = buffer.iter_at_offset(b as i32);
                for tag in &h.0.tags {
                    buffer.remove_tag(tag, &first, &last);
                    if h.0.generation.get() != generation {
                        return glib::ControlFlow::Break;
                    }
                }
                for (a, b, k) in runs {
                    if let Some(index) = KINDS.iter().position(|x| *x == k) {
                        buffer.apply_tag(
                            &h.0.tags[index],
                            &buffer.iter_at_offset(a as i32),
                            &buffer.iter_at_offset(b as i32),
                        );
                        if h.0.generation.get() != generation {
                            return glib::ControlFlow::Break;
                        }
                    }
                }
                if h.0.generation.get() != generation {
                    return glib::ControlFlow::Break;
                }
                if started.elapsed() >= Duration::from_millis(3) {
                    break;
                }
            }
            h.0.max_batch
                .set(h.0.max_batch.get().max(started.elapsed()));
            glib::ControlFlow::Continue
        });
        self.0.source.replace(Some(id));
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.cancel.get_mut().store(true, Ordering::Relaxed);
        if let Some(id) = self.source.get_mut().take() {
            id.remove();
        }
        if let Some(id) = self.receiver.get_mut().take() {
            id.remove();
        }
        if let (Some(b), Some(id)) = (self.buffer.upgrade(), self.handler.get_mut().take()) {
            b.disconnect(id);
        }
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn incremental_checkpoints_match_fresh_lexing() {
        let original = "value = compute(123, \"😀\")  # comment\n".repeat(200);
        let cancel = AtomicBool::new(false);
        let first = lex_cached("Python", &original, &cancel, None);
        let mut cache = LexCache {
            language: "Python".into(),
            text: original.clone(),
            runs: first.runs,
            checkpoints: first.checkpoints,
        };
        let cases = [
            format!("# new\n{original}"),
            original.replacen("compute", "other", 1),
            format!("\"\"\"\n{original}"),
            original.clone(),
            original.replacen("😀", "hello\nworld", 1),
        ];
        for text in cases {
            let result = lex_cached("Python", &text, &cancel, Some(&cache));
            assert_eq!(result.runs, tokenize("Python", &text));
            cache = LexCache {
                language: "Python".into(),
                text,
                runs: result.runs,
                checkpoints: result.checkpoints,
            };
        }
    }
    #[test]
    fn markdown_fence_checkpoints_match_fresh_lexing() {
        let text = format!(
            "# Heading\n```python\n{}\n```\nEnd\n",
            "x = 1\n".repeat(150)
        );
        let cancel = AtomicBool::new(false);
        let first = lex_cached("Markdown", &text, &cancel, None);
        let cache = LexCache {
            language: "Markdown".into(),
            text: text.clone(),
            runs: first.runs,
            checkpoints: first.checkpoints,
        };
        let changed = text.replacen("x = 1", "x = \"open", 1);
        assert_eq!(
            lex_cached("Markdown", &changed, &cancel, Some(&cache)).runs,
            tokenize("Markdown", &changed)
        );
    }
}
