//! Measurements for the explorer: big-directory expansion, search throughput, idle CPU.
//!
//! ```text
//! xvfb-run -a cargo run --release -p impro-explorer --example perf -- bigdir [ENTRIES]
//! xvfb-run -a cargo run --release -p impro-explorer --example perf -- tree  [FILES]
//! xvfb-run -a cargo run --release -p impro-explorer --example perf -- search PATH QUERY [--regex]
//! xvfb-run -a cargo run --release -p impro-explorer --example perf -- idle
//! ```
//! Every scenario prints `key=value` lines. `max-block-ms` is the longest single
//! main-loop iteration (dispatch plus any repaint) seen while the work ran, i.e. the
//! worst stall the user could feel.
use gtk::prelude::*;
use impro_explorer::explorer::ExplorerPanel;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

thread_local! {
    /// Frame-clock phase marks (`PERF_PHASES=1`) of the current main-loop iteration.
    static MARKS: RefCell<Vec<(&'static str, Instant)>> = const { RefCell::new(Vec::new()) };
}

fn mark(name: &'static str) {
    MARKS.with(|m| m.borrow_mut().push((name, Instant::now())));
}

thread_local! {
    static DRAWS: RefCell<Vec<(&'static str, f64)>> = const { RefCell::new(Vec::new()) };
}

/// Inclusive draw time of `widget` (`PERF_PHASES=1`), reported per slow iteration.
fn watch_draw(widget: &impl IsA<gtk::Widget>, name: &'static str) {
    if std::env::var_os("PERF_PHASES").is_none() {
        return;
    }
    let start = Rc::new(Cell::new(Instant::now()));
    let s = start.clone();
    widget.connect_draw(move |_, _| {
        s.set(Instant::now());
        glib::Propagation::Proceed
    });
    widget.connect_closure(
        "draw",
        true,
        glib::closure_local!(move |_w: gtk::Widget, _cr: gtk::cairo::Context| -> bool {
            let ms = start.get().elapsed().as_secs_f64() * 1e3;
            DRAWS.with(|d| d.borrow_mut().push((name, ms)));
            false
        }),
    );
}

/// Records when each frame-clock phase handler of `widget`'s window runs (they are
/// connected after GTK's own, so a mark is the end of that phase's GTK work).
fn watch_phases(widget: &impl IsA<gtk::Widget>) {
    if std::env::var_os("PERF_PHASES").is_none() {
        return;
    }
    let Some(clock) = widget.frame_clock() else {
        return;
    };
    clock.connect_update(|_| mark("update"));
    clock.connect_layout(|_| mark("layout"));
    clock.connect_paint(|_| mark("paint"));
    clock.connect_after_paint(|_| mark("after-paint"));
}

#[derive(Default)]
struct Probe {
    max: Duration,
    iterations: usize,
    busy: Duration,
    samples: Vec<f64>,
    rows: Rc<Cell<usize>>,
    row_samples: Vec<f64>,
    /// On-CPU time of the same iterations: wall time minus whatever the VM or other
    /// processes took away, so a preempted slice is not mistaken for slow work.
    cpu_samples: Vec<f64>,
}

/// Nanoseconds this thread has spent on a CPU (`/proc/thread-self/schedstat`).
fn thread_cpu_ns() -> u64 {
    std::fs::read_to_string("/proc/thread-self/schedstat")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok()))
        .unwrap_or(0)
}

/// Run the main loop until `done`, recording how long each dispatching iteration blocked.
fn run_until(probe: &mut Probe, timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        let started = Instant::now();
        let cpu_started = thread_cpu_ns();
        // Non-blocking: waiting for events is idle time, not main-thread work.
        probe.rows.set(0);
        MARKS.with(|m| m.borrow_mut().clear());
        DRAWS.with(|d| d.borrow_mut().clear());
        if context.iteration(false) {
            let spent = started.elapsed();
            if spent > Duration::from_millis(3) && std::env::var_os("PERF_PHASES").is_some() {
                let marks: Vec<String> = MARKS.with(|m| {
                    m.borrow()
                        .iter()
                        .map(|(n, t)| {
                            format!("{n}@{:.1}", t.duration_since(started).as_secs_f64() * 1e3)
                        })
                        .collect()
                });
                let draws: Vec<String> = DRAWS.with(|d| {
                    d.borrow_mut()
                        .drain(..)
                        .map(|(n, ms)| format!("{n} {ms:.1}"))
                        .collect()
                });
                eprintln!("  draws [{}]", draws.join(", "));
                eprintln!(
                    "iteration {} total {:.1} ms rows {} marks [{}]",
                    probe.iterations + 1,
                    spent.as_secs_f64() * 1e3,
                    probe.rows.get(),
                    marks.join(" ")
                );
            }
            probe.iterations += 1;
            probe.busy += spent;
            probe.samples.push(spent.as_secs_f64() * 1000.);
            probe
                .cpu_samples
                .push(thread_cpu_ns().saturating_sub(cpu_started) as f64 / 1e6);
            if probe.rows.get() > 0 {
                probe.row_samples.push(spent.as_secs_f64() * 1000.);
            }
            probe.max = probe.max.max(spent);
            if spent > Duration::from_millis(12) && std::env::var_os("PERF_VERBOSE").is_some() {
                eprintln!(
                    "slow iteration: {} ms (iteration {})",
                    ms(spent),
                    probe.iterations
                );
            }
        } else {
            std::thread::sleep(Duration::from_micros(300));
        }
    }
    false
}

fn print_percentiles(probe: &Probe) {
    let mut samples = probe.samples.clone();
    samples.sort_by(f64::total_cmp);
    let mut rows = probe.row_samples.clone();
    rows.sort_by(f64::total_cmp);
    if !rows.is_empty() {
        println!(
            "row-dispatch-p50-ms={:.3} row-dispatch-p95-ms={:.3} row-dispatch-max-ms={:.3}",
            rows[(rows.len() - 1) / 2],
            rows[(rows.len() - 1) * 95 / 100],
            rows[rows.len() - 1]
        );
    }
    let mut cpu = probe.cpu_samples.clone();
    cpu.sort_by(f64::total_cmp);
    if !cpu.is_empty() {
        println!(
            "cpu-dispatch-p50-ms={:.3} cpu-dispatch-p95-ms={:.3} cpu-dispatch-max-ms={:.3}",
            cpu[(cpu.len() - 1) / 2],
            cpu[(cpu.len() - 1) * 95 / 100],
            cpu[cpu.len() - 1]
        );
    }
    if !samples.is_empty() {
        println!(
            "dispatch-p50-ms={:.3} dispatch-p95-ms={:.3}",
            samples[(samples.len() - 1) / 2],
            samples[(samples.len() - 1) * 95 / 100]
        );
    }
}

fn ms(duration: Duration) -> String {
    format!("{:.1}", duration.as_secs_f64() * 1000.0)
}

fn cpu_seconds() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let fields: Vec<&str> = stat
        .rsplit(')')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    // utime and stime are fields 14 and 15 (indices 11 and 12 after the command).
    let ticks = |i: usize| {
        fields
            .get(i)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (ticks(11) + ticks(12)) / 100.0
}

fn window_for(child: &impl IsA<gtk::Widget>) -> gtk::Window {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(300, 720);
    window.set_decorated(false);
    window.add(child);
    window.show_all();
    window
}

fn make_tree(root: &Path, files: usize) {
    let dirs = (files / 50).max(1);
    for d in 0..dirs {
        let dir = root.join(format!("pkg{:03}/sub{}", d / 10, d % 10));
        std::fs::create_dir_all(&dir).unwrap();
        for f in 0..files / dirs {
            let body = format!(
                "// file {d}/{f}\nfn function_{f}() {{\n    let value = {};\n    println!(\"hello {{}}\", value);\n}}\n{}\n",
                d * 7 + f,
                if (d + f) % 97 == 0 { "// needle marker" } else { "// filler text for the search engine to read" }
            );
            std::fs::write(dir.join(format!("module_{f}.rs")), body).unwrap();
        }
    }
}

fn bigdir(entries: usize) {
    let temp = tempfile::tempdir().unwrap();
    let big = temp.path().join("big");
    std::fs::create_dir(&big).unwrap();
    for i in 0..entries {
        // A mix like a real cache folder: mostly files, a few sub-folders.
        if i % 500 == 0 {
            std::fs::create_dir(big.join(format!("dir{i:06}"))).unwrap();
        } else {
            std::fs::File::create(big.join(format!("File_{i:06}.dat"))).unwrap();
        }
    }
    let panel = ExplorerPanel::new(temp.path());
    let window = window_for(&panel);
    let mut probe = Probe::default();
    run_until(&mut probe, Duration::from_secs(5), || !panel.is_busy());
    // Let the first paint of the window finish before timing the expansion.
    run_until(&mut probe, Duration::from_millis(600), || false);
    println!("entries={entries}");
    let first = Rc::new(Cell::new(None::<Duration>));
    let inserted = Rc::new(Cell::new(0usize));
    let started = Rc::new(Cell::new(Instant::now()));
    {
        let (first, inserted, started) = (first.clone(), inserted.clone(), started.clone());
        let big_str = big.to_string_lossy().into_owned();
        let store = panel.store();
        panel.store().connect_row_inserted(move |_, path, iter| {
            // Rows directly below `big` (depth 2: root, big, row).
            if path.depth() == 3 {
                if let Some(parent) = store.iter_parent(iter) {
                    if impro_explorer::util::string(&store, &parent, 2) == big_str {
                        inserted.set(inserted.get() + 1);
                        if first.get().is_none() {
                            first.set(Some(started.get().elapsed()));
                        }
                    }
                }
            }
        });
    }
    let painted = Rc::new(Cell::new(None::<Duration>));
    {
        let (painted, first, started) = (painted.clone(), first.clone(), started.clone());
        panel.tree().connect_draw(move |_, _| {
            if painted.get().is_none() && first.get().is_some() {
                painted.set(Some(started.get().elapsed()));
            }
            glib::Propagation::Proceed
        });
    }
    let row = panel.find_row(&big).expect("big folder row");
    let path = impro_explorer::util::path_of(&panel.store(), &row);
    let mut probe = Probe::default();
    started.set(Instant::now());
    panel.tree().expand_row(&path, false);
    let finished = run_until(&mut probe, Duration::from_secs(60), || {
        !panel.is_busy() && inserted.get() > 0
    });
    let total = started.get().elapsed();
    println!("first-rows-ms={}", ms(first.get().unwrap_or_default()));
    println!(
        "first-paint-after-rows-ms={}",
        painted.get().map_or("-".into(), ms)
    );
    println!("all-rows-ms={}", ms(total));
    println!("rows={} finished={finished}", inserted.get());
    println!(
        "main-loop-dispatches={} max-block-ms={} main-thread-busy-ms={}",
        probe.iterations,
        ms(probe.max),
        ms(probe.busy)
    );
    print_percentiles(&probe);
    // Responsiveness while rows keep arriving: how long does a click-sized task wait?
    drop(window);
}

fn tree_scenario(files: usize) {
    let temp = tempfile::tempdir().unwrap();
    let begin = Instant::now();
    make_tree(temp.path(), files);
    println!("files={files} created-ms={}", ms(begin.elapsed()));
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(temp.path())
            .args(["-c", "user.name=T", "-c", "user.email=t@e.invalid"])
            .args(args)
            .output()
            .unwrap()
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-qm", "base"]);
    for i in 0..200 {
        let path = temp
            .path()
            .join(format!("pkg000/sub0/module_{}.rs", i % 50));
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        text.push_str("// edit\n");
        std::fs::write(path, text).unwrap();
    }
    for i in 0..300 {
        std::fs::write(temp.path().join(format!("untracked_{i}.txt")), "x").unwrap();
    }
    let started = Instant::now();
    let snapshot = impro_explorer::gitstatus::collect_status(
        temp.path(),
        impro_explorer::gitstatus::MAX_ENTRIES,
    );
    println!(
        "git-status-ms={} files={} folders={}",
        ms(started.elapsed()),
        snapshot.files.len(),
        snapshot.folders.len()
    );
    search_path(temp.path(), "needle", false);
    search_path(temp.path(), "function_1[0-9]\\b", true);
    search_path(temp.path(), "zzzz-no-match", false);
}

fn search_path(root: &Path, query: &str, regex: bool) {
    let panel = ExplorerPanel::new(root);
    let window = window_for(&panel);
    watch_phases(&window);
    {
        let search = panel.search();
        watch_draw(&window, "window");
        watch_draw(&search, "panel");
        watch_draw(&search.entry(), "entry");
        watch_draw(&search.stack(), "stack");
        watch_draw(&search.summary(), "summary");
        watch_draw(&search.tree(), "tree");
        watch_draw(&search.case_button(), "case");
    }
    let mut probe = Probe::default();
    run_until(&mut probe, Duration::from_secs(20), || !panel.is_busy());
    // Separate the existing window's startup paint from search dispatch work.
    run_until(&mut probe, Duration::from_millis(600), || false);
    let search = panel.search();
    if std::env::var_os("PERF_PROVIDER").is_some() {
        // As in the app: the editor answers for its open buffers only.
        search.set_text_provider(Some(Rc::new(|_| None)));
    }
    search.regex_button().set_active(regex);
    // The toggle schedules a debounced run for an empty query: nothing to cancel.
    let first_batch = Rc::new(RefCell::new(None::<Duration>));
    let started = Instant::now();
    {
        let (first, search_clone) = (first_batch.clone(), search.clone());
        let store = search.store();
        store.connect_row_inserted(move |_, _, _| {
            if first.borrow().is_none() && search_clone.is_searching() {
                *first.borrow_mut() = Some(started.elapsed());
            }
        });
    }
    let mut probe = Probe::default();
    let inserted = probe.rows.clone();
    search
        .store()
        .connect_row_inserted(move |_, _, _| inserted.set(inserted.get() + 1));
    if std::env::var_os("PERF_PHASES").is_some() {
        eprintln!("search-start");
    }
    search.search_now(Some(query));
    let finished = run_until(&mut probe, Duration::from_secs(120), || {
        !search.is_searching() && !search.timeout_pending()
    });
    let total = started.elapsed();
    let results = search.results();
    println!(
        "query={query:?} regex={regex} total-ms={} first-row-ms={} finished={finished}",
        ms(total),
        first_batch.borrow().map_or("-".into(), ms)
    );
    println!(
        "  matches={} files={} names={} truncated={} summary={:?}",
        results.total_matches,
        results.total_files,
        results.file_names.len(),
        results.truncated,
        search.summary().text()
    );
    println!(
        "  main-loop-dispatches={} max-block-ms={} main-thread-busy-ms={}",
        probe.iterations,
        ms(probe.max),
        ms(probe.busy)
    );
    print_percentiles(&probe);
    // Typing a new query cancels the previous run: measure how fast the old one stops.
    let cancel_started = Instant::now();
    search.search_now(Some("needle"));
    search.search_now(Some("other-query"));
    run_until(&mut probe, Duration::from_secs(120), || {
        !search.is_searching() && !search.timeout_pending()
    });
    println!("  supersede-and-finish-ms={}", ms(cancel_started.elapsed()));
    if let Some(seconds) = std::env::var("PERF_PAINT_LOOP")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
    {
        // Repaint the finished result list continuously (for sampling profilers).
        search.search_now(Some("function"));
        run_until(&mut probe, Duration::from_secs(60), || {
            !search.is_searching() && !search.timeout_pending()
        });
        println!("paint loop {seconds}s: pid {}", std::process::id());
        let end = Instant::now() + Duration::from_secs(seconds);
        let tree = search.tree();
        let mut frames = 0u32;
        while Instant::now() < end {
            tree.queue_draw();
            run_until(&mut probe, Duration::from_millis(40), || false);
            frames += 1;
        }
        println!("frames {frames}");
    }
    drop(window);
}

fn idle() {
    let temp = tempfile::tempdir().unwrap();
    make_tree(temp.path(), 500);
    let bare = std::env::var_os("PERF_BARE").is_some();
    let panel = ExplorerPanel::new(temp.path());
    let window = if bare {
        // Control: a window with a plain label, to separate GTK/session noise.
        window_for(&gtk::Label::new(Some("bare window")))
    } else {
        window_for(&panel)
    };
    if !bare {
        panel.show_notes_section(true);
        panel.set_current_file(Some(&temp.path().join("pkg000/sub0/module_3.rs")));
    }
    let mut probe = Probe::default();
    run_until(&mut probe, Duration::from_secs(10), || !panel.is_busy());
    // GTK's overlay scrollbars fade out for a couple of seconds after the first paint.
    run_until(&mut probe, Duration::from_millis(4000), || false);
    // Per-second CPU in the real main loop (no timers of ours): is any work left over?
    if std::env::var_os("PERF_IDLE_SERIES").is_some() {
        let quit = glib::MainLoop::new(None, false);
        let stop = quit.clone();
        let last = Rc::new(Cell::new(cpu_seconds()));
        let ticks = Rc::new(Cell::new(0u32));
        glib::timeout_add_local(Duration::from_secs(1), move || {
            let now = cpu_seconds();
            println!("idle-second {} cpu-s={:.3}", ticks.get(), now - last.get());
            last.set(now);
            ticks.set(ticks.get() + 1);
            if ticks.get() >= 8 {
                stop.quit();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
        quit.run();
    }
    // Block in the real main loop (no timers) to measure the true idle cost.
    let quit = glib::MainLoop::new(None, false);
    let stop = quit.clone();
    let (cpu1, wall1) = (cpu_seconds(), Instant::now());
    glib::timeout_add_local_once(Duration::from_secs(5), move || stop.quit());
    quit.run();
    let (cpu2, wall2) = (cpu_seconds(), wall1.elapsed());
    println!(
        "idle-main-loop cpu-s={:.3} over {:.1}s  => {:.2}% of one core",
        cpu2 - cpu1,
        wall2.as_secs_f64(),
        100.0 * (cpu2 - cpu1) / wall2.as_secs_f64()
    );
    drop(window);
}

fn main() {
    if std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    gtk::init().expect("display");
    impro_ui::init();
    impro_explorer::init();
    impro_ui::apply_theme(impro_core::themes::get_theme(None), None, None, None).unwrap();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("bigdir") => bigdir(args.get(1).and_then(|n| n.parse().ok()).unwrap_or(50_000)),
        Some("tree") => tree_scenario(args.get(1).and_then(|n| n.parse().ok()).unwrap_or(20_000)),
        Some("search") => {
            let root = PathBuf::from(args.get(1).expect("PATH"));
            search_path(
                &root,
                args.get(2).expect("QUERY"),
                args.iter().any(|a| a == "--regex"),
            );
        }
        Some("search-generated") => {
            let temp = tempfile::tempdir().unwrap();
            make_tree(temp.path(), 20_000);
            search_path(temp.path(), "function", false);
        }
        Some("idle") => idle(),
        _ => eprintln!("usage: perf bigdir|tree|search|idle"),
    }
}
