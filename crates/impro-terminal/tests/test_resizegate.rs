#![cfg(unix)]
mod common;
use common::*;
use gtk::{prelude::*, subclass::prelude::*};
use impro_terminal::resizegate::ResizeGate;
use impro_vte::Terminal;
mod probe {
    use super::*;
    #[derive(Default)]
    pub struct Probe;
    #[glib::object_subclass]
    impl ObjectSubclass for Probe {
        const NAME: &'static str = "ImproTestAllocationProbe";
        type Type = super::Probe;
        type ParentType = gtk::DrawingArea;
    }
    impl ObjectImpl for Probe {}
    impl WidgetImpl for Probe {
        fn preferred_width(&self) -> (i32, i32) {
            (1, 160)
        }
        fn preferred_height(&self) -> (i32, i32) {
            (1, 90)
        }
    }
    impl DrawingAreaImpl for Probe {}
}
glib::wrapper! {pub struct Probe(ObjectSubclass<probe::Probe>) @extends gtk::DrawingArea,gtk::Widget,@implements gtk::Buildable;}
mod host {
    use super::*;
    #[derive(Default)]
    pub struct Host;
    #[glib::object_subclass]
    impl ObjectSubclass for Host {
        const NAME: &'static str = "ImproTestAllocationHost";
        type Type = super::Host;
        type ParentType = gtk::Fixed;
    }
    impl ObjectImpl for Host {}
    impl WidgetImpl for Host {
        fn size_allocate(&self, a: &gtk::Allocation) {
            self.obj().set_allocation(a);
        }
    }
    impl ContainerImpl for Host {}
    impl FixedImpl for Host {}
}
glib::wrapper! {pub struct Host(ObjectSubclass<host::Host>) @extends gtk::Fixed,gtk::Container,gtk::Widget,@implements gtk::Buildable;}
struct Fixture {
    w: gtk::Window,
    host: Host,
    g: ResizeGate,
    c: Probe,
}
impl Fixture {
    fn new() -> Self {
        let w = gtk::Window::new(gtk::WindowType::Toplevel);
        w.set_default_size(600, 320);
        let host: Host = glib::Object::new();
        w.add(&host);
        let g = ResizeGate::new();
        let c: Probe = glib::Object::new();
        g.add(&c);
        host.put(&g, 20, 20);
        w.show_all();
        allocate(&g, 200, 120);
        g.commit();
        pump(30);
        Self { w, host, g, c }
    }
    fn hold(&self, ms: u64) -> Terminal {
        self.g.set_pace_ms(ms);
        let source = Terminal::new();
        self.g.wait_for_programs(std::slice::from_ref(&source));
        source
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            self.w.destroy();
        }
        pump(10);
    }
}
fn allocate(g: &ResizeGate, w: i32, h: i32) {
    g.size_allocate(&gtk::Allocation::new(20, 20, w, h));
}
#[test]
fn test_first_allocation_and_realization_are_immediate() {
    run(|| {
        let f = Fixture::new();
        assert!(f.g.is_realized());
        assert_eq!(f.c.allocated_width(), 200);
        assert_eq!(f.c.allocated_height(), 120);
        assert_eq!(f.g.children(), [f.c.clone().upcast::<gtk::Widget>()]);
        assert!(!f.g.has_pending_sources());
    });
}
#[test]
fn test_plain_child_follows_every_allocation() {
    run(|| {
        let f = Fixture::new();
        for (w, h) in [(260, 130), (280, 140), (150, 90), (200, 120)] {
            allocate(&f.g, w, h);
            assert_eq!((f.c.allocated_width(), f.c.allocated_height()), (w, h));
            assert_eq!(f.g.committed(), Some((w, h)));
        }
        assert!(!f.g.has_pending_sources());
    });
}
#[test]
fn test_empty_vte_does_not_wait_for_a_nonexistent_program() {
    run(|| {
        let f = Fixture::new();
        f.g.remove(&f.c);
        let t = Terminal::new();
        f.g.add(&t);
        f.g.show_all();
        for w in [500, 300, 450] {
            allocate(&f.g, w, 200);
            assert_eq!(f.g.committed(), Some((w, 200)));
            assert_eq!(f.g.waiting_count(), 0);
        }
        assert!(t.pty().is_none());
    });
}
#[test]
fn test_waiting_terminal_holds_geometry_until_it_answers() {
    run(|| {
        let f = Fixture::new();
        let t = f.hold(10000);
        allocate(&f.g, 260, 130);
        allocate(&f.g, 280, 140);
        pump(60);
        assert_eq!(f.c.allocated_width(), 200);
        t.feed(b"redrawn");
        poll(|| f.c.allocated_width() == 280);
        assert_eq!(f.c.allocated_height(), 140);
        assert!(!f.g.has_pending_sources());
    });
}
#[test]
fn test_silent_terminal_is_released_after_pace() {
    run(|| {
        let f = Fixture::new();
        let _t = f.hold(60);
        allocate(&f.g, 260, 130);
        let start = std::time::Instant::now();
        assert_eq!(f.c.allocated_width(), 200);
        poll(|| f.c.allocated_width() == 260);
        assert!(start.elapsed().as_millis() >= 40);
        assert_eq!(f.g.waiting_count(), 0);
    });
}
#[test]
fn test_commit_when_answered_waits_then_commits() {
    run(|| {
        let f = Fixture::new();
        let t = f.hold(10000);
        let count = f.g.commit_count();
        f.g.commit_when_answered();
        assert_eq!(f.g.commit_count(), count);
        t.feed(b"answer");
        poll(|| f.g.commit_count() == count + 1);
        assert_eq!(f.g.waiting_count(), 0);
    });
}
#[test]
fn test_identical_allocation_while_waiting_keeps_committed_geometry() {
    run(|| {
        let f = Fixture::new();
        let _t = f.hold(10000);
        allocate(&f.g, 250, 130);
        f.c.queue_resize();
        allocate(&f.g, 250, 130);
        assert_eq!(f.c.allocated_width(), 200);
        assert_eq!(f.g.waiting_count(), 1);
    });
}
#[test]
fn test_commit_skips_waiting_and_return_to_committed_size() {
    run(|| {
        let f = Fixture::new();
        let _t = f.hold(10000);
        allocate(&f.g, 270, 140);
        f.g.commit();
        assert_eq!(f.c.allocated_width(), 270);
        allocate(&f.g, 310, 150);
        allocate(&f.g, 270, 140);
        pump(50);
        assert_eq!(f.c.allocated_width(), 270);
    });
}
#[test]
fn test_destroy_cancels_waiting() {
    run(|| {
        let f = Fixture::new();
        let t = f.hold(10000);
        allocate(&f.g, 250, 130);
        unsafe {
            f.g.destroy();
        }
        assert!(!f.g.has_pending_sources());
        assert_eq!(f.g.waiting_count(), 0);
        f.g.commit();
        t.feed(b"late");
        pump(30);
    });
}
#[test]
fn test_hidden_removed_destroyed_and_replacement_children() {
    run(|| {
        let f = Fixture::new();
        f.c.hide();
        allocate(&f.g, 250, 130);
        f.g.commit();
        assert_eq!(f.c.allocated_width(), 200);
        f.g.show_all();
        allocate(&f.g, 250, 130);
        assert_eq!(f.c.allocated_width(), 250);
        f.g.remove(&f.c);
        assert!(!f.g.has_pending_sources());
        let replacement: Probe = glib::Object::new();
        f.g.add(&replacement);
        f.g.show_all();
        allocate(&f.g, 280, 140);
        assert_eq!(replacement.allocated_width(), 280);
        unsafe {
            replacement.destroy();
        }
        f.g.commit();
        assert!(f.g.child().is_none());
    });
}
#[test]
fn test_reparent_while_waiting() {
    run(|| {
        let f = Fixture::new();
        let _t = f.hold(10000);
        allocate(&f.g, 280, 140);
        f.host.remove(&f.g);
        assert!(!f.g.has_pending_sources());
        let host: Host = glib::Object::new();
        let w = window(&host);
        host.put(&f.g, 0, 0);
        w.show_all();
        allocate(&f.g, 280, 140);
        f.g.commit();
        assert_eq!(f.c.allocated_width(), 280);
        unsafe {
            w.destroy();
        }
    });
}
#[test]
fn test_minimum_natural_size_and_tiny_paned() {
    run(|| {
        let f = Fixture::new();
        assert_eq!(f.g.preferred_width(), (0, 160));
        assert_eq!(f.g.preferred_height(), (0, 90));
        f.g.set_child_minimum(true);
        assert_eq!(f.g.preferred_width(), (1, 160));
        allocate(&f.g, 1, 1);
        assert_eq!(f.c.allocated_width(), 1);
    });
}
#[test]
fn test_nonpositive_allocation_and_child_margins() {
    run(|| {
        let f = Fixture::new();
        f.c.set_margin_start(8);
        f.c.set_margin_end(8);
        allocate(&f.g, 0, 0);
        f.g.commit();
        assert_eq!(f.g.committed(), Some((1, 1)));
        assert!(f.c.allocated_width() >= 1);
    });
}
#[test]
fn test_background_padding_and_native_child_clipping_pixels() {
    run(|| {
        let f = Fixture::new();
        f.g.set_background("#445566").unwrap();
        assert!(f.g.set_background("invalid").is_err());
        let _t = f.hold(10000);
        allocate(&f.g, 300, 150);
        poll(|| {
            f.g.window()
                .unwrap()
                .pixbuf(270, 130, 1, 1)
                .is_some_and(|p| p.read_pixel_bytes()[..3] == [68, 85, 102])
        });
        let pix = f.g.window().unwrap().pixbuf(270, 130, 1, 1).unwrap();
        assert_eq!(&pix.read_pixel_bytes()[..3], &[68, 85, 102]);
        assert_eq!(f.c.allocated_width(), 200);
        allocate(&f.g, 100, 60);
        assert_eq!(f.g.window().unwrap().width(), 100);
        assert_eq!(f.c.allocated_width(), 200);
    });
}
#[test]
fn test_vte_page_reflow_setting_and_history_are_preserved() {
    run(|| {
        let d = BashDock::new(false, 1, false, "C");
        d.ready();
        let t = d.dock.terminal(0);
        command(&t, "printf 'history-byte-line\\n'");
        poll(|| text(&t).contains("history-byte-line"));
        let before = text(&t);
        d.resize(500, 750);
        d.resize(1200, 750);
        assert!(t.rewrap_on_resize());
        assert!(text(&t).contains("history-byte-line"));
        assert!(before.contains("history-byte-line"));
    });
}
#[test]
fn test_pty_terminal_ignores_contents_change_before_output() {
    run(|| {
        let d = BashDock::new(false, 1, false, "C");
        d.ready();
        let p = d.dock.panel.panes()[0].clone();
        let t = p.terminal.as_ref().unwrap();
        let g = p.gate.as_ref().unwrap();
        g.set_pace_ms(200);
        unsafe {
            libc::kill(p.pid().unwrap(), libc::SIGSTOP);
        }
        g.wait_for_programs(std::slice::from_ref(t));
        t.emit_by_name::<()>("contents-changed", &[]);
        assert_eq!(g.waiting_count(), 1);
        unsafe {
            libc::kill(p.pid().unwrap(), libc::SIGCONT);
        }
        poll(|| g.waiting_count() == 0);
    });
}
#[test]
fn test_drag_comparison() {
    run(|| {
        for stacked in [false, true] {
            let d = BashDock::new(stacked, 2, false, "C");
            d.ready();
            for delta in [-40., 70., -20., -10.] {
                d.dock.panel.body().move_handle(0, delta);
                pump(25);
            }
            pump(160);
            d.assert_prompt();
            d.assert_wraps();
        }
    });
}
#[test]
fn test_one_manual_resize_needs_reflow_mitigation() {
    run(|| {
        let d = BashDock::new(false, 1, false, "C");
        d.ready();
        d.resize(250, 750);
        d.resize(1200, 750);
        d.assert_prompt();
    });
}
#[test]
fn test_answer_in_flight_extends_release_once() {
    run(|| {
        let d = BashDock::new(false, 1, false, "C");
        d.ready();
        let p = d.dock.panel.panes()[0].clone();
        let t = p.terminal.as_ref().unwrap();
        let g = p.gate.as_ref().unwrap();
        g.set_pace_ms(60);
        let block =
            t.connect_contents_changed(|t| t.stop_signal_emission_by_name("contents-changed"));
        let started = std::time::Instant::now();
        g.wait_for_programs(std::slice::from_ref(t));
        t.feed_child(&[0x0c]);
        pump(80);
        assert_eq!(g.waiting_count(), 1);
        poll(|| g.waiting_count() == 0);
        assert!(started.elapsed().as_millis() >= 100);
        t.disconnect(block);
    });
}
#[test]
fn test_fatal_criticals_subprocess() {
    let exe = std::env::current_exe().unwrap();
    let status = std::process::Command::new(exe)
        .args(["--exact", "test_nonpositive_allocation_and_child_margins"])
        .env("G_DEBUG", "fatal-criticals")
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
}
