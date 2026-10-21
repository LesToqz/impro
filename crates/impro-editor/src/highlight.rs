//! Scope-based syntax classification and cancellable background buffer colouring.
use gtk::prelude::*;
use impro_core::themes::{SyntaxColors, KINDS};
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
use syntect::{
    parsing::{ParseState, ScopeStack, SyntaxSet},
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
        "css" | "scss" | "sass" => "CSS",
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
    SYNTAXES.get_or_init(SyntaxSet::load_defaults_newlines)
}
/// Pure, thread-safe classification into the 26 theme roles; offsets count characters.
/// This blocking API loads syntax data lazily. UI callers must use a worker.
pub fn tokenize(path_or_language: &str, text: &str) -> Vec<Run> {
    tokenize_cancel(path_or_language, text, &AtomicBool::new(false))
}
fn tokenize_cancel(path: &str, text: &str, cancel: &AtomicBool) -> Vec<Run> {
    if text.len() > MAX_BYTES || text.split('\n').any(|s| s.chars().count() > MAX_LINE) {
        return Vec::new();
    }
    let language = language_name(path, text.lines().next().unwrap_or(""));
    if language == "Plain Text" || cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let set = syntaxes();
    let syntax_name = match language {
        "Shell Script" | "Dockerfile" => "Bourne Again Shell (bash)",
        "TypeScript" | "TypeScript React" | "JavaScript React" => "JavaScript",
        "TOML" | "INI" => "YAML",
        _ => language,
    };
    let syntax = set
        .find_syntax_by_name(syntax_name)
        .or_else(|| set.find_syntax_by_extension(Path::new(path).extension()?.to_str()?))
        .unwrap_or_else(|| set.find_syntax_plain_text());
    let mut parser = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut result = Vec::new();
    let mut offset = 0;
    for line in LinesWithEndings::from(text) {
        if cancel.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let ops = parser.parse_line(line, set).unwrap_or_default();
        let mut byte = 0;
        let mut ch = offset;
        for (end, op) in ops {
            if end > byte {
                classify_span(&mut result, language, line, byte, end, ch, &stack);
                ch += line[byte..end].chars().count();
            }
            let _ = stack.apply(&op);
            byte = end;
        }
        if byte < line.len() {
            classify_span(&mut result, language, line, byte, line.len(), ch, &stack);
        }
        offset += line.chars().count();
    }
    result
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
    } else if scope.contains("string") {
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
    if start == end {
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
    let scopes = stack
        .as_slice()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let kind = scopes.iter().rev().find_map(|s| map_scope(s));
    let all = scopes.join(" ");
    let mut kind = if all.contains("comment") {
        Some("comment")
    } else {
        kind
    };
    if all.contains("string")
        && !all.contains("escape")
        && !all.contains("interpolation")
        && !all.contains("embedded")
    {
        kind = Some("string");
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
        if all.contains("at-rule") && value.chars().any(char::is_alphabetic) {
            kind = Some("control");
        }
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
            let k = if c.is_ascii_digit() { "number" }
                else if "true false True False None null nil NULL undefined Ellipsis NotImplemented".split_whitespace().any(|s| s == word) { "constant" }
                else if "if elif else elseif for foreach while do return try except catch finally throw raise yield await break continue pass switch case default goto with assert import from export as match when then fi done esac until unless rescue ensure defer loop select of end begin endif package".split_whitespace().any(|s| s == word) && !matches!(lang,"SQL"|"Dockerfile"|"Makefile"|"INI") { "control" }
                else if "class def fn func function struct enum trait impl interface extends new implements type let pub static const var mut use mod in and or not is async lambda del global nonlocal package".split_whitespace().any(|s| s == word) || matches!(kind, Some("keyword"|"operator")) { "keyword" }
                else if "class extends new implements struct enum interface trait impl instanceof union namespace type".split_whitespace().any(|s| s == previous) { "class" }
                else if after.starts_with('(') || after.starts_with("!(") || matches!(kind,Some("function")) { if lang == "Python" && "print len range enumerate zip open sum min max isinstance".split_whitespace().any(|s| s == word) { "builtin" } else { "function" } }
                else if matches!(kind,Some("class")) || "int str float bool bytes bytearray list dict set tuple object type complex memoryview slice string number".split_whitespace().any(|s| s == word) || (c.is_uppercase() && word.chars().any(char::is_lowercase) && !before.ends_with('.')) { "class" }
                else if matches!(lang,"JavaScript"|"TypeScript") && after.starts_with(':') && (before.ends_with('{') || before.ends_with(',')) { "property" }
                else if lang == "Shell Script" && "echo cd export printf read".split_whitespace().any(|s| s == word) { "builtin" }
                else { kind.unwrap_or("variable") };
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
    max_batch: Cell<Duration>,
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
            max_batch: Cell::new(Duration::ZERO),
        }));
        this.set_style(styles);
        let weak = Rc::downgrade(&this.0);
        let id = buffer.connect_changed(move |_| {
            if let Some(inner) = weak.upgrade() {
                Self(inner).restart(DEBOUNCE_MS);
            }
        });
        this.0.handler.replace(Some(id));
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
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .unwrap_or_default()
            .to_string();
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
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT_IDLE);
        std::thread::spawn(move || {
            let skipped =
                text.len() > MAX_BYTES || text.split('\n').any(|s| s.chars().count() > MAX_LINE);
            let runs = if skipped {
                Vec::new()
            } else {
                tokenize_cancel(&language, &text, &cancel)
            };
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            // Small fixed character spans cap the cost of one GTK tag transaction.
            let size = text.chars().count();
            let mut chunks: Vec<(usize, usize, Vec<Run>)> = (0..size)
                .step_by(256)
                .map(|a| (a, (a + 256).min(size), Vec::new()))
                .collect();
            for (a, b, k) in runs {
                for index in a / 256..=(b.saturating_sub(1) / 256) {
                    if let Some(chunk) = chunks.get_mut(index) {
                        chunk.2.push((a.max(chunk.0), b.min(chunk.1), k));
                    }
                }
            }
            let pivot = (visible / 256).min(chunks.len().saturating_sub(1));
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
                    h.0.pending.set(false);
                    h.0.source.take();
                    return glib::ControlFlow::Break;
                };
                let first = buffer.iter_at_offset(a as i32);
                let last = buffer.iter_at_offset(b as i32);
                for tag in &h.0.tags {
                    buffer.remove_tag(tag, &first, &last);
                }
                for (a, b, k) in runs {
                    if let Some(index) = KINDS.iter().position(|x| *x == k) {
                        buffer.apply_tag(
                            &h.0.tags[index],
                            &buffer.iter_at_offset(a as i32),
                            &buffer.iter_at_offset(b as i32),
                        );
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
