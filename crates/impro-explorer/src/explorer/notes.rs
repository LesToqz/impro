//! The collapsible Notes section below the file tree.
use super::*;
use crate::pool::{self, Lane};
use gio::prelude::*;
use impro_core::notes;
use impro_ui::motion::{self, AnimateOptions, StateFader};
use std::time::Duration;

/// Quiet period before external notes changes refresh the explorer.
const WATCH_DEBOUNCE: Duration = Duration::from_millis(150);

/// A notes folder listing taken on a worker thread.
pub(crate) struct Listing {
    /// `.impro/Notes` exists.
    pub exists: bool,
    /// Existing folders worth watching: the root, `.impro` and the notes folder.
    pub watch: Vec<PathBuf>,
    /// Notes with their modification time (seconds since the epoch).
    pub notes: Vec<(PathBuf, Option<f64>)>,
}

/// "just now", "5 min ago", "2 hr ago", "yesterday" or "3 Oct" for a modification time.
pub fn note_edited_text(timestamp: f64, now: Option<&glib::DateTime>) -> String {
    let now = now.cloned().or_else(|| glib::DateTime::now_local().ok());
    let Some(now) = now else { return String::new() };
    let Ok(edited) = glib::DateTime::from_unix_local(timestamp.floor() as i64) else {
        return String::new();
    };
    let now_seconds = now.to_unix() as f64 + f64::from(now.microsecond()) / 1e6;
    let seconds = (now_seconds - timestamp).max(0.0);
    if seconds < 60.0 {
        return "just now".into();
    }
    if seconds < 3600.0 {
        return format!("{} min ago", (seconds / 60.0) as i64);
    }
    let day = |t: &glib::DateTime| (t.to_unix() + t.utc_offset().as_seconds()).div_euclid(86_400);
    let (today, then) = (day(&now), day(&edited));
    if today == then {
        return format!("{} hr ago", (seconds / 3600.0) as i64);
    }
    if today - then == 1 {
        return "yesterday".into();
    }
    format!(
        "{} {}",
        edited.day_of_month(),
        edited
            .format("%b")
            .map(|s| s.to_string())
            .unwrap_or_default()
    )
}

/// Cross-fade a button's background with the pointer, as the original does.
fn soft_hover(button: &impl IsA<gtk::Widget>, filled: bool) -> StateFader {
    let provider = gtk::CssProvider::new();
    button
        .style_context()
        .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
    StateFader::new(
        button,
        move |amount| {
            let opacity = if filled {
                0.055 + amount * 0.04
            } else {
                amount * 0.055
            };
            let _ = provider.load_from_data(
                format!("button {{ background: alpha(@impro_notes_overlay, {opacity}); }}")
                    .as_bytes(),
            );
        },
        128.0,
        0.0,
    )
}

impl ExplorerPanel {
    pub(super) fn build_notes_section(&self) -> NotesWidgets {
        let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        section.style_context().add_class("impro-notes-section");
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        header.style_context().add_class("impro-notes-header");
        let collapse = gtk::ToggleButton::new();
        collapse.set_relief(gtk::ReliefStyle::None);
        set_accessible_name(&collapse, "Collapse notes");
        let title = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let chevron = gtk::Image::from_icon_name(Some("pan-down-symbolic"), gtk::IconSize::Menu);
        let chevron_css = gtk::CssProvider::new();
        chevron
            .style_context()
            .add_provider(&chevron_css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        title.pack_start(&chevron, false, false, 0);
        let label = gtk::Label::new(Some("Notes"));
        label.set_xalign(0.0);
        title.pack_start(&label, false, false, 0);
        let count = gtk::Label::new(Some("0"));
        count.set_valign(gtk::Align::Center);
        count.style_context().add_class("impro-notes-count");
        title.pack_start(&count, false, false, 0);
        collapse.add(&title);
        let weak = self.downgrade();
        collapse.connect_toggled(move |button| {
            if let Some(panel) = weak.upgrade() {
                panel.set_notes_collapsed(button.is_active());
            }
        });
        header.pack_start(&collapse, true, true, 0);
        let new_button =
            gtk::Button::from_icon_name(Some("list-add-symbolic"), gtk::IconSize::Menu);
        new_button.set_relief(gtk::ReliefStyle::None);
        new_button.set_tooltip_text(Some("New Note"));
        set_accessible_name(&new_button, "New Note");
        let weak = self.downgrade();
        new_button.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.new_note_dialog();
            }
        });
        header.pack_end(&new_button, false, false, 0);
        section.pack_start(&header, false, false, 0);

        let store = gtk::ListStore::new(&[
            String::static_type(),
            String::static_type(),
            String::static_type(),
        ]);
        let tree = gtk::TreeView::with_model(&store);
        tree.set_headers_visible(false);
        set_accessible_name(&tree, "Notes");
        tree.style_context().add_class("impro-notes-list");
        tree.set_margin_start(8);
        tree.set_margin_end(8);
        tree.set_margin_top(4);
        tree.set_margin_bottom(4);
        let hover_css = gtk::CssProvider::new();
        tree.style_context()
            .add_provider(&hover_css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
        tree.set_hover_selection(false);
        for signal in ["motion-notify-event", "leave-notify-event"] {
            let weak = self.downgrade();
            let handler = move |tree: &gtk::TreeView, motion: Option<(f64, f64)>| {
                if let Some(panel) = weak.upgrade() {
                    panel.notes_pointer(tree, motion);
                }
            };
            if signal == "motion-notify-event" {
                tree.connect_motion_notify_event(move |tree, event| {
                    handler(tree, Some(event.position()));
                    glib::Propagation::Proceed
                });
            } else {
                tree.connect_leave_notify_event(move |tree, _| {
                    handler(tree, None);
                    glib::Propagation::Proceed
                });
            }
        }
        let column = gtk::TreeViewColumn::new();
        column.set_sizing(gtk::TreeViewColumnSizing::Fixed);
        column.set_expand(true);
        let marker = gtk::CellRendererText::new();
        marker.set_text(Some(""));
        marker.set_xpad(0);
        marker.set_ypad(0);
        marker.set_fixed_size(4, -1);
        util::pack(&column, &marker, false);
        let weak = self.downgrade();
        util::data_func(&column, &marker, move |cell, model, iter| {
            if let Some(panel) = weak.upgrade() {
                panel.notes_open_marker(cell, model, iter);
            }
        });
        let icon = crate::icons::IconCell::new();
        icon.set_xpad(4);
        util::pack(&column, &icon, false);
        util::attribute(&column, &icon, "icon-name", 0);
        let cell = gtk::CellRendererText::new();
        cell.set_ellipsize(pango::EllipsizeMode::End);
        cell.set_xpad(4);
        util::pack(&column, &cell, true);
        util::attribute(&column, &cell, "text", 1);
        let edited_cell = gtk::CellRendererText::new();
        edited_cell.set_xalign(1.0);
        edited_cell.set_xpad(8);
        edited_cell.set_scale(0.82);
        util::pack_end(&column, &edited_cell, false);
        let weak = self.downgrade();
        util::data_func(&column, &edited_cell, move |cell, model, iter| {
            if let Some(panel) = weak.upgrade() {
                panel.notes_row_data(cell, model, iter);
            }
        });
        tree.append_column(&column);
        tree.set_activate_on_single_click(true);
        let weak = self.downgrade();
        tree.connect_row_activated(move |tree, path, _| {
            let Some(panel) = weak.upgrade() else { return };
            if let Some(iter) = tree.model().and_then(|m| m.iter(path)) {
                if let Some(model) = tree.model() {
                    let file = util::string(&model, &iter, 2);
                    panel.emit_file_activated(&util::decode_path(&file));
                }
            }
        });
        let weak = self.downgrade();
        tree.connect_button_press_event(move |tree, event| {
            weak.upgrade().map_or(glib::Propagation::Proceed, |panel| {
                if panel.notes_button_press(tree, event) {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            })
        });
        let weak = self.downgrade();
        tree.connect_popup_menu(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.notes_menu(None);
            }
            true
        });
        let weak = self.downgrade();
        tree.connect_key_press_event(move |_, event| {
            if weak.upgrade().is_some_and(|panel| panel.notes_key(event)) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });

        let empty = gtk::Box::new(gtk::Orientation::Vertical, 4);
        empty.style_context().add_class("impro-notes-empty");
        empty.set_valign(gtk::Align::Center);
        let empty_icon =
            gtk::Image::from_icon_name(Some("document-edit-symbolic"), gtk::IconSize::Dialog);
        empty_icon.set_pixel_size(28);
        empty_icon
            .style_context()
            .add_class("impro-notes-empty-icon");
        empty.pack_start(&empty_icon, false, false, 4);
        let empty_title = gtk::Label::new(Some("No notes yet"));
        empty_title
            .style_context()
            .add_class("impro-notes-empty-title");
        empty.pack_start(&empty_title, false, false, 0);
        empty.pack_start(
            &gtk::Label::new(Some("A little space for your ideas.")),
            false,
            false,
            0,
        );
        let empty_new = gtk::Button::with_label("New note");
        empty_new.set_halign(gtk::Align::Center);
        empty_new.set_relief(gtk::ReliefStyle::None);
        let weak = self.downgrade();
        empty_new.connect_clicked(move |_| {
            if let Some(panel) = weak.upgrade() {
                panel.new_note_dialog();
            }
        });
        empty.pack_start(&empty_new, false, false, 0);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .build();
        scroll.style_context().add_class("impro-notes-scroll");
        scroll.set_overlay_scrolling(true);
        scroll.add(&tree);
        let stack = gtk::Stack::new();
        stack.add_named(&scroll, "list");
        stack.add_named(&empty, "empty");
        let body = gtk::Revealer::new();
        body.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        body.set_reveal_child(true);
        body.add(&stack);
        let weak = self.downgrade();
        body.connect_notify_local(Some("child-revealed"), move |_, _| {
            if let Some(panel) = weak.upgrade() {
                panel.layout_notes();
            }
        });
        section.pack_start(&body, true, true, 0);
        // Each fader is owned by its button's signal handlers.
        soft_hover(&collapse, false);
        soft_hover(&new_button, false);
        soft_hover(&empty_new, true);
        section.show_all();
        NotesWidgets {
            section,
            header,
            collapse,
            chevron,
            chevron_css,
            count,
            new_button,
            store,
            tree,
            hover_css,
            cell,
            marker,
            edited_cell,
            empty,
            empty_new,
            scroll,
            stack,
            body,
        }
    }

    fn notes_open_marker(
        &self,
        cell: &gtk::CellRenderer,
        model: &gtk::TreeModel,
        iter: &gtk::TreeIter,
    ) {
        let path = util::string(model, iter, 2);
        let active = self.imp().active_file.borrow().as_deref().map(model_path);
        cell.set_property(
            "text",
            if active.as_deref() == Some(path.as_str()) {
                "▏"
            } else {
                ""
            },
        );
        if let Some(colour) = self
            .w()
            .notes
            .tree
            .style_context()
            .lookup_color("impro_notes_accent")
        {
            cell.set_property("foreground-rgba", colour);
        }
    }

    fn notes_row_data(
        &self,
        cell: &gtk::CellRenderer,
        model: &gtk::TreeModel,
        iter: &gtk::TreeIter,
    ) {
        let path = util::decode_path(&util::string(model, iter, 2));
        let text = self
            .imp()
            .notes_edited
            .borrow()
            .get(&path)
            .cloned()
            .unwrap_or_default();
        cell.set_property("text", text);
        cell.set_property("size-points", self.imp().font_size.get());
        if let Some(colour) = self
            .w()
            .notes
            .tree
            .style_context()
            .lookup_color("impro_notes_muted")
        {
            cell.set_property("foreground-rgba", colour);
        }
    }

    fn notes_pointer(&self, tree: &gtk::TreeView, position: Option<(f64, f64)>) {
        let imp = self.imp();
        let hit = position.and_then(|(x, y)| tree.path_at_pos(x as i32, y as i32));
        let path = hit.and_then(|(path, ..)| path).and_then(|path| {
            let store = &self.w().notes.store;
            store.iter(&path).map(|iter| util::string(store, &iter, 2))
        });
        if *imp.notes_hover_path.borrow() == path {
            return;
        }
        *imp.notes_hover_path.borrow_mut() = path.clone();
        let mut starts = imp.notes_hover.borrow().clone();
        if let Some(path) = &path {
            starts.entry(path.clone()).or_insert(0.0);
        }
        let weak = self.downgrade();
        let tree_weak = tree.downgrade();
        let update = move |t: f64| {
            let (Some(panel), Some(tree)) = (weak.upgrade(), tree_weak.upgrade()) else {
                return;
            };
            let imp = panel.imp();
            let next: HashMap<String, f64> = starts
                .iter()
                .filter(|(key, _)| Some(*key) == path.as_ref() || t < 1.0)
                .map(|(key, value)| {
                    let target = if Some(key) == path.as_ref() { 1.0 } else { 0.0 };
                    (key.clone(), value + (target - value) * t)
                })
                .collect();
            let amount = match &path {
                Some(path) => next.get(path).copied().unwrap_or(0.0),
                None => next.values().copied().fold(0.0, f64::max),
            };
            *imp.notes_hover.borrow_mut() = next;
            let _ = panel.w().notes.hover_css.load_from_data(
                format!(
                    "treeview.impro-notes-list:hover:not(:selected) {{ background: alpha(@impro_notes_overlay, {}); }}",
                    amount * 0.055
                )
                .as_bytes(),
            );
            tree.queue_draw();
        };
        motion::animate(tree, 128, update, AnimateOptions::default());
    }

    #[doc(hidden)]
    pub fn notes_section_visible(&self) -> bool {
        self.imp().notes_visible.get()
    }

    pub fn set_notes_enabled(&self, enabled: bool) {
        let imp = self.imp();
        imp.notes_enabled.set(enabled);
        let button = &self.w().notes_button;
        button.set_no_show_all(!enabled);
        button.set_visible(enabled);
        let wanted = imp
            .notes_requested
            .get()
            .unwrap_or_else(|| imp.notes_dir_exists.get());
        self.set_notes_visible(enabled && wanted);
    }

    pub fn show_notes_section(&self, visible: bool) {
        let imp = self.imp();
        imp.notes_requested.set(Some(visible));
        self.set_notes_visible(visible && imp.notes_enabled.get());
        if imp.notes_visible.get() {
            self.refresh_notes();
        }
    }

    fn set_notes_visible(&self, visible: bool) {
        let imp = self.imp();
        let w = self.w();
        imp.notes_visible.set(visible);
        w.notes.section.set_visible(visible);
        if w.notes_button.is_active() != visible {
            // Updating the button must not turn automatic visibility into a user override.
            let requested = imp.notes_requested.get();
            w.notes_button.set_active(visible);
            imp.notes_requested.set(requested);
        }
        self.layout_notes();
    }

    pub fn set_notes_collapsed(&self, collapsed: bool) {
        let imp = self.imp();
        let w = self.w();
        imp.notes_collapsed.set(collapsed);
        if w.notes.collapse.is_active() != collapsed {
            w.notes.collapse.set_active(collapsed);
            return;
        }
        let start = imp.notes_angle.get();
        let target = if collapsed {
            -std::f64::consts::FRAC_PI_2
        } else {
            0.0
        };
        let weak = self.downgrade();
        motion::animate(
            &w.notes.chevron,
            150,
            move |t| {
                let Some(panel) = weak.upgrade() else { return };
                let angle = start + (target - start) * t;
                panel.imp().notes_angle.set(angle);
                let _ = panel.w().notes.chevron_css.load_from_data(
                    format!("image {{ -gtk-icon-transform: rotate({angle}rad); }}").as_bytes(),
                );
            },
            AnimateOptions::default(),
        );
        set_accessible_name(
            &w.notes.collapse,
            if collapsed {
                "Expand notes"
            } else {
                "Collapse notes"
            },
        );
        w.notes
            .body
            .set_transition_duration(if motion::animations_enabled() { 150 } else { 0 });
        w.notes.body.set_reveal_child(!collapsed);
        self.layout_notes();
    }

    #[doc(hidden)]
    pub fn notes_collapsed(&self) -> bool {
        self.imp().notes_collapsed.get()
    }

    #[doc(hidden)]
    pub fn notes_height(&self) -> Option<i32> {
        self.imp().notes_height.get()
    }

    #[doc(hidden)]
    pub fn notes_angle(&self) -> f64 {
        self.imp().notes_angle.get()
    }

    pub(super) fn layout_notes(&self) {
        let imp = self.imp();
        if !imp.notes_visible.get() || imp.notes_layout.get() {
            return;
        }
        let w = self.w();
        let height = w.notes_split.allocated_height() - 6;
        if height < 1 {
            return;
        }
        let header = w.notes.header.preferred_height().0;
        let collapsed = imp.notes_collapsed.get();
        let minimum = if collapsed { header } else { 96 };
        w.notes.section.set_size_request(-1, minimum);
        let wanted = if collapsed {
            header
        } else {
            imp.notes_height
                .get()
                .unwrap_or_else(|| (f64::from(height) * 0.3).round() as i32)
        };
        let section_height = minimum.max(wanted.min(height - 120));
        imp.notes_layout.set(true);
        w.notes_split.set_position(120.max(height - section_height));
        imp.notes_layout.set(false);
    }

    pub(super) fn notes_position_changed(&self) {
        let imp = self.imp();
        let w = self.w();
        if imp.notes_layout.get()
            || !imp.notes_visible.get()
            || imp.notes_collapsed.get()
            || !w.notes_split.is_mapped()
        {
            return;
        }
        let height = w.notes_split.allocated_height() - 6;
        if height >= 216 {
            imp.notes_height.set(Some(
                96.max((height - w.notes_split.position()).min(height - 120)),
            ));
            self.layout_notes();
        }
    }

    pub fn focus_notes(&self) {
        self.show_notes_section(true);
        self.set_notes_collapsed(false);
        let w = self.w();
        if w.notes.store.iter_n_children(None) > 0 {
            w.notes.tree.grab_focus();
        } else {
            w.notes.new_button.grab_focus();
        }
    }

    pub fn set_notes_sort(&self, sort: &str) -> Result<(), String> {
        if !["name", "modified"].contains(&sort) {
            return Err("Notes sort must be 'name' or 'modified'.".into());
        }
        *self.imp().notes_sort.borrow_mut() = sort.to_owned();
        self.refresh_notes();
        Ok(())
    }

    /// Reload the notes list on a worker, preserving the active editor's highlight.
    pub fn refresh_notes(&self) {
        let imp = self.imp();
        if imp.destroyed.get() {
            return;
        }
        if imp.notes_running.get() {
            imp.notes_dirty.set(true);
            return;
        }
        let Some(sender) = self.sender() else { return };
        imp.notes_running.set(true);
        let generation = imp.notes_generation.get() + 1;
        imp.notes_generation.set(generation);
        let root = imp.root.borrow().clone();
        let sort = imp.notes_sort.borrow().clone();
        pool::spawn(Lane::Interactive, move || {
            let folder = notes::notes_dir(&root);
            let exists = folder.is_dir();
            let watch = [
                root.clone(),
                folder.parent().map(Path::to_path_buf).unwrap_or_default(),
                folder.clone(),
            ]
            .into_iter()
            .filter(|p| !p.as_os_str().is_empty() && p.is_dir())
            .collect();
            let notes = notes::list_notes(&root, &sort)
                .unwrap_or_default()
                .into_iter()
                .map(|path| {
                    let modified = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64());
                    (path, modified)
                })
                .collect();
            let _ = sender.send(Msg::Notes {
                generation,
                listing: Listing {
                    exists,
                    watch,
                    notes,
                },
            });
        });
    }

    pub(super) fn deliver_notes(&self, generation: u64, listing: Listing) {
        let imp = self.imp();
        imp.notes_running.set(false);
        if generation == imp.notes_generation.get() {
            self.apply_notes(listing);
        }
        if imp.notes_dirty.replace(false) {
            self.refresh_notes();
        }
    }

    fn apply_notes(&self, listing: Listing) {
        let imp = self.imp();
        let w = self.w();
        let selected = self.selected_note();
        w.notes.store.clear();
        let mut edited = HashMap::new();
        let now = glib::DateTime::now_local().ok();
        for (path, modified) in &listing.notes {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let stem: String = {
                let count = name.chars().count();
                name.chars().take(count.saturating_sub(3)).collect()
            };
            w.notes.store.insert_with_values(
                None,
                &[
                    (0, &"text-x-generic-symbolic"),
                    (1, &stem),
                    (2, &model_path(path)),
                ],
            );
            edited.insert(
                path.clone(),
                modified.map_or_else(String::new, |t| note_edited_text(t, now.as_ref())),
            );
        }
        *imp.notes_edited.borrow_mut() = edited;
        let count = listing.notes.len();
        w.notes.count.set_text(&count.to_string());
        w.notes
            .stack
            .set_visible_child_name(if count > 0 { "list" } else { "empty" });
        imp.notes_dir_exists.set(listing.exists);
        let pending = imp.notes_pending_select.borrow_mut().take();
        let target = pending
            .or_else(|| imp.active_file.borrow().clone())
            .or(selected);
        if let Some(target) = target {
            self.select_note(&target);
        }
        if imp.notes_requested.get().is_none() {
            self.set_notes_visible(imp.notes_enabled.get() && listing.exists);
        }
        self.watch_notes(&listing.watch);
    }

    fn watch_notes(&self, paths: &[PathBuf]) {
        let imp = self.imp();
        let wanted: HashSet<&PathBuf> = paths.iter().collect();
        let stale: Vec<PathBuf> = imp
            .notes_monitors
            .borrow()
            .keys()
            .filter(|p| !wanted.contains(p))
            .cloned()
            .collect();
        for path in stale {
            if let Some(monitor) = imp.notes_monitors.borrow_mut().remove(&path) {
                monitor.cancel();
            }
        }
        for path in paths {
            if imp.notes_monitors.borrow().contains_key(path) {
                continue;
            }
            let monitor = gio::File::for_path(path)
                .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE);
            if let Ok(monitor) = monitor {
                let weak = self.downgrade();
                monitor.connect_changed(move |_, file, other, _| {
                    if let Some(panel) = weak.upgrade() {
                        panel.notes_changed(file, other);
                    }
                });
                imp.notes_monitors
                    .borrow_mut()
                    .insert(path.clone(), monitor);
            }
        }
    }

    fn notes_changed(&self, file: &gio::File, other: Option<&gio::File>) {
        let imp = self.imp();
        let folder = notes::notes_dir(&*imp.root.borrow());
        let parent = folder.parent().map(Path::to_path_buf);
        let relevant = [Some(file), other].into_iter().flatten().any(|item| {
            item.path().is_some_and(|p| {
                Some(&p) == parent.as_ref() || p == folder || p.parent() == Some(folder.as_path())
            })
        });
        if relevant && imp.notes_refresh.borrow().is_none() {
            let weak = self.downgrade();
            let source = glib::timeout_add_local_once(WATCH_DEBOUNCE, move || {
                if let Some(panel) = weak.upgrade() {
                    panel.imp().notes_refresh.take();
                    panel.refresh();
                }
            });
            *imp.notes_refresh.borrow_mut() = Some(source);
        }
    }

    pub(super) fn notes_toplevel_changed(&self) {
        let imp = self.imp();
        if let Some((window, handler)) = imp.notes_window.take() {
            if let Some(window) = window.upgrade() {
                window.disconnect(handler);
            }
        }
        let window = self
            .toplevel()
            .and_then(|top| top.downcast::<gtk::Window>().ok());
        if let Some(window) = window {
            let weak = self.downgrade();
            let handler = window.connect_focus_in_event(move |_, _| {
                if let Some(panel) = weak.upgrade() {
                    panel.refresh_notes();
                }
                glib::Propagation::Proceed
            });
            *imp.notes_window.borrow_mut() = Some((window.downgrade(), handler));
        }
    }

    pub(super) fn stop_notes(&self) {
        let imp = self.imp();
        for (_, monitor) in imp.notes_monitors.take() {
            monitor.cancel();
        }
        if let Some(source) = imp.notes_refresh.take() {
            source.remove();
        }
        if let Some((window, handler)) = imp.notes_window.take() {
            if let Some(window) = window.upgrade() {
                window.disconnect(handler);
            }
        }
    }

    // ---- selection and actions ---------------------------------------------------------------

    #[doc(hidden)]
    pub fn selected_note(&self) -> Option<PathBuf> {
        if self.is_destroyed() {
            return None;
        }
        let (_, iter) = self.w().notes.tree.selection().selected()?;
        Some(util::decode_path(&util::string(
            &self.w().notes.store,
            &iter,
            2,
        )))
    }

    #[doc(hidden)]
    pub fn select_note(&self, path: &Path) {
        if self.is_destroyed() {
            return;
        }
        let w = self.w();
        let selection = w.notes.tree.selection();
        selection.unselect_all();
        let wanted = model_path(path);
        if let Some(iter) = w.notes.store.iter_first() {
            loop {
                if util::string(&w.notes.store, &iter, 2) == wanted {
                    selection.select_iter(&iter);
                    break;
                }
                if !w.notes.store.iter_next(&iter) {
                    break;
                }
            }
        }
    }

    fn rename_note(&self) {
        let Some(path) = self.selected_note() else {
            return;
        };
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem: String = name
            .chars()
            .take(name.chars().count().saturating_sub(3))
            .collect();
        let weak = self.downgrade();
        self.name_dialog(
            "Rename",
            &stem,
            Rc::new(move |name, done| {
                let Some(panel) = weak.upgrade() else { return };
                match notes::note_filename(&name) {
                    Ok(filename) => {
                        panel.rename(&path, &filename, Box::new(move |r| done(r.map(|_| ()))))
                    }
                    Err(error) => done(Err(error)),
                }
            }),
        );
    }

    #[doc(hidden)]
    pub fn reveal_note(&self) {
        if let Some(path) = self.selected_note() {
            self.set_show_hidden(true);
            self.reveal_path(Some(&path));
            self.w().tree.grab_focus();
        }
    }

    /// Context menu of the notes list (Open, Rename, Delete, Reveal in file tree).
    #[doc(hidden)]
    pub fn build_notes_menu(&self) -> gtk::Menu {
        let path = self.selected_note();
        let menu = gtk::Menu::builder()
            .attach_widget(&self.w().notes.tree)
            .build();
        type Callback = Box<dyn Fn(&ExplorerPanel)>;
        let items: [(&str, Callback); 4] = [
            (
                "Open",
                Box::new(|p| {
                    if let Some(path) = p.selected_note() {
                        p.emit_file_activated(&path);
                    }
                }),
            ),
            ("Rename", Box::new(|p| p.rename_note())),
            (
                "Delete",
                Box::new(|p| {
                    if let Some(path) = p.selected_note() {
                        p.delete_dialog(Some(&path));
                    }
                }),
            ),
            ("Reveal in file tree", Box::new(|p| p.reveal_note())),
        ];
        for (title, callback) in items {
            let item = gtk::MenuItem::with_label(title);
            item.set_sensitive(path.is_some());
            if title == "Delete" {
                item.style_context().add_class("destructive-action");
            }
            let weak = self.downgrade();
            item.connect_activate(move |_| {
                if let Some(panel) = weak.upgrade() {
                    callback(&panel);
                }
            });
            menu.append(&item);
        }
        menu.show_all();
        menu.connect_selection_done(|menu| {
            // SAFETY: the menu is dismissed and referenced only by GTK's attach list.
            unsafe { menu.destroy() }
        });
        menu
    }

    pub(super) fn notes_menu(&self, event: Option<&gdk::EventButton>) -> bool {
        let menu = self.build_notes_menu();
        match event {
            Some(event) => menu.popup_at_pointer(Some(event)),
            None => menu.popup_at_widget(
                &self.w().notes.tree,
                gdk::Gravity::Center,
                gdk::Gravity::NorthWest,
                None,
            ),
        }
        true
    }

    fn notes_button_press(&self, tree: &gtk::TreeView, event: &gdk::EventButton) -> bool {
        if event.button() != 3 {
            return false;
        }
        let (x, y) = event.position();
        match tree
            .path_at_pos(x as i32, y as i32)
            .and_then(|(path, ..)| path)
        {
            Some(path) => tree.set_cursor(&path, None::<&gtk::TreeViewColumn>, false),
            None => tree.selection().unselect_all(),
        }
        tree.grab_focus();
        self.notes_menu(Some(event))
    }

    pub(super) fn notes_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::keys::constants as key;
        let value = event.keyval();
        if value == key::Return || value == key::KP_Enter {
            if let Some(path) = self.selected_note() {
                self.emit_file_activated(&path);
            }
        } else if value == key::F2 {
            self.rename_note();
        } else if value == key::Delete || value == key::KP_Delete {
            if let Some(path) = self.selected_note() {
                self.delete_dialog(Some(&path));
            }
        } else if value == key::F5 {
            self.refresh();
        } else {
            return false;
        }
        true
    }

    /// Create a note, list and select it, and open it in the editor.
    pub fn create_note(&self, name: &str, done: Option<OpCallback>) {
        let root = self.root();
        let name = name.to_owned();
        let weak = self.downgrade();
        self.run_op(
            move || notes::create_note(&root, &name),
            move |result| {
                let Some(panel) = weak.upgrade() else { return };
                if let Ok(path) = &result {
                    let imp = panel.imp();
                    *imp.notes_pending_select.borrow_mut() = Some(path.clone());
                    panel.show_notes_section(true);
                    panel.refresh_notes();
                    if imp.show_hidden.get() {
                        panel.rebuild(panel.expanded_paths(), panel.selected_path());
                    }
                    panel.refresh_git();
                    if imp.open_after_create.get() {
                        panel.emit_file_activated(path);
                    }
                }
                if let Some(done) = done {
                    done(result);
                }
            },
        );
    }

    pub fn new_note_dialog(&self) {
        let weak = self.downgrade();
        self.name_dialog(
            "New Note",
            "",
            Rc::new(move |name, done| {
                let Some(panel) = weak.upgrade() else { return };
                panel.create_note(&name, Some(Box::new(move |r| done(r.map(|_| ())))));
            }),
        );
    }

    // ---- observation helpers for the host and tests --------------------------------------------

    #[doc(hidden)]
    pub fn notes_widgets(&self) -> NotesHandles {
        let n = &self.w().notes;
        NotesHandles {
            section: n.section.clone(),
            collapse: n.collapse.clone(),
            new_button: n.new_button.clone(),
            empty_new: n.empty_new.clone(),
            store: n.store.clone(),
            tree: n.tree.clone(),
            stack: n.stack.clone(),
            empty: n.empty.clone(),
            count: n.count.clone(),
            body: n.body.clone(),
            scroll: n.scroll.clone(),
            cell: n.cell.clone(),
            marker: n.marker.clone(),
            edited_cell: n.edited_cell.clone(),
            split: self.w().notes_split.clone(),
            button: self.w().notes_button.clone(),
        }
    }

    #[doc(hidden)]
    pub fn notes_edited(&self, path: &Path) -> Option<String> {
        self.imp().notes_edited.borrow().get(path).cloned()
    }

    /// Stop watching the notes folders (monitors are re-created by the next refresh).
    #[doc(hidden)]
    pub fn drop_notes_monitors(&self) {
        for (_, monitor) in self.imp().notes_monitors.take() {
            monitor.cancel();
        }
    }

    #[doc(hidden)]
    pub fn active_file(&self) -> Option<PathBuf> {
        self.imp().active_file.borrow().clone()
    }
}

/// Widgets of the Notes section, for hosts that style or inspect them.
#[doc(hidden)]
#[derive(Clone)]
pub struct NotesHandles {
    pub section: gtk::Box,
    pub collapse: gtk::ToggleButton,
    pub new_button: gtk::Button,
    pub empty_new: gtk::Button,
    pub store: gtk::ListStore,
    pub tree: gtk::TreeView,
    pub stack: gtk::Stack,
    pub empty: gtk::Box,
    pub count: gtk::Label,
    pub body: gtk::Revealer,
    pub scroll: gtk::ScrolledWindow,
    pub cell: gtk::CellRendererText,
    pub marker: gtk::CellRendererText,
    pub edited_cell: gtk::CellRendererText,
    pub split: gtk::Paned,
    pub button: gtk::ToggleButton,
}
