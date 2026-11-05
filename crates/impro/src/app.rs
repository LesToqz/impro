//! Application lifecycle and a first-paint PNG helper reused by later window tasks.
use crate::{launch::Launch, MainWindow};
use gtk::prelude::*;
use impro_core::identity;
use std::{
    cell::{Cell, RefCell},
    path::{Path, PathBuf},
    rc::Rc,
    time::Instant,
};
pub fn run(launch: Launch, started: Instant) -> glib::ExitCode {
    impro_ui::icons::init();
    // The welcome page's CSS joins the theme build, which happens when the window is made.
    crate::welcome::init();
    crate::launch::seed(launch.state.clone());
    // Read preferences before GTK starts dispatching UI events; no startup file IO in callbacks.
    let preferences = crate::preferences::load_values();
    timing("preferences", started);
    let qa_path = std::env::var_os(identity::QA_SCREENSHOT_ENV).map(PathBuf::from);
    // Windows is deployed without a session bus. NON_UNIQUE disables bus-name
    // ownership and remote activation; each invocation owns its own window. On Linux a
    // plain `impro` focuses the running window, but a launch that names paths must not
    // lose them to it, so those get their own process.
    let flags = if cfg!(windows) || qa_path.is_some() || launch.has_paths {
        gio::ApplicationFlags::NON_UNIQUE
    } else {
        gio::ApplicationFlags::FLAGS_NONE
    };
    timing("application-new", started);
    let app = gtk::Application::new(Some(identity::APP_ID), flags);
    timing("application", started);
    app.connect_startup(move |_| timing("gtk-startup", started));
    // Split "application" .. "gtk-startup": registering the application is where
    // GLib/GIO set up the platform backend. Timing runs only; a failure here
    // is left to run_with_args to report as before.
    if timing_enabled() {
        let _ = app.register(gio::Cancellable::NONE);
        timing("application-register", started);
    }
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
        let owner = MainWindow::with_values(app, &launch.root, preferences.borrow().clone());
        crate::welcome::install(&owner);
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
        crate::app_icon::install_after_first_paint(window);
        window.show_all();
        timing("shown", started);
        window.present();
        timing("presented", started);
        crate::launch::start_background(launch.record_target());
        open_startup_files(&owner.editor, launch.files.clone());
    });
    let exit = app.run_with_args::<&str>(&[]);
    crate::launch::flush();
    if failed.get() {
        glib::ExitCode::FAILURE
    } else {
        exit
    }
}
/// Open the files named on the command line once the window exists. Loads are chained so
/// the tabs appear in argument order, and the first file ends up selected.
pub fn open_startup_files(editor: &impro_editor::EditorPanel, files: Vec<PathBuf>) {
    fn step(
        editor: impro_editor::EditorPanel,
        mut files: std::vec::IntoIter<PathBuf>,
        first: PathBuf,
    ) {
        match files.next() {
            Some(path) => {
                let again = editor.clone();
                editor.open_file_with(path, move |_| step(again, files, first));
            }
            None if editor.tab_for_path(&first).is_some() => editor.open_file(first),
            None => {}
        }
    }
    if let Some(first) = files.first().cloned() {
        step(editor.clone(), files.into_iter(), first);
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
    // The Windows GDK backend returns a blank buffer from GdkWindow::pixbuf, so
    // render the widget tree offscreen. An unallocated top level keeps the old path.
    #[cfg(windows)]
    if window.is_mapped() && window.allocated_width() > 1 && window.allocated_height() > 1 {
        return capture_offscreen(window, path);
    }
    let native = window.window().ok_or("window has no native surface")?;
    let pixbuf = native
        .pixbuf(0, 0, window.allocated_width(), window.allocated_height())
        .ok_or("cannot read the window surface")?;
    pixbuf.savev(path, "png", &[]).map_err(|e| e.to_string())
}

#[cfg(windows)]
fn capture_offscreen(window: &gtk::Widget, path: &Path) -> Result<(), String> {
    let scale = window.scale_factor().max(1);
    let (w, h) = (window.allocated_width(), window.allocated_height());
    let (pw, ph) = (w * scale, h * scale);
    let surface = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, pw, ph)
        .map_err(|e| e.to_string())?;
    surface.set_device_scale(f64::from(scale), f64::from(scale));
    {
        let cr = gtk::cairo::Context::new(&surface).map_err(|e| e.to_string())?;
        window.draw(&cr);
    }
    surface.flush();
    let pixbuf = gdk::pixbuf_get_from_surface(&surface, 0, 0, pw, ph)
        .ok_or("cannot convert the surface to a pixbuf")?;
    pixbuf.savev(path, "png", &[]).map_err(|e| e.to_string())
}

fn timing_enabled() -> bool {
    std::env::var_os("IMPRO_QA_TIMING").is_some()
}

fn timing(phase: &str, started: Instant) {
    let elapsed = started.elapsed().as_secs_f64() * 1000.0;
    if std::env::var_os("IMPRO_QA_TIMING").is_some() {
        println!("app-{phase}-ms={elapsed:.3}");
    } else if std::env::var_os("IMPRO_QA_TRACE").is_some() {
        eprintln!("app-{phase}-ms={elapsed:.3}");
    }
}
