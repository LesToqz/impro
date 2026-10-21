mod common;
use common::*;
use gdk::keys::constants as key;
use gtk::prelude::*;
use impro_core::fsops::FsOpError;
use impro_core::themes;
use impro_explorer::explorer::{
    scan_count, set_scan_counting, ConfirmRequest, DeleteMode, DialogHooks, ExplorerPanel,
    NameAction,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

struct Fix {
    temp: tempfile::TempDir,
    panel: ExplorerPanel,
}

impl Fix {
    fn root(&self) -> PathBuf {
        self.temp.path().to_path_buf()
    }
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
    let root = temp.path();
    std::fs::create_dir_all(root.join("Zdir/inner")).unwrap();
    std::fs::create_dir(root.join("adir")).unwrap();
    std::fs::File::create(root.join("Zdir/inner/deep.py")).unwrap();
    for name in ["b.py", "A.py", ".hidden"] {
        std::fs::File::create(root.join(name)).unwrap();
    }
    let panel = ExplorerPanel::new(root);
    let fix = Fix { temp, panel };
    wait_idle(&fix.panel);
    fix
}

fn root_name(fix: &Fix) -> String {
    fix.root()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn test_root_sort_and_hidden_files() {
    run(|| {
        let f = setup();
        let store = f.panel.store();
        let root = store.iter_first().unwrap();
        assert_eq!(
            impro_explorer::util::string(&store, &root, 1),
            root_name(&f)
        );
        assert_eq!(
            names(&f.panel, &f.root()),
            ["adir", "Zdir", ".hidden", "A.py", "b.py"]
        );
        let icons: Vec<_> = children(&f.panel, &f.root())
            .iter()
            .take(2)
            .map(|r| r[0].clone())
            .collect();
        assert_eq!(
            icons,
            [Some("folder".to_owned()), Some("folder".to_owned())]
        );
    });
}

#[test]
fn test_lazy_loading_one_scandir_per_expansion() {
    run(|| {
        let f = setup();
        let folder = f.p("Zdir");
        assert_eq!(
            children(&f.panel, &folder),
            [[None, Some(String::new()), Some(String::new())]]
        );
        set_scan_counting(true);
        expand(&f.panel, &folder);
        assert_eq!(scan_count(&folder), 1);
        assert_eq!(names(&f.panel, &folder), ["inner"]);
        assert_eq!(
            children(&f.panel, &folder.join("inner")),
            [[None, Some(String::new()), Some(String::new())]]
        );
        collapse(&f.panel, &folder);
        expand(&f.panel, &folder);
        assert_eq!(scan_count(&folder), 1);
        set_scan_counting(false);
    });
}

#[test]
fn test_create_selects_file_and_folder() {
    run(|| {
        let f = setup();
        let folder = f.p("Zdir");
        let path = create(&f.panel, &folder, "new.txt", false).unwrap();
        assert!(path.is_file());
        assert_eq!(f.panel.selected_path(), Some(path));
        assert!(f.panel.expanded_paths().contains(&folder));
        let created = create(&f.panel, &folder, "new-folder", true).unwrap();
        assert!(created.is_dir());
        assert_eq!(f.panel.selected_path(), Some(created));
    });
}

#[test]
fn test_rename_delete_and_folder_signals() {
    run(|| {
        let f = setup();
        let renamed = Rc::new(RefCell::new(Vec::new()));
        let deleted = Rc::new(RefCell::new(Vec::new()));
        let (r, d) = (renamed.clone(), deleted.clone());
        f.panel.connect_path_renamed(move |_, old, new| {
            r.borrow_mut().push((old.to_path_buf(), new.to_path_buf()))
        });
        f.panel
            .connect_path_deleted(move |_, path| d.borrow_mut().push(path.to_path_buf()));
        for name in ["A.py", "Zdir"] {
            let old = f.p(name);
            let new = rename(&f.panel, &old, &format!("renamed-{name}")).unwrap();
            assert_eq!(
                renamed.borrow().last().unwrap(),
                &(old.clone(), new.clone())
            );
            assert!(f.panel.find_row(&old).is_none());
            assert_eq!(f.panel.selected_path(), Some(new.clone()));
            delete(&f.panel, &new, false).unwrap();
            assert!(!new.exists());
            assert_eq!(deleted.borrow().last().unwrap(), &new);
        }
        assert_eq!(renamed.borrow().len(), 2);
        assert_eq!(deleted.borrow().len(), 2);
    });
}

#[test]
fn test_root_protection_and_failed_operations_do_not_emit() {
    run(|| {
        let f = setup();
        let signals = Rc::new(RefCell::new(0));
        let (a, b) = (signals.clone(), signals.clone());
        f.panel
            .connect_path_renamed(move |_, _, _| *a.borrow_mut() += 1);
        f.panel
            .connect_path_deleted(move |_, _| *b.borrow_mut() += 1);
        assert!(rename(&f.panel, &f.root(), "other").is_err());
        assert!(delete(&f.panel, &f.root(), false).is_err());
        assert!(rename(&f.panel, &f.p("A.py"), "b.py").is_err());
        assert!(create(&f.panel, &f.root(), "A.py", false).is_err());
        assert_eq!(*signals.borrow(), 0);
    });
}

#[test]
fn test_activation_files_and_folder_toggle() {
    run(|| {
        let f = setup();
        let activated = Rc::new(RefCell::new(Vec::new()));
        let sink = activated.clone();
        f.panel
            .connect_file_activated(move |_, path| sink.borrow_mut().push(path.to_path_buf()));
        let tree = f.panel.tree();
        let column = tree.column(0).unwrap();
        let file = f.p("A.py");
        let row = f.panel.find_row(&file).unwrap();
        tree.row_activated(
            &impro_explorer::util::path_of(&f.panel.store(), &row),
            &column,
        );
        assert_eq!(*activated.borrow(), std::slice::from_ref(&file));
        let row = f.panel.find_row(&f.p("Zdir")).unwrap();
        let path = impro_explorer::util::path_of(&f.panel.store(), &row);
        tree.row_activated(&path, &column);
        assert!(tree.row_expanded(&path));
        tree.row_activated(&path, &column);
        assert!(!tree.row_expanded(&path));
        assert_eq!(*activated.borrow(), [file]);
    });
}

#[test]
fn test_refresh_and_rename_preserve_nested_expansion() {
    run(|| {
        let f = setup();
        let folder = f.p("Zdir");
        expand(&f.panel, &folder);
        expand(&f.panel, &folder.join("inner"));
        f.panel.select(&folder.join("inner/deep.py"));
        let expanded = f.panel.expanded_paths();
        f.panel.refresh();
        wait_idle(&f.panel);
        assert_eq!(f.panel.expanded_paths(), expanded);
        assert_eq!(f.panel.selected_path(), Some(folder.join("inner/deep.py")));
        let new = rename(&f.panel, &folder, "newdir").unwrap();
        assert!(f.panel.expanded_paths().contains(&new.join("inner")));
        assert!(f.panel.find_row(&new.join("inner/deep.py")).is_some());
    });
}

#[test]
fn test_new_targets_and_set_root() {
    run(|| {
        let f = setup();
        f.panel.tree().selection().unselect_all();
        assert_eq!(f.panel.target_folder(), f.root());
        f.panel.select(&f.p("A.py"));
        assert_eq!(f.panel.target_folder(), f.root());
        f.panel.select(&f.p("Zdir"));
        assert_eq!(f.panel.target_folder(), f.p("Zdir"));
        f.panel.set_root(&f.p("Zdir"));
        wait_idle(&f.panel);
        assert_eq!(f.panel.root(), f.p("Zdir"));
        assert_eq!(names(&f.panel, &f.p("Zdir")), ["inner"]);
    });
}

#[test]
fn test_permission_error_and_symlink_cycle() {
    run(|| {
        let f = setup();
        let folder = f.p("Zdir");
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o000)).unwrap();
        expand(&f.panel, &folder);
        assert!(children(&f.panel, &folder).is_empty());
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = f.p("loop");
        std::os::unix::fs::symlink(f.root(), &link).unwrap();
        f.panel.refresh();
        wait_idle(&f.panel);
        assert!(children(&f.panel, &link).is_empty());
        assert_eq!(
            f.panel
                .store()
                .iter_n_children(f.panel.find_row(&link).as_ref()),
            0
        );
    });
}

fn stem_selection_dialog_fixture(
    panel: &ExplorerPanel,
    attempts: Rc<RefCell<Vec<String>>>,
) -> gtk::Dialog {
    let log = attempts.clone();
    let action: NameAction = Rc::new(move |name, done| {
        let first = {
            let mut log = log.borrow_mut();
            log.push(name);
            log.len() == 1
        };
        if first {
            done(Err(FsOpError::Invalid("Already exists".into())));
        } else {
            done(Ok(()));
        }
    });
    panel.name_dialog("Rename", "sample.py", action);
    panel.open_dialog().expect("dialog is open")
}

#[test]
fn test_name_dialog_keeps_errors_open_and_selects_stem() {
    run(|| {
        let f = setup();
        let attempts = Rc::new(RefCell::new(Vec::new()));
        let dialog = stem_selection_dialog_fixture(&f.panel, attempts.clone());
        let content = dialog.content_area().children();
        let entry = content
            .iter()
            .find_map(|w| w.clone().downcast::<gtk::Entry>().ok())
            .unwrap();
        let label = content
            .iter()
            .find_map(|w| w.clone().downcast::<gtk::Label>().ok())
            .unwrap();
        assert_eq!(label.text(), "");
        assert_eq!(entry.selection_bounds(), Some((0, 6)));
        entry.set_text("new.py");
        dialog.response(gtk::ResponseType::Ok);
        assert_eq!(label.text(), "Already exists");
        assert!(f.panel.open_dialog().is_some());
        dialog.response(gtk::ResponseType::Ok);
        assert_eq!(*attempts.borrow(), ["new.py", "new.py"]);
        assert!(f.panel.open_dialog().is_none());
    });
}

/// Press `keyval` on the explorer and report the actions the hooks saw.
fn press_on(panel: &ExplorerPanel, keyval: gdk::keys::Key, state: gdk::ModifierType) -> bool {
    panel.on_key(
        &key_event(keyval, state)
            .downcast::<gdk::EventKey>()
            .unwrap(),
    )
}

#[test]
fn test_shortcuts() {
    run(|| {
        let f = setup();
        let recorder = Recorder::default();
        recorder.install(&f.panel, None, |_| false);
        f.panel.select(&f.p("b.py"));
        assert!(press_on(&f.panel, key::F2, gdk::ModifierType::empty()));
        assert_eq!(
            recorder.names.borrow().last().unwrap(),
            &("Rename".to_owned(), "b.py".to_owned())
        );
        assert!(press_on(&f.panel, key::Delete, gdk::ModifierType::empty()));
        assert_eq!(recorder.confirms.borrow().len(), 1);
        assert!(press_on(
            &f.panel,
            key::N,
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
        ));
        assert_eq!(recorder.names.borrow().last().unwrap().0, "New Folder");
        std::fs::File::create(f.p("external.txt")).unwrap();
        assert!(f.panel.find_row(&f.p("external.txt")).is_none());
        assert!(press_on(&f.panel, key::F5, gdk::ModifierType::empty()));
        wait_idle(&f.panel);
        assert!(f.panel.find_row(&f.p("external.txt")).is_some());
        assert!(!press_on(&f.panel, key::F1, gdk::ModifierType::empty()));
    });
}

#[test]
fn test_search_box_under_header_and_forwarding() {
    run(|| {
        let f = setup();
        let children = f.panel.children();
        assert_eq!(children.len(), 2);
        let notes = f.panel.notes_widgets();
        assert_eq!(children[1], notes.split.clone().upcast::<gtk::Widget>());
        assert_eq!(
            notes.split.child1().unwrap(),
            f.panel.search().upcast::<gtk::Widget>()
        );
        let search = f.panel.search();
        let row = search.children()[0].clone().downcast::<gtk::Box>().unwrap();
        assert_eq!(row.children()[0], search.entry().upcast::<gtk::Widget>());
        let received = Rc::new(RefCell::new(Vec::new()));
        let sink = received.clone();
        f.panel
            .connect_search_result_activated(move |_, path, a, b, c| {
                sink.borrow_mut().push((path.to_path_buf(), a, b, c));
            });
        search.emit_by_name::<()>(
            "result-activated",
            &[
                &f.p("b.py").to_string_lossy().into_owned(),
                &3i32,
                &4i32,
                &5i32,
            ],
        );
        assert_eq!(*received.borrow(), [(f.p("b.py"), 3, 4, 5)]);
        let escaped = Rc::new(RefCell::new(0));
        let count = escaped.clone();
        f.panel
            .connect_search_escaped(move |_| *count.borrow_mut() += 1);
        search.emit_by_name::<()>("escaped", &[]);
        assert_eq!(*escaped.borrow(), 1);
        f.panel.set_current_file(Some(&f.p("A.py")));
        assert_eq!(search.current_path(), Some(f.p("A.py")));
        f.panel.set_current_file(None);
        assert_eq!(search.current_path(), None);
        f.panel.set_font_size(13.0);
        assert_eq!(search.font_size(), 13.0);
        let other = f.p("Zdir");
        f.panel.set_root(&other);
        assert_eq!(search.root(), other);
    });
}

#[test]
fn test_focus_search_with_query_and_clear() {
    run(|| {
        let f = setup();
        std::fs::write(f.p("b.py"), "needle").unwrap();
        f.panel.focus_search(Some("needle"));
        wait_idle(&f.panel);
        assert_eq!(f.panel.search().query(), "needle");
        assert_eq!(f.panel.search().results().total_matches, 1);
        f.panel.clear_search();
        assert_eq!(f.panel.search().query(), "");
        assert_eq!(
            f.panel.search().stack().visible_child_name().as_deref(),
            Some("content")
        );
    });
}

#[test]
fn test_shortcuts_ignored_when_search_has_focus() {
    run(|| {
        let f = setup();
        let recorder = Recorder::default();
        recorder.install(&f.panel, None, |_| false);
        f.panel.select(&f.p("b.py"));
        let window = common::window(&f.panel, 300, 500);
        for widget in [
            f.panel.search().entry().upcast::<gtk::Widget>(),
            f.panel.search().tree().upcast(),
        ] {
            widget.grab_focus();
            assert!(!press_on(&f.panel, key::F2, gdk::ModifierType::empty()));
        }
        assert!(recorder.names.borrow().is_empty());
        f.panel.tree().grab_focus();
        assert!(press_on(&f.panel, key::F2, gdk::ModifierType::empty()));
        assert_eq!(recorder.names.borrow().len(), 1);
        destroy(&window);
    });
}

#[test]
fn test_hidden_preference_filters_lazy_children_and_preserves_expansion() {
    run(|| {
        let f = setup();
        let folder = f.p("Zdir");
        std::fs::File::create(folder.join(".secret")).unwrap();
        std::fs::create_dir(f.p(".private")).unwrap();
        f.panel.refresh();
        wait_idle(&f.panel);
        expand(&f.panel, &folder);
        f.panel.select(&f.p("A.py"));
        f.panel.set_show_hidden(false);
        wait_idle(&f.panel);
        assert!(f.panel.find_row(&f.p(".hidden")).is_none());
        assert!(f.panel.find_row(&f.p(".private")).is_none());
        assert!(f.panel.find_row(&folder.join(".secret")).is_none());
        assert!(f.panel.expanded_paths().contains(&folder));
        assert_eq!(f.panel.selected_path(), Some(f.p("A.py")));
        f.panel.set_show_hidden(true);
        wait_idle(&f.panel);
        assert!(f.panel.find_row(&folder.join(".secret")).is_some());
        assert!(f.panel.find_row(&f.p(".hidden")).is_some());
    });
}

#[test]
fn test_current_file_reveals_nested_path_without_activating_it() {
    run(|| {
        let f = setup();
        let activated = Rc::new(RefCell::new(0));
        let count = activated.clone();
        f.panel
            .connect_file_activated(move |_, _| *count.borrow_mut() += 1);
        let path = f.p("Zdir/inner/deep.py");
        f.panel.set_current_file(Some(&path));
        wait_idle(&f.panel);
        assert_eq!(f.panel.selected_path(), Some(path.clone()));
        assert!(f.panel.expanded_paths().contains(&f.p("Zdir/inner")));
        assert_eq!(f.panel.search().current_path(), Some(path));
        f.panel.set_current_file(None);
        assert_eq!(f.panel.selected_path(), None);
        f.panel
            .set_current_file(Some(Path::new("/outside-project.py")));
        assert_eq!(f.panel.selected_path(), None);
        assert_eq!(*activated.borrow(), 0);
    });
}

fn confirm_text(request: &ConfirmRequest) -> (String, String) {
    (request.title.clone(), request.detail.clone())
}

#[test]
fn test_delete_defaults_confirmation_and_cancel() {
    run(|| {
        let f = setup();
        let path = f.p("A.py");
        f.panel.select(&path);
        assert!(f.panel.show_hidden());
        assert!(f.panel.confirm_delete());
        assert_eq!(f.panel.delete_mode(), DeleteMode::Delete);
        let recorder = Recorder::default();
        recorder.install(&f.panel, None, |_| false);
        f.panel.delete_dialog(None);
        assert_eq!(recorder.confirms.borrow().len(), 1);
        assert_eq!(
            confirm_text(&recorder.confirms.borrow()[0]).0,
            "Delete \"A.py\"?"
        );
        wait_idle(&f.panel);
        assert!(path.exists());
        recorder.install(&f.panel, None, |_| true);
        f.panel.delete_dialog(None);
        let p = path.clone();
        pump_until("file deleted", move || !p.exists());
        wait_idle(&f.panel);
    });
}

#[test]
fn test_disable_confirmation_skips_prompt_and_invalid_mode_keeps_setting() {
    run(|| {
        let f = setup();
        let path = f.p("A.py");
        f.panel.select(&path);
        f.panel.set_confirm_delete(false);
        assert!(DeleteMode::parse("invalid").is_err());
        assert_eq!(f.panel.delete_mode(), DeleteMode::Delete);
        let recorder = Recorder::default();
        recorder.install(&f.panel, None, |_| false);
        f.panel.delete_dialog(None);
        let p = path.clone();
        pump_until("file deleted", move || !p.exists());
        assert!(recorder.confirms.borrow().is_empty());
    });
}

fn trash_hook(destination: PathBuf) -> impro_explorer::explorer::RemoveHook {
    Arc::new(move |trash, path, _| {
        assert!(trash, "trash mode must request the trash");
        std::fs::rename(path, &destination).unwrap();
        Some(Ok(path.to_path_buf()))
    })
}

fn set_remove_hook(
    panel: &ExplorerPanel,
    recorder: &Recorder,
    remove: impro_explorer::explorer::RemoveHook,
    confirm: bool,
) {
    recorder.install(panel, None, move |_| confirm);
    let installed = recorder.clone();
    let hooks = DialogHooks {
        name_dialog: None,
        confirm_removal: Some(Rc::new({
            let confirms = installed.confirms.clone();
            move |request| {
                confirms.borrow_mut().push(request.clone());
                confirm
            }
        })),
        error_dialog: Some(Rc::new({
            let errors = installed.errors.clone();
            move |message| errors.borrow_mut().push(message.to_owned())
        })),
        remove: Some(remove),
    };
    panel.set_dialog_hooks(hooks);
}

#[test]
fn test_trash_success_updates_tree_and_emits_once() {
    run(|| {
        let f = setup();
        let path = f.p("A.py");
        let destination = f.p("trashed.py");
        f.panel.select(&path);
        f.panel.set_delete_mode(DeleteMode::Trash);
        f.panel.set_confirm_delete(false);
        let deleted = Rc::new(RefCell::new(Vec::new()));
        let sink = deleted.clone();
        f.panel
            .connect_path_deleted(move |_, item| sink.borrow_mut().push(item.to_path_buf()));
        let recorder = Recorder::default();
        set_remove_hook(&f.panel, &recorder, trash_hook(destination.clone()), true);
        f.panel.delete_dialog(None);
        let d = deleted.clone();
        pump_until("deleted", move || !d.borrow().is_empty());
        wait_idle(&f.panel);
        assert!(recorder.confirms.borrow().is_empty());
        assert_eq!(*deleted.borrow(), std::slice::from_ref(&path));
        assert!(f.panel.find_row(&path).is_none());
        assert!(destination.exists());
    });
}

#[test]
fn test_trash_fallback_requires_separate_permanent_confirmation() {
    run(|| {
        for confirm in [false, true] {
            for accept in [false, true] {
                let f = setup();
                let path = f.p("A.py");
                std::fs::write(&path, "keep").unwrap();
                f.panel.refresh();
                wait_idle(&f.panel);
                f.panel.select(&path);
                f.panel.set_delete_mode(DeleteMode::Trash);
                f.panel.set_confirm_delete(confirm);
                let recorder = Recorder::default();
                let hook: impro_explorer::explorer::RemoveHook = Arc::new(|trash, p, root| {
                    if trash {
                        Some(Err(FsOpError::TrashUnavailable("no trash".into())))
                    } else {
                        Some(impro_core::fsops::delete(p, Some(root)))
                    }
                });
                recorder.install(&f.panel, None, move |request| {
                    if request.title.starts_with("Move ") {
                        true
                    } else {
                        accept
                    }
                });
                let hooks = DialogHooks {
                    confirm_removal: Some(Rc::new({
                        let confirms = recorder.confirms.clone();
                        move |request| {
                            confirms.borrow_mut().push(request.clone());
                            request.title.starts_with("Move ") || accept
                        }
                    })),
                    remove: Some(hook),
                    ..DialogHooks::default()
                };
                f.panel.set_dialog_hooks(hooks);
                f.panel.delete_dialog(None);
                let expected = if confirm { 2 } else { 1 };
                let confirms = recorder.confirms.clone();
                pump_until("prompts", move || confirms.borrow().len() >= expected);
                wait_idle(&f.panel);
                pump(60);
                let prompts = recorder.confirms.borrow();
                assert_eq!(prompts.len(), expected, "confirm={confirm} accept={accept}");
                assert!(prompts
                    .last()
                    .unwrap()
                    .detail
                    .contains("Delete permanently instead?"));
                assert_eq!(path.exists(), !accept, "confirm={confirm} accept={accept}");
            }
        }
    });
}

#[test]
fn test_trash_permission_failure_reports_error_without_fallback() {
    run(|| {
        let f = setup();
        let path = f.p("A.py");
        f.panel.select(&path);
        f.panel.set_delete_mode(DeleteMode::Trash);
        f.panel.set_confirm_delete(false);
        let recorder = Recorder::default();
        let hook: impro_explorer::explorer::RemoveHook =
            Arc::new(|_, _, _| Some(Err(FsOpError::Io("Permission denied".into()))));
        set_remove_hook(&f.panel, &recorder, hook, true);
        f.panel.delete_dialog(None);
        let errors = recorder.errors.clone();
        pump_until("error reported", move || !errors.borrow().is_empty());
        assert_eq!(*recorder.errors.borrow(), ["Permission denied"]);
        assert!(recorder.confirms.borrow().is_empty());
        assert!(path.exists());
    });
}

#[test]
fn test_real_trash_moves_file_into_freedesktop_trash() {
    run(|| {
        let f = setup();
        let data = tempfile::tempdir_in(f.root()).unwrap();
        std::env::set_var("XDG_DATA_HOME", data.path());
        let path = f.p("A.py");
        f.panel.set_delete_mode(DeleteMode::Trash);
        let result = delete(&f.panel, &path, false);
        std::env::remove_var("XDG_DATA_HOME");
        assert!(result.is_ok(), "{result:?}");
        assert!(!path.exists());
        assert!(data.path().join("Trash/files/A.py").exists());
        assert!(data.path().join("Trash/info/A.py.trashinfo").exists());
    });
}

fn show_panel(f: &Fix, theme: &str) -> gtk::Window {
    impro_ui::apply_theme(themes::get_theme(Some(theme)), None, None, None).unwrap();
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_default_size(300, 400);
    window.add(&f.panel);
    window.show_all();
    window.present();
    let w = window.clone();
    pump_until("window active", move || w.is_active());
    window
}

fn row_pixel(window: &gtk::Window, tree: &gtk::TreeView, x: i32, y: i32) -> (u8, u8, u8) {
    let (x, y) = tree.convert_bin_window_to_widget_coords(x, y);
    let (x, y) = tree.translate_coordinates(window, x, y).unwrap();
    let pixbuf = window_pixbuf(&window.window().unwrap(), x, y, 1, 1).unwrap();
    let bytes = pixbuf.read_pixel_bytes();
    (bytes[0], bytes[1], bytes[2])
}

fn rgb(hex: &str) -> (u8, u8, u8) {
    let value = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    (value(1), value(3), value(5))
}

#[test]
fn test_tree_geometry_rounded_selection_hover_and_live_themes() {
    run(|| {
        let f = setup();
        let window = show_panel(&f, "impro-dark");
        let tree = f.panel.tree();
        f.panel.select(&f.p("A.py"));
        f.panel.search().entry().grab_focus();
        window.set_focus_visible(false);
        let path = tree.selection().selected_rows().0[0].clone();
        for name in ["impro-dark", "impro-light"] {
            let theme = themes::get_theme(Some(name));
            impro_ui::apply_theme(theme, None, None, None).unwrap();
            let t = tree.clone();
            pump_until("tree allocated", move || t.allocated_width() > 1);
            let column = tree.column(0).unwrap();
            let area = tree.background_area(Some(&path), Some(&column));
            assert!((24..=26).contains(&area.height()), "{}", area.height());
            assert_eq!(tree.margin_start(), 12);
            assert_eq!(tree.margin_end(), 12);
            let sidebar = rgb(&theme.ui["sidebar_bg"]);
            let overlay = rgb(&theme.ui["overlay"]);
            let mix = |a: u8, b: u8| (f64::from(a) * 0.933 + f64::from(b) * 0.067).round() as u8;
            let tint = (
                mix(sidebar.0, overlay.0),
                mix(sidebar.1, overlay.1),
                mix(sidebar.2, overlay.2),
            );
            let close = move |p: (u8, u8, u8)| {
                p.0.abs_diff(tint.0) <= 1 && p.1.abs_diff(tint.1) <= 1 && p.2.abs_diff(tint.2) <= 1
            };
            let (w, t) = (window.clone(), tree.clone());
            let (ax, ay) = (area.x(), area.y());
            pump_until("corner is sidebar", move || {
                row_pixel(&w, &t, ax, ay) == sidebar
            });
            let (w, t) = (window.clone(), tree.clone());
            let (cx, cy) = (area.width() - 15, area.y() + area.height() / 2);
            pump_until("selection tint", move || close(row_pixel(&w, &t, cx, cy)));
            // A real pointer hover produces the same tint, without a timed fade.
            tree.selection().unselect_all();
            let origin = tree.bin_window().unwrap().origin();
            let display = gdk::Display::default().unwrap();
            let pointer = display.default_seat().unwrap().pointer().unwrap();
            pointer.warp(
                &gdk::Screen::default().unwrap(),
                origin.1 + cx,
                origin.2 + cy,
            );
            let (w, t) = (window.clone(), tree.clone());
            pump_until("hover tint", move || close(row_pixel(&w, &t, cx, cy)));
            pointer.warp(&gdk::Screen::default().unwrap(), 0, 0);
            let (w, t) = (window.clone(), tree.clone());
            pump_until("hover cleared", move || {
                row_pixel(&w, &t, cx, cy) == sidebar
            });
            f.panel.select(&f.p("A.py"));
            f.panel.search().entry().grab_focus();
        }
        destroy(&window);
    });
}

#[test]
fn test_header_icons_accessibility_and_instant_css_for_all_themes() {
    run(|| {
        let f = setup();
        let window = show_panel(&f, "impro-dark");
        let header = f.panel.children()[0]
            .clone()
            .downcast::<gtk::Box>()
            .unwrap();
        let name = |w: &gtk::Widget| {
            atk::prelude::AtkObjectExt::name(&w.accessible().unwrap()).map(|n| n.to_string())
        };
        assert_eq!(name(header.upcast_ref()).as_deref(), Some("EXPLORER"));
        assert_eq!(header.tooltip_text().as_deref(), Some("EXPLORER"));
        let actions = header.children()[0].clone().downcast::<gtk::Box>().unwrap();
        assert_eq!(actions.halign(), gtk::Align::Center);
        let labels: Vec<_> = actions
            .children()
            .iter()
            .map(|w| name(w).unwrap())
            .collect();
        assert_eq!(labels, ["Notes", "New File", "New Folder", "Refresh"]);
        for theme in themes::themes() {
            let css = impro_ui::style::build_css(theme);
            impro_ui::style::create_provider(&css).unwrap_or_else(|e| panic!("{}: {e}", theme.id));
            assert!(!impro_explorer::explorer_css(&theme.ui).contains("transition:"));
        }
        destroy(&window);
    });
}

#[test]
fn test_header_actions_create_in_selected_folder_and_refresh() {
    run(|| {
        let f = setup();
        let window = show_panel(&f, "impro-dark");
        let folder = f.p("Zdir");
        f.panel.select(&folder);
        let recorder = Recorder::default();
        recorder.install(
            &f.panel,
            Some(Rc::new(|title: &str| {
                if title == "New File" {
                    "created.txt".into()
                } else {
                    "created-folder".into()
                }
            })),
            |_| true,
        );
        f.panel.header_button("New File").unwrap().clicked();
        let p = folder.join("created.txt");
        pump_until("file created", {
            let p = p.clone();
            move || p.is_file()
        });
        wait_idle(&f.panel);
        assert_eq!(f.panel.selected_path(), Some(p));
        f.panel.header_button("New Folder").unwrap().clicked();
        let p = folder.join("created-folder");
        pump_until("folder created", {
            let p = p.clone();
            move || p.is_dir()
        });
        wait_idle(&f.panel);
        assert_eq!(f.panel.selected_path(), Some(p.clone()));
        let external = folder.join("external.txt");
        std::fs::File::create(&external).unwrap();
        assert!(f.panel.find_row(&external).is_none());
        f.panel.header_button("Refresh").unwrap().clicked();
        wait_idle(&f.panel);
        assert!(f.panel.find_row(&external).is_some());
        assert_eq!(f.panel.selected_path(), Some(p));
        destroy(&window);
    });
}

#[test]
fn test_delete_menu_item_inherits_base_padding_radius_and_danger() {
    run(|| {
        let f = setup();
        let window = show_panel(&f, "impro-dark");
        f.panel.select(&f.p("A.py"));
        let menu = f.panel.build_menu();
        let item = menu
            .children()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::MenuItem>().ok())
            .find(|i| i.label().as_deref() == Some("Delete"))
            .unwrap();
        assert!(item.is_sensitive());
        let context = item.style_context();
        assert!(context.has_class("destructive-action"));
        let expected = rgb(&themes::get_theme(Some("impro-dark")).ui["danger"]);
        for state in [gtk::StateFlags::NORMAL, gtk::StateFlags::PRELIGHT] {
            #[allow(deprecated)]
            let colour = context.color(state);
            let got = (
                (colour.red() * 255.0).round() as u8,
                (colour.green() * 255.0).round() as u8,
                (colour.blue() * 255.0).round() as u8,
            );
            assert_eq!(got, expected);
            #[allow(deprecated)]
            let padding = context.padding(state);
            let space = &impro_ui::style::TOKENS["spacing"];
            let (compact, control) = (
                space["compact"].as_i64().unwrap() as i16,
                space["control"].as_i64().unwrap() as i16,
            );
            assert_eq!(
                (padding.top, padding.right, padding.bottom, padding.left),
                (compact, control, compact, control)
            );
            let radius: i32 = context
                .style_property_for_state("border-radius", state)
                .get()
                .unwrap();
            assert_eq!(
                i64::from(radius),
                impro_ui::style::TOKENS["radius"]["item"].as_i64().unwrap()
            );
        }
        unsafe { menu.destroy() };
        destroy(&window);
    });
}

#[test]
fn test_menu_sensitivity_for_root_and_selection() {
    run(|| {
        let f = setup();
        f.panel.select(&f.root());
        let menu = f.panel.build_menu();
        let state = |label: &str| {
            menu.children()
                .into_iter()
                .filter_map(|w| w.downcast::<gtk::MenuItem>().ok())
                .find(|i| i.label().as_deref() == Some(label))
                .map(|i| i.is_sensitive())
                .unwrap()
        };
        assert!(state("New File..."));
        assert!(!state("Rename..."));
        assert!(!state("Delete"));
        assert!(state("Copy Path"));
        unsafe { menu.destroy() };
    });
}

#[test]
fn test_no_filesystem_work_needed_on_main_thread_for_selection_target() {
    run(|| {
        // The target folder is derived from the model icon, so no stat call is needed.
        let f = setup();
        let link = f.p("linkdir");
        std::os::unix::fs::symlink(f.p("adir"), &link).unwrap();
        f.panel.refresh();
        wait_idle(&f.panel);
        f.panel.select(&link);
        assert_eq!(f.panel.target_folder(), f.root());
    });
}

#[test]
fn test_set_of_expanded_paths_helper_is_consistent() {
    run(|| {
        let f = setup();
        let expected: HashSet<PathBuf> = [f.root()].into_iter().collect();
        assert_eq!(f.panel.expanded_paths(), expected);
    });
}
