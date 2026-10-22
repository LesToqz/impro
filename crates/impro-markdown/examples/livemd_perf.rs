//! Criterion-free timings of the live renderer with real GTK draws. Run optimized
//! under Xvfb: `cargo run --release -p impro-markdown --example livemd_perf`.
use gtk::prelude::*;
use impro_editor::{EditorPanel, Response};
use impro_markdown::livemd;
use std::{
    cell::{Cell, RefCell},
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

fn pump() {
    while gtk::events_pending() {
        gtk::main_iteration_do(false);
    }
}
fn until(f: impl Fn() -> bool) {
    let end = Instant::now() + Duration::from_secs(120);
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

/// A mixed note of about `bytes`: headings, prose with inline spans, lists,
/// tasks, quotes, code, rules, links and small tables.
fn mixed_note(bytes: usize) -> String {
    let mut out = String::with_capacity(bytes + 1024);
    let mut n = 0;
    while out.len() < bytes {
        n += 1;
        out.push_str(&format!("## Section {n}\n\n"));
        out.push_str("Prose with **bold**, *italic*, `code` and a [link](https://example.org/page) plus [[note|alias]]. More words follow here so the line wraps a little in the column.\n\n");
        out.push_str("- first item\n  - nested item\n- [ ] open task\n- [x] done task\n\n");
        out.push_str("1. one\n2. two\n3. three\n\n> A quoted remark worth keeping.\n\n");
        if n % 3 == 0 {
            out.push_str("```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n");
        }
        if n % 4 == 0 {
            out.push_str("| Task | Owner | State |\n| :--- | :---: | ---: |\n| Review | Team | Ready |\n| Write | Me | Next |\n\n");
        }
        if n % 5 == 0 {
            out.push_str("---\n\n");
        }
    }
    out
}
fn table_note(bytes: usize) -> String {
    let mut out = String::new();
    let mut n = 0;
    while out.len() < bytes {
        n += 1;
        out.push_str(&format!(
            "### Table {n}\n\n| Name | Value | Notes |\n| --- | ---: | :---: |\n"
        ));
        for row in 0..20 {
            out.push_str(&format!(
                "| item {n}-{row} | {} | **note** {row} |\n",
                row * 7
            ));
        }
        out.push('\n');
    }
    out
}

struct Run {
    panel: EditorPanel,
    window: gtk::Window,
}
fn new_run() -> Run {
    impro_ui::init();
    impro_editor::init();
    impro_markdown::init();
    impro_ui::apply_style(None).unwrap();
    let panel = EditorPanel::default();
    panel.set_dialog_hook(Some(Rc::new(|_, _| Response::Discard)));
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(1000, 800);
    window.add(&panel);
    window.show_all();
    pump();
    Run { panel, window }
}

fn open_and_measure(run: &Run, path: &Path, label: &str) -> impro_markdown::livemd::LiveMarkdown {
    let begin = Instant::now();
    let first = Rc::new(Cell::new(None));
    let seen = first.clone();
    let weak = run.panel.downgrade();
    let handler = run.panel.connect_local("draw", true, move |_| {
        if seen.get().is_none() {
            if let Some(tab) = weak.upgrade().and_then(|p| p.current_tab()) {
                if let Some(live) = livemd::for_view(&tab.view) {
                    if live.enabled() && live.viewport_painted() && live.block_count() > 0 {
                        seen.set(Some(begin.elapsed()));
                    }
                }
            }
        }
        Some(false.to_value())
    });
    run.panel.open_file(path);
    until(|| first.get().is_some());
    run.panel.disconnect(handler);
    let tab = run.panel.current_tab().unwrap();
    let live = livemd::for_view(&tab.view).expect("live renderer");
    let decorated = first.get().unwrap();
    let settle = Instant::now();
    until(|| !live.pending() && live.unpainted_blocks() == 0 && live.jobs_in_flight() == 0);
    println!(
        "{label}: bytes={} blocks={} first-decorated-screen-ms={:.1} fully-painted-after-ms={:.1} max-slice-ms={:.2}",
        tab.buffer.char_count(),
        live.block_count(),
        ms(decorated),
        ms(settle.elapsed()),
        ms(live.max_slice_time())
    );
    live
}

fn typing(run: &Run, label: &str) {
    let tab = run.panel.current_tab().unwrap();
    let adjustment = tab.source_scroll.vadjustment();
    adjustment.set_value((adjustment.upper() - adjustment.page_size()) / 2.0);
    until(|| true);
    let (iter, _) = tab.view.line_at_y(tab.view.visible_rect().y() + 200);
    tab.buffer.place_cursor(&iter);
    let mut end = tab.buffer.iter_at_mark(&tab.buffer.get_insert().unwrap());
    end.forward_to_line_end();
    tab.buffer.place_cursor(&end);
    until(|| true);
    let samples = Rc::new(RefCell::new(Vec::new()));
    let pending = Rc::new(Cell::new(None::<Instant>));
    let (s, p) = (samples.clone(), pending.clone());
    let handler = tab.view.connect_local("draw", true, move |_| {
        if let Some(start) = p.take() {
            s.borrow_mut().push(ms(start.elapsed()));
        }
        Some(false.to_value())
    });
    let mut inserts = Vec::new();
    let text = "typing into a rendered note ";
    for index in 0..200 {
        let start = Instant::now();
        pending.set(Some(start));
        tab.buffer.begin_user_action();
        tab.buffer
            .insert_at_cursor(&text[index % text.len()..index % text.len() + 1]);
        tab.buffer.end_user_action();
        inserts.push(ms(start.elapsed()));
        tab.view.queue_draw();
        until(|| samples.borrow().len() > index);
        if index % 40 == 0 && std::env::var_os("PERF_TRACE").is_some() {
            eprintln!(
                "typed {index}: insert {:.1}ms total {:.1}ms",
                inserts[index],
                ms(start.elapsed())
            );
        }
    }
    tab.view.disconnect(handler);
    inserts.sort_by(f64::total_cmp);
    let mut paints = samples.borrow().clone();
    paints.sort_by(f64::total_cmp);
    println!(
        "{label}: type-200 insert-median-ms={:.3} insert-max-ms={:.3} key-to-paint-median-ms={:.3} p95-ms={:.3} max-ms={:.3}",
        inserts[100], inserts[199], paints[100], paints[190], paints[199]
    );
}

fn scroll_sweep(run: &Run, label: &str) {
    let tab = run.panel.current_tab().unwrap();
    let frames = Rc::new(Cell::new(0usize));
    let count = frames.clone();
    let handler = tab.view.connect_local("draw", true, move |_| {
        count.set(count.get() + 1);
        Some(false.to_value())
    });
    let adjustment = tab.source_scroll.vadjustment();
    let maximum = (adjustment.upper() - adjustment.page_size()).max(0.0);
    let start = Instant::now();
    let (mut worst_dispatch, mut worst_frame) = (Duration::ZERO, Duration::ZERO);
    for i in 0..100 {
        let before = frames.get();
        let frame = Instant::now();
        adjustment.set_value(maximum * f64::from(i) / 100.0);
        pump();
        worst_dispatch = worst_dispatch.max(frame.elapsed());
        tab.view.queue_draw();
        until(|| frames.get() > before);
        worst_frame = worst_frame.max(frame.elapsed());
    }
    tab.view.disconnect(handler);
    println!(
        "{label}: scroll-sweep-100-ms={:.1} max-dispatch-ms={:.2} max-frame-ms={:.2}",
        ms(start.elapsed()),
        ms(worst_dispatch),
        ms(worst_frame)
    );
}

fn idle_cpu(label: &str) {
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_millis(400) {
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    let (cpu, wall) = (cpu_time(), Instant::now());
    let main = glib::MainLoop::new(None, false);
    let finish = main.clone();
    glib::timeout_add_local_once(Duration::from_secs(1), move || finish.quit());
    main.run();
    println!(
        "{label}: idle-cpu-percent={:.3}",
        (cpu_time() - cpu) / wall.elapsed().as_secs_f64() * 100.0
    );
}
fn cpu_time() -> f64 {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut value) };
    value.tv_sec as f64 + value.tv_nsec as f64 / 1e9
}

fn main() {
    let dir = tempfile::tempdir().unwrap();
    let notes = dir.path().join(".impro/Notes");
    std::fs::create_dir_all(&notes).unwrap();
    gtk::init().expect("GTK display");
    let run = new_run();
    let which = std::env::args().nth(1).unwrap_or_default();
    let cases: [(&str, String); 3] = [
        ("note-500kb", mixed_note(500_000)),
        ("note-1mb", mixed_note(1_040_000)),
        ("tables-200kb", table_note(200_000)),
    ];
    for (name, text) in cases {
        if !which.is_empty() && which != name {
            continue;
        }
        let text = if name == "note-1mb" {
            let mut t = text;
            t.truncate(livemd::MAX_BYTES - 16);
            while !t.is_char_boundary(t.len()) {
                t.pop();
            }
            t
        } else {
            text
        };
        let path = notes.join(format!("{name}.md"));
        std::fs::write(&path, &text).unwrap();
        let live = open_and_measure(&run, &path, name);
        if std::env::var_os("PERF_SOURCE").is_some() {
            run.panel.current_tab().unwrap().set_markdown_mode("source");
            until(|| true);
        }
        scroll_sweep(&run, name);
        typing(&run, name);
        idle_cpu(name);
        let _ = live;
        let tab = run.panel.current_tab().unwrap();
        tab.buffer.set_modified(false);
    }
    // Scrolling the whole document while it is still being painted.
    if which.is_empty() || which == "scroll-while-painting" {
        let path = notes.join("note-500kb.md");
        let tab = run.panel.current_tab();
        drop(tab);
        run.panel.close_current(|_| {});
        pump();
        let text = mixed_note(500_000);
        std::fs::write(&path, text).unwrap();
        run.panel.open_file(&path);
        let begin = Instant::now();
        until(|| {
            run.panel.current_tab().is_some_and(|t| {
                livemd::for_view(&t.view).is_some_and(|l| l.enabled() && l.block_count() > 0)
            })
        });
        let tab = run.panel.current_tab().unwrap();
        let live = livemd::for_view(&tab.view).unwrap();
        let adjustment = tab.source_scroll.vadjustment();
        let maximum = (adjustment.upper() - adjustment.page_size()).max(0.0);
        let mut worst = Duration::ZERO;
        for i in 0..40 {
            let frame = Instant::now();
            adjustment.set_value(maximum * f64::from(i) / 40.0);
            until(|| live.viewport_painted());
            worst = worst.max(frame.elapsed());
        }
        println!(
            "scroll-while-painting: jump-to-painted-screen-max-ms={:.1} total-ms={:.1} unpainted-left={}",
            ms(worst),
            ms(begin.elapsed()),
            live.unpainted_blocks()
        );
    }
    run.panel.shutdown();
    unsafe { run.window.destroy() };
    pump();
}
