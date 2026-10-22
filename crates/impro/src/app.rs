//! Application lifecycle and a first-paint PNG helper reused by later window tasks.
use crate::MainWindow;
use gtk::prelude::*;
use impro_core::identity;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};
pub fn run(root: PathBuf, started: Instant) -> glib::ExitCode {
    impro_ui::icons::init();
    // Read preferences before GTK starts dispatching UI events; no startup file IO in callbacks.
    let preferences = crate::preferences::load_values();
    timing("preferences", started);
    let qa_path = std::env::var_os(identity::QA_SCREENSHOT_ENV).map(PathBuf::from);
    let flags = if qa_path.is_some() {
        gio::ApplicationFlags::NON_UNIQUE
    } else {
        gio::ApplicationFlags::FLAGS_NONE
    };
    let app = gtk::Application::new(Some(identity::APP_ID), flags);
    timing("application", started);
    app.connect_startup(move |_| timing("gtk-startup", started));
    let failed = Rc::new(Cell::new(false));
    let preferences = Rc::new(RefCell::new(preferences));
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
        timing("activate", started);
        let owner = MainWindow::with_values(app, &root, preferences.borrow().clone());
        timing("window-built", started);
        let window = &owner.window;
        let state = preferences.clone();
        let keep = RefCell::new(Some(owner.clone()));
        window.connect_destroy(move |_| {
            if let Some(owner) = keep.borrow_mut().take() {
                *state.borrow_mut() = owner.values();
            }
        });
        if let Some(path) = qa_path.as_ref() {
            let weak = owner.weak();
            install_screenshot_helper_when(
                window,
                path.clone(),
                started,
                screenshot_failed.clone(),
                move || {
                    weak.upgrade()
                        .is_some_and(|owner| MainWindow(owner).layout_ready())
                },
            );
        }
        window.show_all();
        timing("shown", started);
        window.present();
        timing("presented", started);
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
    install_screenshot_helper_when(window, path, started, failed, || true);
}

fn install_screenshot_helper_when(
    window: &gtk::ApplicationWindow,
    path: PathBuf,
    started: Instant,
    failed: Rc<Cell<bool>>,
    ready: impl Fn() -> bool + 'static,
) {
    let armed = Rc::new(Cell::new(false));
    // map/after-paint can occur before GtkWidget has drawn its first contents.
    // Queue one idle from draw: GDK has ended its paint buffer before it executes.
    window.connect_draw(move |window, _| {
        if !ready() || armed.replace(true) {
            return glib::Propagation::Proceed;
        }
        timing("draw", started);
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

fn timing(phase: &str, started: Instant) {
    if std::env::var_os("IMPRO_QA_TRACE").is_some() || std::env::var_os("IMPRO_QA_TIMING").is_some()
    {
        eprintln!(
            "app-{phase}-ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}
