//! User actions: create, rename and delete (with dialogs), the context menu, shortcuts and
//! the commands the window calls into the explorer.
use super::*;
use impro_core::{fsops, notes};
use std::sync::Arc;

/// What happens to files removed from the explorer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteMode {
    /// Remove permanently (the default).
    Delete,
    /// Move to the freedesktop trash.
    Trash,
}

impl DeleteMode {
    pub fn parse(mode: &str) -> Result<Self, String> {
        match mode {
            "delete" => Ok(Self::Delete),
            "trash" => Ok(Self::Trash),
            _ => Err("Delete mode must be 'trash' or 'delete'.".into()),
        }
    }
}

/// Completion of a name dialog action: `Ok` closes the dialog, `Err` shows the message.
pub type NameAction = Rc<dyn Fn(String, Box<dyn FnOnce(Result<(), FsOpError>)>)>;

/// The question asked before removing a path.
#[derive(Clone, Debug)]
pub struct ConfirmRequest {
    pub path: PathBuf,
    /// Trash was unavailable; this asks about deleting permanently instead.
    pub fallback: bool,
    pub title: String,
    pub detail: String,
}

/// Replacement for the removal itself: `(trash, path, project_root)`; `None` runs the
/// real operation. Called on a worker thread.
pub type RemoveHook = Arc<dyn Fn(bool, &Path, &Path) -> Option<Result<PathBuf, FsOpError>> + Send + Sync>;

/// Replacements for the modal dialogs, so a host (or a test) can answer without a user.
#[derive(Clone, Default)]
pub struct DialogHooks {
    /// `(title, initial text, action)`; call `action(text, done)` to submit a name.
    pub name_dialog: Option<Rc<dyn Fn(&str, &str, NameAction)>>,
    pub confirm_removal: Option<Rc<dyn Fn(&ConfirmRequest) -> bool>>,
    pub error_dialog: Option<Rc<dyn Fn(&str)>>,
    pub remove: Option<RemoveHook>,
}

pub type OpCallback = Box<dyn FnOnce(Result<PathBuf, FsOpError>)>;

/// `os.path.splitext(name)[0]`, as a character count.
fn stem_chars(name: &str) -> usize {
    let leading = name.chars().take_while(|c| *c == '.').count();
    let rest: String = name.chars().skip(leading).collect();
    let stem = rest.rfind('.').map_or(rest.chars().count(), |i| rest[..i].chars().count());
    leading + stem
}

impl ExplorerPanel {
    fn window(&self) -> Option<gtk::Window> {
        self.toplevel().and_then(|top| top.downcast::<gtk::Window>().ok())
    }

    /// Install dialog replacements (see [`DialogHooks`]).
    pub fn set_dialog_hooks(&self, hooks: DialogHooks) {
        *self.imp().hooks.borrow_mut() = hooks;
    }

    /// The modal dialog currently open, if any.
    #[doc(hidden)]
    pub fn open_dialog(&self) -> Option<gtk::Dialog> {
        self.imp().dialog.borrow().clone()
    }

    // ---- state commands -------------------------------------------------------------------

    /// Point the explorer (and its search) at another folder.
    pub fn set_root(&self, path: &Path) {
        let imp = self.imp();
        *imp.git_snapshot.borrow_mut() = Rc::new(crate::gitstatus::Snapshot::default());
        self.set_git_monitor(None);
        self.watch_current_file(None);
        imp.notes_requested.set(None);
        *imp.active_file.borrow_mut() = None;
        let root = fsops::absolute(path);
        *imp.root.borrow_mut() = root.clone();
        self.refresh_git();
        self.rebuild_clear([root.clone()].into_iter().collect(), None);
        self.w().search.set_root(&root);
        self.refresh_notes();
    }

    /// Re-list the visible folders, then refresh search results, Git and notes.
    pub fn refresh(&self) {
        self.rebuild(self.expanded_paths(), self.selected_path());
        self.refresh_search();
        self.refresh_git();
        self.refresh_notes();
    }

    pub fn set_folders_first(&self, enabled: bool) {
        let imp = self.imp();
        imp.folders_first.set(enabled);
        // Sibling order changes, so merging in place would not line up: start over.
        self.rebuild_clear(self.expanded_paths(), self.selected_path());
        self.refresh_search();
        self.refresh_git();
        self.refresh_notes();
    }

    pub fn folders_first(&self) -> bool {
        self.imp().folders_first.get()
    }

    /// Show dot-files and dot-folders in the navigator (default `true`).
    pub fn set_show_hidden(&self, show: bool) {
        if self.imp().show_hidden.get() != show {
            self.imp().show_hidden.set(show);
            self.refresh();
        }
    }

    /// Confirm ordinary removals (default `true`); the trash fallback always asks.
    pub fn set_confirm_delete(&self, confirm: bool) {
        self.imp().confirm_delete.set(confirm);
    }

    /// Choose whether subsequent removals delete permanently or use the trash.
    pub fn set_delete_mode(&self, mode: DeleteMode) {
        self.imp().delete_mode.set(mode);
    }

    /// Reveal the active editor file and list its search results first.
    pub fn set_current_file(&self, path: Option<&Path>) {
        let imp = self.imp();
        let path = path.filter(|p| !p.as_os_str().is_empty());
        *imp.active_file.borrow_mut() = path.map(fsops::absolute);
        self.watch_current_file(path);
        self.w().search.set_current_file(path);
        match path {
            Some(path) if notes::is_note(&*imp.root.borrow(), Some(path)) => {
                self.select_note(&fsops::absolute(path));
            }
            _ => self.w().notes.tree.selection().unselect_all(),
        }
        self.reveal_path(path);
    }

    fn refresh_search(&self) {
        // Never leave results pointing at renamed or deleted files.
        let search = &self.w().search;
        if !search.query().is_empty() {
            search.search_now(None);
        }
    }

    // ---- filesystem actions ---------------------------------------------------------------------

    /// Create a file or folder below `parent`, then select it. `done` gets the outcome.
    pub fn create(&self, parent: &Path, name: &str, folder: bool, done: OpCallback) {
        let (parent, name) = (parent.to_path_buf(), name.to_owned());
        let (job_parent, job_name) = (parent.clone(), name);
        let weak = self.downgrade();
        self.run_op(
            move || {
                if folder {
                    fsops::create_folder(&job_parent, &job_name)
                } else {
                    fsops::create_file(&job_parent, &job_name)
                }
            },
            move |result| {
                let Some(panel) = weak.upgrade() else { return };
                if let Ok(created) = &result {
                    let mut expanded = panel.expanded_paths();
                    expanded.insert(parent);
                    expanded.insert(panel.root());
                    panel.rebuild(expanded, Some(created.clone()));
                    panel.refresh_search();
                    panel.refresh_git();
                    panel.refresh_notes();
                }
                done(result);
            },
        );
    }

    /// Rename `path` to `new_name` (a plain name); emits `path-renamed(old, new)`.
    pub fn rename(&self, path: &Path, new_name: &str, done: OpCallback) {
        let root = self.root();
        if fsops::absolute(path) == root {
            done(Err(FsOpError::Invalid("The project root cannot be renamed.".into())));
            return;
        }
        let expanded = self.expanded_paths();
        let (old, name) = (path.to_path_buf(), new_name.to_owned());
        let job_old = old.clone();
        let weak = self.downgrade();
        self.run_op(
            move || fsops::rename(&job_old, &name),
            move |result| {
                let Some(panel) = weak.upgrade() else { return };
                if let Ok(new) = &result {
                    let remapped = expanded
                        .into_iter()
                        .map(|dir| match dir.strip_prefix(&old) {
                            Ok(rest) => new.join(rest),
                            Err(_) => dir,
                        })
                        .collect();
                    panel.rebuild(remapped, Some(new.clone()));
                    {
                        let mut active = panel.imp().active_file.borrow_mut();
                        if let Some(file) = active.as_ref().and_then(|f| f.strip_prefix(&old).ok()) {
                            *active = Some(new.join(file));
                        }
                    }
                    panel.emit_by_name::<()>(
                        "path-renamed",
                        &[&model_path(&old), &model_path(new)],
                    );
                    // After the signal, so open tabs already carry their new paths.
                    panel.refresh_search();
                    panel.refresh_git();
                    panel.refresh_notes();
                }
                done(result);
            },
        );
    }

    /// Delete (or trash, per [`DeleteMode`] unless `permanent`) `path`; emits `path-deleted`.
    pub fn delete(&self, path: &Path, permanent: bool, done: OpCallback) {
        let root = self.root();
        if fsops::absolute(path) == root {
            done(Err(FsOpError::Invalid("The project root cannot be deleted.".into())));
            return;
        }
        let trash = self.imp().delete_mode.get() == DeleteMode::Trash && !permanent;
        let hook = self.imp().hooks.borrow().remove.clone();
        let (target, job_root) = (path.to_path_buf(), root.clone());
        let job_target = target.clone();
        let weak = self.downgrade();
        self.run_op(
            move || {
                if let Some(result) = hook.and_then(|hook| hook(trash, &job_target, &job_root)) {
                    return result;
                }
                if trash {
                    fsops::trash(&job_target, Some(&job_root))
                } else {
                    fsops::delete(&job_target, Some(&job_root))
                }
            },
            move |result| {
                let Some(panel) = weak.upgrade() else { return };
                if let Ok(removed) = &result {
                    panel.rebuild(panel.expanded_paths(), panel.selected_path());
                    {
                        let mut active = panel.imp().active_file.borrow_mut();
                        if active.as_ref().is_some_and(|f| f.starts_with(&target)) {
                            *active = None;
                        }
                    }
                    panel.emit_by_name::<()>("path-deleted", &[&model_path(removed)]);
                    panel.refresh_search();
                    panel.refresh_git();
                    panel.refresh_notes();
                }
                done(result);
            },
        );
    }

    // ---- dialogs ---------------------------------------------------------------------------------

    /// Ask for a name. `action` runs on OK; an error it reports keeps the dialog open.
    pub fn name_dialog(&self, title: &str, initial: &str, action: NameAction) {
        let hook = self.imp().hooks.borrow().name_dialog.clone();
        if let Some(hook) = hook {
            hook(title, initial, action);
            return;
        }
        let dialog = gtk::Dialog::with_buttons(
            Some(title),
            self.window().as_ref(),
            gtk::DialogFlags::MODAL,
            &[("Cancel", gtk::ResponseType::Cancel), ("OK", gtk::ResponseType::Ok)],
        );
        dialog.set_default_response(gtk::ResponseType::Ok);
        let content = dialog.content_area();
        content.set_spacing(8);
        content.set_border_width(12);
        let entry = gtk::Entry::new();
        entry.set_text(initial);
        entry.set_activates_default(true);
        entry.set_size_request(280, -1);
        let error_label = gtk::Label::new(None);
        error_label.set_xalign(0.0);
        error_label.set_line_wrap(true);
        content.add(&entry);
        content.add(&error_label);
        dialog.show_all();
        entry.grab_focus();
        let stem = stem_chars(initial);
        entry.select_region(0, if stem > 0 { stem as i32 } else { -1 });
        *self.imp().dialog.borrow_mut() = Some(dialog.clone());
        let weak = self.downgrade();
        dialog.connect_response(move |dialog, response| {
            if response != gtk::ResponseType::Ok {
                close_dialog(&weak, dialog);
                return;
            }
            dialog.set_response_sensitive(gtk::ResponseType::Ok, false);
            let (dialog_weak, label_weak, panel_weak) = (dialog.downgrade(), error_label.downgrade(), weak.clone());
            action(
                entry.text().to_string(),
                Box::new(move |result| {
                    let Some(dialog) = dialog_weak.upgrade() else { return };
                    match result {
                        Ok(()) => close_dialog(&panel_weak, &dialog),
                        Err(error) => {
                            if let Some(label) = label_weak.upgrade() {
                                label.set_text(&error.to_string());
                            }
                            dialog.set_response_sensitive(gtk::ResponseType::Ok, true);
                        }
                    }
                }),
            );
        });
    }

    pub fn new_dialog(&self, folder: bool) {
        let parent = self.target_folder();
        let weak = self.downgrade();
        self.name_dialog(
            if folder { "New Folder" } else { "New File" },
            "",
            Rc::new(move |name, done| {
                let Some(panel) = weak.upgrade() else { return };
                panel.create(&parent, &name, folder, Box::new(move |r| done(r.map(|_| ()))));
            }),
        );
    }

    pub fn rename_dialog(&self) {
        let Some(path) = self.selected_path() else { return };
        if path == self.root() {
            return;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let weak = self.downgrade();
        self.name_dialog(
            "Rename",
            &name,
            Rc::new(move |new_name, done| {
                let Some(panel) = weak.upgrade() else { return };
                panel.rename(&path, &new_name, Box::new(move |r| done(r.map(|_| ()))));
            }),
        );
    }

    /// Ask before removing `path`; `done(true)` when accepted.
    pub fn confirm_removal(&self, path: &Path, fallback: bool, done: Box<dyn FnOnce(bool)>) {
        let trash = self.imp().delete_mode.get() == DeleteMode::Trash && !fallback;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let title = if trash {
            format!("Move \"{name}\" to trash?")
        } else {
            format!("Delete \"{name}\"?")
        };
        let mut detail = if trash {
            "You can restore this item from the system trash."
        } else {
            "Folders and their contents are deleted recursively. This cannot be undone."
        }
        .to_owned();
        if fallback {
            detail = format!("Trash is unavailable here. Delete permanently instead?\n{detail}");
        }
        let request = ConfirmRequest {
            path: path.to_path_buf(),
            fallback,
            title,
            detail,
        };
        let hook = self.imp().hooks.borrow().confirm_removal.clone();
        if let Some(hook) = hook {
            done(hook(&request));
            return;
        }
        let dialog = gtk::MessageDialog::new(
            self.window().as_ref(),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Warning,
            gtk::ButtonsType::OkCancel,
            &request.title,
        );
        dialog.set_secondary_text(Some(&request.detail));
        *self.imp().dialog.borrow_mut() = Some(dialog.clone().upcast());
        let done = RefCell::new(Some(done));
        let weak = self.downgrade();
        dialog.connect_response(move |dialog, response| {
            close_dialog(&weak, dialog.upcast_ref());
            if let Some(done) = done.take() {
                done(response == gtk::ResponseType::Ok);
            }
        });
        dialog.show();
    }

    fn error_dialog(&self, message: &str) {
        let hook = self.imp().hooks.borrow().error_dialog.clone();
        if let Some(hook) = hook {
            hook(message);
            return;
        }
        let dialog = gtk::MessageDialog::new(
            self.window().as_ref(),
            gtk::DialogFlags::MODAL,
            gtk::MessageType::Error,
            gtk::ButtonsType::Close,
            message,
        );
        *self.imp().dialog.borrow_mut() = Some(dialog.clone().upcast());
        let weak = self.downgrade();
        dialog.connect_response(move |dialog, _| close_dialog(&weak, dialog.upcast_ref()));
        dialog.show();
    }

    /// Delete the selected path (or `path`) after confirmation, falling back to a second
    /// confirmation when the trash is unavailable.
    pub fn delete_dialog(&self, path: Option<&Path>) {
        let Some(path) = path.map(Path::to_path_buf).or_else(|| self.selected_path()) else {
            return;
        };
        if path == self.root() {
            return;
        }
        if self.imp().confirm_delete.get() {
            let weak = self.downgrade();
            let target = path.clone();
            self.confirm_removal(
                &path,
                false,
                Box::new(move |accepted| {
                    if let (true, Some(panel)) = (accepted, weak.upgrade()) {
                        panel.delete_confirmed(&target);
                    }
                }),
            );
        } else {
            self.delete_confirmed(&path);
        }
    }

    fn delete_confirmed(&self, path: &Path) {
        let weak = self.downgrade();
        let target = path.to_path_buf();
        self.delete(
            path,
            false,
            Box::new(move |result| {
                let Some(panel) = weak.upgrade() else { return };
                match result {
                    Ok(_) => {}
                    Err(FsOpError::TrashUnavailable(_)) => {
                        // Consent to trash is not consent to irreversible deletion, even
                        // when ordinary confirmation prompts are disabled.
                        let weak = panel.downgrade();
                        let path = target.clone();
                        panel.confirm_removal(
                            &target,
                            true,
                            Box::new(move |accepted| {
                                if let (true, Some(panel)) = (accepted, weak.upgrade()) {
                                    let weak = panel.downgrade();
                                    panel.delete(
                                        &path,
                                        true,
                                        Box::new(move |result| {
                                            if let (Err(error), Some(panel)) = (result, weak.upgrade()) {
                                                panel.error_dialog(&error.to_string());
                                            }
                                        }),
                                    );
                                }
                            }),
                        );
                    }
                    Err(error) => panel.error_dialog(&error.to_string()),
                }
            }),
        );
    }

    fn copy_path(&self) {
        let path = self.selected_path().unwrap_or_else(|| self.root());
        gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&model_path(&path));
    }

    // ---- menu, pointer and keys ----------------------------------------------------------------------

    /// The tree's context menu for the current selection.
    #[doc(hidden)]
    pub fn build_menu(&self) -> gtk::Menu {
        let selected = self.selected_path();
        let editable = selected.as_ref().is_some_and(|p| *p != self.root());
        let menu = gtk::Menu::builder().attach_widget(&self.w().tree).build();
        type Callback = Box<dyn Fn(&ExplorerPanel)>;
        let items: [(&str, Callback); 6] = [
            ("New File...", Box::new(|p| p.new_dialog(false))),
            ("New Folder...", Box::new(|p| p.new_dialog(true))),
            ("Rename...", Box::new(|p| p.rename_dialog())),
            ("Delete", Box::new(|p| p.delete_dialog(None))),
            ("Refresh", Box::new(|p| p.refresh())),
            ("Copy Path", Box::new(|p| p.copy_path())),
        ];
        for (title, callback) in items {
            let item = gtk::MenuItem::with_label(title);
            if title == "Delete" {
                item.style_context().add_class("destructive-action");
            }
            let weak = self.downgrade();
            item.connect_activate(move |_| {
                if let Some(panel) = weak.upgrade() {
                    callback(&panel);
                }
            });
            if title == "Rename..." || title == "Delete" {
                item.set_sensitive(editable);
            }
            menu.append(&item);
        }
        menu.show_all();
        menu.connect_selection_done(|menu| {
            // SAFETY: the menu is dismissed and referenced only by GTK's attach list.
            unsafe { menu.destroy() }
        });
        menu
    }

    pub(super) fn popup_menu(&self, event: Option<&gdk::EventButton>) {
        let menu = self.build_menu();
        match event {
            Some(event) => menu.popup_at_pointer(Some(event)),
            None => menu.popup_at_widget(
                &self.w().tree,
                gdk::Gravity::Center,
                gdk::Gravity::NorthWest,
                None,
            ),
        }
    }

    pub(super) fn on_button(&self, tree: &gtk::TreeView, event: &gdk::EventButton) -> glib::Propagation {
        // A click is the user taking over from any pending reveal.
        self.imp().want_select.borrow_mut().take();
        if event.button() != 3 {
            return glib::Propagation::Proceed;
        }
        let (x, y) = event.position();
        match tree.path_at_pos(x as i32, y as i32).and_then(|(path, ..)| path) {
            Some(path) => tree.set_cursor(&path, None::<&gtk::TreeViewColumn>, false),
            None => tree.selection().unselect_all(),
        }
        tree.grab_focus();
        self.popup_menu(Some(event));
        glib::Propagation::Stop
    }

    fn search_has_focus(&self) -> bool {
        let focus = self
            .window()
            .and_then(|window| window.focused_widget());
        let w = self.w();
        focus.is_some_and(|focus| {
            focus.is_ancestor(&w.search) && !focus.is_ancestor(&w.scroll)
        })
    }

    /// Explorer shortcuts: F2 rename, Delete, F5 refresh, Ctrl+Shift+N new folder.
    #[doc(hidden)]
    pub fn on_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as key;
        if self.search_has_focus() {
            return false;
        }
        let in_notes = self
            .window()
            .and_then(|window| window.focused_widget())
            .is_some_and(|focus| focus == *self.w().notes.tree.upcast_ref::<gtk::Widget>());
        if in_notes {
            return self.notes_key(event);
        }
        let value = event.keyval();
        let state = event.state();
        if value == key::F2 {
            self.rename_dialog();
        } else if value == key::Delete || value == key::KP_Delete {
            self.delete_dialog(None);
        } else if value == key::F5 {
            self.refresh();
        } else if (value == key::n || value == key::N)
            && state.contains(gdk::ModifierType::CONTROL_MASK)
            && state.contains(gdk::ModifierType::SHIFT_MASK)
        {
            self.new_dialog(true);
        } else {
            return false;
        }
        true
    }
}

fn close_dialog(panel: &glib::WeakRef<ExplorerPanel>, dialog: &gtk::Dialog) {
    if let Some(panel) = panel.upgrade() {
        panel.imp().dialog.take();
    }
    // SAFETY: the dialog is modal and owned by the explorer; nothing uses it afterwards.
    unsafe { dialog.destroy() }
}

