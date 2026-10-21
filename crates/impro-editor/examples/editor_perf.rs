//! Criterion-free timings with real GTK draws. Run optimized under Xvfb.
use gtk::prelude::*;
use impro_editor::{EditorPanel, Response};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
fn pump() {
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
}
fn until(f: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(60);
    while !f() {
        pump();
        assert!(Instant::now() < end, "timed out");
        std::thread::sleep(Duration::from_micros(200));
    }
    pump();
}
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
fn main() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.py");
    let line = "value = compute(123, \"text\")  # a short comment     \n";
    let text = line.repeat(20000);
    std::fs::write(&path, &text).unwrap();
    let big = dir.path().join("scroll.txt");
    let limit = 5 * 1024 * 1024;
    let scroll_line = "a plain line of text\n";
    let mut scroll_text = scroll_line.repeat(limit / scroll_line.len());
    scroll_text.push_str(&"x".repeat(limit - scroll_text.len()));
    std::fs::write(&big, &scroll_text).unwrap();
    gtk::init().expect("GTK display");
    impro_editor::init();
    impro_ui::apply_style(None).unwrap();
    let panel = EditorPanel::default();
    panel.set_dialog_hook(Some(Rc::new(|_, _| Response::Discard)));
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(900, 600);
    window.add(&panel);
    window.show_all();
    pump();
    let begin = Instant::now();
    let painted = Rc::new(Cell::new(None));
    let p = painted.clone();
    let handler = panel.connect_draw(move |panel, _| {
        if !panel.tabs().is_empty() && p.get().is_none() {
            p.set(Some(begin.elapsed()));
        }
        glib::Propagation::Proceed
    });
    panel.open_file(&path);
    until(|| painted.get().is_some());
    let open = painted.get().unwrap();
    panel.disconnect(handler);
    let tab = panel.current_tab().unwrap();
    let highlight = Instant::now();
    until(|| !tab.highlighter.pending());
    println!("open-bytes={} lines={} first-text-paint-ms={:.3} highlight-after-paint-ms={:.3} max-tag-batch-ms={:.3}",text.len(),tab.buffer.line_count(),ms(open),ms(highlight.elapsed()),ms(tab.highlighter.max_batch()));
    tab.buffer.place_cursor(&tab.buffer.start_iter());
    pump();
    let painted_edits = Rc::new(RefCell::new(Vec::new()));
    let pending = Rc::new(Cell::new(None::<Instant>));
    let samples = painted_edits.clone();
    let start_draw = pending.clone();
    let handler = tab.view.connect_draw(move |_, _| {
        if let Some(start) = start_draw.take() {
            samples.borrow_mut().push(ms(start.elapsed()));
        }
        glib::Propagation::Proceed
    });
    let mut edits = Vec::new();
    for index in 0..200 {
        let start = Instant::now();
        pending.set(Some(start));
        tab.buffer.begin_user_action();
        tab.buffer.insert_at_cursor("x");
        tab.buffer.end_user_action();
        edits.push(ms(start.elapsed()));
        tab.view.queue_draw();
        until(|| painted_edits.borrow().len() > index);
    }
    tab.view.disconnect(handler);
    edits.sort_by(f64::total_cmp);
    let mut paints = painted_edits.borrow().clone();
    paints.sort_by(f64::total_cmp);
    println!("type-200-edit-total-ms={:.3} median-ms={:.3} p95-ms={:.3} max-ms={:.3} edit-to-draw-median-ms={:.3} p95-ms={:.3} max-ms={:.3}",edits.iter().sum::<f64>(),edits[100],edits[190],edits[199],paints[100],paints[190],paints[199]);
    let incremental = Instant::now();
    until(|| !tab.highlighter.pending());
    println!(
        "incremental-highlight-after-typing-ms={:.3}",
        ms(incremental.elapsed())
    );
    panel.open_file(&big);
    until(|| !panel.is_busy());
    let tab = panel.current_tab().unwrap();
    assert_eq!(tab.path(), big, "scroll fixture must actually open");
    assert_eq!(tab.buffer.char_count() as usize, limit);
    pump();
    let frames = Rc::new(Cell::new(0usize));
    let count = frames.clone();
    let handler = tab.view.connect_draw(move |_, _| {
        count.set(count.get() + 1);
        glib::Propagation::Proceed
    });
    let start = Instant::now();
    let mut worst_cpu = Duration::ZERO;
    let mut worst_frame = Duration::ZERO;
    for i in 0..100 {
        let before = frames.get();
        let frame = Instant::now();
        let adjustment = tab.source_scroll.vadjustment();
        let maximum = (adjustment.upper() - adjustment.page_size()).max(0.0);
        adjustment.set_value(maximum * i as f64 / 100.0);
        pump();
        worst_cpu = worst_cpu.max(frame.elapsed());
        tab.view.queue_draw();
        until(|| frames.get() > before);
        worst_frame = worst_frame.max(frame.elapsed());
    }
    tab.view.disconnect(handler);
    println!("scroll-bytes={} rendered-sweep-100-ms={:.3} max-dispatch-ms={:.3} max-frame-ms={:.3} visible-lines={}",5000000,ms(start.elapsed()),ms(worst_cpu),ms(worst_frame),tab.chrome.painted_lines());
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_millis(500) {
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    let cpu_start = cpu_time();
    let idle_start = Instant::now();
    let event_loop = glib::MainLoop::new(None, false);
    let finish = event_loop.clone();
    glib::timeout_add_local_once(Duration::from_secs(1), move || finish.quit());
    event_loop.run();
    println!(
        "idle-process-cpu-percent={:.3}",
        (cpu_time() - cpu_start) / idle_start.elapsed().as_secs_f64() * 100.0
    );
    panel.shutdown();
    unsafe {
        window.destroy();
    }
    pump();
}
fn cpu_time() -> f64 {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut value);
    }
    value.tv_sec as f64 + value.tv_nsec as f64 / 1e9
}
