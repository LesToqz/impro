//! Explorer and search in a window.
//!
//! ```text
//! cargo run -p impro-explorer --example explorer_demo -- [ROOT] [options]
//!   --size WxH          window size (default 300x720, the sidebar width of the app)
//!   --theme ID          theme id (default impro-dark)
//!   --search QUERY      type QUERY into the search box
//!   --notes             show the Notes section
//!   --expand PATH       reveal PATH (relative to ROOT) in the tree
//!   --screenshot FILE   write a PNG once the tree has settled, then quit
//! ```
use gtk::prelude::*;
use impro_core::themes;
use impro_explorer::ExplorerPanel;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Options {
    root: Option<PathBuf>,
    size: Option<(i32, i32)>,
    theme: Option<String>,
    search: Option<String>,
    notes: bool,
    expand: Vec<String>,
    screenshot: Option<PathBuf>,
}

fn parse() -> Options {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--size" => {
                options.size = args.next().and_then(|s| {
                    let (w, h) = s.split_once('x')?;
                    Some((w.parse().ok()?, h.parse().ok()?))
                })
            }
            "--theme" => options.theme = args.next(),
            "--search" => options.search = args.next(),
            "--notes" => options.notes = true,
            "--expand" => options.expand.extend(args.next()),
            "--screenshot" => options.screenshot = args.next().map(PathBuf::from),
            other => options.root = Some(PathBuf::from(other)),
        }
    }
    options
}

fn main() {
    let options = parse();
    gtk::init().expect("display");
    impro_ui::init();
    impro_explorer::init();
    let theme = themes::get_theme(options.theme.as_deref());
    if let Err(error) = impro_ui::apply_theme(theme, None, None, None) {
        eprintln!("theme: {error}");
    }
    let root = options
        .root
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let started = Instant::now();
    let explorer = ExplorerPanel::new(&root);
    explorer.connect_file_activated(|_, path| println!("file-activated {}", path.display()));
    explorer.connect_path_renamed(|_, old, new| {
        println!("path-renamed {} -> {}", old.display(), new.display())
    });
    explorer.connect_path_deleted(|_, path| println!("path-deleted {}", path.display()));
    explorer.connect_search_result_activated(|_, path, line, column, length| {
        println!(
            "search-result-activated {} {line}:{column}+{length}",
            path.display()
        );
    });
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    let (width, height) = options.size.unwrap_or((300, 720));
    window.set_default_size(width, height);
    window.set_title("impro explorer");
    window.add(&explorer);
    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    if options.notes {
        explorer.show_notes_section(true);
    }
    for path in &options.expand {
        explorer.set_current_file(Some(&root.join(path)));
    }
    if let Some(query) = &options.search {
        explorer.focus_search(Some(query));
    }
    if let Some(file) = options.screenshot.clone() {
        let explorer = explorer.clone();
        let window = window.clone();
        let mut quiet = 0;
        glib::timeout_add_local(Duration::from_millis(60), move || {
            let busy = explorer.is_busy()
                || explorer.search().is_searching()
                || explorer.search().timeout_pending();
            quiet = if busy { 0 } else { quiet + 1 };
            // Settled for a few frames: the tree, search results and notes are in place.
            if quiet < 6 && started.elapsed() < Duration::from_secs(8) {
                return glib::ControlFlow::Continue;
            }
            println!("settled-ms={}", started.elapsed().as_millis());
            let Some(gdk_window) = window.window() else {
                gtk::main_quit();
                return glib::ControlFlow::Break;
            };
            use glib::translate::*;
            // SAFETY: the window is realized; the call returns a new pixbuf or NULL.
            let pixbuf: Option<gdk_pixbuf::Pixbuf> = unsafe {
                from_glib_full(gdk::ffi::gdk_pixbuf_get_from_window(
                    gdk_window.to_glib_none().0,
                    0,
                    0,
                    gdk_window.width(),
                    gdk_window.height(),
                ))
            };
            match pixbuf.map(|p| p.savev(&file, "png", &[])) {
                Some(Ok(())) => println!("screenshot {}", file.display()),
                other => eprintln!("screenshot failed: {other:?}"),
            }
            gtk::main_quit();
            glib::ControlFlow::Break
        });
    }
    gtk::main();
}
