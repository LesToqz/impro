#![allow(dead_code)]
use gtk::prelude::*;
use impro_chrome::*;
use std::{
    sync::{mpsc, OnceLock},
    time::{Duration, Instant},
};
pub fn run(f: impl FnOnce() + Send + 'static) {
    type Job = Box<dyn FnOnce() + Send>;
    static GTK: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    let worker = GTK.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            gtk::init().expect("Xvfb display");
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
            tx.send(result).unwrap();
        }))
        .unwrap();
    if let Err(error) = rx.recv().unwrap() {
        eprintln!(
            "GTK assertion: {:?}",
            error
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| error.downcast_ref::<&str>().map(|s| s.to_string()))
        );
        std::panic::resume_unwind(error);
    }
}
pub fn pump(ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if Instant::now() >= end {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub fn poll(mut f: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !f() && Instant::now() < end {
        pump(5);
    }
    assert!(f(), "condition timed out");
}
pub fn theme(id: &str) {
    init();
    impro_ui::apply_style(Some(impro_core::themes::get_theme(Some(id)))).unwrap();
}
pub struct Fixture {
    pub window: gtk::Window,
    pub host: OverlayHost,
    pub text: gtk::TextView,
}
impl Fixture {
    pub fn new() -> Self {
        theme("impro-dark");
        impro_ui::motion::set_reduce_motion(true);
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(900, 600);
        let text = gtk::TextView::new();
        let host = OverlayHost::new(Some(text.upcast_ref()));
        window.add(host.widget());
        window.show_all();
        window.present();
        pump(40);
        text.grab_focus();
        Self { window, host, text }
    }
    pub fn show(&self, p: &Prompt) {
        self.host.show_prompt(p);
        pump(30);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.host.hide_prompt();
        self.host.clear_notices();
        unsafe {
            self.window.destroy();
        }
        pump(10);
        impro_ui::motion::set_reduce_motion(false);
    }
}
pub fn prompt(count: usize) -> Prompt {
    Prompt::new(
        move |q| {
            let values = (0..count).map(|n| format!("item {n}")).collect::<Vec<_>>();
            rank(q, &values, false, Some(100))
                .into_iter()
                .map(|(text, _, ranges)| {
                    let n = text.trim_start_matches("item ").parse::<usize>().unwrap();
                    PromptItem {
                        label: text,
                        label_ranges: ranges,
                        ..Default::default()
                    }
                    .with_data(n)
                })
                .collect()
        },
        "Find...",
    )
}
pub fn notice_rect(host: &OverlayHost, n: &Notice) -> (i32, i32, i32, i32) {
    let bx = n.box_widget();
    let (x, y) = bx.translate_coordinates(host.widget(), 0, 0).unwrap();
    (x, y, bx.allocated_width(), bx.allocated_height())
}
pub fn click(widget: &impl IsA<gtk::Widget>, x: f64, y: f64) {
    use glib::translate::*;
    let widget = widget.as_ref();
    let window = widget.window().unwrap();
    let mut event = gdk::Event::new(gdk::EventType::ButtonPress);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        (*raw).button.window = window.to_glib_full();
        (*raw).button.button = 1;
        (*raw).button.x = x;
        (*raw).button.y = y;
    }
    widget.emit_by_name::<bool>("button-press-event", &[&event]);
}
