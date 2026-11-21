//! Pointer and keyboard handling of the file tree, following Zed's project panel.
//!
//! * A click on a folder expands or collapses it; a click on a file opens it as a preview
//!   (`file-previewed`), keeping the keyboard in the tree. A double click on a file opens it
//!   for editing (`file-activated`, the editor takes the focus).
//! * Left collapses the selected folder, or else the folder holding the selection, and
//!   selects it; Right expands the selected folder, or moves into it when it is open.
//!   Ctrl+Left collapses every folder below the root.
//! * Enter and F2 rename, Space opens (a file as a preview, a folder expands or collapses),
//!   Delete and Backspace remove, Ctrl+N creates a file and Alt+N a folder, Shift+Alt+C copies
//!   the path. Up and Down move the selection (GTK's own bindings).
use super::*;

impl ExplorerPanel {
    pub(super) fn connect_navigation(&self) {
        let w = self.w();
        let tree = &w.tree;
        tree.add_events(gdk::EventMask::BUTTON_RELEASE_MASK);
        let weak = self.downgrade();
        tree.connect_button_press_event(move |tree, event| {
            if let Some(panel) = weak.upgrade() {
                panel.primary_press(tree, event);
            }
            glib::Propagation::Proceed
        });
        let weak = self.downgrade();
        tree.connect_button_release_event(move |tree, event| {
            if let Some(panel) = weak.upgrade() {
                panel.primary_release(tree, event);
            }
            glib::Propagation::Proceed
        });
        let weak = self.downgrade();
        tree.connect_key_press_event(move |_, event| {
            if weak.upgrade().is_some_and(|panel| panel.tree_key(event)) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        let weak = self.downgrade();
        tree.connect_local("draw", true, move |args| {
            if let (Some(panel), Ok(cr)) = (weak.upgrade(), args[1].get::<gtk::cairo::Context>()) {
                panel.paint_decorations(&cr);
            }
            Some(false.to_value())
        });
        // The active guide follows the selection, the border the focus.
        tree.connect_cursor_changed(|tree| tree.queue_draw());
        tree.connect_focus_in_event(|tree, _| {
            tree.queue_draw();
            glib::Propagation::Proceed
        });
        tree.connect_focus_out_event(|tree, _| {
            tree.queue_draw();
            glib::Propagation::Proceed
        });
    }

    fn primary_press(&self, tree: &gtk::TreeView, event: &gdk::EventButton) {
        let imp = self.imp();
        if event.button() != 1 {
            return;
        }
        if event.event_type() == gdk::EventType::ButtonPress {
            // GTK emits `row-activated` for a double click while it handles this press;
            // the flag ends with the press (GTK may stop the emission, so no after-handler).
            imp.pressing.set(true);
            let weak = self.downgrade();
            glib::idle_add_local_once(move || {
                if let Some(panel) = weak.upgrade() {
                    panel.imp().pressing.set(false);
                }
            });
            imp.mouse_down.set(true);
            let (x, y) = event.position();
            *imp.pressed_row.borrow_mut() = tree
                .path_at_pos(x as i32, y as i32)
                .and_then(|(path, ..)| path);
            tree.queue_draw();
        }
    }

    fn primary_release(&self, tree: &gtk::TreeView, event: &gdk::EventButton) {
        let imp = self.imp();
        if event.button() != 1 {
            return;
        }
        imp.mouse_down.set(false);
        imp.pressing.set(false);
        tree.queue_draw();
        let pressed = imp.pressed_row.borrow_mut().take();
        let modifiers = gdk::ModifierType::SHIFT_MASK
            | gdk::ModifierType::CONTROL_MASK
            | gdk::ModifierType::MOD1_MASK;
        if event.state().intersects(modifiers) {
            return;
        }
        let (x, y) = event.position();
        let released = tree
            .path_at_pos(x as i32, y as i32)
            .and_then(|(path, ..)| path);
        let (Some(pressed), Some(released)) = (pressed, released) else {
            return;
        };
        if pressed != released {
            return;
        }
        self.click_row(&released);
    }

    /// Click the row of `path` as a single primary click does (test seam).
    #[doc(hidden)]
    pub fn click(&self, path: &Path) {
        if let Some(row) = self.find_row(path) {
            self.click_row(&util::path_of(&self.w().store, &row));
        }
    }

    /// A single click: a folder expands or collapses, a file opens as a preview.
    fn click_row(&self, path: &gtk::TreePath) {
        let w = self.w();
        let Some(iter) = w.store.iter(path) else {
            return;
        };
        let full_path = util::string(&w.store, &iter, 2);
        if full_path.is_empty() {
            return;
        }
        if util::string(&w.store, &iter, 0) == "folder" {
            if w.tree.row_expanded(path) {
                w.tree.collapse_row(path);
            } else {
                w.tree.expand_row(path, false);
            }
        } else {
            self.emit_file_previewed(&util::decode_path(&full_path));
        }
    }

    /// `row-activated` during a primary press is the second press of a double click.
    pub(super) fn double_click_activation(&self) -> bool {
        self.imp().pressing.get()
    }

    /// Zed's key bindings for the tree; `true` when the key was used.
    pub(super) fn tree_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as key;
        use gdk::ModifierType as M;
        let state = event.state() & gtk::accelerator_get_default_mod_mask();
        let value = event.keyval();
        let lower = value.to_lower();
        let (cursor, _) = self.w().tree.cursor();
        let left = value == key::Left || value == key::KP_Left;
        let right = value == key::Right || value == key::KP_Right;
        if state.is_empty() && left {
            if let Some(path) = cursor {
                self.collapse_towards_root(path);
            }
        } else if state.is_empty() && right {
            if let Some(path) = cursor {
                self.expand_or_enter(&path);
            }
        } else if state == M::CONTROL_MASK && left {
            self.collapse_all_below_root();
        } else if state.is_empty()
            && (value == key::Return || value == key::KP_Enter || value == key::ISO_Enter)
        {
            self.rename_dialog();
        } else if state.is_empty() && (value == key::space || value == key::KP_Space) {
            if let Some(path) = cursor {
                self.click_row(&path);
            }
        } else if state.is_empty() && value == key::BackSpace {
            self.delete_dialog(None);
        } else if state == M::CONTROL_MASK && lower == key::n {
            self.new_dialog(false);
        } else if state == M::MOD1_MASK && lower == key::n {
            self.new_dialog(true);
        } else if state == M::SHIFT_MASK | M::MOD1_MASK && lower == key::c {
            self.copy_path();
        } else {
            return false;
        }
        true
    }

    /// Collapse the selected folder if it is open, otherwise the nearest open folder above it,
    /// and select that folder.
    fn collapse_towards_root(&self, mut path: gtk::TreePath) {
        let w = self.w();
        loop {
            let is_folder = w
                .store
                .iter(&path)
                .is_some_and(|iter| util::string(&w.store, &iter, 0) == "folder");
            if is_folder && w.tree.row_expanded(&path) {
                w.tree.collapse_row(&path);
                w.tree
                    .set_cursor(&path, None::<&gtk::TreeViewColumn>, false);
                return;
            }
            if path.depth() <= 1 || !path.up() {
                return;
            }
        }
    }

    /// Expand the selected folder; when it is already open, select its first row.
    fn expand_or_enter(&self, path: &gtk::TreePath) {
        let w = self.w();
        let Some(iter) = w.store.iter(path) else {
            return;
        };
        if util::string(&w.store, &iter, 0) != "folder" {
            return;
        }
        if !w.tree.row_expanded(path) {
            w.tree.expand_row(path, false);
        } else if let Some(next) = self.next_visible_row(path) {
            w.tree
                .set_cursor(&next, None::<&gtk::TreeViewColumn>, false);
        }
    }

    /// Collapse every folder except the root, keeping the root open.
    fn collapse_all_below_root(&self) {
        let w = self.w();
        let root = gtk::TreePath::new_first();
        w.tree.collapse_all();
        w.tree.expand_row(&root, false);
        w.tree
            .set_cursor(&root, None::<&gtk::TreeViewColumn>, false);
    }
}
