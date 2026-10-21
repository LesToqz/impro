mod common;
use common::*;
use gtk::prelude::*;
use impro_editor::highlight::*;
fn check(path: &str, source: &str, cases: &[(&str, usize, &str)]) {
    let runs = tokenize(path, source);
    let mut failures = Vec::new();
    for (needle, n, expected) in cases {
        let byte = source.match_indices(needle).nth(*n).expect("needle").0;
        let pos = source[..byte].chars().count();
        let actual = runs
            .iter()
            .find(|(a, b, _)| pos >= *a && pos < *b)
            .map(|r| r.2);
        if actual != Some(*expected) {
            failures.push(format!("{needle:?}: {actual:?} != {expected}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
#[test]
fn test_python() {
    check("a.py","#!/usr/bin/env python3\n@app.route(\"/x\")\nclass Foo(Base):\n    \"\"\"Docstring.\"\"\"\n    LIMIT = 0x1F + 2.5\n\n    def method(self, value, flag=None):\n        # a comment\n        text = f\"v={value!r}\\n\" + 'plain'\n        for item in range(3):\n            if flag and not item:\n                return len(text) + self.count(value)\n        raise ValueError(\"bad\")\n",&[("#!/usr",0,"comment"),("@app.route",0,"decorator"),(".route",0,"decorator"),("class",0,"keyword"),("Foo",0,"class"),("Base",0,"class"),("\"\"\"Docstring.\"\"\"",0,"doc"),("0x1F",0,"number"),("2.5",0,"number"),("def",0,"keyword"),("method",0,"function"),("self",0,"variable"),("value",0,"variable"),("None",0,"constant"),("# a comment",0,"comment"),("f\"v=",0,"string"),("{",0,"keyword"),("value",1,"variable"),("\\n",0,"escape"),("'plain'",0,"string"),("for",0,"control"),("in range",0,"keyword"),("range",0,"builtin"),("and",0,"keyword"),("return",0,"control"),("len",0,"builtin"),("count",0,"function"),("ValueError",0,"class"),("\"bad\"",0,"string")]);
}
#[test]
fn test_python_call_versus_definition_and_types() {
    check(
        "a.py",
        "def helper(x: int) -> str:\n    return helper(x) + other.name\n",
        &[
            ("helper", 0, "function"),
            ("helper", 1, "function"),
            ("int", 0, "class"),
            ("str", 0, "class"),
            ("name", 0, "variable"),
        ],
    );
}
#[test]
fn test_json() {
    check(
        "a.json",
        "{\"name\": \"x\", \"n\": [1, 2.5e3, true, null], \"o\": {\"k\": \"v\"}}",
        &[
            ("\"name\"", 0, "property"),
            ("\"x\"", 0, "string"),
            ("\"k\"", 0, "property"),
            ("\"v\"", 0, "string"),
            ("2.5e3", 0, "number"),
            ("true", 0, "constant"),
            ("null", 0, "constant"),
        ],
    );
}
#[test]
fn test_markdown() {
    check("a.md","# Title\nSome **bold** and *em* and `code` and [text](http://x.y)\n```python\ndef f(): pass\n```\n- item\n",&[("# Title",0,"heading"),("**bold**",0,"strong"),("*em*",0,"emphasis"),("`code`",0,"string"),("http://x.y",0,"link"),("```python",0,"string"),("def",0,"keyword"),("f()",0,"function"),("- item",0,"keyword")]);
}
#[test]
fn test_html() {
    check("a.html","<!DOCTYPE html>\n<html lang=\"en\"><head><style>a { color: red; }</style><script>var n = go(1);</script></head><p class=\"x\">Hi &amp; bye</p><!-- c --></html>",&[("html",1,"tag"),("<head",0,"punctuation"),("lang",0,"attribute"),("\"en\"",0,"string"),("&amp;",0,"constant"),("<!-- c -->",0,"comment"),("a {",0,"selector"),("color",0,"property"),("var",0,"keyword"),("go",0,"function")]);
}
#[test]
fn test_css() {
    check("a.css","@media (max-width: 10px) { a:hover, .b > #c { color: red; margin: 10px 2em; --x: 1; background: rgba(0, 0, 0, .5) !important; } }\n/* note */\n",&[("media",0,"control"),("max-width",0,"property"),("a:hover",0,"selector"),("hover",0,"selector"),("b >",0,"selector"),(".b",0,"selector"),("#c",0,"selector"),("color",0,"property"),("red",0,"string"),("10px",0,"number"),("--x",0,"variable"),("rgba",0,"function"),("!important",0,"keyword"),("/* note */",0,"comment")]);
}
#[test]
fn test_javascript() {
    check("a.js","import { a } from \"x\";\nconst obj = { key: 1 };\nclass Foo extends Bar { constructor() { super(); } }\nfunction run(q) { if (q === 1) return `t ${q}`; throw new Error(\"e\"); }\nlet re = /ab+c/gi.test(s) ? null : undefined; // tail\nitems.map(v => v * 2); console.log(go(1));\n",&[("import",0,"control"),("from",0,"control"),("const",0,"keyword"),("obj",0,"variable"),("key",0,"property"),("class",0,"keyword"),("Foo",0,"class"),("Bar",0,"class"),("function",0,"keyword"),("run",0,"function"),("return",0,"control"),("`t ",0,"string"),("Error",0,"class"),("/ab+c/gi",0,"regex"),("test",0,"function"),("null",0,"constant"),("// tail",0,"comment"),("map",0,"function"),("log",0,"function"),("go",0,"function")]);
}
#[test]
fn test_typescript() {
    check("a.ts","interface Foo { a: string }\n@Component({x: 1}) class D implements Foo {}\nconst n: number = 1;\n",&[("interface",0,"keyword"),("Foo",0,"class"),("string",0,"class"),("@Component",0,"decorator"),("number",0,"class"),("D",0,"class")]);
}
#[test]
fn test_shell() {
    check("run.sh","#!/bin/bash\nexport NAME=\"a $USER\"\nif [ -f \"$f\" ]; then echo hi; fi\nfor i in 1 2; do cd /tmp; done  # tail\n",&[("#!/bin",0,"comment"),("export",0,"builtin"),("NAME",0,"variable"),("\"a ",0,"string"),("$USER",0,"variable"),("if",0,"control"),("then",0,"control"),("echo",0,"builtin"),("do",0,"control"),("# tail",0,"comment")]);
}
#[test]
fn test_yaml() {
    check(
        "a.yml",
        "# c\nname: test\nenabled: true\ncount: 3\nitems:\n  - uses: \"q\"\n",
        &[
            ("# c", 0, "comment"),
            ("name", 0, "property"),
            ("test", 0, "string"),
            ("true", 0, "constant"),
            ("3", 0, "number"),
            ("uses", 0, "property"),
            ("\"q\"", 0, "string"),
        ],
    );
}
#[test]
fn test_toml() {
    check(
        "a.toml",
        "# c\n[tool.x]\nname = \"x\"\nn = 1.5\nok = true\n",
        &[
            ("[tool.x]", 0, "tag"),
            ("name", 0, "property"),
            ("\"x\"", 0, "string"),
            ("1.5", 0, "number"),
            ("true", 0, "constant"),
            ("# c", 0, "comment"),
        ],
    );
}
#[test]
fn test_c() {
    check("a.c","#include <stdio.h>\nstatic int main(int argc) {\n  printf(\"hi\\n\");\n  if (argc) return NULL;\n  struct Foo *f = malloc(8);\n}\n",&[("#include",0,"preproc"),("<stdio.h>",0,"string"),("static",0,"keyword"),("int",0,"keyword"),("main",0,"function"),("printf",0,"function"),("\\n",0,"escape"),("if",0,"control"),("NULL",0,"constant"),("Foo",0,"class"),("malloc",0,"function")]);
}
#[test]
fn test_rust() {
    check("a.rs","#[derive(Debug)]\npub struct Foo { x: i32, y: Option<String> }\nimpl Foo { pub fn new() -> Self { let s = \"a\"; println!(\"{}\", s); } }\n",&[("#[derive(Debug)]",0,"decorator"),("struct",0,"keyword"),("Foo",0,"class"),("i32",0,"class"),("Option",0,"class"),("fn",0,"keyword"),("new",0,"function"),("let",0,"keyword"),("println!",0,"function")]);
}
#[test]
fn test_go() {
    check("a.go","package main\nimport \"fmt\"\ntype Foo struct { X int }\nfunc (f *Foo) Bar(a string) error { return nil }\nfunc main() { fmt.Println(\"hi\") }\n",&[("package",0,"control"),("\"fmt\"",0,"string"),("type",0,"keyword"),("Foo",0,"class"),("func",0,"keyword"),("Bar",0,"function"),("string",0,"class"),("nil",0,"constant"),("Println",0,"function")]);
}
#[test]
fn test_sql() {
    check(
        "q.sql",
        "SELECT a, COUNT(*) FROM t WHERE b = 'x' AND c > 10; -- tail\n",
        &[
            ("SELECT", 0, "keyword"),
            ("FROM", 0, "keyword"),
            ("WHERE", 0, "keyword"),
            ("'x'", 0, "string"),
            ("AND", 0, "keyword"),
            ("10", 0, "number"),
            ("-- tail", 0, "comment"),
        ],
    );
}
#[test]
fn test_map_token() {
    assert_eq!(
        map_scope("comment.line.number-sign.python"),
        Some("comment")
    );
    assert_eq!(map_scope("constant.numeric.integer"), Some("number"));
    assert_eq!(map_scope("source.python"), None);
}
#[test]
fn test_every_kind_has_a_dark_plus_style() {
    let theme = impro_core::themes::get_theme(Some("impro-dark"));
    for kind in impro_core::themes::KINDS {
        assert!(theme.syntax.contains_key(kind));
    }
}
#[test]
fn test_offsets_are_code_points_and_cover_the_text() {
    let text = "s = \"😀😀\"\ndef g():\n    pass\n";
    let runs = tokenize("emoji.py", text);
    assert!(runs
        .iter()
        .all(|(a, b, _)| *a < *b && *b <= text.chars().count()));
    for needle in ["def", "pass"] {
        let offset = text[..text.find(needle).unwrap()].chars().count();
        assert!(runs.iter().any(|(a, b, _)| offset >= *a && offset < *b));
    }
}
#[test]
fn test_by_file_name() {
    for (path, expected) in [
        ("a.py", "Python"),
        ("a.pyi", "Python"),
        ("a.rs", "Rust"),
        ("README.MD", "Markdown"),
        ("a.jsonc", "JSON"),
        (".bashrc", "Shell Script"),
        (".env.local", "Shell Script"),
        ("Dockerfile.dev", "Dockerfile"),
        ("Makefile", "Makefile"),
        ("CMakeLists.txt", "CMake"),
        ("a.tsx", "TypeScript React"),
        ("a.svg", "XML"),
    ] {
        assert_eq!(language_name(path, ""), expected);
    }
}
#[test]
fn test_plain_and_unknown() {
    for path in [
        "a.txt",
        "a.log",
        ".gitignore",
        ".gitattributes",
        "mystery.unknown",
    ] {
        assert_eq!(language_name(path, ""), "Plain Text");
        assert!(tokenize(path, "def f(): pass").is_empty());
    }
}
#[test]
fn test_shebang() {
    for (first, expected) in [
        ("#!/usr/bin/env python3", "Python"),
        ("#!/usr/bin/env -S NAME=x node --flag", "JavaScript"),
        ("#!/bin/bash", "Shell Script"),
        ("#!/usr/bin/env deno", "TypeScript"),
    ] {
        assert_eq!(language_name("tool", first), expected);
    }
    assert_eq!(
        language_name("file.txt", "#!/usr/bin/python3"),
        "Plain Text"
    );
}
#[test]
fn test_instances_are_cached_without_stripping_text() {
    assert_eq!(
        tokenize("a.py", "\n  x = 1\n"),
        tokenize("a.py", "\n  x = 1\n")
    );
}
#[test]
fn test_tags_are_applied_to_an_opened_file() {
    run(|| {
        let h = Harness::new();
        let text = "def greet(name):\n    return \"hi \" + name  # note\n";
        let t = h.open("a.py", text);
        h.settled();
        assert_eq!(
            h.panel.status().text(),
            "Ln 3, Col 1  |  UTF-8  |  LF  |  Python"
        );
        for (needle, expected) in [
            ("def", "keyword"),
            ("greet", "function"),
            ("\"hi", "string"),
            ("return", "control"),
            ("# note", "comment"),
        ] {
            assert_eq!(
                tag(&t, text.find(needle).unwrap() as i32),
                Some(format!("hl-{expected}"))
            );
        }
        assert_eq!(tag(&t, 3), None);
        assert_eq!(tag(&t, text.find('(').unwrap() as i32), None);
        for kind in impro_core::themes::KINDS {
            assert!(t
                .buffer
                .tag_table()
                .unwrap()
                .lookup(&format!("hl-{kind}"))
                .is_some());
        }
    });
}
#[test]
fn test_tag_properties_follow_dark_plus() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "x = 1\n");
        let table = t.buffer.tag_table().unwrap();
        assert_eq!(
            table
                .lookup("hl-comment")
                .unwrap()
                .foreground_rgba()
                .unwrap()
                .to_string(),
            "rgb(106,153,85)"
        );
        assert!(table.lookup("hl-heading").unwrap().is_weight_set());
        assert!(table.lookup("hl-emphasis").unwrap().is_style_set());
        assert!(table.lookup("hl-link").unwrap().is_underline_set());
    });
}
#[test]
fn test_multiline_triple_quoted_string() {
    run(|| {
        let h = Harness::new();
        let text = "x = 1\ny = \"\"\"first\nsecond\nthird\"\"\"\nz = 2\n";
        let t = h.open("a.py", text);
        h.settled();
        for needle in ["first", "second", "third"] {
            assert_eq!(
                tag(&t, text.find(needle).unwrap() as i32),
                Some("hl-string".into())
            );
        }
        assert_eq!(
            tag(&t, text.find('z').unwrap() as i32),
            Some("hl-variable".into())
        );
    });
}
#[test]
fn test_typing_an_opening_quote_recolours_and_deleting_restores() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "a = 1\nb = 2\nc = 3\n");
        h.settled();
        assert_eq!(tag(&t, 10), Some("hl-number".into()));
        t.buffer.insert(&mut t.buffer.iter_at_offset(6), "\"\"\"\n");
        h.settled();
        assert_eq!(tag(&t, 10), Some("hl-string".into()));
        t.buffer.delete(
            &mut t.buffer.iter_at_offset(6),
            &mut t.buffer.iter_at_offset(10),
        );
        h.settled();
        assert_eq!(tag(&t, 10), Some("hl-number".into()));
    });
}
#[test]
fn test_editing_near_the_top_of_a_large_file() {
    run(|| {
        let h = Harness::new();
        let t = h.open(
            "big.py",
            &"value = compute(1, \"text\")  # comment\n".repeat(5000),
        );
        h.settled();
        assert_eq!(
            tag(&t, t.buffer.char_count() - 5),
            Some("hl-comment".into())
        );
        t.buffer
            .insert(&mut t.buffer.start_iter(), "def top():\n    \"\"\"open\n");
        h.settled();
        assert_eq!(tag(&t, 0), Some("hl-keyword".into()));
        assert_eq!(tag(&t, t.buffer.char_count() - 5), Some("hl-string".into()));
    });
}
#[test]
fn test_edits_during_a_running_pass_converge() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", &"def f(a):\n    return a + 1  # c\n".repeat(1000));
        pump(130);
        t.buffer.insert(&mut t.buffer.start_iter(), "\"\"\"\n");
        t.buffer.insert_at_cursor("\"\"\"\n");
        h.settled();
        let text = t.text();
        let reference = tokenize("a.py", &text);
        for (a, _, kind) in reference.iter().step_by(37) {
            assert_eq!(tag(&t, *a as i32), Some(format!("hl-{kind}")));
        }
    });
}
#[test]
fn test_rename_switches_highlighting_on_and_off() {
    run(|| {
        let h = Harness::new();
        let t = h.open("notes.txt", "def f():\n    return 1\n");
        assert_eq!(tag(&t, 0), None);
        let py = h.dir.path().join("notes.py");
        h.panel.on_path_renamed(t.path(), &py);
        h.settled();
        assert_eq!(tag(&t, 0), Some("hl-keyword".into()));
        let md = h.dir.path().join("notes.md");
        h.panel.on_path_renamed(t.path(), &md);
        h.settled();
        assert_eq!(t.language(), "Markdown (Live)");
        let plain = h.dir.path().join("notes.log");
        h.panel.on_path_renamed(t.path(), &plain);
        h.settled();
        assert_eq!(tag(&t, 0), None);
        assert_eq!(t.language(), "Plain Text");
    });
}
#[test]
fn test_folder_rename_redetects_children() {
    run(|| {
        let h = Harness::new();
        let t = h.open("pkg/mod.py", "import os\n");
        h.settled();
        h.panel
            .on_path_renamed(h.dir.path().join("pkg"), h.dir.path().join("pkg2"));
        assert_eq!(t.language(), "Python");
        assert_eq!(tag(&t, 0), Some("hl-control".into()));
    });
}
#[test]
fn test_shebang_detects_extensionless_file() {
    run(|| {
        let h = Harness::new();
        let t = h.open("tool", "#!/usr/bin/env python3\nimport sys\n");
        h.settled();
        assert_eq!(t.language(), "Python");
        assert_eq!(
            tag(&t, 23),
            Some("hl-control".into()),
            "{:?}",
            tokenize("Python", &t.text())
        );
    });
}
#[test]
fn test_crlf_file_offsets() {
    run(|| {
        let h = Harness::new();
        let t = h.open("win.py", "x = 1\r\ndef f():\r\n    return \"a\"\r\n");
        h.settled();
        assert_eq!(t.ending(), "CRLF");
        assert!(!t.text().contains('\r'));
        assert_eq!(tag(&t, 6), Some("hl-keyword".into()));
    });
}
#[test]
fn test_astral_characters_keep_offsets_aligned() {
    run(|| {
        let h = Harness::new();
        let text = "s = \"😀😀\"\ndef g():\n    pass\n";
        let t = h.open("a.py", text);
        h.settled();
        let pos = text[..text.find("def").unwrap()].chars().count();
        assert_eq!(tag(&t, pos as i32), Some("hl-keyword".into()));
        assert_eq!(tag(&t, 5), Some("hl-string".into()));
    });
}
#[test]
fn test_buffer_stays_unmodified_and_history_untouched() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "def f():\n    pass\n");
        let count = t.history_len();
        h.settled();
        t.highlighter.set_language("JSON");
        h.settled();
        t.highlighter.set_language("Python");
        h.settled();
        assert!(!t.buffer.is_modified());
        assert_eq!(t.history_len(), count);
        assert_eq!(t.header.label.text(), "a.py");
    });
}
#[test]
fn test_edit_then_undo_returns_to_unmodified_with_colours() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "def f():\n    pass\n");
        t.buffer.begin_user_action();
        t.buffer.insert(&mut t.buffer.start_iter(), "# ");
        t.buffer.end_user_action();
        h.settled();
        assert_eq!(tag(&t, 0), Some("hl-comment".into()));
        t.undo(false);
        h.settled();
        assert!(!t.buffer.is_modified());
        assert_eq!(tag(&t, 0), Some("hl-keyword".into()));
    });
}
#[test]
fn test_find_selection_and_cursor_survive_highlighting() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "alpha = 1\nbeta = 2\n");
        h.panel.search_entry().set_text("beta");
        h.panel.find(false, false);
        let cursor = t.cursor();
        let bounds = t
            .buffer
            .selection_bounds()
            .map(|(a, z)| (a.offset(), z.offset()));
        h.settled();
        assert_eq!(t.cursor(), cursor);
        assert_eq!(
            t.buffer
                .selection_bounds()
                .map(|(a, z)| (a.offset(), z.offset())),
            bounds
        );
        assert_eq!(tag(&t, 10), Some("hl-variable".into()));
    });
}
#[test]
fn test_size_limits_skip_but_keep_the_language() {
    run(|| {
        let h = Harness::new();
        let t = h.open("big.py", &"x = 1\n".repeat(260000));
        h.settled();
        assert!(t.highlighter.skipped());
        assert_eq!(tag(&t, 0), None);
        assert_eq!(t.language(), "Python");
        let wide = h.open(
            "wide.py",
            &format!("y = \"{}\"\n", "a".repeat(MAX_LINE + 1)),
        );
        h.settled();
        assert!(wide.highlighter.skipped());
        replace(&wide, "def g(): pass\n");
        h.settled();
        assert!(!wide.highlighter.skipped());
        assert_eq!(tag(&wide, 0), Some("hl-keyword".into()));
    });
}
#[test]
fn test_closing_a_tab_destroys_its_highlighter() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "x = 1\n");
        t.buffer.insert_at_cursor("y");
        assert!(t.highlighter.pending());
        h.answer(impro_editor::Response::Discard);
        assert!(h.close(&t));
        assert!(!t.highlighter.pending());
        pump(150);
    });
}
#[test]
fn test_destroyed_highlighter_is_inert() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "def f(): pass\n");
        t.highlighter.destroy();
        t.highlighter.destroy();
        t.highlighter.set_language("Python");
        t.buffer.insert(&mut t.buffer.start_iter(), "# ");
        pump(150);
        assert!(!t.highlighter.pending());
        assert_eq!(tag(&t, 3), None);
    });
}
#[test]
fn test_set_syntax_style_recolours_existing_and_new_tabs() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "def f(): pass\n");
        h.settled();
        let mut styles = h.panel.syntax_style();
        let keyword = styles.get_mut("keyword").unwrap();
        keyword.foreground = "#ff0000".into();
        keyword.bold = true;
        keyword.italic = true;
        keyword.underline = true;
        styles.remove("comment");
        h.panel.set_syntax_style(&styles);
        let table = t.buffer.tag_table().unwrap();
        assert!(table.lookup("hl-keyword").unwrap().is_weight_set());
        assert!(!table.lookup("hl-comment").unwrap().is_foreground_set());
        let b = h.open("b.py", "class A: pass\n");
        assert_eq!(
            b.buffer
                .tag_table()
                .unwrap()
                .lookup("hl-keyword")
                .unwrap()
                .foreground_rgba()
                .unwrap()
                .to_string(),
            "rgb(255,0,0)"
        );
    });
}
#[test]
fn test_invalid_colours_are_ignored() {
    run(|| {
        let h = Harness::new();
        let t = h.open("a.py", "x = 1\n");
        let mut styles = h.panel.syntax_style();
        styles.get_mut("keyword").unwrap().foreground = "not-a-colour".into();
        h.panel.set_syntax_style(&styles);
        assert!(!t
            .buffer
            .tag_table()
            .unwrap()
            .lookup("hl-keyword")
            .unwrap()
            .is_foreground_set());
    });
}
#[test]
fn test_markdown_and_json_files() {
    run(|| {
        let h = Harness::new();
        let t = h.open("r.md", "# Title\n\nText with `code`.\n");
        h.panel.set_markdown_mode("source", None).unwrap();
        h.settled();
        assert_eq!(tag(&t, 2), Some("hl-heading".into()));
        let t = h.open("d.json", "{\"k\": 1}");
        h.settled();
        assert_eq!(tag(&t, 2), Some("hl-property".into()));
        assert_eq!(tag(&t, 6), Some("hl-number".into()));
    });
}
#[test]
#[ignore = "performance measurement; run explicitly on an idle machine"]
fn test_twenty_thousand_lines_highlight_quickly_without_blocking() {
    run(|| {
        let h = Harness::new();
        let text="class Widget(Base):\n    \"\"\"Doc.\"\"\"\n    LIMIT = 1\n\n    @property\n    def value(self, a, b=None):\n        # note\n        items = [x * 2 for x in range(10)]\n        text = f\"item {a!r}\" + 'plain'\n        return len(items) + self.count(a)\n\n".repeat(2000);
        assert!(text.matches('\n').count() >= 20000);
        let begin = std::time::Instant::now();
        let t = h.open("big.py", &text);
        h.settled();
        let elapsed = begin.elapsed();
        eprintln!(
            "20k-line highlight: {elapsed:?}; max slice {:?}",
            t.highlighter.max_batch()
        );
        assert!(elapsed.as_secs_f64() < 8.0);
        assert!(t.highlighter.max_batch().as_secs_f64() < 0.05);
        assert!(!t.highlighter.skipped());
        assert_eq!(tag(&t, 0), Some("hl-keyword".into()));
    });
}
