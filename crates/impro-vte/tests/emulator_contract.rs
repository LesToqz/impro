//! Compare portable extraction with the Unix wrapper on the same small grid.
#![cfg(all(unix, feature = "emulator"))]
use gtk::prelude::*;
use impro_vte::{emu::Screen, Format, Terminal};
use std::time::{Duration, Instant};
#[test]
fn extraction_matches_unix_selection_viewport_and_ranges() {
    gtk::init().unwrap();
    for bytes in [
        "abc",
        "ab界cdef\r\nnext",
        "first\r\nsecond\r\nthird\r\nfourth\r\nfifth",
    ] {
        let t = Terminal::new();
        t.set_size(6, 4);
        let window = gtk::OffscreenWindow::new();
        window.add(&t);
        window.set_default_size(
            6 * t.char_width() as i32 + 2,
            4 * t.char_height() as i32 + 2,
        );
        window.show_all();
        t.feed(bytes.as_bytes());
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            glib::MainContext::default().iteration(false);
            if t.text_format(Format::Text)
                .is_some_and(|text| !text.trim().is_empty())
            {
                break;
            }
            assert!(Instant::now() < deadline, "VTE feed timed out");
        }
        let mut screen = Screen::new(t.column_count() as usize, t.row_count() as usize, 10_000);
        screen.feed(bytes.as_bytes());
        t.select_all();
        screen.select_all();
        let native = t.text_selected(Format::Text).unwrap().to_string();
        let portable = screen.selection().unwrap();
        assert_eq!(portable, native, "selection: {bytes:?}");
        assert_eq!(
            screen.viewport_text(),
            t.text_format(Format::Text).unwrap().as_str(),
            "viewport: {bytes:?}"
        );
        for (a, b, c, d) in [
            (0, 0, 0, 6),
            (0, 2, 1, 4),
            (0, 0, 1, 3),
            (1, 1, 2, 2),
            (0, 3, 0, 4),
        ] {
            let native = t.text_range_format(Format::Text, a, b, c, d).unwrap();
            assert_eq!(
                screen.range(a, b, c, d),
                native.as_str(),
                "range ({a},{b},{c},{d}): {bytes:?}"
            );
        }
        t.reset(false, false);
        eprintln!(
            "reset retained native {:?} cursor {:?}",
            t.text_format(Format::Text),
            t.cursor_position()
        );
        unsafe {
            window.destroy();
        }
    }
}
