//! Expanded folders are watched: changes made by other programs appear on their own.
mod common;
use common::*;
use impro_explorer::explorer::{scan_count, set_scan_counting, ExplorerPanel};
use std::path::{Path, PathBuf};

struct Fix {
    temp: tempfile::TempDir,
    panel: ExplorerPanel,
}

impl Fix {
    fn p(&self, name: &str) -> PathBuf {
        self.temp.path().join(name)
    }
}

impl Drop for Fix {
    fn drop(&mut self) {
        destroy(&self.panel);
    }
}

fn setup() -> Fix {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("sub/inner")).unwrap();
    std::fs::create_dir(temp.path().join("other")).unwrap();
    std::fs::write(temp.path().join("sub/a.txt"), "").unwrap();
    std::fs::write(temp.path().join("root.txt"), "").unwrap();
    let panel = ExplorerPanel::new(temp.path());
    wait_idle(&panel);
    Fix { temp, panel }
}

fn listed(f: &Fix, dir: &Path, wanted: &[&str]) -> bool {
    let (panel, dir) = (f.panel.clone(), dir.to_path_buf());
    let wanted: Vec<String> = wanted.iter().map(|s| s.to_string()).collect();
    poll(4000, move || names(&panel, &dir) == wanted)
}

#[test]
fn test_root_and_expanded_folders_follow_outside_changes() {
    run(|| {
        let f = setup();
        let sub = f.p("sub");
        expand(&f.panel, &sub);
        pump(50);
        assert_eq!(
            f.panel.watched_folders(),
            [f.temp.path().to_path_buf(), sub.clone()]
        );
        // Created, renamed and removed by "another program".
        std::fs::write(f.p("new.txt"), "").unwrap();
        assert!(listed(
            &f,
            f.temp.path(),
            &["other", "sub", "new.txt", "root.txt"]
        ));
        std::fs::write(sub.join("b.txt"), "").unwrap();
        assert!(listed(&f, &sub, &["inner", "a.txt", "b.txt"]));
        std::fs::rename(sub.join("b.txt"), sub.join("c.txt")).unwrap();
        assert!(listed(&f, &sub, &["inner", "a.txt", "c.txt"]));
        std::fs::remove_file(sub.join("a.txt")).unwrap();
        assert!(listed(&f, &sub, &["inner", "c.txt"]));
        std::fs::remove_dir(f.p("other")).unwrap();
        assert!(listed(&f, f.temp.path(), &["sub", "new.txt", "root.txt"]));
    });
}

#[test]
fn test_collapsed_and_unopened_folders_are_not_watched() {
    run(|| {
        let f = setup();
        let sub = f.p("sub");
        expand(&f.panel, &sub);
        pump(50);
        assert!(f.panel.watched_folders().contains(&sub));
        collapse(&f.panel, &sub);
        pump(50);
        assert_eq!(f.panel.watched_folders(), [f.temp.path().to_path_buf()]);
        // Never expanded folders are not watched either.
        assert!(!f.panel.watched_folders().contains(&f.p("other")));
        std::fs::write(sub.join("late.txt"), "").unwrap();
        pump(600);
        // Collapsed: not re-listed behind the user's back (the list shows what it had).
        assert_eq!(names(&f.panel, &sub), ["inner", "a.txt"]);
        let before = f.panel.watched_folders().len();
        expand(&f.panel, &sub);
        pump(50);
        assert_eq!(f.panel.watched_folders().len(), before + 1);
    });
}

#[test]
fn test_bursts_are_relisted_once_and_hidden_names_are_skipped() {
    run(|| {
        let f = setup();
        let sub = f.p("sub");
        expand(&f.panel, &sub);
        pump(50);
        set_scan_counting(true);
        for i in 0..200 {
            std::fs::write(sub.join(format!("burst-{i:03}.txt")), "").unwrap();
        }
        let panel = f.panel.clone();
        let target = sub.clone();
        assert!(poll(4000, move || names(&panel, &target).len() == 202));
        wait_idle(&f.panel);
        let scans = scan_count(&sub);
        assert!((1..=3).contains(&scans), "{scans} scans for one burst");
        // Hidden names are not shown, so changes to them cost nothing.
        f.panel.set_show_hidden(false);
        wait_idle(&f.panel);
        set_scan_counting(true);
        std::fs::write(sub.join(".secret"), "").unwrap();
        pump(700);
        assert_eq!(scan_count(&sub), 0);
        set_scan_counting(false);
    });
}

#[test]
fn test_watching_can_be_switched_off_and_stops_with_the_panel() {
    run(|| {
        let f = setup();
        assert!(f.panel.watch_folders());
        f.panel.set_watch_folders(false);
        pump(50);
        assert!(f.panel.watched_folders().is_empty());
        std::fs::write(f.p("quiet.txt"), "").unwrap();
        pump(700);
        assert!(!names(&f.panel, f.temp.path()).contains(&"quiet.txt".to_owned()));
        f.panel.set_watch_folders(true);
        pump(50);
        assert_eq!(f.panel.watched_folders(), [f.temp.path().to_path_buf()]);
        // Re-enabling does not list by itself; the next change does.
        std::fs::write(f.p("loud.txt"), "").unwrap();
        assert!(poll(4000, {
            let panel = f.panel.clone();
            let root = f.temp.path().to_path_buf();
            move || names(&panel, &root).contains(&"loud.txt".to_owned())
        }));
        let panel = f.panel.clone();
        destroy(&panel);
        assert!(panel.watched_folders().is_empty());
    });
}
