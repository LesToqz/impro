//! Live-preview Markdown never lets GTK hit-test a layout it aborts on.
//!
//! GTK 3.24 ends the process with `Gtk-ERROR: Byte index N is off the end of the
//! line` when a pixel in the `pixels-below-lines` gap of a line holding
//! invisible text is hit-tested (pointer motion, clicks, drags, selection, caret
//! movement, and the renderer's own viewport probe on every frame). That cannot
//! be caught, so each scenario runs in a child process (this test binary,
//! re-executed) that sweeps a real `EditorPanel` with synthetic input; a scenario
//! passes only if the child exits 0 and reports `sweep ok`.
//!
//! Run one scenario by hand: `IMPRO_HIT_SCENARIO=<name> cargo test -p impro-markdown
//! --test test_hit_safety -- --ignored --exact child --nocapture`.
mod support;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_editor::EditorPanel;
use impro_markdown::livemd;
use std::{path::PathBuf, process::Command};
use support::*;

const MIXED: &str = "# Title\n\nIntro with **bold**, `code` and a [link](https://example.org).\n\n## Section\n\n- one\n  - nested\n    - deeper\n- two\n\n1. first\n2. second\n\n- [ ] todo\n- [x] done\n\n> quote\n> > nested\n\n---\n\n### Code\n\n```python\nx = 1\ny = 2\n```\n\nafter code\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\nlast words\n";

fn long() -> String {
    ["wrapped"; 45].join(" ")
}
/// GTK's abort depends on how long a line and its neighbours are, so cover many
/// lengths for each hidden prefix.
fn varied() -> String {
    let mut out = String::new();
    for prefix in ["- ", "# ", "> ", "1. ", "- [ ] ", "## "] {
        for length in 1..=10 {
            out.push_str(&format!("{prefix}{}\n", "x".repeat(length)));
        }
    }
    out
}

/// `(name, markdown, kind, theme)`; kind `note` lives under `.impro/Notes`, `project` is a plain project `.md`.
fn scenarios() -> Vec<(&'static str, String, &'static str, &'static str)> {
    let long = long();
    vec![
        ("two_headings", "# one\n# two\n".into(), "note", "impro-dark"),
        ("headings_levels", "# one\n## two\n### three\n#### four\n".into(), "note", "impro-light"),
        ("heading_then_plain", "# one\nplain\n".into(), "note", "impro-dark"),
        ("bullet_list", "- one\n- two\n".into(), "note", "impro-dark"),
        ("bullet_list_light", "- one\n- two\n- three\n".into(), "note", "impro-light"),
        ("numbered_list", "1. one\n2. two\n3. three\n".into(), "note", "impro-dark"),
        ("checklist", "- [ ] one\n- [x] two\n".into(), "note", "impro-dark"),
        ("nested_list", "- a\n  - b\n    - c\n  - d\n- e\n".into(), "note", "impro-dark"),
        ("list_after_blank", "# one\n\n- a\n\n- b\n".into(), "note", "impro-dark"),
        ("quote", "> one\n> two\n".into(), "note", "impro-dark"),
        ("nested_quote", "> a\n> > b\n> > > c\n> d\n".into(), "note", "impro-light"),
        ("rule_mid", "before\n\n---\n\nafter\n".into(), "note", "impro-dark"),
        ("rule_first", "---\n\nafter\n".into(), "note", "impro-dark"),
        ("rule_last", "before\n\n---\n".into(), "note", "impro-dark"),
        ("rule_last_no_newline", "before\n\n***".into(), "note", "impro-dark"),
        ("rules_adjacent", "---\n---\n- a\n".into(), "note", "impro-dark"),
        ("code_after_list", "- a\n- b\n```\ncode\n```\nafter\n".into(), "note", "impro-dark"),
        ("unclosed_code", "- a\n```py\nx = 1\ny = 2\n".into(), "note", "impro-dark"),
        (
            "table_beside_hidden_prefix",
            "- a\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n# h\n> q\n".into(),
            "note",
            "impro-dark",
        ),
        ("image_placeholder", "![pic](pic.png)\n- a\n![gone](missing.png)\n# h\n".into(), "note", "impro-dark"),
        ("varied_lengths", varied(), "note", "impro-dark"),
        ("mixed_note", MIXED.into(), "note", "impro-dark"),
        ("mixed_note_light", MIXED.into(), "note", "impro-light"),
        ("mixed_project_md", MIXED.into(), "project", "impro-dark"),
        ("project_md_list", "# one\n\n- a\n- b\n\n> q\n".into(), "project", "impro-light"),
        ("wrapped_list_item", format!("- {long}\n- short\n  - {long}\n"), "note", "impro-dark"),
        ("wrapped_quote_heading", format!("> {long}\n# {long}\n"), "note", "impro-dark"),
        (
            "unicode",
            "- é 日本 😀\n- é 日本語 😀😀 text\n# 日本 😀\n> é\n".into(),
            "note",
            "impro-dark",
        ),
    ]
}

fn run_scenario(name: &str) -> (Option<i32>, String, String) {
    let exe = std::env::current_exe().expect("test binary");
    let output = Command::new(exe)
        .args(["--ignored", "--exact", "child", "--nocapture", "--test-threads=1"])
        .env("IMPRO_HIT_SCENARIO", name)
        .env("GDK_BACKEND", "x11")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("spawn child");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

fn check(name: &str) {
    let (code, out, err) = run_scenario(name);
    let tail: Vec<&str> = err.trim().lines().rev().take(12).collect();
    assert_eq!(
        code,
        Some(0),
        "{name}: child exited {code:?}\n{}",
        tail.into_iter().rev().collect::<Vec<_>>().join("\n")
    );
    assert!(out.contains("sweep ok"), "{name}: {out}");
}

macro_rules! scenario_tests {
    ($($name:ident),* $(,)?) => {
        $(#[test] fn $name() { check(stringify!($name)); })*
    };
}
scenario_tests!(
    two_headings,
    headings_levels,
    heading_then_plain,
    bullet_list,
    bullet_list_light,
    numbered_list,
    checklist,
    nested_list,
    list_after_blank,
    quote,
    nested_quote,
    rule_mid,
    rule_first,
    rule_last,
    rule_last_no_newline,
    rules_adjacent,
    code_after_list,
    unclosed_code,
    table_beside_hidden_prefix,
    image_placeholder,
    varied_lengths,
    mixed_note,
    mixed_note_light,
    mixed_project_md,
    project_md_list,
    wrapped_list_item,
    wrapped_quote_heading,
    unicode,
);

#[test]
fn every_scenario_has_a_test() {
    assert_eq!(scenarios().len(), 28);
}

// -- child process: drive a real EditorPanel with synthetic input -------------

fn motion_event(window: &gdk::Window, x: f64, y: f64, state: gdk::ModifierType, time: u32) -> gdk::Event {
    use glib::translate::*;
    let mut event = gdk::Event::new(gdk::EventType::MotionNotify);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).motion.window = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_full(window);
        (*raw).motion.send_event = 1;
        (*raw).motion.x = x;
        (*raw).motion.y = y;
        (*raw).motion.state = state.bits();
        (*raw).motion.time = time;
    }
    if let Some(seat) = gdk::Display::default().and_then(|d| d.default_seat()) {
        event.set_device(seat.pointer().as_ref());
    }
    event
}
fn press_event(
    kind: gdk::EventType,
    window: &gdk::Window,
    x: f64,
    y: f64,
    state: gdk::ModifierType,
    time: u32,
) -> gdk::Event {
    use glib::translate::*;
    let event = button_event_at(kind, window, x, y, 1);
    let mut event = event;
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).button.state = state.bits();
        (*raw).button.time = time;
    }
    event
}

#[test]
#[ignore = "run through the scenario tests (child process)"]
fn child() {
    let name = std::env::var("IMPRO_HIT_SCENARIO").unwrap_or_default();
    let Some((_, text, kind, theme_id)) = scenarios().into_iter().find(|s| s.0 == name) else {
        return;
    };
    run(move || sweep(&name, &text, kind, theme_id));
}

fn sweep(name: &str, text: &str, kind: &str, theme_id: &str) {
    let config = tempfile::tempdir().unwrap();
    std::env::set_var("XDG_CONFIG_HOME", config.path());
    impro_ui::init();
    impro_editor::init();
    impro_markdown::init();
    impro_ui::apply_style(None).unwrap();
    impro_ui::motion::set_reduce_motion(true);
    let folder = tempfile::tempdir().unwrap();
    let path: PathBuf = folder.path().join(if kind == "note" { ".impro/Notes/n.md" } else { "guide.md" });
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    let pixbuf = gdk_pixbuf::Pixbuf::new(gdk_pixbuf::Colorspace::Rgb, false, 8, 40, 30).unwrap();
    pixbuf.fill(0x3366_99ff);
    pixbuf.savev(path.parent().unwrap().join("pic.png"), "png", &[]).unwrap();
    let root = folder.path().canonicalize().unwrap();
    let panel = EditorPanel::new(Some(&root));
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(760, 420);
    window.add(&panel);
    window.show_all();
    let theme = impro_core::themes::get_theme(Some(theme_id));
    impro_ui::apply_theme(theme, Some("#c15d84"), None, None).unwrap();
    panel.open_file(&path);
    poll(|| !panel.is_busy());
    let tab = panel.tab_for_path(&path).expect("tab");
    panel.set_syntax_style(&theme.syntax);
    panel.set_chrome(&theme.chrome);
    let live = livemd::for_view(&tab.view).expect("live renderer");
    let (view, buffer) = (tab.view.clone(), tab.buffer.clone());
    let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while std::time::Instant::now() < until && (live.pending() || !view.is_mapped()) {
        pump(5);
    }
    pump(150);
    assert!(live.enabled(), "{name}: live preview did not start");
    let editable = view.is_editable();
    let adjustment = tab.source_scroll.vadjustment();
    let display = gdk::Display::default().unwrap();
    let keymap = gdk::Keymap::for_display(&display).unwrap();
    let mut probes = 0u64;
    let mut clock = 1000u32;
    let text_window = || TextViewExt::window(&view, gtk::TextWindowType::Text).expect("text window");

    let mut send_key = |keyval: gdk::keys::Key, state: gdk::ModifierType, clock: &mut u32| {
        use glib::translate::*;
        for kind in [gdk::EventType::KeyPress, gdk::EventType::KeyRelease] {
            let mut event = gdk::Event::new(kind);
            *clock += 40;
            unsafe {
                let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
                (*raw).key.window = ToGlibPtr::<*mut gdk::ffi::GdkWindow>::to_glib_full(&text_window());
                (*raw).key.send_event = 1;
                (*raw).key.keyval = *keyval;
                (*raw).key.state = state.bits();
                (*raw).key.time = *clock;
                if let Some(first) = keymap.entries_for_keyval(*keyval).first() {
                    (*raw).key.hardware_keycode = first.keycode() as u16;
                }
            }
            if let Some(seat) = display.default_seat() {
                event.set_device(seat.keyboard().as_ref());
            }
            view.event(&event);
        }
        pump(4);
    };

    let edges = |probes: &mut u64| {
        let last = buffer.end_iter();
        let (top, height) = view.line_yrange(&last);
        let left = view.left_margin();
        let right = view.allocated_width() - view.right_margin();
        for y in -4..top + height + 24 {
            for x in [-3, 0, 1, left - 1, left, left + 9, (left + right) / 2, right - 1, right + 6] {
                let _ = view.iter_at_location(x, y);
                let _ = view.iter_at_position(x, y);
                *probes += 2;
            }
        }
    };
    let hover = |probes: &mut u64, clock: &mut u32| {
        let height = view.allocated_height();
        let mut y = 0;
        while y < height {
            for x in [0, 17, 45, 96, 240, 480, 700] {
                *clock += 40;
                let event = motion_event(&text_window(), f64::from(x), f64::from(y), gdk::ModifierType::empty(), *clock);
                view.event(&event);
                *probes += 1;
            }
            y += 3;
        }
        pump(20);
    };
    let line_targets = || -> Vec<(i32, i32)> {
        let mut targets = Vec::new();
        let visible = view.visible_rect();
        for number in 0..buffer.line_count() {
            let (top, height) = view.line_yrange(&buffer.iter_at_line(number));
            for y in [top + height / 2, top + 1.max(height - 2)] {
                let (_, wy) = view.buffer_to_window_coords(gtk::TextWindowType::Text, 0, y);
                if (0..visible.height()).contains(&wy) {
                    for dx in [-6, 30] {
                        targets.push((view.left_margin() + dx, wy));
                    }
                }
            }
        }
        targets
    };
    let clicks = |probes: &mut u64, clock: &mut u32| {
        let targets = line_targets();
        let none = gdk::ModifierType::empty();
        for &(x, y) in &targets {
            // Three quick presses make a single, double and triple click in GTK's multi-press gesture.
            for _ in 0..3 {
                *clock += 40;
                view.event(&press_event(gdk::EventType::ButtonPress, &text_window(), f64::from(x), f64::from(y), none, *clock));
                *clock += 40;
                view.event(&press_event(gdk::EventType::ButtonRelease, &text_window(), f64::from(x), f64::from(y), gdk::ModifierType::BUTTON1_MASK, *clock));
            }
            *clock += 40;
            view.event(&press_event(gdk::EventType::ButtonPress, &text_window(), f64::from(x), f64::from(y), gdk::ModifierType::SHIFT_MASK, *clock));
            *clock += 40;
            view.event(&press_event(
                gdk::EventType::ButtonRelease,
                &text_window(),
                f64::from(x),
                f64::from(y),
                gdk::ModifierType::SHIFT_MASK | gdk::ModifierType::BUTTON1_MASK,
                *clock,
            ));
            *probes += 4;
            pump(3);
        }
        for (&(x1, y1), &(x2, y2)) in targets.iter().zip(targets.iter().rev()) {
            *clock += 40;
            view.event(&press_event(gdk::EventType::ButtonPress, &text_window(), f64::from(x1), f64::from(y1), none, *clock));
            for step in 1..=6 {
                *clock += 40;
                let (x, y) = (x1 + (x2 - x1) * step / 6, y1 + (y2 - y1) * step / 6);
                view.event(&motion_event(&text_window(), f64::from(x), f64::from(y), gdk::ModifierType::BUTTON1_MASK, *clock));
            }
            *clock += 40;
            view.event(&press_event(gdk::EventType::ButtonRelease, &text_window(), f64::from(x2), f64::from(y2), gdk::ModifierType::BUTTON1_MASK, *clock));
            *probes += 7;
        }
        pump(50);
    };
    let navigate = |probes: &mut u64, clock: &mut u32, send_key: &mut dyn FnMut(gdk::keys::Key, gdk::ModifierType, &mut u32)| {
        let none = gdk::ModifierType::empty();
        let mut sequence = vec![k::Down; 7];
        sequence.extend(vec![k::Up; 7]);
        for _ in 0..2 {
            sequence.extend([k::Page_Down, k::Page_Up, k::End, k::Home, k::Right, k::Left]);
        }
        for key in sequence {
            send_key(key, none, clock);
        }
        for key in [k::Down, k::Up, k::End, k::Home, k::Page_Down, k::Page_Up] {
            for _ in 0..3 {
                send_key(key, gdk::ModifierType::SHIFT_MASK, clock);
            }
        }
        send_key(k::Home, gdk::ModifierType::CONTROL_MASK, clock);
        send_key(k::End, gdk::ModifierType::CONTROL_MASK, clock);
        for (step, count) in [
            (gtk::MovementStep::DisplayLines, 1),
            (gtk::MovementStep::DisplayLines, -1),
            (gtk::MovementStep::Pages, 1),
            (gtk::MovementStep::Pages, -1),
            (gtk::MovementStep::DisplayLineEnds, 1),
            (gtk::MovementStep::DisplayLineEnds, -1),
            (gtk::MovementStep::ParagraphEnds, 1),
            (gtk::MovementStep::VisualPositions, 1),
        ] {
            for extend in [false, true] {
                for _ in 0..4 {
                    view.emit_by_name::<()>("move-cursor", &[&step, &count, &extend]);
                }
            }
        }
        for number in 0..buffer.line_count() {
            let iter = buffer.iter_at_line(number);
            view.scroll_to_iter(&mut iter.clone(), 0.0, false, 0.0, 0.0);
            view.scroll_to_iter(&mut iter.clone(), 0.1, true, 0.5, 0.5);
            buffer.place_cursor(&iter);
            view.scroll_mark_onscreen(&buffer.get_insert().unwrap());
            view.place_cursor_onscreen();
            pump(10);
            *probes += 4;
        }
    };
    let scrolling = |probes: &mut u64| {
        let upper = (adjustment.upper() - adjustment.page_size()) as i32;
        let mut value = 0;
        while value <= upper.max(1) {
            adjustment.set_value(f64::from(value));
            live.position();
            *probes += 1;
            if value % 11 == 0 {
                pump(6);
            }
            value += 2;
        }
    };
    let caret_at = |offset: Option<i32>| {
        if let (true, Some(offset)) = (editable, offset) {
            buffer.place_cursor(&buffer.iter_at_offset(offset));
            pump(100);
        }
    };
    let scrolled_hover = |probes: &mut u64, clock: &mut u32| {
        let upper = ((adjustment.upper() - adjustment.page_size()) as i32).max(0);
        for value in [0, upper / 2, upper] {
            adjustment.set_value(f64::from(value));
            pump(50);
            hover(probes, clock);
        }
        adjustment.set_value(0.0);
    };

    let length = text.chars().count() as i32;
    let mut moved = std::collections::BTreeSet::new();
    for caret in [None, Some(0), Some(length / 2), Some(length)] {
        caret_at(caret);
        edges(&mut probes);
        scrolled_hover(&mut probes, &mut clock);
    }
    caret_at(None);
    scrolling(&mut probes);
    for caret in [None, Some(length / 2)] {
        caret_at(caret);
        clicks(&mut probes, &mut clock);
    }
    for caret in [0, length / 2] {
        caret_at(Some(caret));
        let before = buffer.iter_at_mark(&buffer.get_insert().unwrap()).offset();
        navigate(&mut probes, &mut clock, &mut send_key);
        moved.insert(buffer.iter_at_mark(&buffer.get_insert().unwrap()).offset() != before);
        edges(&mut probes);
    }
    assert!(
        !editable || moved.contains(&true),
        "{name}: synthetic keys never moved the caret; the sweep proved nothing"
    );
    unsafe { window.destroy() };
    println!("sweep ok {probes}");
}
