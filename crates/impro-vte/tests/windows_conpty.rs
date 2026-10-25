//! Real Windows integration. All GTK work stays on one test thread.
#![cfg(windows)]
use gtk::prelude::*;
use impro_vte::{CursorBlinkMode, Format, Terminal};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

thread_local! {
    /// Terminals created by this test, dumped when a wait times out.
    static HOSTED: RefCell<Vec<Terminal>> = const { RefCell::new(Vec::new()) };
}
fn dump_grids() -> String {
    HOSTED.with(|hosted| {
        hosted
            .borrow()
            .iter()
            .enumerate()
            .map(|(i, t)| {
                format!(
                    "--- terminal {i} ({}x{}) ---\n{}",
                    t.column_count(),
                    t.row_count(),
                    t.text_format(Format::Text).unwrap_or_default().trim_end()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    })
}
/// Wait (pumping GTK) until every named condition has held. A condition is latched the first time
/// it is true: events such as "the text reached the grid" and "the child exited" need not hold at
/// the same instant. On timeout report which conditions never held, the grids and the timeline.
fn wait_all(label: &str, conditions: &[(&str, &dyn Fn() -> bool)]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let latched: Vec<Cell<bool>> = conditions.iter().map(|_| Cell::new(false)).collect();
    let all = || {
        let mut all = true;
        for ((_, condition), latch) in conditions.iter().zip(&latched) {
            if !latch.get() && condition() {
                latch.set(true);
            }
            all &= latch.get();
        }
        all
    };
    while !all() {
        if Instant::now() >= deadline {
            let state = conditions
                .iter()
                .zip(&latched)
                .map(|((name, c), latch)| {
                    format!("  {name}: ever true: {}, now: {}", latch.get(), c())
                })
                .collect::<Vec<_>>()
                .join("\n");
            panic!(
                "ConPTY timeout: {label}\nconditions:\n{state}\n{}\n--- backend timeline ---\n{}",
                dump_grids(),
                impro_vte::diag::timeline_text()
            );
        }
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn wait(label: &str, condition: impl Fn() -> bool) {
    wait_all(label, &[("condition", &condition)]);
}
/// `child-exited` must arrive exactly once: pump a little longer and compare.
fn expect_exits(exits: &ExitResults, expected: &[i32]) {
    settle();
    assert_eq!(
        *exits.borrow(),
        expected,
        "unexpected child-exited sequence\n--- backend timeline ---\n{}",
        impro_vte::diag::timeline_text()
    );
}
/// Exactly `count` exits, whatever the status (a hung-up shell has a platform-defined one).
fn expect_exit_count(exits: &ExitResults, count: usize) {
    settle();
    assert_eq!(
        exits.borrow().len(),
        count,
        "unexpected child-exited count: {:?}\n--- backend timeline ---\n{}",
        exits.borrow(),
        impro_vte::diag::timeline_text()
    );
}
/// Pump GTK for `duration`, running `each` after every iteration.
fn pump_for(duration: Duration, mut each: impl FnMut()) {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        glib::MainContext::default().iteration(false);
        each();
        std::thread::sleep(Duration::from_millis(1));
    }
}
/// Whether the timeline says the child with this id exited.
fn exit_logged(pid: i32) -> bool {
    impro_vte::diag::timeline()
        .iter()
        .any(|l| l.contains(&format!("pid={pid}]")) && l.contains("process exit observed"))
}
/// Latches (on every content change) the first moment `needle` is on the grid.
fn watch_text(t: &Terminal, needle: &'static str) -> Rc<Cell<bool>> {
    let seen = Rc::new(Cell::new(false));
    let flag = seen.clone();
    t.connect_contents_changed(move |t| {
        if text(t).contains(needle) {
            flag.set(true);
        }
    });
    seen
}
// Failures that must not hide the stages after them; reported together at the end.
thread_local! {
    static SOFT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
fn soft_fail(message: String) {
    eprintln!("SOFT FAILURE: {message}");
    SOFT.with(|soft| soft.borrow_mut().push(message));
}
/// An interactive shell must keep running after its start (the exit comes only from the user),
/// take input, and leave through its own `exit` with that status, closing exactly this terminal.
fn interactive_shell(argv: &[&str], echo: &[u8], marker: &str, exit: &[u8], status: i32) {
    let (window, t) = host();
    let (result, exits) = spawn(&t, argv);
    let pid = impro_vte::pid_number(*result.borrow().as_ref().unwrap().as_ref().unwrap());
    assert!(pid > 0);
    pump_for(Duration::from_secs(2), || {
        assert!(
            exits.borrow().is_empty() && !exit_logged(pid),
            "{argv:?} left by itself within 2 s of its start\n{}\n--- backend timeline ---\n{}",
            dump_grids(),
            impro_vte::diag::timeline_text()
        );
    });
    t.feed_child(echo);
    wait(&format!("{argv:?} takes input"), || {
        text(&t).contains(marker)
    });
    t.feed_child(exit);
    wait(&format!("{argv:?} exits through its own exit"), || {
        !exits.borrow().is_empty()
    });
    expect_exits(&exits, &[status]);
    unsafe {
        window.destroy();
    }
}
fn settle() {
    let until = Instant::now() + Duration::from_millis(300);
    while Instant::now() < until {
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn released() -> usize {
    impro_vte::diag::timeline()
        .iter()
        .filter(|l| l.contains("resources released"))
        .count()
}
fn text(t: &Terminal) -> String {
    t.text_format(Format::Text).unwrap_or_default().to_string()
}
fn host() -> (gtk::Window, Terminal) {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(800, 480);
    let t = Terminal::new();
    t.set_cursor_blink_mode(CursorBlinkMode::Off);
    window.add(&t);
    window.show_all();
    wait("terminal map", || t.is_mapped());
    HOSTED.with(|hosted| hosted.borrow_mut().push(t.clone()));
    (window, t)
}
type SpawnResult = Rc<RefCell<Option<Result<glib::Pid, glib::Error>>>>;
type ExitResults = Rc<RefCell<Vec<i32>>>;
fn spawn(t: &Terminal, argv: &[&str]) -> (SpawnResult, ExitResults) {
    let result = Rc::new(RefCell::new(None));
    let out = result.clone();
    let exits = Rc::new(RefCell::new(Vec::new()));
    let output = exits.clone();
    t.connect_child_exited(move |_, status| output.borrow_mut().push(status));
    t.spawn_async(
        None,
        argv,
        &["IMPRO_CONPTY_TEST=alive"],
        glib::SpawnFlags::SEARCH_PATH,
        20_000,
        None,
        move |r| *out.borrow_mut() = Some(r),
    );
    wait("spawn callback", || result.borrow().is_some());
    assert!(
        result.borrow().as_ref().unwrap().is_ok(),
        "spawn failed: {:?}",
        result.borrow()
    );
    (result, exits)
}
#[test]
fn windows_terminal_lifecycle_and_widget() {
    // Print the backend timeline (one line per lifecycle event) to stderr and keep it for failures.
    std::env::set_var("IMPRO_TERM_DIAG", "1");
    gtk::init().expect("Windows GTK display must be available");
    {
        let (window, t) = host();
        let entered = Rc::new(Cell::new(false));
        let guard = entered.clone();
        t.connect_contents_changed(move |t| {
            if !guard.replace(true) {
                let _ = t.text_format(Format::Text);
                t.set_colors(None, None, &[]);
                t.select_all();
                t.unselect_all();
            }
        });
        t.connect_selection_changed(|t| {
            let _ = t.has_selection();
            let _ = t.text_selected(Format::Text);
        });
        t.feed(b"reentrant");
        assert!(entered.get());
        let initial_len = text(&t).len();
        let outcome = Rc::new(RefCell::new(None));
        let out = outcome.clone();
        t.spawn_async(
            None,
            &["impro-missing-terminal-executable.exe"],
            &[],
            glib::SpawnFlags::SEARCH_PATH,
            10_000,
            None,
            move |r| *out.borrow_mut() = Some(r),
        );
        wait("failed spawn callback", || outcome.borrow().is_some());
        assert!(outcome.borrow().as_ref().unwrap().is_err());
        assert!(
            text(&t).len() > initial_len,
            "spawn failure should be visible"
        );
        let cancel = gio::Cancellable::new();
        cancel.cancel();
        let outcome = Rc::new(RefCell::new(None));
        let out = outcome.clone();
        t.spawn_async(
            None,
            &["cmd.exe"],
            &[],
            glib::SpawnFlags::DEFAULT,
            10_000,
            Some(&cancel),
            move |r| *out.borrow_mut() = Some(r),
        );
        wait("cancelled spawn callback", || outcome.borrow().is_some());
        assert!(outcome.borrow().as_ref().unwrap().is_err());
        unsafe {
            window.destroy();
        }
    }
    // Repeat instant exits to exercise the callback/output/exit ordering race.
    for _ in 0..5 {
        let (window, t) = host();
        let seen = watch_text(&t, "impro-ok");
        let (_, exits) = spawn(&t, &["cmd.exe", "/d", "/c", "echo impro-ok"]);
        wait_all(
            "echo and exit",
            &[
                ("output 'impro-ok' reached the grid", &|| {
                    seen.get() || text(&t).contains("impro-ok")
                }),
                ("child-exited emitted", &|| !exits.borrow().is_empty()),
            ],
        );
        expect_exits(&exits, &[0]);
        if !text(&t).contains("impro-ok") {
            // The output arrived (latched above) but is gone again: something erased the grid.
            // The raw chunks in the timeline ('output chunk #n') show whether the console host
            // sent the erase. Keep going so the later stages still report.
            soft_fail(format!(
                "output 'impro-ok' was on the grid and is gone after the exit\n{}\n--- backend timeline ---\n{}",
                dump_grids(),
                impro_vte::diag::timeline_text()
            ));
            unsafe {
                window.destroy();
            }
            continue;
        }
        t.select_all();
        assert!(t.has_selection());
        t.copy_clipboard_format(Format::Text);
        let copied = Rc::new(RefCell::new(None));
        let out = copied.clone();
        gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD)
            .request_text(move |_, text| *out.borrow_mut() = text.map(str::to_owned));
        wait("clipboard", || copied.borrow().is_some());
        assert!(copied.borrow().as_ref().unwrap().contains("impro-ok"));
        t.unselect_all();
        assert!(!t.has_selection());
        unsafe {
            window.destroy();
        }
    }
    {
        let (window, t) = host();
        let (_, exits) = spawn(&t, &["cmd.exe", "/d", "/c", "exit 7"]);
        wait("exit status", || !exits.borrow().is_empty());
        expect_exits(&exits, &[7]);
        unsafe {
            window.destroy();
        }
    }
    // A shell that is not told to leave keeps running, takes input and leaves with its own status.
    interactive_shell(
        &["cmd.exe", "/d", "/q"],
        b"echo impro-alive\r",
        "impro-alive",
        b"exit 5\r",
        5,
    );
    if std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", "exit 0"])
        .status()
        .is_ok()
    {
        interactive_shell(
            &["powershell.exe", "-NoLogo", "-NoProfile", "-NoExit"],
            b"Write-Host impro-alive\r",
            "impro-alive",
            b"exit 3\r",
            3,
        );
    }
    {
        let (window, t) = host();
        let (result, exits) = spawn(&t, &["cmd.exe", "/d", "/q"]);
        let previous_grid = (t.column_count(), t.row_count());
        window.resize(1040, 600);
        wait_all(
            "resize reaches ConPTY",
            &[
                ("grid changed", &|| {
                    (t.column_count(), t.row_count()) != previous_grid
                }),
                ("ConPTY size acknowledged", &|| {
                    t.pty().is_some_and(|p| {
                        p.size().unwrap() == (t.column_count() as i32, t.row_count() as i32)
                    })
                }),
            ],
        );
        t.feed_child(b"echo %IMPRO_CONPTY_TEST%\r");
        wait("environment and input", || text(&t).contains("alive"));
        let pid = impro_vte::pid_number(*result.borrow().as_ref().unwrap().as_ref().unwrap());
        assert!(pid > 0);
        impro_vte::hangup_pid(pid);
        wait("hangup closes child", || !exits.borrow().is_empty());
        expect_exit_count(&exits, 1);
        unsafe {
            window.destroy();
        }
    }
    {
        // Widget closed while the child runs: the session must be terminated and released.
        let before = released();
        let (window, t) = host();
        let (_, exits) = spawn(&t, &["cmd.exe", "/d", "/q"]);
        wait("cmd prompt", || !text(&t).trim().is_empty());
        unsafe {
            window.destroy();
        }
        wait("closed widget releases its session", || released() > before);
        assert!(
            exits.borrow().is_empty(),
            "closed widget must not emit child-exited"
        );
    }
    {
        let (window, t) = host();
        t.set_scrollback_lines(5000);
        let beat = Rc::new(Cell::new(0usize));
        let count = beat.clone();
        let source = glib::timeout_add_local(Duration::from_millis(5), move || {
            count.set(count.get() + 1);
            glib::ControlFlow::Continue
        });
        let (_, exits) = spawn(
            &t,
            &[
                "cmd.exe",
                "/d",
                "/c",
                "for /L %i in (1,1,20000) do @echo impro-line-%i",
            ],
        );
        beat.set(0);
        wait("bulk output drains", || !exits.borrow().is_empty());
        source.remove();
        assert!(text(&t).contains("impro-line-20000"));
        assert!(
            t.vadjustment().unwrap().upper() > t.row_count() as f64,
            "scrollback did not grow"
        );
        assert!(beat.get() > 0, "GTK failed to dispatch during output");
        eprintln!(
            "longest terminal main-thread slice: {:?}",
            t.maximum_output_slice()
        );
        assert!(
            t.maximum_output_slice() < Duration::from_millis(50),
            "terminal callback exceeded 50ms: {:?}",
            t.maximum_output_slice()
        );
        unsafe {
            window.destroy();
        }
    }
    if std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-Command", "exit 0"])
        .status()
        .is_ok()
    {
        let (window, t) = host();
        let (_, exits) = spawn(
            &t,
            &[
                "powershell.exe",
                "-NoLogo",
                "-NoProfile",
                "-Command",
                "Write-Host \"$([char]27)[38;2;255;0;0mimpro-red$([char]27)[0m\"; exit 0",
            ],
        );
        wait_all(
            "PowerShell ANSI output",
            &[
                ("child-exited emitted", &|| !exits.borrow().is_empty()),
                ("output 'impro-red' reached the grid", &|| {
                    text(&t).contains("impro-red")
                }),
            ],
        );
        expect_exits(&exits, &[0]);
        assert!(!text(&t).contains("[31m"));
        assert!(t.text_format(Format::Html).unwrap().contains("#ff0000"));
        unsafe {
            window.destroy();
        }
    } else {
        eprintln!("powershell.exe unavailable; PowerShell integration skipped");
    }
    SOFT.with(|soft| {
        assert!(
            soft.borrow().is_empty(),
            "{} soft failure(s):\n{}",
            soft.borrow().len(),
            soft.borrow().join("\n=====\n")
        );
    });
}
