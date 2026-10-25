#![cfg(windows)]
use gtk::prelude::*;
use impro_terminal::TerminalPanel;
use impro_vte::{diag, Format};
use std::{
    cell::Cell,
    rc::Rc,
    time::{Duration, Instant},
};

fn grid(terminal: &impro_vte::Terminal) -> String {
    terminal
        .text_format(Format::Text)
        .unwrap_or_default()
        .to_string()
}
/// Panes, their child ids and grids, for failure messages.
fn dump(panel: &TerminalPanel) -> String {
    let panes = panel.panes();
    let mut out = format!(
        "panes: {}, closing: {}\n",
        panes.len(),
        panel.closing_panes().len()
    );
    for pane in panes {
        let terminal = pane.terminal.as_ref().unwrap();
        out += &format!(
            "--- pane {} pid={:?} closed={} grid {}x{} ---\n{}\n",
            pane.id,
            pane.pid(),
            pane.is_closed(),
            terminal.column_count(),
            terminal.row_count(),
            grid(terminal).trim_end()
        );
    }
    out
}
/// Why a step failed: the message, the panes (ids, child ids, grids) and the backend timeline.
fn fail(label: &str, panel: &TerminalPanel, message: &str) -> ! {
    panic!(
        "Windows dock step failed: {label}\n{message}\n{}--- backend timeline ---\n{}",
        dump(panel),
        diag::timeline_text()
    );
}
/// One step of the scenario. The pane count must be `before` when it starts and must stay within
/// `allowed` for its whole duration, so a condition over panes can never hold vacuously (`all()`
/// over an empty list is true). Conditions are latched: each must have been true at some point.
fn step(
    label: &str,
    panel: &TerminalPanel,
    before: usize,
    allowed: &[usize],
    conditions: &[(&str, &dyn Fn() -> bool)],
) {
    let count = || panel.panes().len();
    if count() != before {
        fail(
            label,
            panel,
            &format!("precondition: {before} pane(s) expected, found {}", count()),
        );
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let latched: Vec<Cell<bool>> = conditions.iter().map(|_| Cell::new(false)).collect();
    loop {
        if !allowed.contains(&count()) {
            fail(
                label,
                panel,
                &format!("pane count {} left the allowed {allowed:?}", count()),
            );
        }
        let mut all = true;
        for ((_, condition), latch) in conditions.iter().zip(&latched) {
            if !latch.get() && condition() {
                latch.set(true);
            }
            all &= latch.get();
        }
        if all {
            return;
        }
        if Instant::now() >= deadline {
            let state = conditions
                .iter()
                .zip(&latched)
                .map(|((name, c), latch)| {
                    format!("  {name}: ever true: {}, now: {}", latch.get(), c())
                })
                .collect::<Vec<_>>()
                .join("\n");
            fail(label, panel, &format!("timeout\nconditions:\n{state}"));
        }
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(2));
    }
}
/// The shells must keep running by themselves: pump for `duration` and fail the moment a pane is
/// gone, closed, lost its child id or the backend reports a process exit.
fn stay_alive(label: &str, panel: &TerminalPanel, panes: usize, duration: Duration) {
    let until = Instant::now() + duration;
    while Instant::now() < until {
        let now = panel.panes();
        if now.len() != panes
            || now
                .iter()
                .any(|p| p.is_closed() || p.is_closing() || p.pid().is_none())
        {
            fail(
                label,
                panel,
                &format!("expected {panes} live pane(s), found {}", now.len()),
            );
        }
        if let Some(line) = diag::timeline()
            .iter()
            .find(|l| l.contains("process exit observed"))
        {
            fail(label, panel, &format!("a shell exited by itself: {line}"));
        }
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn released() -> usize {
    diag::timeline()
        .iter()
        .filter(|l| l.contains("resources released"))
        .count()
}
fn resized(panel: &TerminalPanel, old: &[(i64, i64)]) -> bool {
    let panes = panel.panes();
    !old.is_empty()
        && panes.len() == old.len()
        && panes.iter().zip(old).all(|(p, old)| {
            let t = p.terminal.as_ref().unwrap();
            (t.column_count(), t.row_count()) != *old
                && t.pty().is_some_and(|pty| {
                    pty.size().unwrap() == (t.column_count() as i32, t.row_count() as i32)
                })
        })
}
fn grids(panel: &TerminalPanel) -> Vec<(i64, i64)> {
    panel
        .panes()
        .iter()
        .map(|p| {
            let t = p.terminal.as_ref().unwrap();
            (t.column_count(), t.row_count())
        })
        .collect()
}

#[test]
fn split_resize_close_and_input() {
    // One timeline line per backend lifecycle event, printed to stderr and kept for failures.
    std::env::set_var("IMPRO_TERM_DIAG", "1");
    gtk::init().expect("Windows GTK display");
    impro_terminal::init();
    let panel = TerminalPanel::new(std::env::temp_dir(), false, false, 11);
    panel.set_animation_ms(0);
    panel.set_shell("cmd.exe");
    let last_closed = Rc::new(Cell::new(false));
    let flag = last_closed.clone();
    panel.connect_last_closed(move |_| flag.set(true));
    assert!(panel.new_terminal());
    assert!(panel.new_terminal());
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(1100, 650);
    window.add(&panel);
    window.show_all();

    // 1. Two panes, each with a running shell, and both still running two seconds later: an
    //    interactive shell only leaves when told to.
    step(
        "both children",
        &panel,
        2,
        &[2],
        &[("both panes have a child id", &|| {
            panel.panes().iter().all(|p| p.pid().is_some())
        })],
    );
    let ids: Vec<u64> = panel.panes().iter().map(|p| p.id).collect();
    stay_alive(
        "shells stay alive after start",
        &panel,
        2,
        Duration::from_secs(2),
    );
    for pane in panel.panes() {
        pane.terminal
            .as_ref()
            .unwrap()
            .feed_child(b"echo impro-dock-ok\r");
    }
    step(
        "input reaches both panes",
        &panel,
        2,
        &[2],
        &[("echo visible in every pane", &|| {
            let panes = panel.panes();
            panes.len() == 2
                && panes
                    .iter()
                    .all(|p| grid(p.terminal.as_ref().unwrap()).contains("impro-dock-ok"))
        })],
    );

    // 2. Resize reaches both grids and both pseudo consoles.
    let previous = grids(&panel);
    assert_eq!(
        previous.len(),
        2,
        "grids before the resize\n{}",
        dump(&panel)
    );
    window.resize(1300, 750);
    step(
        "resized grids",
        &panel,
        2,
        &[2],
        &[("both grids changed and acknowledged", &|| {
            resized(&panel, &previous)
        })],
    );

    // 3. Typing `exit` into one live pane closes exactly that pane (child-exited -> close_pane).
    let leaving = panel.panes()[1].clone();
    assert_eq!(leaving.id, ids[1]);
    leaving.terminal.as_ref().unwrap().feed_child(b"exit\r");
    step(
        "typed exit closes its pane",
        &panel,
        2,
        &[2, 1],
        &[
            ("one pane remains", &|| panel.panes().len() == 1),
            ("the other pane is the survivor", &|| {
                panel.panes().first().is_some_and(|p| p.id == ids[0])
            }),
        ],
    );
    assert!(!last_closed.get(), "the survivor must stay open");
    assert!(
        panel.panes()[0].pid().is_some() && !panel.panes()[0].is_closed(),
        "the survivor's shell must still run\n{}",
        dump(&panel)
    );

    // 4. The survivor still resizes and takes input.
    let previous = grids(&panel);
    assert_eq!(
        previous.len(),
        1,
        "grids before the resize\n{}",
        dump(&panel)
    );
    window.resize(1100, 650);
    step(
        "survivor resized",
        &panel,
        1,
        &[1],
        &[("grid changed and acknowledged", &|| {
            resized(&panel, &previous)
        })],
    );
    panel.panes()[0]
        .terminal
        .as_ref()
        .unwrap()
        .feed_child(b"echo impro-survivor-ok\r");
    step(
        "surviving pane input",
        &panel,
        1,
        &[1],
        &[("echo visible in the survivor", &|| {
            panel
                .panes()
                .first()
                .is_some_and(|p| grid(p.terminal.as_ref().unwrap()).contains("impro-survivor-ok"))
        })],
    );

    // 5. Closing a pane whose shell still runs removes it at once and releases its session.
    let before = released();
    assert_eq!(
        panel.panes().len(),
        1,
        "before close_current\n{}",
        dump(&panel)
    );
    panel.close_current();
    assert_eq!(
        panel.panes().len(),
        0,
        "close_current must remove the pane synchronously\n{}",
        dump(&panel)
    );
    step(
        "closed pane releases its session",
        &panel,
        0,
        &[0],
        &[
            ("session resources released", &|| released() > before),
            ("last-closed emitted", &|| last_closed.get()),
        ],
    );
    unsafe {
        window.destroy();
    }
}
