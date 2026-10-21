//! Application lifecycle and a first-paint PNG helper reused by later window tasks.
use gtk::prelude::*;
use impro_core::{identity, settings, themes};
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};
pub fn run(root: PathBuf, started: Instant) -> glib::ExitCode {
    // Read preferences before GTK starts dispatching UI events; no startup file IO in callbacks.
    let preferences = settings::get_values();
    let qa_path = std::env::var_os(identity::QA_SCREENSHOT_ENV).map(PathBuf::from);
    let flags = if qa_path.is_some() {
        gio::ApplicationFlags::NON_UNIQUE
    } else {
        gio::ApplicationFlags::FLAGS_NONE
    };
    let app = gtk::Application::new(Some(identity::APP_ID), flags);
    let failed = Rc::new(Cell::new(false));
    let startup_failed = failed.clone();
    app.connect_startup(move |app| {
        impro_ui::init();
        impro_ui::motion::set_reduce_motion(
            preferences["reduce_motion"].as_bool().unwrap_or(false),
        );
        let theme = themes::get_theme(preferences["theme"].as_str());
        if let Err(error) = impro_ui::apply_theme(
            theme,
            preferences["accent"].as_str(),
            preferences["ui_font"].as_str(),
            preferences["editor_font"].as_str(),
        ) {
            eprintln!("impro: {error}");
            startup_failed.set(true);
            app.quit();
        }
    });
    let activation_failed = failed.clone();
    let screenshot_failed = failed.clone();
    app.connect_activate(move |app| {
        if activation_failed.get() {
            app.quit();
            return;
        }
        if let Some(window) = app.active_window() {
            window.present();
            return;
        }
        let window = placeholder_window(app, &root);
        if let Some(path) = qa_path.as_ref() {
            install_screenshot_helper(&window, path.clone(), started, screenshot_failed.clone());
        }
        window.show_all();
        window.present();
    });
    let exit = app.run_with_args::<&str>(&[]);
    if failed.get() {
        glib::ExitCode::FAILURE
    } else {
        exit
    }
}
fn placeholder_window(app: &gtk::Application, root: &Path) -> gtk::ApplicationWindow {
    let window = gtk::ApplicationWindow::new(app);
    window.set_title(identity::WINDOW_TITLE);
    window.set_default_size(1200, 800);
    let layout = gtk::Box::new(gtk::Orientation::Vertical, 12);
    layout.set_halign(gtk::Align::Center);
    layout.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some(identity::APP_NAME));
    let folder = gtk::Label::new(Some(&root.display().to_string()));
    layout.pack_start(&title, false, false, 0);
    layout.pack_start(&folder, false, false, 0);
    window.add(&layout);
    window
}
/// Attach before show_all(). Captures the first completed paint, with no fixed sleep/timer.
/// Later window implementations can reuse this directly.
pub fn install_screenshot_helper(
    window: &gtk::ApplicationWindow,
    path: PathBuf,
    started: Instant,
    failed: Rc<Cell<bool>>,
) {
    let armed = Rc::new(Cell::new(false));
    window.connect_map_event(move |window, _| {
        if armed.replace(true) {
            return glib::Propagation::Proceed;
        }
        let Some(clock) = window.frame_clock() else {
            return glib::Propagation::Proceed;
        };
        let weak = window.downgrade();
        let path = path.clone();
        let failed = failed.clone();
        let handler = Rc::new(RefCell::new(None));
        let pending = handler.clone();
        let id = clock.connect_after_paint(move |clock| {
            let signal = pending.borrow_mut().take();
            if let Some(signal) = signal {
                clock.disconnect(signal);
            }
            let Some(window) = weak.upgrade() else { return };
            let elapsed = started.elapsed();
            let result = capture_window(&window, &path);
            if let Err(error) = result {
                eprintln!("impro: screenshot failed: {error}");
                failed.set(true);
            }
            if std::env::var_os(identity::QA_TIMING_ENV).is_some() {
                println!("first-paint-ms={:.3}", elapsed.as_secs_f64() * 1000.0);
            }
            if let Some(app) = window.application() {
                app.quit();
            }
        });
        *handler.borrow_mut() = Some(id);
        clock.request_phase(gdk::FrameClockPhase::AFTER_PAINT);
        glib::Propagation::Proceed
    });
}
pub fn capture_window(window: &impl IsA<gtk::Widget>, path: &Path) -> Result<(), String> {
    let window = window.as_ref();
    let native = window.window().ok_or("window has no native surface")?;
    let pixbuf = native
        .pixbuf(0, 0, window.allocated_width(), window.allocated_height())
        .ok_or("cannot read the window surface")?;
    pixbuf.savev(path, "png", &[]).map_err(|e| e.to_string())
}
