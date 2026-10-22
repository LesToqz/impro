use crate::MainWindow;
use gtk::prelude::*;
use impro_core::{settings, themes};
use impro_settings_ui::SettingsDialog;
use serde_json::{Map, Value};

impl MainWindow {
    pub fn settings_dialog(&self) -> Option<SettingsDialog> {
        self.settings_dialog.borrow().clone()
    }
    pub fn open_settings(&self, page: Option<&str>) {
        if self.settings_dialog().is_none() {
            let values = self.values();
            let dialog = SettingsDialog::with_values(
                Some(self.window.upcast_ref()),
                impro_settings_ui::DialogOptions {
                    project_root: Some(self.root()),
                    ..Default::default()
                },
                &values,
                values.get("graph").unwrap_or(&Value::Null),
            );
            let weak = self.weak();
            dialog.connect_theme_selected(move |id| {
                if let Some(this) = weak.upgrade().map(Self) {
                    this.set_theme(id, true);
                }
            });
            let weak = self.weak();
            dialog.connect_font_size_changed(move |key, size| {
                if let Some(this) = weak.upgrade().map(Self) {
                    let _ = this.set_setting(key, Value::from(size), true);
                }
            });
            let weak = self.weak();
            dialog.connect_setting_changed(move |key, value| {
                if let Some(this) = weak.upgrade().map(Self) {
                    let _ = this.set_setting(key, value.clone(), false);
                }
            });
            *self.settings_dialog.borrow_mut() = Some(dialog);
        }
        if let Some(dialog) = self.settings_dialog() {
            dialog.set_values(&self.values());
            dialog.set_hotkeys(&self.hotkeys());
            dialog.set_project_root(Some(&self.root()));
            dialog.present(page);
        }
    }
    pub(crate) fn apply_initial_preferences(&self) {
        let values = self.values();
        self.editor.apply_settings(&values);
        for panel in self.panels() {
            panel.apply_settings(&values);
        }
        self.explorer
            .set_font_size(values["explorer_font_size"].as_i64().unwrap_or(10) as f64);
        for key in values.keys() {
            if !matches!(key.as_str(), "accent" | "theme" | "ui_font" | "editor_font")
                && !key.starts_with("feature_")
            {
                self.apply_setting(key);
            }
        }
        for kind in ["explorer", "terminal", "graph", "gitgraph", "notes"] {
            self.sync_features(kind);
        }
        self.apply_widget_theme();
    }
    /// Apply a validated value immediately. Disk writes use the settings write-behind worker.
    pub fn set_setting(&self, key: &str, value: Value, persist: bool) -> Result<(), String> {
        let value = if key == "graph" {
            Value::Object(impro_settings_ui::normalize_graph(&value))
        } else {
            settings::normalize(key, &value).map_err(|e| e.to_string())?
        };
        self.values.borrow_mut().insert(key.into(), value.clone());
        self.apply_setting(key);
        if persist {
            impro_settings_ui::save_value_async(key, value);
        }
        if let Some(dialog) = self.settings_dialog() {
            dialog.set_values(&self.values());
        }
        if key == "terminal_autostart" {
            self.notify(
                "The start-up terminal setting applies from the next start.",
                "info",
            );
        }
        Ok(())
    }
    fn apply_setting(&self, key: &str) {
        let values = self.values();
        let value = &values[key];
        let flag = value.as_bool().unwrap_or(false);
        let number = value.as_i64().unwrap_or(0) as i32;
        let text = value.as_str().unwrap_or_default();
        match key {
            "theme" | "accent" | "ui_font" | "editor_font" => self.refresh_style(),
            "reduce_motion" => {
                impro_ui::motion::set_reduce_motion(flag);
                for view in self.graph_views() {
                    view.set_reduce_motion(flag);
                }
                for view in self.gitgraph_views() {
                    view.set_reduce_motion(flag);
                }
                for view in self.commit_views() {
                    view.set_reduce_motion(flag);
                }
            }
            "editor_font_size" => {
                self.editor.set_font_size(number);
                self.editor.set_default_font_size(number);
            }
            "explorer_font_size" => self.explorer.set_font_size(number as f64),
            "terminal_font_size" => {
                for panel in self.panels() {
                    panel.set_font_size(number);
                }
            }
            "line_numbers" => self.editor.set_line_numbers(flag),
            "highlight_line" => self.editor.set_highlight_line(flag),
            "auto_pair" => self.editor.set_auto_pair(flag),
            "indent_tabs" | "indent_width" => {
                let _ = self.editor.set_indent(
                    values["indent_tabs"] == true,
                    values["indent_width"].as_i64().unwrap_or(4) as i32,
                );
            }
            "md_live_preview" => self.editor.set_md_live_preview(flag),
            "md_readable_width" => self.editor.set_md_readable_width(flag),
            "notes_slash_commands" => self.editor.set_slash_commands(flag),
            "notes_toolbar" | "notes_selection_toolbar" => self.sync_note_toolbars(),
            "notes_sort" => {
                let _ = self.explorer.set_notes_sort(text);
            }
            "notes_open_after_create" => self.explorer.set_open_after_create(flag),
            "show_hidden" => {
                self.explorer.set_show_hidden(flag);
                self.switcher_epoch.set(self.switcher_epoch.get() + 1);
            }
            "confirm_delete" => self.explorer.set_confirm_delete(flag),
            "delete_mode" => self.explorer.set_delete_mode(if text == "trash" {
                impro_explorer::DeleteMode::Trash
            } else {
                impro_explorer::DeleteMode::Delete
            }),
            "explorer_git_colors" => self.explorer.set_git_colors(flag),
            "explorer_folders_first" => self.explorer.set_folders_first(flag),
            "explorer_compact" => self.explorer.set_compact(flag),
            "terminal_max_panes" => {
                for panel in self.panels() {
                    panel.set_max_panes(number as usize);
                }
            }
            "terminal_shell" => {
                // Executability checks stay off GTK's thread, including live changes.
                let shell = text.to_owned();
                let expected = shell.clone();
                let weak = self.weak();
                self.worker(move || {
                    use std::os::unix::fs::PermissionsExt;
                    shell.is_empty() || std::fs::metadata(&shell).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                }, move |valid| {
                    if let Some(this) = weak.upgrade().map(Self) {
                        if this.values()["terminal_shell"].as_str() != Some(&expected) { return; }
                        for panel in this.panels() { panel.set_shell(if valid { &expected } else { "" }); }
                        if !valid { this.notify(&format!("Shell {expected} is not executable; using the default shell."), "info"); }
                    }
                });
            }
            "terminal_copy_on_select" => {
                for panel in self.panels() {
                    panel.set_copy_on_select(flag);
                }
            }
            "terminal_bell" => {
                for panel in self.panels() {
                    panel.set_bell(flag);
                }
            }
            "terminal_cursor" => {
                for panel in self.panels() {
                    let _ = panel.set_cursor_shape(text);
                }
            }
            "terminal_blink" => {
                for panel in self.panels() {
                    panel.set_cursor_blink(flag);
                }
            }
            "terminal_scrollback" => {
                for panel in self.panels() {
                    panel.set_scrollback(value.as_i64().unwrap_or(10000));
                }
            }
            "graph" => {
                for view in self.graph_views() {
                    view.apply_prefs(value);
                }
            }
            key if key.starts_with("feature_") => self.sync_features(&key[8..]),
            key if key.starts_with("gitgraph_") => {
                for view in self.gitgraph_views() {
                    view.set_settings(self.git_settings());
                }
            }
            _ => {} // Startup policy and graph default affect future panes only.
        }
    }
    pub fn theme_id(&self) -> String {
        self.values()["theme"]
            .as_str()
            .unwrap_or("impro-dark")
            .into()
    }
    pub fn set_theme(&self, id: &str, persist: bool) {
        let theme = themes::get_theme(Some(id));
        self.values
            .borrow_mut()
            .insert("theme".into(), theme.id.clone().into());
        self.refresh_style();
        if let Some(dialog) = self.settings_dialog() {
            dialog.set_theme_id(Some(&theme.id));
        }
        if persist {
            impro_settings_ui::save_value_async("theme", theme.id.clone().into());
        }
    }
    pub fn next_theme(&self) {
        let themes = themes::themes();
        let index = themes
            .iter()
            .position(|t| t.id == self.theme_id())
            .unwrap_or(0);
        self.set_theme(&themes[(index + 1) % themes.len()].id, true);
    }
    pub(crate) fn refresh_style(&self) {
        let values = self.values();
        let theme = themes::get_theme(values["theme"].as_str());
        if let Err(error) = impro_ui::apply_theme(
            theme,
            values["accent"].as_str(),
            values["ui_font"].as_str(),
            values["editor_font"].as_str(),
        ) {
            self.notify(&error.to_string(), "error");
            return;
        }
        self.apply_widget_theme();
    }
    pub(crate) fn apply_widget_theme(&self) {
        let values = self.values();
        let theme = themes::get_theme(values["theme"].as_str());
        self.editor.apply_theme(theme);
        self.editor.set_font_family(values["editor_font"].as_str());
        for panel in self.panels() {
            panel.apply_theme(theme);
        }
        let effective =
            impro_ui::current_theme().unwrap_or_else(|| std::rc::Rc::new(theme.clone()));
        let ui = self.git_theme_ui();
        let accent = &effective.ui["accent"];
        for view in self.graph_views() {
            view.set_theme(&effective.ui, accent);
            view.set_font(values["ui_font"].as_str());
        }
        for view in self.gitgraph_views() {
            view.set_theme(&ui, accent);
        }
        for view in self.commit_views() {
            view.set_theme(theme, &ui, accent, self.editor.font_size());
        }
    }
    pub(crate) fn git_theme_ui(&self) -> themes::UiColors {
        let values = self.values();
        let mut ui = impro_ui::current_theme()
            .map(|t| t.ui.clone())
            .unwrap_or_else(|| themes::get_theme(values["theme"].as_str()).ui.clone());
        for key in ["ui_font", "editor_font"] {
            ui.insert(key.into(), values[key].as_str().unwrap_or_default().into());
        }
        ui
    }
    pub(crate) fn git_settings(&self) -> impro_gitviews::GraphSettings {
        impro_gitviews::GraphSettings::from_value(&Value::Object(self.values()))
    }
    pub(crate) fn graph_preferences_changed(&self, value: &Value) {
        let normalized = Value::Object(impro_settings_ui::normalize_graph(value));
        if self.values().get("graph") == Some(&normalized) {
            return;
        }
        self.values
            .borrow_mut()
            .insert("graph".into(), normalized.clone());
        for view in self.graph_views() {
            if view.prefs().to_value() != normalized {
                view.apply_prefs(&normalized);
            }
        }
        if let Some(dialog) = self.settings_dialog() {
            let mut values = Map::new();
            values.insert("graph".into(), normalized.clone());
            dialog.set_values(&values);
        }
        impro_settings_ui::save_value_async("graph", normalized);
    }
}
