//! Bounded, geometry-aware async shell startup with early-input replay.
use crate::{
    panel::{pick_shell, safe_cwd, Pane},
    TerminalPanel,
};
use gtk::prelude::*;
use impro_vte::Terminal;
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

pub(crate) struct Startup {
    panel: glib::WeakRef<TerminalPanel>,
    pane: Weak<Pane>,
    terminal: glib::WeakRef<Terminal>,
    command: RefCell<Option<(String, PathBuf)>>,
    requested_shell: String,
    cwd: PathBuf,
    started: Cell<bool>,
    cancelled: Cell<bool>,
    fallback_ready: Cell<bool>,
    grid: Cell<(i64, i64)>,
    changed: Cell<Instant>,
    painted: Cell<u32>,
    buffered: RefCell<Vec<u8>>,
    handlers: RefCell<Vec<glib::SignalHandlerId>>,
    fallback: RefCell<Option<glib::SourceId>>,
    idle: RefCell<Option<glib::SourceId>>,
    replay: RefCell<Option<glib::SourceId>>,
    tick: RefCell<Option<gtk::TickCallbackId>>,
    paint: RefCell<Option<(gdk::FrameClock, glib::SignalHandlerId)>>,
}
impl Startup {
    pub fn new(panel: &TerminalPanel, pane: &Rc<Pane>, shell: String, cwd: PathBuf) -> Rc<Self> {
        Rc::new(Self {
            panel: panel.downgrade(),
            pane: Rc::downgrade(pane),
            terminal: pane.terminal.as_ref().expect("terminal pane").downgrade(),
            command: RefCell::new(None),
            requested_shell: shell,
            cwd,
            started: Cell::new(false),
            cancelled: Cell::new(false),
            fallback_ready: Cell::new(false),
            grid: Cell::new((0, 0)),
            changed: Cell::new(Instant::now()),
            painted: Cell::new(0),
            buffered: RefCell::new(Vec::new()),
            handlers: RefCell::new(Vec::new()),
            fallback: RefCell::new(None),
            idle: RefCell::new(None),
            replay: RefCell::new(None),
            tick: RefCell::new(None),
            paint: RefCell::new(None),
        })
    }
    pub fn begin(self: &Rc<Self>) {
        let Some(t) = self.terminal.upgrade() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let handler = t.connect_commit(move |t, text| {
            if t.pty().is_none() {
                if let Some(s) = weak.upgrade() {
                    s.buffered.borrow_mut().extend_from_slice(text.as_bytes());
                }
            }
        });
        self.handlers.borrow_mut().push(handler);
        let weak = Rc::downgrade(self);
        let handler = t.connect_size_allocate(move |t, _| {
            if let Some(s) = weak.upgrade() {
                s.allocated(t);
            }
        });
        self.handlers.borrow_mut().push(handler);
        let weak = Rc::downgrade(self);
        let handler = t.connect_map(move |t| {
            if let Some(s) = weak.upgrade() {
                s.allocated(t);
            }
        });
        self.handlers.borrow_mut().push(handler);
        let weak = Rc::downgrade(self);
        let tick = t.add_tick_callback(move |_, clock| {
            let Some(s) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if s.paint.borrow().is_none() {
                let weak = Rc::downgrade(&s);
                let handler = clock.connect_after_paint(move |_| {
                    if let Some(s) = weak.upgrade() {
                        s.after_paint();
                    }
                });
                *s.paint.borrow_mut() = Some((clock.clone(), handler));
            }
            glib::ControlFlow::Continue
        });
        *self.tick.borrow_mut() = Some(tick);
        let weak = Rc::downgrade(self);
        *self.fallback.borrow_mut() = Some(glib::timeout_add_local_once(
            Duration::from_secs(1),
            move || {
                if let Some(s) = weak.upgrade() {
                    s.fallback.borrow_mut().take();
                    s.fallback_ready.set(true);
                    s.spawn();
                }
            },
        ));
        let weak = Rc::downgrade(self);
        *self.idle.borrow_mut() = Some(glib::idle_add_local_once(move || {
            if let Some(s) = weak.upgrade() {
                s.idle.borrow_mut().take();
                let unattached = s
                    .panel
                    .upgrade()
                    .is_some_and(|p| p.toplevel().is_none_or(|w| !w.is::<gtk::Window>()));
                if unattached {
                    s.fallback_ready.set(true);
                    s.spawn();
                }
            }
        }));
        // Resolve paths away from GTK. Delivery uses the main context, never a runtime.
        #[allow(deprecated)]
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        let shell = self.requested_shell.clone();
        let cwd = self.cwd.clone();
        std::thread::spawn(move || {
            let shell = if shell.is_empty() {
                pick_shell().to_string_lossy().into_owned()
            } else {
                shell
            };
            let _ = tx.send((shell, safe_cwd(cwd)));
        });
        let weak = Rc::downgrade(self);
        rx.attach(None, move |command| {
            if let Some(s) = weak.upgrade() {
                if !s.cancelled.get() {
                    *s.command.borrow_mut() = Some(command);
                    if s.fallback_ready.get() {
                        s.spawn();
                    }
                }
            }
            glib::ControlFlow::Break
        });
        self.allocated(&t);
    }
    fn allocated(&self, t: &Terminal) {
        if self.started.get() || !t.is_mapped() {
            return;
        }
        let grid = (t.column_count(), t.row_count());
        if self.grid.replace(grid) != grid {
            self.changed.set(Instant::now());
            self.painted.set(0);
        }
    }
    fn after_paint(self: &Rc<Self>) {
        if self.started.get() || self.cancelled.get() {
            return;
        }
        let Some(t) = self.terminal.upgrade().filter(|t| t.is_mapped()) else {
            return;
        };
        if !self
            .panel
            .upgrade()
            .is_some_and(|p| p.initial_extent_ready())
        {
            return;
        }
        self.allocated(&t);
        self.painted.set(self.painted.get() + 1);
        if self.painted.get() >= 2 && self.changed.get().elapsed() >= Duration::from_millis(32) {
            self.spawn();
        }
    }
    fn stop_frames(&self) {
        let tick = self.tick.borrow_mut().take();
        if let Some(t) = tick {
            t.remove();
        }
        let paint = self.paint.borrow_mut().take();
        if let Some((c, h)) = paint {
            crate::disconnect(&c, h);
        }
    }
    fn stop_timers(&self) {
        for source in [&self.fallback, &self.idle, &self.replay] {
            let id = source.borrow_mut().take();
            if let Some(id) = id {
                id.remove();
            }
        }
    }
    fn cleanup(&self) {
        self.stop_frames();
        self.stop_timers();
        let handlers = std::mem::take(&mut *self.handlers.borrow_mut());
        if let Some(t) = self.terminal.upgrade() {
            for h in handlers {
                crate::disconnect(&t, h);
            }
        }
    }
    pub fn cancel(&self) {
        if self.cancelled.replace(true) {
            return;
        }
        self.cleanup();
        self.buffered.borrow_mut().clear();
    }
    fn spawn(self: &Rc<Self>) {
        if self.started.get() || self.cancelled.get() {
            return;
        }
        let command = self.command.borrow_mut().take();
        let Some((shell, cwd)) = command else { return };
        let (Some(pane), Some(t)) = (self.pane.upgrade(), self.terminal.upgrade()) else {
            return;
        };
        if pane.is_closed() {
            return;
        }
        self.started.set(true);
        self.stop_frames();
        self.stop_timers();
        let state = self.clone();
        let terminal = t.downgrade();
        let cancellable = pane.cancellable.clone();
        t.spawn_async_path(
            Some(&cwd),
            &[&shell, "-i"],
            &["TERM=xterm-256color", "COLORTERM=truecolor"],
            glib::SpawnFlags::DEFAULT,
            -1,
            Some(&cancellable),
            move |result| {
                state.cleanup();
                match result {
                    Ok(pid) => {
                        pane.pid.set(Some(pid.0));
                        if pane.is_closed() || state.cancelled.get() {
                            unsafe {
                                libc::kill(pid.0, libc::SIGHUP);
                            }
                            pane.pid.set(None);
                            return;
                        }
                        let pending = std::mem::take(&mut *state.buffered.borrow_mut());
                        if pending.is_empty() {
                            return;
                        }
                        let deadline = Instant::now() + Duration::from_secs(1);
                        let weak = Rc::downgrade(&state);
                        let terminal = terminal.clone();
                        let id = glib::timeout_add_local(Duration::from_millis(10), move || {
                            let Some(s) = weak.upgrade() else {
                                return glib::ControlFlow::Break;
                            };
                            let Some(t) = terminal.upgrade() else {
                                return glib::ControlFlow::Break;
                            };
                            if !s.cancelled.get()
                                && Instant::now() < deadline
                                && t.pty().is_some_and(|p| p.canonical_or_echo())
                            {
                                return glib::ControlFlow::Continue;
                            }
                            s.replay.borrow_mut().take();
                            if !s.cancelled.get() {
                                t.feed_child(&pending);
                            }
                            glib::ControlFlow::Break
                        });
                        *state.replay.borrow_mut() = Some(id);
                    }
                    Err(error) => {
                        if !pane.is_closed() {
                            if let Some(t) = terminal.upgrade() {
                                t.feed(
                                    format!("Unable to start shell: {}\r\n", error.message())
                                        .as_bytes(),
                                );
                            }
                        }
                    }
                }
            },
        );
    }
}
