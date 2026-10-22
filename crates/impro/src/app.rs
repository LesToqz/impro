//! Application lifecycle and a first-paint PNG helper reused by later window tasks.
use crate::MainWindow;
use gtk::prelude::*;
use impro_core::{identity, settings};
use std::{
    cell::Cell,
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
        let owner = MainWindow::with_values(app, &root, preferences.clone());
        let window = &owner.window;
        if let Some(path) = qa_path.as_ref() {
            install_screenshot_helper(window, path.clone(), started, screenshot_failed.clone());
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
/// Attach before show_all(). Captures the first completed paint, with no fixed sleep/timer.
/// Later window implementations can reuse this directly.
pub fn install_screenshot_helper(
    window: &gtk::ApplicationWindow,
    path: PathBuf,
    started: Instant,
    failed: Rc<Cell<bool>>,
) {
    let armed = Rc::new(Cell::new(false));
    // map/after-paint can occur before GtkWidget has drawn its first contents.
    // Queue one idle from draw: GDK has ended its paint buffer before it executes.
    window.connect_draw(move |window, _| {
        if armed.replace(true) {
            return glib::Propagation::Proceed;
        }
        let weak = window.downgrade();
        let path = path.clone();
        let failed = failed.clone();
        glib::idle_add_local_once(move || {
            let Some(window) = weak.upgrade() else {
                return;
            };
            window.display().sync();
            let elapsed = started.elapsed();
            if let Err(error) = capture_window(&window, &path) {
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
