//! TEMPORARY stand-in for `impro.exe` used to exercise the Windows packaging
//! pipeline (bundle layout, installer, smoke test) before the product binary
//! builds on Windows. See packaging/windows/README.md for how to remove it.
//!
//! Honours the command line and QA contract that `smoke.ps1` expects of `impro`:
//! `--version`, `--help`, exit code 2 for a missing folder, and
//! `IMPRO_QA_SCREENSHOT` / `IMPRO_QA_TIMING`. Every bundled runtime piece is
//! probed and reported as `STANDIN-CHECK <name> ok|skipped|FAIL <detail>`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use gtk::gdk_pixbuf::Pixbuf;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;

const USAGE: &str = "usage: impro [-h] [--version] [folder]";

/// Release GUI executables have no console by default. Attach to the parent's
/// console only when a standard handle is missing, so redirected pipes keep
/// working. Self-contained on purpose: the product host has its own copy.
fn attach_console() {
    #[cfg(windows)]
    {
        use std::ffi::c_void;
        type Handle = *mut c_void;
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
        const STD_ERROR_HANDLE: u32 = -12i32 as u32;
        extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
            fn GetStdHandle(which: u32) -> Handle;
            fn SetStdHandle(which: u32, handle: Handle) -> i32;
        }
        let invalid = usize::MAX as Handle;
        let valid = |h: Handle| !h.is_null() && h != invalid;
        // SAFETY: only process-owned standard handles and the documented parent
        // process constant are used; no handle is closed or dereferenced.
        unsafe {
            let output = GetStdHandle(STD_OUTPUT_HANDLE);
            let error = GetStdHandle(STD_ERROR_HANDLE);
            // AttachConsole replaces the standard handles: restore the valid ones.
            if (!valid(output) || !valid(error)) && AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
                if valid(output) {
                    SetStdHandle(STD_OUTPUT_HANDLE, output);
                }
                if valid(error) {
                    SetStdHandle(STD_ERROR_HANDLE, error);
                }
            }
        }
    }
}

enum Command {
    Version,
    Help,
    Run(PathBuf),
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut folder: Option<PathBuf> = None;
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--version" => return Ok(Command::Version),
            s if s.starts_with('-') => return Err(format!("unrecognized arguments: {s}")),
            s if folder.is_some() => return Err(format!("unrecognized arguments: {s}")),
            s => folder = Some(PathBuf::from(s)),
        }
    }
    let folder = folder.unwrap_or_else(|| PathBuf::from("."));
    if !folder.is_dir() {
        return Err(format!("not a directory: {}", folder.display()));
    }
    folder
        .canonicalize()
        .map(Command::Run)
        .map_err(|e| format!("cannot open directory {}: {e}", folder.display()))
}

#[derive(Default)]
struct Checks {
    failed: Cell<bool>,
}

impl Checks {
    fn report(&self, name: &str, result: Result<Option<String>, String>) {
        match result {
            Ok(None) => println!("STANDIN-CHECK {name} ok"),
            Ok(Some(note)) => println!("STANDIN-CHECK {name} ok {note}"),
            Err(detail) => {
                self.failed.set(true);
                println!("STANDIN-CHECK {name} FAIL {detail}");
            }
        }
    }

    fn skipped(&self, name: &str) {
        println!("STANDIN-CHECK {name} skipped");
    }
}

fn main() {
    let code = run();
    std::process::exit(code);
}

fn run() -> i32 {
    let started = Instant::now();
    match parse_args(std::env::args().skip(1)) {
        Ok(Command::Version) => {
            attach_console();
            println!("impro 0.0.0-standin");
            0
        }
        Ok(Command::Help) => {
            attach_console();
            println!("{USAGE}\n\nimpro packaging stand-in\n\npositional arguments:\n  folder\n\noptions:\n  -h, --help   show this help message and exit\n  --version    show the application version and exit");
            0
        }
        Err(error) => {
            attach_console();
            eprintln!("{USAGE}\nimpro: error: {error}");
            2
        }
        Ok(Command::Run(folder)) => run_gui(&folder, started),
    }
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default()
}

fn run_gui(folder: &Path, started: Instant) -> i32 {
    attach_console();
    if let Err(e) = gtk::init() {
        eprintln!("impro: cannot initialise GTK: {e}");
        return 1;
    }
    if std::env::var_os("IMPRO_QA_TIMING").is_some() {
        println!(
            "app-gtk-startup-ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        for key in [
            "PANGOCAIRO_BACKEND",
            "DBUS_SESSION_BUS_ADDRESS",
            "GSETTINGS_BACKEND",
            "FC_DEBUG",
        ] {
            println!(
                "STANDIN-ENV {key}={}",
                std::env::var(key).unwrap_or_else(|_| "<unset>".into())
            );
        }
    }
    // Resource checks relative to the executable only make sense in a bundle.
    let bundle = cfg!(windows) || std::env::var_os("IMPRO_STANDIN_BUNDLE_CHECK").is_some();
    let checks = Rc::new(Checks::default());

    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro packaging stand-in");
    window.set_icon_name(Some("impro"));
    window.set_default_size(520, 320);
    let grid = gtk::Box::new(gtk::Orientation::Vertical, 8);
    grid.set_border_width(16);
    window.add(&grid);

    // (a) hicolor `impro` icon by name, and the SVG through the pixbuf loader.
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    grid.pack_start(&row, false, false, 0);
    let theme = gtk::IconTheme::default();
    let has_icon = theme.as_ref().is_some_and(|t| t.has_icon("impro"));
    row.pack_start(
        &gtk::Image::from_icon_name(Some("impro"), gtk::IconSize::Dialog),
        false,
        false,
        0,
    );
    if has_icon {
        checks.report("icon-impro", Ok(None));
    } else if bundle {
        checks.report(
            "icon-impro",
            Err("hicolor icon `impro` not found in the icon theme".into()),
        );
    } else {
        checks.skipped("icon-impro");
    }
    if bundle {
        let svg = exe_dir().join("share/icons/hicolor/scalable/apps/impro.svg");
        let result = Pixbuf::from_file_at_size(&svg, 64, 64)
            .map(|pixbuf| {
                row.pack_start(&gtk::Image::from_pixbuf(Some(&pixbuf)), false, false, 0);
                Some(format!("{}x{}", pixbuf.width(), pixbuf.height()))
            })
            .map_err(|e| format!("{}: {e}", svg.display()));
        checks.report("svg-pixbuf", result);
    } else {
        checks.skipped("svg-pixbuf");
    }

    // (b) symbolic Adwaita icon through the icon theme.
    if bundle {
        let found = theme.as_ref().and_then(|t| {
            t.lookup_icon("document-open-symbolic", 16, gtk::IconLookupFlags::empty())
        });
        match found {
            Some(info) => {
                row.pack_start(
                    &gtk::Image::from_icon_name(
                        Some("document-open-symbolic"),
                        gtk::IconSize::LargeToolbar,
                    ),
                    false,
                    false,
                    0,
                );
                checks.report(
                    "icon-symbolic",
                    Ok(info.filename().map(|p| p.display().to_string())),
                );
            }
            None => checks.report(
                "icon-symbolic",
                Err("document-open-symbolic not found".into()),
            ),
        }
    } else {
        checks.skipped("icon-symbolic");
    }

    // (c) compiled GSettings schemas.
    if bundle {
        let schema = gtk::gio::SettingsSchemaSource::default()
            .and_then(|s| s.lookup("org.gtk.Settings.FileChooser", true));
        checks.report(
            "gsettings",
            schema
                .map(|_| None)
                .ok_or_else(|| "schema org.gtk.Settings.FileChooser not installed".to_string()),
        );
    } else {
        checks.skipped("gsettings");
    }

    // (d) text in the Pango default font.
    let label = gtk::Label::new(Some(&format!(
        "impro packaging stand-in\nfolder: {}",
        folder.display()
    )));
    label.set_xalign(0.0);
    grid.pack_start(&label, true, true, 0);
    let layout = label.create_pango_layout(Some("The quick brown fox jumps over the lazy dog"));
    let (width, height) = layout.pixel_size();
    let family = label
        .pango_context()
        .font_description()
        .and_then(|d| d.family().map(|f| f.to_string()))
        .unwrap_or_default();
    checks.report(
        "pango-font",
        if width > 0 && height > 0 {
            Ok(Some(format!("{family} {width}x{height}px")))
        } else {
            Err(format!("text layout is empty (font '{family}')"))
        },
    );

    let screenshot = std::env::var_os("IMPRO_QA_SCREENSHOT").map(PathBuf::from);
    let timing = std::env::var_os("IMPRO_QA_TIMING").is_some();
    let exit_code = Rc::new(Cell::new(0));
    let done = Rc::new(RefCell::new(false));

    window.connect_destroy(|_| gtk::main_quit());
    let finish = {
        let checks = checks.clone();
        let exit_code = exit_code.clone();
        let window = window.clone();
        move || {
            if timing {
                println!(
                    "first-paint-ms={:.3}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
            if let Some(path) = &screenshot {
                if let Err(e) = capture(&window, path) {
                    checks.failed.set(true);
                    println!("STANDIN-CHECK screenshot FAIL {e}");
                }
            }
            if checks.failed.get() {
                println!("STANDIN-FAIL");
                exit_code.set(1);
            }
            if screenshot.is_some() || timing {
                gtk::main_quit();
            }
        }
    };
    // First complete draw: after the frame clock painted the mapped window.
    window.connect_map(move |win| {
        let Some(clock) = win.frame_clock() else {
            return;
        };
        let finish = finish.clone();
        let done = done.clone();
        clock.connect_after_paint(move |_| {
            if !done.replace(true) {
                let finish = finish.clone();
                gtk::glib::idle_add_local_once(finish);
            }
        });
    });
    window.show_all();
    gtk::main();
    exit_code.get()
}

/// Render the window content into a cairo image surface and write it as PNG.
fn capture(window: &gtk::Window, path: &Path) -> Result<(), String> {
    let (w, h) = (window.allocated_width(), window.allocated_height());
    let surface = gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w, h)
        .map_err(|e| e.to_string())?;
    {
        let cr = gtk::cairo::Context::new(&surface).map_err(|e| e.to_string())?;
        window.draw(&cr);
    }
    surface.flush();
    // cairo's own PNG writer needs a crate feature impro-ui does not enable; the
    // pixbuf PNG saver (also part of the bundled runtime) writes the same image.
    let pixbuf = gtk::gdk::pixbuf_get_from_surface(&surface, 0, 0, w, h)
        .ok_or("cannot convert the surface to a pixbuf")?;
    pixbuf.savev(path, "png", &[]).map_err(|e| e.to_string())
}
