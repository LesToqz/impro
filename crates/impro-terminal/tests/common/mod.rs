#![allow(dead_code)]
use gtk::prelude::*;
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
pub const PROMPT: &str = "user@host:~/projects/impro$ ";
pub struct BashDock {
    pub dock: Dock,
    pub directory: tempfile::TempDir,
    pub grids: std::rc::Rc<std::cell::RefCell<Vec<(impro_vte::Terminal, i64, i64)>>>,
}
impl BashDock {
    pub fn new(stacked: bool, count: usize, slow: bool, locale: &str) -> Self {
        use gtk::prelude::*;
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let shell = directory.path().join("shell");
        let rc = directory.path().join("rc");
        std::fs::write(&rc,format!("PS1='\\[\\033[01;32m\\]user@host\\[\\033[00m\\]:\\[\\033[01;34m\\]~/projects/impro\\[\\033[00m\\]$ '\nstty size > '{}/size-'$$\n",directory.path().display())).unwrap();
        let prefix = if slow {
            format!(
                "/usr/bin/python3 '{}' 0.025 ",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/slow_pty.py")
                    .display()
            )
        } else {
            String::new()
        };
        std::fs::write(&shell,format!("#!/bin/sh\nexec env LC_ALL='{locale}' HISTFILE=/dev/null {prefix}/bin/bash --noprofile --rcfile '{}' -i\n",rc.display())).unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o700)).unwrap();
        let dock = Dock::new(stacked, 0);
        dock.panel.set_shell(shell.to_str().unwrap());
        let grids = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        for _ in 0..count {
            assert!(dock.panel.new_terminal());
            let t = dock
                .panel
                .panes()
                .into_iter()
                .max_by_key(|p| p.id)
                .unwrap()
                .terminal
                .as_ref()
                .unwrap()
                .clone();
            let g = grids.clone();
            t.connect_notify_local(Some("pty"), move |t, _| {
                if t.pty().is_some() {
                    g.borrow_mut()
                        .push((t.clone(), t.column_count(), t.row_count()));
                }
            });
        }
        Self {
            dock,
            directory,
            grids,
        }
    }
    pub fn ready(&self) {
        self.dock.ready();
        poll(|| {
            self.dock.panel.panes().iter().all(|p| {
                text(p.terminal.as_ref().unwrap())
                    .replace('\n', "")
                    .contains(PROMPT.trim())
            })
        });
        pump(120);
    }
    pub fn assert_prompt(&self) {
        for p in self.dock.panel.panes() {
            let t = p.terminal.as_ref().unwrap();
            if !t.is_mapped() {
                continue;
            }
            let grid = (t.column_count(), t.row_count());
            let pty = t.pty().unwrap().size().unwrap();
            assert_eq!(grid, (pty.0 as i64, pty.1 as i64));
            let output = text(t);
            assert_eq!(output.matches("user@host").count(), 1, "{output:?}");
            assert_eq!(output.replace('\n', "").trim(), PROMPT.trim(), "{output:?}");
            assert_eq!(
                t.cursor_position(),
                (PROMPT.len() as i64 % grid.0, PROMPT.len() as i64 / grid.0)
            );
        }
    }
    pub fn assert_wraps(&self) {
        for p in self.dock.panel.panes() {
            let t = p.terminal.as_ref().unwrap();
            let columns = t.column_count();
            let typed = "x".repeat(columns as usize + 7);
            t.feed_child(typed.as_bytes());
            let len = (PROMPT.len() + typed.len()) as i64;
            poll(|| t.cursor_position() == (len % columns, len / columns));
            let output = text(t).replace('\n', "");
            assert!(output.trim_end().ends_with(&typed), "{output:?}");
            let line = format!("{PROMPT}{typed}");
            for (row, chunk) in line.as_bytes().chunks(columns as usize).enumerate() {
                let actual = t
                    .text_range_format(impro_vte::Format::Text, row as i64, 0, row as i64, columns)
                    .unwrap();
                assert_eq!(
                    actual.trim_end(),
                    std::str::from_utf8(chunk).unwrap().trim_end()
                );
            }
        }
    }
    pub fn resize(&self, width: i32, height: i32) {
        self.dock.window.resize(width, height);
        pump(160);
        poll(|| {
            self.dock.panel.panes().iter().all(|p| {
                p.gate
                    .as_ref()
                    .is_none_or(|g| g.target() == g.committed() && g.waiting_count() == 0)
            })
        });
    }
}
pub fn pointer_event(handle: &gtk::EventBox, kind: gdk::EventType, coordinate: f64) -> gdk::Event {
    use glib::translate::*;
    let mut event = gdk::Event::new(kind);
    unsafe {
        let raw: *mut gdk::ffi::GdkEvent = event.to_glib_none_mut().0;
        if kind == gdk::EventType::MotionNotify {
            (*raw).motion.x_root = coordinate;
            (*raw).motion.y_root = coordinate;
            (*raw).motion.window = handle.window().unwrap().to_glib_full();
        } else {
            (*raw).button.button = 1;
            (*raw).button.x_root = coordinate;
            (*raw).button.y_root = coordinate;
            (*raw).button.window = handle.window().unwrap().to_glib_full();
        }
    }
    let device = handle.display().default_seat().and_then(|s| s.pointer());
    event.set_device(device.as_ref());
    event
}
pub fn pointer(handle: &gtk::EventBox, kind: gdk::EventType, coordinate: f64) {
    let event = pointer_event(handle, kind, coordinate);
    assert!(handle.event(&event));
}
