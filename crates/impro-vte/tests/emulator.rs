#![cfg(feature = "emulator")]
use impro_vte::emu::{
    self,
    index::{Column, Line},
    keys::{encode, Key, Modifiers},
    term::{cell::Flags, TermMode},
    Screen,
};

#[test]
fn key_encoding_tables() {
    let plain = Modifiers::default();
    let cases = [
        (Key::Up, "\x1b[A"),
        (Key::Down, "\x1b[B"),
        (Key::Right, "\x1b[C"),
        (Key::Left, "\x1b[D"),
        (Key::Home, "\x1b[H"),
        (Key::End, "\x1b[F"),
        (Key::PageUp, "\x1b[5~"),
        (Key::PageDown, "\x1b[6~"),
        (Key::Insert, "\x1b[2~"),
        (Key::Delete, "\x1b[3~"),
        (Key::Enter, "\r"),
        (Key::Backspace, "\x7f"),
        (Key::Escape, "\x1b"),
    ];
    for (key, expected) in cases {
        assert_eq!(
            encode(key, plain, false, false),
            expected.as_bytes(),
            "{key:?}"
        );
    }
    for (n, expected) in [
        "\x1bOP", "\x1bOQ", "\x1bOR", "\x1bOS", "\x1b[15~", "\x1b[17~", "\x1b[18~", "\x1b[19~",
        "\x1b[20~", "\x1b[21~", "\x1b[23~", "\x1b[24~",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            encode(Key::Function(n as u8 + 1), plain, false, false),
            expected.as_bytes()
        );
    }
    for shift in [false, true] {
        for alt in [false, true] {
            for control in [false, true] {
                let modifiers = Modifiers {
                    shift,
                    alt,
                    control,
                };
                let p = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(control);
                if p > 1 {
                    assert_eq!(
                        encode(Key::Up, modifiers, true, false),
                        format!("\x1b[1;{p}A").as_bytes()
                    );
                    assert_eq!(
                        encode(Key::Function(1), modifiers, false, false),
                        format!("\x1b[1;{p}P").as_bytes()
                    );
                    assert_eq!(
                        encode(Key::Delete, modifiers, false, false),
                        format!("\x1b[3;{p}~").as_bytes()
                    );
                }
            }
        }
    }
    assert_eq!(
        encode(
            Key::Tab,
            Modifiers {
                shift: true,
                ..plain
            },
            false,
            false
        ),
        b"\x1b[Z"
    );
    assert_eq!(encode(Key::Up, plain, true, false), b"\x1bOA");
    assert_eq!(encode(Key::Keypad('7'), plain, false, true), b"\x1bOw");
    assert_eq!(encode(Key::Keypad('\r'), plain, false, true), b"\x1bOM");
    for c in 'a'..='z' {
        assert_eq!(
            encode(
                Key::Text(c),
                Modifiers {
                    control: true,
                    ..plain
                },
                false,
                false
            ),
            vec![c as u8 - b'a' + 1]
        );
    }
    assert_eq!(
        encode(
            Key::Text('x'),
            Modifiers { alt: true, ..plain },
            false,
            false
        ),
        b"\x1bx"
    );
    assert_eq!(encode(Key::Text('€'), plain, false, false), "€".as_bytes());
}
#[test]
fn cursor_sgr_erase_and_utf8_fragmentation() {
    let mut s = Screen::new(20, 4, 100);
    for b in "abc\x1b[2D\x1b[1;3;4;9;38;2;10;20;30m界\u{301}".as_bytes() {
        s.feed(&[*b]);
    }
    assert!(s.text().starts_with("a界\u{301}"), "{:?}", s.text());
    let c = &s.term.grid()[Line(0)][Column(1)];
    assert_eq!(
        c.fg,
        emu::Color::Spec(emu::Rgb {
            r: 10,
            g: 20,
            b: 30
        })
    );
    assert!(c.flags.contains(
        Flags::BOLD | Flags::ITALIC | Flags::UNDERLINE | Flags::STRIKEOUT | Flags::WIDE_CHAR
    ));
    assert_eq!(s.cursor(), (3, 0));
    s.feed(b"\x1b[0m\x1b[H\x1b[2Kdone");
    assert!(s.text().starts_with("done"));
    s.feed(b"\x1b[2J");
    assert!(s
        .range(
            s.history() as i64,
            0,
            (s.history() + s.rows() - 1) as i64,
            19
        )
        .trim()
        .is_empty());
}
#[test]
fn scroll_regions_and_alternate_screen() {
    let mut s = Screen::new(10, 4, 100);
    s.feed(b"top\r\ninside1\r\ninside2\r\nbottom\x1b[2;3r\x1b[3;1H\nnew");
    assert!(s.range(0, 0, 0, 9).contains("top"));
    assert!(s.range(3, 0, 3, 9).contains("bottom"));
    assert_eq!(s.history(), 0);
    let original = s.text();
    s.feed(b"\x1b[?1049hother");
    assert!(s.mode().contains(TermMode::ALT_SCREEN));
    assert!(s.text().contains("other"));
    s.feed(b"\x1b[?1049l");
    assert_eq!(s.text(), original);
}
#[test]
fn osc_metadata_hyperlinks_and_modes() {
    let mut s = Screen::new(80, 4, 100);
    s.feed(b"\x1b]0;first\x07\x1b]2;second\x1b\\\x1b]7;file:///C:/work\x1b\\\x1b]8;;https://example.org/doc\x1b\\click\x1b]8;;\x1b\\");
    assert_eq!(s.title.as_deref(), Some("second"));
    assert_eq!(s.cwd(), Some("file:///C:/work"));
    assert_eq!(
        s.hyperlink(2, 0).as_deref(),
        Some("https://example.org/doc")
    );
    s.feed(b"\x1b[?2004h\x1b[?1000h\x1b[?1006h\x1b[?1004h\x1b[?1h\x1b=");
    assert!(s.mode().contains(
        TermMode::BRACKETED_PASTE
            | TermMode::MOUSE_REPORT_CLICK
            | TermMode::SGR_MOUSE
            | TermMode::FOCUS_IN_OUT
            | TermMode::APP_CURSOR
            | TermMode::APP_KEYPAD
    ));
    assert_eq!(s.paste("a\nb\x1b[201~"), b"\x1b[200~a\nb[201~\x1b[201~");
    s.feed(b"\x1b[?2004l\x1b[?1000l\x1b[?1006l\x1b[?1004l\x1b[?1l\x1b>");
    assert!(!s.mode().intersects(
        TermMode::BRACKETED_PASTE
            | TermMode::MOUSE_MODE
            | TermMode::SGR_MOUSE
            | TermMode::FOCUS_IN_OUT
            | TermMode::APP_CURSOR
            | TermMode::APP_KEYPAD
    ));
    assert_eq!(s.paste("a\r\nb"), b"a\rb");
}
#[test]
fn selection_joins_soft_wraps_preserves_hard_lines_and_wide_text() {
    let mut s = Screen::new(6, 4, 100);
    s.feed("ab界cdef\r\nnext".as_bytes());
    s.select_all();
    let selected = s.selection().unwrap();
    assert!(selected.starts_with("ab界cdef\nnext"), "{selected:?}");
    s.select_start(0, 0);
    s.select_update(3, 0);
    assert_eq!(s.selection().as_deref(), Some("ab界"));
    assert_eq!(s.range(0, 0, 0, 6), "ab界cd");
    s.term.selection = None;
    assert_eq!(s.selection(), None);
}
#[test]
fn scrollback_resize_and_reset() {
    let mut s = Screen::new(8, 3, 20);
    for n in 0..40 {
        s.feed(format!("line{n}\r\n").as_bytes());
    }
    assert_eq!(s.history(), 20);
    assert!(s.text().contains("line39"));
    s.scroll(5);
    assert_eq!(s.offset(), 5);
    s.bottom();
    assert_eq!(s.offset(), 0);
    s.resize(4, 4, true);
    assert_eq!((s.columns(), s.rows()), (4, 4));
    s.resize(16, 3, false);
    assert_eq!((s.columns(), s.rows()), (16, 3));
    s.reset(true, true);
    assert_eq!(s.history(), 0);
    assert!(s.text().trim().is_empty());
}
#[test]
fn windows_quoting_table() {
    use emu::quoting::quote;
    for (input, expected) in [
        ("", r#""""#),
        ("plain", "plain"),
        ("two words", r#""two words""#),
        (
            r"C:\Program Files\shell.exe",
            r#""C:\Program Files\shell.exe""#,
        ),
        (r"has space\", r#""has space\\""#),
        ("a\"b", r#""a\"b""#),
        ("a\\\"b", r#""a\\\"b""#),
        ("tab\there", "\"tab\there\""),
        ("日本語", "日本語"),
    ] {
        assert_eq!(quote(input), expected, "{input:?}");
    }
    assert_eq!(
        emu::quoting::command_line(&[
            "cmd.exe".into(),
            "/d".into(),
            "/c".into(),
            "echo hello".into()
        ]),
        "cmd.exe /d /c \"echo hello\""
    );
}
#[test]
fn hyperlinks_match_pointer_and_wrapped_url() {
    let line = "open https://example.org/path?q=1, then text";
    assert_eq!(
        emu::hyperlink_at(line, 12),
        Some("https://example.org/path?q=1")
    );
    assert_eq!(emu::hyperlink_at(line, 0), None);
    let mut s = Screen::new(12, 4, 10);
    s.feed(b"https://example.org/path");
    assert_eq!(
        s.hyperlink(3, 1).as_deref(),
        Some("https://example.org/path")
    );
    assert_eq!(emu::html("<&>\""), "<pre>&lt;&amp;&gt;&quot;</pre>");
}
#[test]
fn mouse_packets() {
    assert_eq!(
        emu::mouse(
            TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE,
            0,
            79,
            23,
            16,
            false
        ),
        b"\x1b[<16;80;24M"
    );
    assert_eq!(
        emu::mouse(
            TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE,
            0,
            0,
            0,
            0,
            true
        ),
        b"\x1b[<0;1;1m"
    );
    assert_eq!(
        emu::mouse(TermMode::MOUSE_REPORT_CLICK, 0, 0, 0, 0, false),
        b"\x1b[M !!"
    );
    assert!(emu::mouse(TermMode::empty(), 0, 0, 0, 0, false).is_empty());
}

#[test]
fn x10_and_utf8_mouse_and_styled_html() {
    let mut s = Screen::new(40, 3, 10);
    s.feed(b"\x1b[?9h");
    assert!(s.x10_mouse());
    assert!(s.mode().contains(TermMode::MOUSE_REPORT_CLICK));
    s.feed(b"\x1b[?1000h");
    assert!(!s.x10_mouse());
    s.feed(b"\x1b[?9h\x1b[?9l");
    assert!(!s.x10_mouse());
    let packet = emu::mouse(
        TermMode::MOUSE_REPORT_CLICK | TermMode::UTF8_MOUSE,
        0,
        300,
        0,
        0,
        false,
    );
    assert_eq!(
        String::from_utf8(packet).unwrap(),
        format!("\x1b[M {}!", char::from_u32(333).unwrap())
    );
    s.feed(b"\x1b[1;31mred<\x1b[0m");
    s.select_all();
    let html = s.html(true, |_, _| (255, 0, 0), false).unwrap();
    assert!(html.contains("color:#ff0000"));
    assert!(html.contains("font-weight:bold"));
    assert!(html.contains("&lt;"));
}

#[test]
fn quoting_round_trips_backslash_quote_and_space_combinations() {
    // Independent decoder of the documented Microsoft argument splitting rules.
    fn decode(text: &str) -> Vec<String> {
        let chars = text.chars().collect::<Vec<_>>();
        let mut cursor = 0;
        let mut result = Vec::new();
        while cursor < chars.len() {
            while cursor < chars.len() && chars[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            if cursor == chars.len() {
                break;
            }
            let mut word = String::new();
            let mut quoted = false;
            while cursor < chars.len() && (quoted || !chars[cursor].is_ascii_whitespace()) {
                let mut slashes = 0;
                while cursor < chars.len() && chars[cursor] == '\\' {
                    slashes += 1;
                    cursor += 1;
                }
                if chars.get(cursor) == Some(&'"') {
                    word.extend(std::iter::repeat_n('\\', slashes / 2));
                    if slashes % 2 == 1 {
                        word.push('"');
                    } else {
                        quoted = !quoted;
                    }
                    cursor += 1;
                } else {
                    word.extend(std::iter::repeat_n('\\', slashes));
                    if let Some(&c) = chars.get(cursor) {
                        word.push(c);
                        cursor += 1;
                    }
                }
            }
            result.push(word);
        }
        result
    }
    for a in ["", " ", "\\", "\"", "x", "界", "\t"] {
        for b in ["", " ", "\\", "\"", "x"] {
            for c in ["", " ", "\\", "\"", "x"] {
                let value = format!("{a}{b}{c}");
                let argv = vec!["program.exe".to_owned(), value];
                assert_eq!(decode(&emu::quoting::command_line(&argv)), argv);
            }
        }
    }
}
#[test]
fn coordinates_survive_history_eviction_and_wrapped_hyperlinks_have_cells() {
    let mut s = Screen::new(10, 3, 2);
    for _ in 0..20 {
        s.feed(b"line\r\n");
    }
    assert_eq!(s.cursor(), (0, 20));
    assert_eq!(s.range(19, 0, 19, 10), "line");
    assert_eq!(s.range(0, 0, 0, 10), "");
    s.feed(b"\x1b[?1049h");
    assert_eq!(s.cursor(), (0, 2));
    s.feed(b"\x1b[?1049l");
    assert_eq!(s.cursor(), (0, 20));
    let mut s = Screen::new(12, 4, 10);
    s.feed(b"https://example.org/path");
    let points = s.hyperlink_points("https://example.org/path");
    assert_eq!(points.len(), 24);
    assert!(points.contains(&emu::index::Point::new(Line(1), Column(3))));
}

#[test]
fn reset_flags_preserve_content_cursor_and_custom_tabs() {
    let mut s = Screen::new(16, 4, 100);
    s.feed(b"\x1b[3g\x1b[1;4H\x1bH\x1b[H\tX");
    assert_eq!(s.cursor(), (4, 0));
    let content = s.viewport_text();
    s.reset(false, false);
    assert_eq!(s.viewport_text(), content);
    assert_eq!(s.cursor(), (4, 0));
    s.feed(b"\x1b[H\tY");
    assert_eq!(s.cursor(), (4, 0));
    assert_eq!(s.range(0, 3, 0, 4), "Y");
    s.reset(true, false);
    s.feed(b"\x1b[H\tZ");
    assert_eq!(s.cursor(), (9, 0));
    assert_eq!(s.range(0, 8, 0, 9), "Z");
    s.reset(true, true);
    assert_eq!(s.cursor(), (0, 0));
    assert!(s.viewport_text().trim().is_empty());
}
