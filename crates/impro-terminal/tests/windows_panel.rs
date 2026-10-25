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
/// Wait (pumping GTK) until every named condition holds; on timeout say which one was still false
/// and print the panes and the backend timeline.
fn wait_all(label: &str, panel: &TerminalPanel, conditions: &[(&str, &dyn Fn() -> bool)]) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !conditions.iter().all(|(_, c)| c()) {
        if Instant::now() >= deadline {
            let state = conditions
                .iter()
                .map(|(name, c)| format!("  {name}: {}", c()))
                .collect::<Vec<_>>()
                .join("\n");
            panic!(
                "Windows dock timeout: {label}\nconditions:\n{state}\n{}--- backend timeline ---\n{}",
                dump(panel),
                diag::timeline_text()
            );
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
    panes.len() == old.len()
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

    // 1. Two panes, each with a running shell.
    wait_all(
        "both children",
        &panel,
        &[
            ("two panes", &|| panel.panes().len() == 2),
            ("both panes have a child id", &|| {
                panel.panes().iter().all(|p| p.pid().is_some())
            }),
        ],
    );
    let ids: Vec<u64> = panel.panes().iter().map(|p| p.id).collect();
    for pane in panel.panes() {
        pane.terminal
            .as_ref()
            .unwrap()
            .feed_child(b"echo impro-dock-ok\r");
    }
    wait_all(
        "input reaches both panes",
        &panel,
        &[("echo visible in every pane", &|| {
            panel
                .panes()
                .iter()
                .all(|p| grid(p.terminal.as_ref().unwrap()).contains("impro-dock-ok"))
        })],
    );

    // 2. Resize reaches both grids and both pseudo consoles.
    let previous = grids(&panel);
    window.resize(1300, 750);
    wait_all(
        "resized grids",
        &panel,
        &[("both grids changed and acknowledged", &|| {
            resized(&panel, &previous)
        })],
    );

    // 3. A shell that exits by itself closes its pane (child-exited -> close_pane).
    let leaving = panel.panes()[1].clone();
    leaving.terminal.as_ref().unwrap().feed_child(b"exit\r");
    wait_all(
        "typed exit closes its pane",
        &panel,
        &[
            ("one pane remains", &|| panel.panes().len() == 1),
            ("the other pane is the survivor", &|| {
                panel.panes().first().is_some_and(|p| p.id == ids[0])
            }),
        ],
    );
    assert!(!last_closed.get(), "the survivor must stay open");

    // 4. The survivor still resizes and takes input.
    let previous = grids(&panel);
    window.resize(1100, 650);
    wait_all(
        "survivor resized",
        &panel,
        &[("grid changed and acknowledged", &|| {
            resized(&panel, &previous)
        })],
    );
    panel.panes()[0]
        .terminal
        .as_ref()
        .unwrap()
        .feed_child(b"echo impro-survivor-ok\r");
    wait_all(
        "surviving pane input",
        &panel,
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
    wait_all(
        "closed pane releases its session",
        &panel,
        &[
            ("session resources released", &|| released() > before),
            ("last-closed emitted", &|| last_closed.get()),
        ],
    );
    unsafe {
        window.destroy();
    }
}
