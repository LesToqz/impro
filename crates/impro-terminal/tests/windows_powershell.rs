//! Windows PowerShell in a terminal pane, started the way the application starts it: from a
//! project root that `std::fs::canonicalize` returned in its verbatim `\\?\C:\...` spelling.
#![cfg(windows)]
use gtk::prelude::*;
use impro_terminal::TerminalPanel;
use impro_vte::{diag, Format};
use std::time::{Duration, Instant};

fn grid(panel: &TerminalPanel) -> String {
    panel
        .panes()
        .first()
        .and_then(|p| p.terminal.as_ref()?.text_format(Format::Text))
        .unwrap_or_default()
        .to_string()
}

/// Pump GTK until `done` holds; fail with the grid and the backend timeline after 60 s.
fn wait(label: &str, panel: &TerminalPanel, done: impl Fn(&str) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let text = grid(panel);
        if done(&text) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "PowerShell step failed: {label}\n--- grid ---\n{}\n--- backend timeline ---\n{}",
                text.trim_end(),
                diag::timeline_text()
            );
        }
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn windows_powershell_starts_in_the_plain_path_and_runs_commands() {
    std::env::set_var("IMPRO_TERM_DIAG", "1");
    gtk::init().expect("Windows GTK display");
    impro_terminal::init();
    let root = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(
        root.to_string_lossy().starts_with(r"\\?\"),
        "precondition: std canonicalize gives a verbatim path, got {}",
        root.display()
    );
    let plain = impro_core::platform::simplified(&root);
    let plain_text = plain.to_string_lossy().trim_end_matches('\\').to_owned();

    let panel = TerminalPanel::new(&root, false, false, 11);
    panel.set_animation_ms(0);
    panel.set_shell("powershell.exe");
    assert!(panel.new_terminal());
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(1100, 650);
    window.add(&panel);
    window.show_all();

    // The prompt names the plain path, not the provider-qualified verbatim one.
    let prompt = format!("PS {plain_text}>");
    wait("prompt in the plain path", &panel, |text| {
        text.contains(&prompt)
    });
    let text = grid(&panel);
    assert!(
        !text.contains(r"\\?\"),
        "verbatim prefix in the grid:\n{text}"
    );
    assert!(
        !text.contains("FileSystem::"),
        "provider path in the grid:\n{text}"
    );
    assert_eq!(panel.header_labels(), ["Windows PowerShell 1"]);

    // Commands run in that directory, and non-ASCII output arrives intact.
    let terminal = panel.panes()[0].terminal.clone().unwrap();
    terminal.feed_child(
        b"Write-Output ('impro-' + 'ps-ok'); (Get-Location).Path; 'caf' + [char]0xe9 + ' ' + [char]0x2713\r",
    );
    wait("command output", &panel, |text| {
        text.contains("impro-ps-ok") && text.contains("caf\u{e9} \u{2713}")
    });
    let text = grid(&panel);
    let location = text
        .lines()
        .skip_while(|line| !line.starts_with("impro-ps-ok"))
        .nth(1)
        .unwrap_or_default()
        .trim_end()
        .to_owned();
    assert_eq!(location, plain_text, "Get-Location in the grid:\n{text}");

    // `exit` ends the shell and closes its pane.
    terminal.feed_child(b"exit\r");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !panel.panes().is_empty() {
        assert!(
            Instant::now() < deadline,
            "the pane did not close after exit\n{}",
            diag::timeline_text()
        );
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(5));
    }
    unsafe { window.destroy() };
}
