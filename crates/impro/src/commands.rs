use crate::MainWindow;
use gtk::prelude::*;
use impro_chrome::{Command, CommandPalette, QuickSwitcher};
use std::path::PathBuf;

/// Human-readable accelerator text shared by the palette and Hotkeys page.
pub fn accel_text(accelerator: &str) -> String {
    let (key, modifiers) = gtk::accelerator_parse(accelerator);
    let mut parts = Vec::new();
    for (mask, label) in [
        (gdk::ModifierType::CONTROL_MASK, "Ctrl"),
        (gdk::ModifierType::SHIFT_MASK, "Shift"),
        (gdk::ModifierType::MOD1_MASK, "Alt"),
    ] {
        if modifiers.contains(mask) {
            parts.push(label.to_string());
        }
    }
    parts.push(
        gtk::accelerator_get_label(key, gdk::ModifierType::empty())
            .unwrap_or_default()
            .to_string(),
    );
    parts.join("+")
}

impl MainWindow {
    pub(crate) fn build_commands(&self) {
        let accelerators = gtk::AccelGroup::new();
        self.window.add_accel_group(&accelerators);
        let file = gtk::Menu::new();
        let view = gtk::Menu::new();
        for (label, menu) in [("File", &file), ("View", &view)] {
            let item = gtk::MenuItem::with_label(label);
            item.set_submenu(Some(menu));
            self.menu_bar.append(&item);
        }
        type Action = fn(&MainWindow);
        type MenuEntry<'a> = (&'a gtk::Menu, &'a str, &'a str, Action, &'a [&'a str], bool);
        let entries: &[MenuEntry<'_>] = &[
            (
                &file,
                "Quick Switcher…",
                "File: Quick Switcher",
                Self::open_quick_switcher,
                &["<Control>p", "<Control>o"],
                true,
            ),
            (
                &file,
                "Open Folder…",
                "File: Open Folder",
                Self::open_folder,
                &[],
                false,
            ),
            (
                &file,
                "Save",
                "File: Save",
                |w| w.editor.save_current(|_| {}),
                &["<Control>s"],
                false,
            ),
            (
                &file,
                "Save All",
                "File: Save All",
                |w| w.editor.save_all(|_| {}),
                &["<Control><Shift>s"],
                false,
            ),
            (
                &file,
                "Quit",
                "File: Quit",
                Self::request_quit,
                &["<Control>q"],
                false,
            ),
            (
                &view,
                "Command Palette…",
                "View: Command Palette",
                Self::open_command_palette,
                &["<Control><Shift>p"],
                false,
            ),
            (
                &view,
                "Terminal Below",
                "View: Terminal Below",
                Self::toggle_terminal_bottom,
                &["<Control>grave"],
                false,
            ),
            (
                &view,
                "Split Terminal",
                "View: Split Terminal",
                Self::split_terminal,
                &["<Control><Shift>5"],
                false,
            ),
            (
                &view,
                "Toggle Terminal Fullscreen",
                "View: Toggle Terminal Fullscreen",
                Self::toggle_terminal_fullscreen,
                &["<Control><Shift>F11"],
                false,
            ),
            (
                &view,
                "Toggle Explorer Sidebar",
                "View: Toggle Explorer Sidebar",
                Self::toggle_explorer,
                &["<Control>b"],
                false,
            ),
            (
                &view,
                "Search in Files",
                "View: Search in Files",
                Self::search_in_files,
                &["<Control><Shift>f"],
                false,
            ),
            (
                &view,
                "Graph view",
                "Graph view: Open graph view",
                |w| {
                    w.open_graph();
                },
                &["<Control><Alt>g"],
                false,
            ),
            (
                &view,
                "Local graph",
                "Graph view: Open local graph",
                |w| {
                    w.open_local_graph();
                },
                &["<Control><Shift><Alt>g"],
                false,
            ),
            (
                &view,
                "Git graph",
                "Git graph: Open git graph",
                |w| {
                    w.open_gitgraph();
                },
                &["<Control><Alt>h"],
                false,
            ),
        ];
        for (menu, label, command, action, keys, hint) in entries {
            let item = gtk::MenuItem::with_label(label);
            let callback = self.callback(*action);
            item.connect_activate(move |_| callback());
            if let Some(key) = keys.first() {
                let (key, modifiers) = gtk::accelerator_parse(key);
                if *hint {
                    if let Some(label) = item
                        .child()
                        .and_then(|w| w.downcast::<gtk::AccelLabel>().ok())
                    {
                        label.set_accel(key, modifiers);
                    }
                } else {
                    item.add_accelerator(
                        "activate",
                        &accelerators,
                        key,
                        modifiers,
                        gtk::AccelFlags::VISIBLE,
                    );
                }
            }
            self.commands.borrow_mut().push(Command::new(
                *command,
                self.callback(*action),
                keys.iter().map(|key| accel_text(key)).collect(),
            ));
            menu.append(&item);
        }
        for (label, action, keys) in [
            (
                "View: Terminal on Right",
                Self::toggle_terminal_right as Action,
                vec!["Ctrl+Alt+`"],
            ),
            (
                "Settings: Open settings",
                |w: &Self| w.open_settings(None),
                vec!["Ctrl+,"],
            ),
            (
                "Settings: Show hotkeys",
                |w: &Self| w.open_settings(Some("hotkeys")),
                vec!["F1"],
            ),
            ("Graph view: Rebuild graph", Self::rebuild_graph, vec![]),
            ("Graph view: Reset camera", Self::reset_graph_camera, vec![]),
            ("Git graph: Refresh", Self::refresh_gitgraph, vec![]),
            ("Git graph: Jump to HEAD", Self::focus_git_head, vec![]),
            (
                "Git graph: Close commit tabs",
                |w: &Self| {
                    if w.require_feature("gitgraph") {
                        w.close_commit_tabs();
                    }
                },
                vec![],
            ),
            ("Theme: Next theme", Self::next_theme, vec!["Ctrl+Alt+T"]),
            (
                "Markdown: Toggle source mode",
                Self::toggle_markdown_view,
                vec!["Ctrl+Shift+M"],
            ),
            (
                "Editor: Zoom in",
                |w: &Self| w.editor.zoom(1),
                vec!["Ctrl+="],
            ),
            (
                "Editor: Zoom out",
                |w: &Self| w.editor.zoom(-1),
                vec!["Ctrl+-"],
            ),
            (
                "Editor: Reset zoom",
                |w: &Self| w.editor.set_font_size(w.editor.default_font_size()),
                vec!["Ctrl+0"],
            ),
            ("Notes: New note", Self::new_note, vec![]),
            ("Notes: Show notes", Self::show_notes, vec![]),
        ] {
            self.commands.borrow_mut().push(Command::new(
                label,
                self.callback(action),
                keys.into_iter().map(str::to_owned).collect(),
            ));
        }
        for command in impro_core::notes::NOTE_COMMANDS {
            let id = command.id;
            self.commands.borrow_mut().push(Command::new(
                format!("Notes: {}", command.label),
                self.callback(move |w| w.apply_note_command(id)),
                vec![],
            ));
        }
        for (key, action) in [
            ("<Control><Alt>grave", Self::toggle_terminal_right as Action),
            ("<Control>comma", |w: &Self| w.open_settings(None)),
            ("<Control><Alt>t", Self::next_theme),
        ] {
            let (key, modifiers) = gtk::accelerator_parse(key);
            let callback = self.callback(action);
            accelerators.connect_accel_group(
                key,
                modifiers,
                gtk::AccelFlags::VISIBLE,
                move |_, _, pressed, state| {
                    if key == pressed && modifiers == state {
                        callback();
                        true
                    } else {
                        false
                    }
                },
            );
        }
    }
    pub fn commands(&self) -> Vec<Command> {
        self.commands
            .borrow()
            .iter()
            .filter(|c| !c.label.starts_with("Notes:") || self.feature_enabled("notes"))
            .cloned()
            .collect()
    }
    pub fn hotkeys(&self) -> Vec<(String, String)> {
        impro_settings_ui::merge_hotkeys(
            &self
                .commands
                .borrow()
                .iter()
                .map(|c| (c.label.clone(), c.hotkey.clone()))
                .collect::<Vec<_>>(),
        )
    }
    pub fn command_palette(&self) -> Option<CommandPalette> {
        self.palette.borrow().clone()
    }
    pub fn quick_switcher(&self) -> Option<QuickSwitcher> {
        self.switcher.borrow().clone()
    }
    fn ensure_command_palette(&self) {
        if self.command_palette().is_none() {
            let weak = self.weak();
            let palette = CommandPalette::new(move || {
                weak.upgrade()
                    .map(Self)
                    .map(|w| w.commands())
                    .unwrap_or_default()
            });
            *self.palette.borrow_mut() = Some(palette);
        }
    }
    pub fn open_command_palette(&self) {
        self.ensure_command_palette();
        if let Some(palette) = self.command_palette() {
            self.overlay.show_prompt(&palette);
        }
    }

    /// Warm one hidden palette in bounded idle slices after the complete first paint.
    pub(crate) fn stage_command_palette_cache(&self) {
        let weak = self.weak();
        let mut palette: Option<CommandPalette> = None;
        let mut realise = std::collections::VecDeque::<gtk::Widget>::new();
        let mut measure = Vec::<gtk::Widget>::new();
        let source = glib::idle_add_local_full(glib::Priority::LOW, move || {
            let Some(this) = weak.upgrade().map(Self) else {
                return glib::ControlFlow::Break;
            };
            if this.closed.get() {
                this.palette_cache_idle.borrow_mut().take();
                return glib::ControlFlow::Break;
            }
            let started = std::time::Instant::now();
            if palette.is_none() {
                this.ensure_command_palette();
                if let Some(prompt) = this.command_palette() {
                    if !prompt.is_open() {
                        prompt.set_no_show_all(true);
                        prompt.set_child_visible(false);
                        if prompt.parent().is_none() {
                            this.overlay.add_overlay(prompt.widget());
                        }
                        prompt.show();
                        realise.push_back(prompt.widget().clone().upcast());
                    }
                    palette = Some(prompt);
                }
            } else if let Some(prompt) = &palette {
                // A user can open the palette while warmup is pending. Let that
                // interaction own its geometry and focus, then retain the cache.
                if prompt.is_open() {
                    this.overlay.cache_prompt(prompt);
                    this.palette_cache_idle.borrow_mut().take();
                    return glib::ControlFlow::Break;
                }
                // One native widget operation per dispatch; never accumulate
                // a font fallback lookup with other widgets in the same slice.
                if let Some(widget) = realise.pop_front() {
                    let operation = std::time::Instant::now();
                    widget.realize();
                    if std::env::var_os("IMPRO_QA_TIMING").is_some() {
                        println!(
                            "palette-realize-{}-ms={:.3}",
                            widget.type_().name(),
                            operation.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                    if let Some(container) = widget.downcast_ref::<gtk::Container>() {
                        realise.extend(container.children());
                    }
                    measure.push(widget);
                } else if let Some(widget) = measure.pop() {
                    // Children first: the final parent requests then hit GTK's cache.
                    let operation = std::time::Instant::now();
                    widget.preferred_width();
                    widget.preferred_height_for_width(600);
                    if std::env::var_os("IMPRO_QA_TIMING").is_some() {
                        println!(
                            "palette-measure-{}-ms={:.3}",
                            widget.type_().name(),
                            operation.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                } else {
                    this.overlay.cache_prompt(prompt);
                    this.palette_cache_idle.borrow_mut().take();
                    if std::env::var_os("IMPRO_QA_TIMING").is_some() {
                        println!(
                            "palette-cache-final-ms={:.3}",
                            started.elapsed().as_secs_f64() * 1000.0
                        );
                    }
                    return glib::ControlFlow::Break;
                }
            }
            if std::env::var_os("IMPRO_QA_TIMING").is_some() {
                println!(
                    "palette-cache-slice-ms={:.3}",
                    started.elapsed().as_secs_f64() * 1000.0
                );
            }
            glib::ControlFlow::Continue
        });
        *self.palette_cache_idle.borrow_mut() = Some(source);
    }
    pub fn open_quick_switcher(&self) {
        if self.quick_switcher().is_none() {
            let switcher = QuickSwitcher::new(self.root());
            let weak = self.weak();
            switcher.connect_chosen(move |item, _| {
                if let (Some(this), Some(path)) = (weak.upgrade().map(Self), item.data::<PathBuf>())
                {
                    this.editor.open_file(path);
                }
            });
            *self.switcher.borrow_mut() = Some(switcher);
        }
        self.restore_terminal_fullscreen();
        if let Some(switcher) = self.quick_switcher() {
            switcher.set_root(self.root());
            self.overlay.show_prompt(&switcher);
            let root = self.root();
            let show_hidden = self.values()["show_hidden"] == true;
            let epoch = self.switcher_epoch.get() + 1;
            self.switcher_epoch.set(epoch);
            let weak = self.weak();
            self.worker(
                move || impro_explorer::search::list_project_files(&root, show_hidden, 20000),
                move |files| {
                    if let Some(this) = weak.upgrade().map(Self) {
                        if this.switcher_epoch.get() != epoch {
                            return;
                        }
                        let mut recent = this.recent.borrow().clone();
                        for tab in this.editor.tabs() {
                            let path = tab.path();
                            if !recent.contains(&path) {
                                recent.push(path);
                            }
                        }
                        if let Some(switcher) = this.quick_switcher() {
                            switcher.set_files(files, recent);
                            switcher.refresh();
                        }
                    }
                },
            );
        }
    }
    /// Window shortcut policy, called before GTK propagates to focused children.
    pub fn handle_key(&self, event: &gdk::EventKey) -> bool {
        use gdk::{keys::constants as k, ModifierType as M};
        let state = event.state() & gtk::accelerator_get_default_mod_mask();
        let key = event.keyval();
        let lower = key.to_lower();
        let ctrl = M::CONTROL_MASK;
        let shift = ctrl | M::SHIFT_MASK;
        let alt = ctrl | M::MOD1_MASK;
        if state == shift && lower == k::p {
            self.open_command_palette();
            return true;
        }
        if state == ctrl && key == k::comma {
            self.open_settings(None);
            return true;
        }
        if state == alt && lower == k::h {
            self.open_gitgraph();
            return true;
        }
        if (state == alt || state == alt | M::SHIFT_MASK) && lower == k::g {
            if state.contains(M::SHIFT_MASK) {
                self.open_local_graph();
            } else {
                self.open_graph();
            }
            return true;
        }
        if !self.terminal_focused() {
            if state == ctrl && (lower == k::p || lower == k::o) {
                self.open_quick_switcher();
                return true;
            }
            if state.is_empty() && key == k::F1 {
                self.open_settings(Some("hotkeys"));
                return true;
            }
        }
        if state == shift && key == k::F11 {
            self.toggle_terminal_fullscreen();
            return true;
        }
        if state == shift && lower == k::f {
            self.search_in_files();
            return true;
        }
        if state == shift && (key == k::_5 || key == k::percent) {
            self.split_terminal();
            return true;
        }
        if self.terminal_focused()
            && ((state == ctrl && [k::s, k::p, k::o].contains(&lower))
                || (state.is_empty() && key == k::F1))
        {
            self.window.propagate_key_event(event);
            return true;
        }
        if state == ctrl && lower == k::s {
            self.editor.save_current(|_| {});
            return true;
        }
        if state == ctrl && [k::w, k::slash].contains(&lower) && self.focus_in(&self.editor) {
            return self.window.propagate_key_event(event);
        }
        if state == shift && lower == k::m && self.focus_in(&self.editor) {
            self.toggle_markdown_view();
            return true;
        }
        if state.is_empty() && key == k::Escape && self.overlay.prompt().is_some() {
            self.overlay.hide_prompt();
            return true;
        }
        false
    }
}
