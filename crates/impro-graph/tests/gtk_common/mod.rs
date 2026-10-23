//! GTK test harness: every closure runs on one dedicated GTK thread (GTK is
//! thread-bound), against projects built in temporary directories and an
//! in-memory preferences store.
#![allow(dead_code)]

pub mod bigproject;
pub mod drive;

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};

use gtk::prelude::*;
use impro_graph::view::testing as t;
use impro_graph::view::{MemoryStore, ViewOptions};
use impro_graph::{GraphView, OpenMode};
use serde_json::{json, Value};

pub fn run(f: impl FnOnce() + Send + 'static) {
    type Job = Box<dyn FnOnce() + Send>;
    static GTK: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let worker = GTK.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            // IMPRO_TEST_BACKEND=wayland runs against a private compositor
            let backend = std::env::var("IMPRO_TEST_BACKEND").unwrap_or_else(|_| "x11".into());
            std::env::set_var("GDK_BACKEND", backend);
            std::env::remove_var("IMPRO_REDUCE_MOTION");
            gtk::init().expect("an X display (run under xvfb-run)");
            impro_ui::init();
            impro_graph::init();
            while let Ok(job) = rx.recv() {
                job();
            }
        });
        tx
    });
    let (tx, rx) = mpsc::channel();
    worker
        .send(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
            let _ = tx.send(result);
        }))
        .expect("gtk thread");
    if let Err(error) = rx.recv().expect("result") {
        std::panic::resume_unwind(error);
    }
}

pub fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
    std::fs::write(p, text).expect("write");
}

/// A small project: a hub, a chain, a tag, a missing target, an attachment,
/// source files and an orphan.
pub fn sample_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("hub/Hub.md", "# Hub\n[[a]] [[b]] [[c]] #topic\n"),
        ("a.md", "[[b]]\n"),
        ("b.md", "[[c]] [[Missing Note]]\n"),
        ("c.md", "end\n"),
        ("orphan.md", "lonely\n"),
        ("img.md", "![[pic.png]]\n"),
        ("pic.png", "PNG"),
        ("src/main.py", "import util\n"),
        ("src/util.py", ""),
    ]
}

/// What the host would observe.
#[derive(Default)]
pub struct Seen {
    pub opened: Vec<(PathBuf, OpenMode)>,
    pub tags: Vec<String>,
    pub created: Vec<String>,
    pub revealed: Vec<PathBuf>,
    pub copied: Vec<PathBuf>,
    pub settings: Vec<Value>,
    pub errors: Vec<String>,
    pub updates: usize,
}

pub struct Fixture {
    pub view: GraphView,
    pub window: gtk::Window,
    pub store: Arc<MemoryStore>,
    pub project: tempfile::TempDir,
    pub seen: Arc<Mutex<Seen>>,
}

impl Fixture {
    pub fn new() -> Fixture {
        Fixture::with(&sample_files(), json!({}))
    }

    pub fn with(files: &[(&str, &str)], prefs: Value) -> Fixture {
        let project = tempfile::tempdir().expect("tempdir");
        for (rel, text) in files {
            write(project.path(), rel, text);
        }
        Fixture::in_dir(project, prefs, true)
    }

    pub fn in_dir(project: tempfile::TempDir, prefs: Value, scan: bool) -> Fixture {
        Fixture::in_dir_sized(project, prefs, scan, (900, 600))
    }

    /// The user's kind of project (3 000 files) in a window of `size` logical pixels.
    pub fn big(size: (i32, i32)) -> Fixture {
        let project = tempfile::tempdir().expect("tempdir");
        bigproject::BigProject::default().write(project.path());
        Fixture::in_dir_sized(project, json!({}), true, size)
    }

    pub fn in_dir_sized(
        project: tempfile::TempDir,
        prefs: Value,
        scan: bool,
        size: (i32, i32),
    ) -> Fixture {
        let store = Arc::new(MemoryStore::new(prefs));
        let view = GraphView::with_options(
            project.path(),
            ViewOptions {
                store: store.clone(),
                scan,
            },
        );
        let seen = Arc::new(Mutex::new(Seen::default()));
        {
            let s = seen.clone();
            view.connect_open_file(move |p, m| {
                s.lock().expect("seen").opened.push((p.to_path_buf(), m))
            });
            let s = seen.clone();
            view.connect_search_tag(move |tag| s.lock().expect("seen").tags.push(tag.to_string()));
            let s = seen.clone();
            view.connect_create_note(move |n| s.lock().expect("seen").created.push(n.to_string()));
            let s = seen.clone();
            view.connect_reveal(move |p| s.lock().expect("seen").revealed.push(p.to_path_buf()));
            let s = seen.clone();
            view.connect_copy_path(move |p| s.lock().expect("seen").copied.push(p.to_path_buf()));
            let s = seen.clone();
            view.connect_settings_changed(move |v| {
                s.lock().expect("seen").settings.push(v.clone())
            });
            let s = seen.clone();
            view.connect_error(move |e| s.lock().expect("seen").errors.push(e.to_string()));
            let s = seen.clone();
            view.connect_updated(move |_| s.lock().expect("seen").updates += 1);
        }
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(size.0, size.1);
        window.add(&*view);
        window.show_all();
        window.move_(0, 0);
        let fixture = Fixture {
            view,
            window,
            store,
            project,
            seen,
        };
        if scan {
            fixture.wait_ready();
        } else {
            t::pump(100);
        }
        fixture
    }

    /// Waits for the first graph and for the layout to come to rest.
    pub fn wait_ready(&self) {
        assert!(
            t::wait_for(10_000, || self.view.summary().nodes > 0
                || self.seen().updates > 0),
            "no graph arrived"
        );
        t::settle_now(&self.view);
        assert!(t::settle(&self.view, 15_000), "the view never settled");
    }

    pub fn root(&self) -> &Path {
        self.project.path()
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.project.path().join(rel)
    }

    pub fn seen(&self) -> std::sync::MutexGuard<'_, Seen> {
        self.seen.lock().expect("seen")
    }

    pub fn ids(&self) -> Vec<String> {
        t::node_ids(&self.view)
    }

    pub fn has(&self, id: &str) -> bool {
        self.ids().iter().any(|i| i == id)
    }

    /// Pumps until the node set satisfies `pred`.
    pub fn wait_ids(&self, ms: u64, pred: impl Fn(&[String]) -> bool) -> bool {
        t::wait_for(ms, || pred(&self.ids()))
    }

    pub fn settle(&self) {
        t::settle_now(&self.view);
        assert!(t::settle(&self.view, 15_000), "the view never settled");
    }

    /// Screen position of a node, panning the camera so it is in view.
    pub fn screen(&self, id: &str) -> (f64, f64, f64) {
        t::node_screen(&self.view, id).unwrap_or_else(|| panic!("no node {id}"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.view.shutdown();
        self.window.close();
    }
}

/// The saved blobs, newest last, after letting the debounce elapse.
pub fn saved(f: &Fixture) -> Vec<Value> {
    t::pump(450);
    f.store.saves()
}

pub fn none() -> gdk::ModifierType {
    gdk::ModifierType::empty()
}

/// Delivers a pointer event to the canvas through GTK's normal dispatch, as
/// the windowing system would: `kind` is a button press/release or a motion,
/// `state` the modifier and button mask the event carries.
pub fn send_pointer(
    view: &GraphView,
    kind: gdk::EventType,
    (x, y): (f64, f64),
    button: u32,
    state: gdk::ModifierType,
    time: u32,
) {
    use glib::translate::*;
    let canvas = view.canvas();
    let win = canvas.window().expect("realized canvas");
    let (_, ox, oy) = win.origin();
    let mut event = gdk::Event::new(kind);
    // SAFETY: `event` owns a zeroed GdkEvent of the union member written here.
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        if kind == gdk::EventType::MotionNotify {
            let m = &mut (*raw).motion;
            m.window = win.to_glib_full();
            m.send_event = 1;
            m.time = time;
            m.x = x;
            m.y = y;
            m.x_root = x + ox as f64;
            m.y_root = y + oy as f64;
            m.state = state.into_glib();
        } else {
            let b = &mut (*raw).button;
            b.window = win.to_glib_full();
            b.send_event = 1;
            b.time = time;
            b.x = x;
            b.y = y;
            b.x_root = x + ox as f64;
            b.y_root = y + oy as f64;
            b.button = button;
            b.state = state.into_glib();
        }
    }
    let seat = canvas.display().default_seat().expect("seat");
    event.set_device(seat.pointer().as_ref());
    gtk::main_do_event(&mut event);
}

/// A canvas point at least `margin` pixels from every marker.
pub fn empty_spot(f: &Fixture, margin: f64) -> (f64, f64) {
    let (w, h) = t::info(&f.view).size;
    let ids = f.ids();
    let mut y = 4.0;
    while y < h {
        let mut x = 4.0;
        while x < w {
            if ids.iter().all(|id| {
                let (sx, sy, r) = f.screen(id);
                (sx - x).hypot(sy - y) > r + margin
            }) {
                return (x, y);
            }
            x += 16.0;
        }
        y += 16.0;
    }
    panic!("no empty spot on the canvas");
}
