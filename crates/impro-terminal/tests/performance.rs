mod common;
use common::*;
use gtk::prelude::*;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    rc::Rc,
    time::Instant,
};
struct Counting;
thread_local! {static ENABLE:Cell<bool>=const{Cell::new(false)};static ALLOCS:Cell<usize>=const{Cell::new(0)};}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ENABLE.with(|e| {
            if e.get() {
                ALLOCS.with(|c| c.set(c.get() + 1));
            }
        });
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ENABLE.with(|e| {
            if e.get() {
                ALLOCS.with(|c| c.set(c.get() + 1));
            }
        });
        System.realloc(p, l, n)
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
fn rss() -> usize {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .unwrap()
        .parse()
        .unwrap()
}
#[test]
#[ignore = "measured GTK benchmark; run explicitly under Xvfb"]
fn terminal_performance() {
    run(|| {
        let d = Dock::new(false, 0);
        pump(120);
        let baseline = rss();
        let start = Instant::now();
        let paints = Rc::new(std::cell::RefCell::new(Vec::new()));
        for _ in 0..6 {
            let at = Instant::now();
            d.panel.new_terminal();
            let t = d
                .panel
                .panes()
                .into_iter()
                .max_by_key(|p| p.id)
                .unwrap()
                .frame
                .clone();
            let done = Rc::new(Cell::new(false));
            let out = paints.clone();
            t.connect_draw(move |_, _| {
                if !done.replace(true) {
                    out.borrow_mut().push(at.elapsed().as_secs_f64() * 1000.);
                }
                glib::Propagation::Proceed
            });
        }
        let construction = start.elapsed().as_secs_f64() * 1000.;
        poll(|| paints.borrow().len() == 6);
        let first_paint = start.elapsed().as_secs_f64() * 1000.;
        d.ready();
        let open_ms = start.elapsed().as_secs_f64() * 1000.;
        let open_rss = rss();
        println!("open-six construction-ms={construction:.3} all-first-paints-ms={first_paint:.3} async-shells-ready-ms={open_ms:.3} pane-paint-ms={:?} rss-kib={open_rss} delta-kib={}",paints.borrow(),open_rss as i64-baseline as i64);

        while d.panel.pane_count() > 1 {
            d.panel.close_current();
        }
        pump(100);
        let before = rss();
        let start = Instant::now();
        for _ in 0..100 {
            d.panel.new_terminal();
            let p = d.panel.panes().into_iter().max_by_key(|p| p.id).unwrap();
            poll(|| p.pid().is_some());
            let pid = p.pid().unwrap();
            let weak = p.frame.downgrade();
            d.panel.close_pane(p.id);
            drop(p);
            poll(|| dead(pid) && weak.upgrade().is_none());
        }
        pump(120);
        println!(
            "split-kill-100 ms={:.3} rss-before-kib={before} rss-after-kib={} delta-kib={}",
            start.elapsed().as_secs_f64() * 1000.,
            rss(),
            rss() as i64 - before as i64
        );
        d.panel.new_terminal();
        d.ready();
        let body = d.panel.body();
        let handle = body.handles()[0].clone();
        let initial = body.pane_sizes();
        pointer(&handle, gdk::EventType::ButtonPress, 1000.);
        let events: Vec<_> = (0..200)
            .map(|i| pointer_event(&handle, gdk::EventType::MotionNotify, 1000. + i as f64 / 4.))
            .collect();
        let before_commits: Vec<_> = d
            .panel
            .panes()
            .iter()
            .map(|p| p.gate.as_ref().unwrap().commit_count())
            .collect();
        let before_rss = rss();
        let start = Instant::now();
        ALLOCS.set(0);
        ENABLE.set(true);
        for event in &events {
            assert!(handle.event(event));
        }
        ENABLE.set(false);
        let allocs = ALLOCS.get();
        let motion_ms = start.elapsed().as_secs_f64() * 1000.;
        pointer(&handle, gdk::EventType::ButtonRelease, 1049.75);
        pump(180);
        let commits: Vec<_> = d
            .panel
            .panes()
            .iter()
            .zip(before_commits)
            .map(|(p, n)| p.gate.as_ref().unwrap().commit_count() - n)
            .collect();
        println!("drag-200 motion-ms={motion_ms:.3} settle-ms={:.3} rust-allocations={allocs} gate-commits={commits:?} rss-before-kib={before_rss} rss-after-kib={}",start.elapsed().as_secs_f64()*1000.-180.,rss());
        assert_eq!(allocs, 0);
        assert_eq!(body.pane_sizes()[0], initial[0] + 50);
        assert!(commits.iter().all(|n| *n <= 1));
        pump(150);
        assert_eq!(impro_ui::motion::active_count(), 0);
        assert!(!impro_ui::motion::clock_attached());
        for p in d.panel.panes() {
            assert!(!p.gate.as_ref().unwrap().has_pending_sources());
        }
    });
}
