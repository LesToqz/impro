mod common;
use common::*;
use gtk::{atk::prelude::AtkObjectExt, prelude::*};
use impro::launch::{self, State};
use impro::welcome::{self, Welcome};
use std::path::{Path, PathBuf};

fn setup(recent: &[&Path]) -> (Harness, Welcome) {
    // The CSS joins the theme build, so register it before the window is made.
    welcome::init();
    let mut state = State::default();
    for dir in recent.iter().rev() {
        state.record(dir);
    }
    launch::seed(state);
    let h = Harness::new();
    let page = welcome::install(&h.w);
    frame(&h.w.window);
    (h, page)
}

fn dirs(parent: &Path, names: &[&str]) -> Vec<PathBuf> {
    names
        .iter()
        .map(|n| {
            let dir = parent.join(n);
            std::fs::create_dir_all(&dir).unwrap();
            dir.canonicalize().unwrap()
        })
        .collect()
}

fn labels(widget: &impl IsA<gtk::Widget>, class: &str) -> Vec<String> {
    let mut found = Vec::new();
    fn walk(widget: &gtk::Widget, class: &str, found: &mut Vec<String>) {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if label.style_context().has_class(class) {
                found.push(label.text().to_string());
            }
        }
        if let Some(container) = widget.downcast_ref::<gtk::Container>() {
            for child in container.children() {
                walk(&child, class, found);
            }
        }
    }
    walk(widget.upcast_ref(), class, &mut found);
    found
}

fn title(button: &gtk::Button) -> String {
    labels(button, "impro-welcome-label").remove(0)
}

#[test]
fn test_welcome_replaces_the_empty_label_and_yields_to_tabs() {
    run(|| {
        let (h, page) = setup(&[]);
        let stack = h.w.editor.stack();
        assert_eq!(
            stack.child_by_name("empty").as_ref(),
            Some(page.widget()),
            "the placeholder is replaced, not stacked"
        );
        assert_eq!(stack.visible_child_name().as_deref(), Some("empty"));
        assert!(page.widget().is_visible() && page.widget().is_mapped());
        h.open("a.txt", "hello\n");
        assert_eq!(stack.visible_child_name().as_deref(), Some("editor"));
        assert!(result(|cb| h.w.editor.close_current(cb)));
        assert_eq!(stack.visible_child_name().as_deref(), Some("empty"));
        assert_eq!(stack.children().len(), 2);
    });
}

#[test]
fn test_default_empty_state_still_works_without_the_welcome() {
    run(|| {
        let h = Harness::new();
        let stack = h.w.editor.stack();
        let empty = stack.child_by_name("empty").unwrap();
        assert!(empty.style_context().has_class("impro-empty"));
        // Replacing twice, while the editor page shows, keeps the editor page.
        h.open("b.txt", "x\n");
        let first = gtk::Label::new(Some("first"));
        h.w.editor.set_empty_widget(&first);
        assert_eq!(stack.visible_child_name().as_deref(), Some("editor"));
        let second = gtk::Label::new(Some("second"));
        h.w.editor.set_empty_widget(&second);
        assert_eq!(stack.children().len(), 2);
        assert_eq!(
            stack.child_by_name("empty").as_ref(),
            Some(second.upcast_ref())
        );
        assert_eq!(stack.visible_child_name().as_deref(), Some("editor"));
    });
}

#[test]
fn test_start_actions_show_only_real_shortcuts() {
    run(|| {
        let (h, page) = setup(&[]);
        let buttons = page.action_buttons();
        let rows: Vec<_> = buttons
            .iter()
            .map(|b| (title(b), labels(b, "impro-welcome-key")))
            .collect();
        let caps = |accelerator: &str| -> Vec<String> {
            impro::accel_text(accelerator)
                .split('+')
                .map(str::to_owned)
                .collect()
        };
        let none: Vec<String> = Vec::new();
        assert_eq!(
            rows,
            vec![
                ("Open Folder…".to_owned(), none.clone()),
                ("Quick Switcher".to_owned(), caps("<Control>p")),
                ("Command Palette".to_owned(), caps("<Control><Shift>p")),
                ("New Note".to_owned(), none),
                (
                    "Settings".to_owned(),
                    vec!["Ctrl".to_owned(), ",".to_owned()]
                ),
            ]
        );
        // Every chip is a shortcut the window really has.
        for (label, accelerator) in [
            ("File: Quick Switcher", "Ctrl+P"),
            ("View: Command Palette", "Ctrl+Shift+P"),
            ("Settings: Open settings", "Ctrl+,"),
        ] {
            let command =
                h.w.commands()
                    .into_iter()
                    .find(|c| c.label == label)
                    .unwrap();
            assert_eq!(
                command.hotkey.first().map(String::as_str),
                Some(accelerator)
            );
        }
        for button in &buttons {
            assert!(button.can_focus(), "rows are keyboard reachable");
            assert!(button.accessible().is_some_and(|a| a.name().is_some()));
        }
    });
}

#[test]
fn test_start_actions_run_their_commands() {
    run(|| {
        let (h, page) = setup(&[]);
        let buttons = page.action_buttons();
        buttons[1].clicked();
        poll(|| h.w.quick_switcher().is_some());
        assert!(h.w.overlay.prompt().is_some());
        h.w.overlay.hide_prompt();
        drain();
        buttons[2].clicked();
        poll(|| h.w.command_palette().is_some_and(|p| p.is_open()));
        h.w.overlay.hide_prompt();
        drain();
        buttons[4].clicked();
        poll(|| h.w.settings_dialog().is_some());
    });
}

#[test]
fn test_open_folder_uses_a_native_chooser_that_can_be_cancelled() {
    run(|| {
        let (h, page) = setup(&[]);
        let before = gtk::Window::list_toplevels().len();
        page.action_buttons()[0].clicked();
        drain();
        let dialogs: Vec<gtk::Dialog> = gtk::Window::list_toplevels()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Dialog>().ok())
            .filter(|d| d.is_visible())
            .collect();
        assert!(
            gtk::Window::list_toplevels().len() > before && !dialogs.is_empty(),
            "the chooser opened (GTK fallback without a portal)"
        );
        for dialog in dialogs {
            dialog.response(gtk::ResponseType::Cancel);
        }
        drain();
        assert!(h.w.root().is_dir(), "cancelling leaves the root alone");
    });
}

#[test]
fn test_recent_folders_list_excludes_the_current_folder() {
    run(|| {
        let tmp = tempfile::tempdir().unwrap();
        let list = dirs(tmp.path(), &["one", "two", "three"]);
        let current = tempfile::tempdir().unwrap();
        let current_dir = current.path().canonicalize().unwrap();
        let mut order: Vec<&Path> = vec![&list[0], &list[1], &list[2]];
        order.insert(1, &current_dir);
        let (h, page) = setup(&order);
        // The harness opens its own temporary directory, not `current_dir`.
        h.w.set_root(&current_dir);
        poll(|| h.w.root() == current_dir);
        drain();
        let shown: Vec<String> = page.recent_buttons().iter().map(title).collect();
        assert_eq!(shown, ["one", "two", "three"]);
        assert!(page.recent_section().is_visible());
        // Row detail is the folder's path; the tooltip carries the full text.
        let first = &page.recent_buttons()[0];
        assert_eq!(
            labels(first, "impro-welcome-path"),
            [first.tooltip_text().unwrap().to_string()]
        );
    });
}

#[test]
fn test_recent_row_click_opens_the_folder_and_updates_the_list() {
    run(|| {
        let tmp = tempfile::tempdir().unwrap();
        let list = dirs(tmp.path(), &["alpha", "beta"]);
        let (h, page) = setup(&[&list[0], &list[1]]);
        let first_root = h.w.root();
        assert_eq!(page.recent_buttons().len(), 2);
        page.recent_buttons()[1].clicked();
        poll(|| h.w.root() == list[1]);
        drain();
        // `beta` is now current: it left the list and heads the recent order.
        let shown: Vec<String> = page.recent_buttons().iter().map(title).collect();
        assert_eq!(shown, ["alpha"]);
        assert_eq!(launch::recent_folders()[0], list[1]);
        assert_ne!(first_root, h.w.root());
    });
}

#[test]
fn test_recent_rows_open_with_the_keyboard() {
    run(|| {
        let tmp = tempfile::tempdir().unwrap();
        let list = dirs(tmp.path(), &["kbd"]);
        let (h, page) = setup(&[&list[0]]);
        let row = page.recent_buttons()[0].clone();
        h.w.window.set_focus(Some(&row));
        assert!(row.has_focus());
        // The key goes to the focused row, exactly as a key press on a focused window would.
        let press = event(&h.w.window, "Return");
        h.w.window.event(&press);
        // A button answers the key with its own short press animation before it clicks.
        poll(|| h.w.root() == list[0]);
    });
}

#[test]
fn test_focusing_a_row_scrolls_it_into_view_in_a_short_window() {
    run(|| {
        let tmp = tempfile::tempdir().unwrap();
        let many = dirs(tmp.path(), &["a", "b", "c", "d", "e"]);
        let refs: Vec<&Path> = many.iter().map(PathBuf::as_path).collect();
        let (h, page) = setup(&refs);
        h.w.window.resize(900, 520);
        frame(&h.w.window);
        let scroller = page
            .widget()
            .clone()
            .downcast::<gtk::ScrolledWindow>()
            .unwrap();
        let vadjustment = scroller.vadjustment();
        assert!(
            vadjustment.upper() > vadjustment.page_size(),
            "the content is taller than the pane ({} <= {})",
            vadjustment.upper(),
            vadjustment.page_size()
        );
        assert_eq!(vadjustment.value(), 0.0);
        let last = page.recent_buttons().pop().unwrap();
        last.grab_focus();
        frame(&h.w.window);
        assert!(last.has_focus());
        let (_, y) = last
            .translate_coordinates(&scroller, 0, last.allocated_height())
            .unwrap();
        assert!(
            y <= scroller.allocated_height(),
            "the focused row is on screen (bottom {y} of {})",
            scroller.allocated_height()
        );
        assert!(vadjustment.value() > 0.0);
    });
}

#[test]
fn test_startup_files_open_in_argument_order_with_the_first_selected() {
    run(|| {
        let (h, _page) = setup(&[]);
        let files: Vec<PathBuf> = ["one.txt", "two.txt", "three.txt"]
            .iter()
            .map(|name| h.write(name, "text\n"))
            .collect();
        let missing = h.dir.path().join("missing.txt");
        let mut args = files.clone();
        args.insert(1, missing);
        impro::app::open_startup_files(&h.w.editor, args);
        poll(|| h.w.editor.tabs().len() == 3 && !h.w.editor.is_busy());
        let order: Vec<PathBuf> = h.w.editor.tabs().iter().map(|t| t.path()).collect();
        assert_eq!(order, files);
        assert_eq!(h.w.editor.current_path().as_ref(), Some(&files[0]));
        assert_eq!(
            h.w.editor.stack().visible_child_name().as_deref(),
            Some("editor")
        );
        // The unreadable one was reported, not fatal.
        assert!(h.w.editor.last_error().is_some());
        // Nothing to open is fine.
        impro::app::open_startup_files(&h.w.editor, Vec::new());
    });
}

#[test]
fn test_file_menu_lists_recent_folders_after_open_folder() {
    run(|| {
        let tmp = tempfile::tempdir().unwrap();
        let list = dirs(tmp.path(), &["first", "second"]);
        let (h, _page) = setup(&[]);
        let file_item = h.w.menu_bar.children()[0]
            .clone()
            .downcast::<gtk::MenuItem>()
            .unwrap();
        let file = file_item
            .submenu()
            .unwrap()
            .downcast::<gtk::Menu>()
            .unwrap();
        let label = |widget: &gtk::Widget| {
            widget
                .downcast_ref::<gtk::MenuItem>()
                .and_then(|item| item.label())
                .map(|text| text.to_string())
                .unwrap_or_default()
        };
        let items = file.children();
        let names: Vec<String> = items.iter().map(label).collect();
        assert_eq!(
            &names[..3],
            ["Quick Switcher…", "Open Folder…", "Open Recent"]
        );
        let recent = items[2].clone().downcast::<gtk::MenuItem>().unwrap();
        assert!(!recent.is_sensitive(), "nothing to reopen yet");
        let mut state = State::default();
        state.record(&list[1]);
        state.record(&list[0]);
        launch::seed(state);
        // The File menu refreshes its recent list every time it opens.
        file.emit_by_name::<()>("show", &[]);
        assert!(recent.is_sensitive());
        let submenu = recent.submenu().unwrap().downcast::<gtk::Menu>().unwrap();
        let entries = submenu.children();
        let shown: Vec<String> = entries.iter().map(label).collect();
        assert_eq!(
            shown,
            [list[0].display().to_string(), list[1].display().to_string()]
        );
        entries[1]
            .clone()
            .downcast::<gtk::MenuItem>()
            .unwrap()
            .activate();
        poll(|| h.w.root() == list[1]);
        // The folder just opened is not offered again.
        file.emit_by_name::<()>("show", &[]);
        let shown: Vec<String> = submenu.children().iter().map(label).collect();
        assert_eq!(shown, [list[0].display().to_string()]);
    });
}

#[test]
fn test_open_recent_command_picks_from_the_other_recent_folders() {
    run(|| {
        let tmp = tempfile::tempdir().unwrap();
        let list = dirs(tmp.path(), &["north", "south"]);
        let (h, _page) = setup(&[&list[0], &list[1]]);
        h.command("File: Open Recent");
        let prompt =
            h.w.overlay
                .prompt()
                .expect("the recent-folder prompt opens");
        poll(|| prompt.items().len() == 2);
        let labels: Vec<String> = prompt.items().iter().map(|i| i.label.clone()).collect();
        assert_eq!(labels, ["north", "south"]);
        prompt.set_query("sou");
        poll(|| prompt.items().len() == 1);
        prompt.select(0);
        prompt.choose(gdk::ModifierType::empty());
        poll(|| h.w.root() == list[1]);
        assert!(h.w.overlay.prompt().is_none());
    });
}

#[test]
fn test_open_recent_command_with_nothing_to_reopen_only_says_so() {
    run(|| {
        let (h, _page) = setup(&[]);
        h.command("File: Open Recent");
        assert!(h.w.overlay.prompt().is_none());
    });
}

#[test]
fn test_recent_section_is_hidden_when_empty_and_capped_when_long() {
    run(|| {
        let tmp = tempfile::tempdir().unwrap();
        let (_h, page) = setup(&[]);
        assert!(!page.recent_section().is_visible());
        assert!(page.recent_buttons().is_empty());
        drop(page);
        let many = dirs(tmp.path(), &["a", "b", "c", "d", "e", "f", "g", "h"]);
        let refs: Vec<&Path> = many.iter().map(PathBuf::as_path).collect();
        let (_h, page) = setup(&refs);
        assert_eq!(page.recent_buttons().len(), 5, "the page shows five");
        assert!(page.recent_section().is_visible());
        assert_eq!(launch::recent_folders().len(), 8);
    });
}

#[test]
fn test_new_note_row_follows_the_notes_feature() {
    run(|| {
        let (h, page) = setup(&[]);
        let note = page.action_buttons()[3].clone();
        assert!(note.is_visible());
        h.w.set_setting("feature_notes", false.into(), false)
            .unwrap();
        page.refresh();
        assert!(!note.is_visible());
        h.w.set_setting("feature_notes", true.into(), false)
            .unwrap();
        page.refresh();
        assert!(note.is_visible());
        // The row's own children stay shown through a hide and show cycle.
        assert!(note.child().unwrap().is_visible());
    });
}

#[test]
fn test_compact_layout_tightens_rows_and_restores() {
    run(|| {
        let (h, page) = setup(&[]);
        let row = page.action_buttons()[1].clone();
        let roomy = row.allocated_height();
        assert!(roomy > 0);
        page.set_compact(true);
        frame(&h.w.window);
        assert!(
            row.allocated_height() < roomy,
            "{} < {roomy}",
            row.allocated_height()
        );
        page.set_compact(false);
        frame(&h.w.window);
        assert_eq!(row.allocated_height(), roomy);
    });
}

#[test]
fn test_welcome_follows_theme_and_accent_changes() {
    run(|| {
        let (h, _page) = setup(&[]);
        let provider = impro_ui::style::provider().unwrap();
        let css = || provider.to_str().to_string();
        assert!(
            css().contains("impro-welcome-row"),
            "registered with the theme"
        );
        let before = css();
        h.w.set_setting("accent", "#ff8800".into(), false).unwrap();
        frame(&h.w.window);
        // GTK prints colours as rgb(); #ff8800 is rgb(255,136,0).
        assert!(
            css().contains("alpha(rgb(255,136,0), 0.14"),
            "hover tint is the accent"
        );
        assert_ne!(css(), before);
        let accented = css();
        h.w.set_setting("theme", "impro-light".into(), false)
            .unwrap();
        frame(&h.w.window);
        assert_ne!(css(), accented);
        assert!(css().contains("impro-welcome-row"));
    });
}
