//! Standalone settings window.
//!
//! ```sh
//! cargo run -p impro-settings-ui --example settings_demo -- [PAGE] [--theme impro-dark]
//! cargo run -p impro-settings-ui --example settings_demo -- --timing [--prebuild]
//! (--height N [--width N] makes tall or narrow shots)
//! xvfb-run -a cargo run --release -p impro-settings-ui --example settings_demo -- \
//!     --theme impro-light --shots .ceo/shots --prefix rust-light-
//! ```
//! Uses a throw-away settings directory. `--timing` prints first present, first visit and
//! revisit of every page, and the cost of search keystrokes; `--shots DIR` writes one PNG
//! per page (plus a search view) and exits.
use gtk::prelude::*;
use impro_core::themes;
use impro_settings_ui::{SettingsDialog, PAGES};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn shot(window: &gtk::Window, path: &std::path::Path) {
    use glib::translate::*;
    let Some(gdk_window) = window.window() else {
        return;
    };
    let (w, h) = (gdk_window.width(), gdk_window.height());
    // SAFETY: the window is realized; the call returns a new reference or NULL.
    let pixbuf: Option<gdk_pixbuf::Pixbuf> = unsafe {
        from_glib_full(gdk::ffi::gdk_pixbuf_get_from_window(
            gdk_window.to_glib_none().0,
            0,
            0,
            w,
            h,
        ))
    };
    if let Some(pixbuf) = pixbuf {
        if let Err(error) = pixbuf.savev(path, "png", &[]) {
            eprintln!("cannot write {}: {error}", path.display());
        }
    }
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let theme = value("--theme").unwrap_or_else(|| "impro-dark".into());
    let shots = value("--shots");
    let prefix = value("--prefix").unwrap_or_default();
    let timing = args.iter().any(|a| a == "--timing");
    let page = args
        .iter()
        .find(|a| PAGES.iter().any(|p| p.0 == a.as_str()))
        .cloned();
    let temp = std::env::temp_dir().join(format!("impro-settings-demo-{}", std::process::id()));
    std::env::set_var("XDG_CONFIG_HOME", &temp);
    std::env::set_var("GDK_BACKEND", "x11");
    gtk::init().expect("display");
    impro_ui::init();
    impro_settings_ui::init();
    impro_ui::motion::set_reduce_motion(shots.is_some() || timing);
    impro_ui::apply_style(Some(themes::get_theme(Some(&theme)))).expect("theme");

    // In the application the settings window opens over an already mapped main window, so
    // timing mode measures against a warm process, like the real first `open_settings`.
    let parent = gtk::Window::new(gtk::WindowType::Toplevel);
    if timing {
        parent.set_default_size(1200, 800);
        parent.add(&gtk::Label::new(Some("main window")));
        parent.show_all();
        pump(300);
    }
    let started = Instant::now();
    let dialog = SettingsDialog::new(timing.then_some(&parent), Some(&theme), None);
    // Timing mode isolates each cost; the background page building is shown separately.
    let prebuild = args.iter().any(|a| a == "--prebuild");
    dialog.set_prebuild(prebuild || !timing);
    if let Some(height) = value("--height").and_then(|h| h.parse::<i32>().ok()) {
        let width = value("--width")
            .and_then(|w| w.parse::<i32>().ok())
            .unwrap_or(900);
        dialog.set_default_size(width, height); // Tall shots show whole pages below the fold.
    }
    let built = ms(started);
    let draws = Rc::new(Cell::new(0u32));
    let counter = draws.clone();
    let origin = started;
    let first_draw: Rc<Cell<Option<f64>>> = Rc::default();
    let first = first_draw.clone();
    dialog.connect_draw(move |_, _| {
        counter.set(counter.get() + 1);
        if first.get().is_none() {
            first.set(Some(ms(origin)));
        }
        if std::env::var_os("SETTINGS_DEMO_TRACE").is_some() {
            eprintln!("draw #{} at {:.1} ms", counter.get(), ms(origin));
        }
        glib::Propagation::Proceed
    });
    dialog.connect_theme_selected(|id| println!("theme-selected {id}"));
    dialog.connect_font_size_changed(|key, size| println!("font-size-changed {key} {size}"));
    dialog.connect_setting_changed(|key, value| println!("setting-changed {key} {value}"));
    dialog.connect_save_failed(|path| eprintln!("could not save settings to {}", path.display()));
    let frame_cost = Rc::new(Cell::new(0.0f64));
    let present_start = Instant::now();
    dialog.present(page.as_deref());
    let present_call = ms(present_start);
    // Cost of the frame itself (update -> after-paint), without the wait for the frame clock.
    if let Some(clock) = dialog.frame_clock() {
        let begun: Rc<Cell<Option<Instant>>> = Rc::default();
        let start = begun.clone();
        clock.connect_local("update", false, move |_| {
            start.set(Some(Instant::now()));
            None
        });
        let cost = frame_cost.clone();
        clock.connect_local("after-paint", false, move |_| {
            if let Some(begin) = begun.get() {
                cost.set(ms(begin));
            }
            None
        });
    }
    let frame = |dialog: &SettingsDialog| {
        let before = draws.get();
        dialog.queue_draw();
        let start = Instant::now();
        while draws.get() == before && start.elapsed() < Duration::from_millis(500) {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
            std::thread::sleep(Duration::from_micros(200));
        }
    };
    if timing {
        // The frame clock of a bare Xvfb (no window manager) stalls ~170 ms after the first
        // frame; warm up so the later numbers measure our work, not that stall.
        pump(if prebuild { 2500 } else { 500 });
        println!(
            "new: {built:.1} ms; present() call: {present_call:.1} ms; new + present -> first draw: {:.1} ms",
            first_draw.get().unwrap_or(f64::NAN)
        );
        let mut revisit = Vec::new();
        for (id, _, _) in PAGES {
            let start = Instant::now();
            dialog.select_page(id);
            let select = ms(start);
            frame(&dialog);
            println!(
                "first visit {id}: handler {select:.2} ms, frame {:.1} ms (wall to drawn frame {:.1} ms)",
                frame_cost.get(),
                ms(start)
            );
        }
        for (id, _, _) in PAGES {
            let start = Instant::now();
            dialog.select_page(id);
            let select = ms(start);
            frame(&dialog);
            revisit.push(select);
            println!(
                "revisit {id}: handler {select:.2} ms, frame {:.1} ms (wall to drawn frame {:.1} ms)",
                frame_cost.get(),
                ms(start)
            );
        }
        let start = Instant::now();
        dialog.build_all_pages();
        println!("build remaining pages at once: {:.1} ms", ms(start));
        let query = "terminal cursor blink";
        let mut costs = Vec::new();
        for n in 1..=query.len() {
            let start = Instant::now();
            dialog.search_entry().set_text(&query[..n]);
            costs.push(ms(start));
            if std::env::var_os("SETTINGS_DEMO_TRACE").is_some() {
                eprintln!("key {n}: {:.2} ms", costs[costs.len() - 1]);
            }
            frame(&dialog);
        }
        let max = costs.iter().cloned().fold(0.0, f64::max);
        let mean = costs.iter().sum::<f64>() / costs.len() as f64;
        println!("search keystroke (set_text -> filter all rows): mean {mean:.2} ms, max {max:.2} ms over {} keys", costs.len());
        dialog.search_entry().set_text("");
        return;
    }
    frame(&dialog);
    if let Some(dir) = shots {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("shot directory");
        dialog.build_all_pages();
        for (id, _, _) in PAGES {
            dialog.select_page(id);
            pump(150);
            frame(&dialog);
            shot(&dialog, &dir.join(format!("{prefix}{id}.png")));
        }
        dialog.select_page("appearance");
        dialog.search_entry().set_text("cursor");
        pump(150);
        frame(&dialog);
        shot(&dialog, &dir.join(format!("{prefix}search-cursor.png")));
        dialog.search_entry().set_text("");
        return;
    }
    dialog.connect_hide(|_| gtk::main_quit());
    gtk::main();
    impro_settings_ui::flush();
    let _ = std::fs::remove_dir_all(temp);
}
