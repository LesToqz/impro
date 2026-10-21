#![allow(dead_code)]
use std::sync::{mpsc, OnceLock};
/// All GTK tests execute on the same dedicated GTK thread, including main-context pumping.
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
        std::panic::resume_unwind(error);
    }
}
pub fn pump(ms: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    loop {
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
pub fn window(child: &impl gtk::prelude::IsA<gtk::Widget>) -> gtk::Window {
    use gtk::prelude::*;
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(500, 300);
    window.add(child);
    window.show_all();
    pump(30);
    window
}
pub fn poll(f: impl Fn() -> bool) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while !f() {
        assert!(std::time::Instant::now() < until, "operation timed out");
        pump(2);
    }
}
pub struct Dock {
    pub window: gtk::Window,
    pub panel: impro_terminal::TerminalPanel,
}
impl Dock {
    pub fn new(stacked: bool, count: usize) -> Self {
        use gtk::prelude::*;
        impro_terminal::init();
        let _ = impro_ui::apply_style(None);
        let panel = impro_terminal::TerminalPanel::new("/tmp", false, stacked, 11);
        panel.set_animation_ms(0);
        panel.set_shell("/bin/bash");
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(1200, 750);
        window.add(&panel);
        for _ in 0..count {
            assert!(panel.new_terminal());
        }
        window.show_all();
        Self { window, panel }
    }
    pub fn ready(&self) {
        poll(|| self.panel.panes().iter().all(|p| p.pid().is_some()));
        pump(70);
    }
    pub fn terminal(&self, index: usize) -> impro_vte::Terminal {
        self.panel.panes()[index].terminal.as_ref().unwrap().clone()
    }
}
impl Drop for Dock {
    fn drop(&mut self) {
        use gtk::prelude::*;
        unsafe {
            self.window.destroy();
        }
        pump(30);
    }
}
pub fn text(t: &impro_vte::Terminal) -> String {
    t.text_format(impro_vte::Format::Text)
        .map(|s| s.into())
        .unwrap_or_default()
}
pub fn command(t: &impro_vte::Terminal, s: &str) {
    t.feed_child(format!("{s}\n").as_bytes());
}
pub fn dead(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) != 0 }
}
