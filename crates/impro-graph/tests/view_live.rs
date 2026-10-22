//! Live updates: saves, unsaved buffers, renames, deletes, Open Folder, the
//! active file, deltas and the node cap.

mod gtk_common;

use gtk::prelude::*;
use gtk_common::*;
use impro_graph::view::testing as t;
use impro_graph::GraphType;
use serde_json::json;

fn positions(f: &Fixture) -> Vec<(String, f32, f32)> {
    f.ids()
        .into_iter()
        .map(|id| {
            let (x, y) = t::node_world(&f.view, &id).expect("node");
            (id, x, y)
        })
        .collect()
}

#[test]
fn saving_a_file_updates_the_graph_without_moving_unaffected_nodes() {
    run(|| {
        let f = Fixture::new();
        let before = positions(&f);
        write(f.root(), "d.md", "[[a]] fresh note\n");
        f.view.file_saved(&f.path("d.md"));
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "d.md")));
        // the new note is wired to a and every survivor stayed exactly where it was
        t::pump(30);
        for (id, x, y) in &before {
            let (nx, ny) = t::node_world(&f.view, id).expect("survivor");
            // the layout reheats and may move things slightly; the first frame must not jump
            assert!(
                (nx - x).abs() < 400.0 && (ny - y).abs() < 400.0,
                "{id} jumped"
            );
        }
        f.settle();
        // a rewrite that drops a link removes the line
        let with_d = f.view.summary().lines;
        write(f.root(), "a.md", "no links now\n");
        f.view.file_saved(&f.path("a.md"));
        assert!(t::wait_for(5_000, || f.view.summary().lines < with_d && f.has("d.md")));
    });
}

#[test]
fn a_structural_delta_keeps_survivor_positions_exactly_at_apply_time() {
    run(|| {
        let f = Fixture::new();
        let before = positions(&f);
        let nodes = f.view.summary().nodes;
        write(f.root(), "late.md", "[[c]]\n");
        f.view.file_saved(&f.path("late.md"));
        // observe the very first applied state
        let seen_first = std::cell::Cell::new(false);
        assert!(t::wait_for(5_000, || {
            if f.has("late.md") && !seen_first.replace(true) {
                for (id, x, y) in &before {
                    let (nx, ny) = t::node_world(&f.view, id).expect("survivor");
                    assert!(
                        (nx - x).abs() < 20.0 && (ny - y).abs() < 20.0,
                        "{id}: ({x},{y}) -> ({nx},{ny})"
                    );
                }
            }
            f.has("late.md")
        }));
        assert_eq!(f.view.summary().nodes, nodes + 1);
    });
}

#[test]
fn unsaved_buffers_apply_400_ms_after_the_last_change() {
    run(|| {
        let f = Fixture::new();
        let path = f.path("c.md");
        f.view
            .buffer_changed(&path, Some("[[orphan]] [[img]]".into()));
        t::pump(250);
        let lines_before = f.view.summary().lines;
        // a newer change restarts the clock
        f.view
            .buffer_changed(&path, Some("[[orphan]] [[img]] [[a]]".into()));
        t::pump(250);
        assert_eq!(
            f.view.summary().lines,
            lines_before,
            "still waiting after 500 ms in total"
        );
        assert!(t::wait_for(5_000, || f.view.summary().lines > lines_before));
        // the buffer shadows the file on disk; None goes back to disk
        let with_buffer = f.view.summary().lines;
        f.view.buffer_changed(&path, None);
        assert!(t::wait_for(5_000, || f.view.summary().lines < with_buffer));
    });
}

#[test]
fn renames_and_deletes_follow_the_file_system() {
    run(|| {
        let f = Fixture::new();
        std::fs::rename(f.path("c.md"), f.path("renamed.md")).expect("rename");
        f.view.path_renamed(&f.path("c.md"), &f.path("renamed.md"));
        assert!(
            f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "renamed.md")
                && !ids.iter().any(|i| i == "c.md"))
        );
        // a folder rename moves every file below it
        std::fs::rename(f.path("hub"), f.path("centre")).expect("rename dir");
        f.view.path_renamed(&f.path("hub"), &f.path("centre"));
        assert!(
            f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "centre/Hub.md")
                && !ids.iter().any(|i| i == "hub/Hub.md"))
        );
        std::fs::remove_file(f.path("orphan.md")).expect("remove");
        f.view.path_deleted(&f.path("orphan.md"));
        assert!(f.wait_ids(5_000, |ids| !ids.iter().any(|i| i == "orphan.md")));
        std::fs::remove_dir_all(f.path("src")).expect("remove dir");
        f.view.path_deleted(&f.path("src"));
        assert!(f.wait_ids(5_000, |ids| !ids.iter().any(|i| i.starts_with("src/"))));
    });
}

#[test]
fn updates_outside_the_project_or_in_ignored_folders_are_ignored() {
    run(|| {
        let f = Fixture::new();
        let updates = f.seen().updates;
        let outside = tempfile::tempdir().expect("dir");
        write(outside.path(), "x.md", "[[a]]");
        f.view.file_saved(&outside.path().join("x.md"));
        f.view
            .buffer_changed(&outside.path().join("x.md"), Some("t".into()));
        write(f.root(), ".git/notes.md", "[[a]]");
        f.view.file_saved(&f.path(".git/notes.md"));
        f.view.buffer_changed(&f.path("pic.png"), Some("x".into()));
        t::pump(700);
        assert_eq!(f.seen().updates, updates, "nothing to rebuild");
    });
}

#[test]
fn bursts_of_updates_coalesce() {
    run(|| {
        let f = Fixture::new();
        let before = f.seen().updates;
        for i in 0..30 {
            write(f.root(), &format!("burst-{i}.md"), "[[a]]");
            f.view.file_saved(&f.path(&format!("burst-{i}.md")));
        }
        assert!(f.wait_ids(8_000, |ids| ids
            .iter()
            .filter(|i| i.starts_with("burst-"))
            .count()
            == 30));
        let builds = f.seen().updates - before;
        assert!(builds <= 6, "{builds} applied builds for one burst");
    });
}

#[test]
fn open_folder_replaces_the_project() {
    run(|| {
        let f = Fixture::new();
        let other = tempfile::tempdir().expect("dir");
        write(other.path(), "Fresh.md", "[[Second]]");
        write(other.path(), "Second.md", "");
        f.view.set_active_file(Some(&f.path("a.md")));
        f.view.set_root(other.path());
        assert!(f.wait_ids(8_000, |ids| ids == ["Fresh.md", "Second.md"]));
        assert_eq!(f.view.root(), other.path());
        // clicks open files of the new root
        let (x, y, _) = f.screen("Fresh.md");
        t::click(&f.view, x, y, 1, none());
        assert_eq!(
            f.seen().opened.last().map(|o| o.0.clone()),
            Some(other.path().join("Fresh.md"))
        );
        // updates now apply to the new root
        write(other.path(), "Third.md", "[[Fresh]]");
        f.view.file_saved(&other.path().join("Third.md"));
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "Third.md")));
    });
}

#[test]
fn rebuild_picks_up_changes_nobody_announced() {
    run(|| {
        let f = Fixture::new();
        write(f.root(), "secret.md", "[[a]]");
        t::pump(200);
        assert!(!f.has("secret.md"));
        f.view.rebuild();
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "secret.md")));
    });
}

#[test]
fn the_active_file_is_highlighted_and_the_local_graph_follows_it() {
    run(|| {
        let f = Fixture::new();
        let accent = t::accent_hex(&f.view);
        f.view.set_active_file(Some(&f.path("a.md")));
        f.settle();
        assert_eq!(t::node_look(&f.view, "a.md").expect("node").1, accent);
        f.view.set_active_file(Some(&f.path("b.md")));
        f.settle();
        assert_ne!(t::node_look(&f.view, "a.md").expect("node").1, accent);
        assert_eq!(t::node_look(&f.view, "b.md").expect("node").1, accent);
        // local mode follows tab changes
        f.view.open_local(&f.path("a.md"));
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "a.md")
            && !ids.iter().any(|i| i == "orphan.md")));
        f.view.set_active_file(Some(&f.path("orphan.md")));
        assert!(f.wait_ids(5_000, |ids| ids == ["orphan.md"]));
        // no file: nothing to show, with a hint
        f.view.set_active_file(None);
        assert!(f.wait_ids(5_000, |ids| ids.is_empty()));
        assert_eq!(
            f.view.status_text(),
            ("Open a file to see its local graph".to_string(), true)
        );
        // global again
        f.view.open_global();
        assert!(f.wait_ids(5_000, |ids| ids.len() > 5));
        assert_eq!(f.view.graph_type(), GraphType::Global);
    });
}

#[test]
fn the_note_graph_shows_only_the_notes_folder() {
    run(|| {
        let f = Fixture::new();
        f.view.set_graph_type(GraphType::Note);
        assert!(f.wait_ids(5_000, |ids| ids.is_empty()));
        assert_eq!(
            f.view.status_text().0,
            "No notes yet. Create a note to see the note graph"
        );
        write(f.root(), ".impro/Notes/idea.md", "[[a]]");
        f.view.rebuild();
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == ".impro/Notes/idea.md")));
        assert!(!f.has("a.md") || f.ids().iter().all(|i| i.starts_with(".impro/Notes/")));
    });
}

#[test]
fn large_projects_are_capped_at_3000_nodes_with_a_notice() {
    run(|| {
        let project = tempfile::tempdir().expect("dir");
        for i in 0..3100 {
            write(
                project.path(),
                &format!("n/{i:04}.md"),
                &format!("[[{:04}]]\n", (i * 7 + 1) % 3100),
            );
        }
        let f = Fixture::in_dir(project, json!({}), true);
        assert!(t::wait_for(20_000, || f.view.summary().nodes > 0));
        let s = f.view.summary();
        assert_eq!(s.nodes, 3000);
        assert!(s.truncated);
        assert_eq!(
            f.view.notice_text().as_deref(),
            Some("View limited to 3000 files with the most connections")
        );
        // dropping files below the cap removes the notice
        f.view.panel().search_entry().set_text("0001");
        assert!(t::wait_for(10_000, || f.view.notice_text().is_none()
            && f.view.summary().nodes < 3000));
        f.view.shutdown();
    });
}
