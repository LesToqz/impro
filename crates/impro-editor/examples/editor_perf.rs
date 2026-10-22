//! Criterion-free timings with real GTK draws. Run optimized under Xvfb.
use gtk::prelude::*;
use impro_editor::{EditorPanel, Response};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};
fn pump() {
    if gtk::events_pending() {
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
}
fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
fn main() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.py");
    let line = "value = compute(123, \"text\")  # a short comment     \n";
    let size = std::env::args().nth(1).unwrap_or_else(|| "lines".into());
    let text = if size == "lines" {
        line.repeat(20000)
    } else {
        let bytes: usize = size.parse().expect("byte count or lines");
        let mut text = line.repeat(bytes / line.len());
        text.push_str(&" ".repeat(bytes - text.len()));
        text
    };
    std::fs::write(&path, &text).unwrap();
    gtk::init().expect("GTK display");
    gtk::Settings::default()
        .unwrap()
        .set_gtk_cursor_blink(false);
    impro_editor::init();
    impro_ui::apply_style(None).unwrap();
    let panel = EditorPanel::default();
    panel.set_dialog_hook(Some(Rc::new(|_, _| Response::Discard)));
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(900, 600);
    window.add(&panel);
    window.show_all();
    pump();
    if std::env::var_os("IMPRO_EDITOR_TRACE").is_some() {
        let start = Instant::now();
        let _ = gtk::IconTheme::default().unwrap().load_icon(
            "window-close-symbolic",
            14,
            gtk::IconLookupFlags::FORCE_SYMBOLIC,
        );
        eprintln!("warm close icon: {:?}", start.elapsed());
    }
    let begin = Instant::now();
    let painted = Rc::new(Cell::new(None));
    let p = painted.clone();
    let weak = panel.downgrade();
    let handler = panel.connect_local("draw", true, move |_| {
        if weak.upgrade().is_some_and(|panel| !panel.tabs().is_empty()) && p.get().is_none() {
            p.set(Some(begin.elapsed()));
        }
        Some(false.to_value())
    });
    panel.open_file(&path);
    until(|| painted.get().is_some() || (!panel.is_busy() && panel.last_error().is_some()));
    if let Some(error) = panel.last_error() {
        println!("open-bytes={} rejected={error}", text.len());
        panel.shutdown();
        unsafe {
            window.destroy();
        }
        return;
    }
    let open = painted.get().unwrap();
    panel.disconnect(handler);
    let tab = panel.current_tab().unwrap();
    until(|| !panel.is_busy());
    println!(
        "load-complete-ms={:.3} max-load-slice-ms={:.3}",
        ms(begin.elapsed()),
        ms(tab.max_load_slice())
    );
    // Type into the visible first line after its cursor relocation has drawn.
    // A caret left at EOF would measure an implicit document-length scroll too.
    let mut first = tab.buffer.start_iter();
    tab.buffer.place_cursor(&first);
    tab.view.scroll_to_iter(&mut first, 0.0, true, 0.0, 0.0);
    let ready = Rc::new(Cell::new(false));
    let drawn = ready.clone();
    let cursor_draw = tab.view.connect_local("draw", true, move |_| {
        drawn.set(true);
        Some(false.to_value())
    });
    tab.view.queue_draw();
    until(|| ready.get());
    tab.view.disconnect(cursor_draw);
    measure_typing(&tab, "background", 30);
    let highlight = Instant::now();
    until(|| !tab.highlighter.pending());
    println!("open-bytes={} lines={} first-text-paint-ms={:.3} highlight-after-paint-ms={:.3} max-tag-batch-ms={:.3}",text.len(),tab.buffer.line_count(),ms(open),ms(highlight.elapsed()),ms(tab.highlighter.max_batch()));
    if let Ok(path) = std::env::var("IMPRO_EDITOR_SCREENSHOT") {
        window
            .window()
            .unwrap()
            .pixbuf(0, 0, window.allocated_width(), window.allocated_height())
            .unwrap()
            .savev(path, "png", &[])
            .unwrap();
    }
    tab.buffer.place_cursor(&tab.buffer.start_iter());
    pump();
    let painted_edits = Rc::new(RefCell::new(Vec::new()));
    let pending = Rc::new(Cell::new(None::<Instant>));
    let samples = painted_edits.clone();
    let start_draw = pending.clone();
    let handler = tab.view.connect_local("draw", true, move |_| {
        if let Some(start) = start_draw.take() {
            samples.borrow_mut().push(ms(start.elapsed()));
        }
        Some(false.to_value())
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
    if std::env::var_os("IMPRO_EDITOR_PERF_ASSERT").is_some() {
        assert!(
            paints[190] < 8.0,
            "settled typing p95 {:.3} ms exceeds 8 ms",
            paints[190]
        );
    }
    let incremental = Instant::now();
    until(|| !tab.highlighter.pending());
    println!(
        "incremental-highlight-after-typing-ms={:.3}",
        ms(incremental.elapsed())
    );
    let frames = Rc::new(Cell::new(0usize));
    let count = frames.clone();
    let handler = tab.view.connect_local("draw", true, move |_| {
        count.set(count.get() + 1);
        Some(false.to_value())
    });
    let start = Instant::now();
    let mut worst_cpu = Duration::ZERO;
    let mut frame_samples = Vec::new();
    for i in 0..100 {
        let before = frames.get();
        let frame = Instant::now();
        let adjustment = tab.source_scroll.vadjustment();
        let maximum = (adjustment.upper() - adjustment.page_size()).max(0.0);
        adjustment.set_value((adjustment.page_size() * i as f64).min(maximum));
        pump();
        worst_cpu = worst_cpu.max(frame.elapsed());
        tab.view.queue_draw();
        until(|| frames.get() > before);
        frame_samples.push(ms(frame.elapsed()));
    }
    tab.view.disconnect(handler);
    frame_samples.sort_by(f64::total_cmp);
    println!("scroll-bytes={} rendered-pages-100-ms={:.3} max-dispatch-ms={:.3} draw-p50-ms={:.3} draw-p95-ms={:.3} max-frame-ms={:.3} visible-lines={}",text.len(),ms(start.elapsed()),ms(worst_cpu),frame_samples[50],frame_samples[95],frame_samples[99],tab.chrome.painted_lines());
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_millis(500) {
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    until(|| !tab.highlighter.pending());
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

fn measure_typing(tab: &impro_editor::EditorTab, phase: &str, count: usize) {
    let pending = Rc::new(Cell::new(None::<Instant>));
    let samples = Rc::new(RefCell::new(Vec::new()));
    let p = pending.clone();
    let s = samples.clone();
    let handler = tab.view.connect_local("draw", true, move |_| {
        if let Some(start) = p.take() {
            s.borrow_mut().push(ms(start.elapsed()));
        }
        Some(false.to_value())
    });
    let mut active = 0;
    for i in 0..count {
        active += usize::from(tab.highlighter.pending());
        pending.set(Some(Instant::now()));
        tab.buffer.begin_user_action();
        tab.buffer.insert_at_cursor("x");
        tab.buffer.end_user_action();
        tab.view.queue_draw();
        until(|| samples.borrow().len() > i);
    }
    tab.view.disconnect(handler);
    let mut samples = samples.borrow().clone();
    samples.sort_by(f64::total_cmp);
    let p95 = samples[count * 95 / 100];
    println!("typing-phase={phase} samples={count} highlight-pending-samples={active} draw-p50-ms={:.3} draw-p95-ms={p95:.3} draw-max-ms={:.3}", samples[count / 2], samples[count - 1]);
    if std::env::var_os("IMPRO_EDITOR_PERF_ASSERT").is_some() {
        assert!(p95 < 8.0, "{phase} typing p95 {p95:.3} ms exceeds 8 ms");
    }
}
