use crate::MainWindow;
use gtk::prelude::*;
use impro_core::{settings, themes};
use impro_settings_ui::SettingsDialog;
use serde_json::{Map, Value};

/// Read the startup snapshot once, including the opaque graph preferences.
pub(crate) fn load_values() -> settings::Values {
    startup_values(
        settings::load(),
        crate::win::system_apps_use_light_theme(),
        crate::win::system_client_area_animations(),
    )
}

/// Saved settings over the defaults. On Windows a theme or reduce-motion choice the user never
/// saved follows the system colour mode and animation setting instead; nothing is written back,
/// so the system keeps deciding until the user picks a value. Off Windows both inputs are `None`.
fn startup_values(
    mut stored: settings::Values,
    apps_use_light: Option<bool>,
    client_area_animations: Option<bool>,
) -> settings::Values {
    let saved = |key: &str| stored.get(key).is_some_and(|value| !value.is_null());
    let system_theme =
        crate::win::policy::initial_theme(saved("theme"), apps_use_light, themes::themes())
            .map(str::to_owned);
    let system_reduce_motion =
        crate::win::policy::initial_reduce_motion(saved("reduce_motion"), client_area_animations);
    let mut values = settings::defaults().clone();
    for (key, value) in &mut values {
        if let Some(saved) = stored.remove(key) {
            *value = saved;
        }
    }
    if let Some(id) = system_theme {
        values.insert("theme".into(), id.into());
    }
    if let Some(flag) = system_reduce_motion {
        values.insert("reduce_motion".into(), flag.into());
    }
    values.insert(
        "graph".into(),
        stored.remove("graph").unwrap_or(Value::Null),
    );
    values
}

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
        // Use the same dispatch at startup and for live edits, once per key.
        for key in values.keys() {
            if !matches!(
                key.as_str(),
                "accent" | "theme" | "ui_font" | "editor_font" | "terminal_shell"
            ) && !key.starts_with("feature_")
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
            "terminal_shell" => self.configure_shell(false),
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
    pub(crate) fn configure_shell(&self, autostart: bool) {
        let autostart = autostart || self.startup_shell_pending.get();
        let shell = self.values()["terminal_shell"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if shell.is_empty() {
            self.startup_shell_pending.set(false);
            for panel in self.panels() {
                panel.set_shell("");
            }
            if autostart {
                self.terminal_bottom.new_terminal();
            }
            return;
        }
        // A saved custom shell must be validated before the first pane captures it.
        if autostart {
            self.startup_shell_pending.set(true);
        }
        let expected = shell.clone();
        let weak = self.weak();
        self.worker(
            move || impro_core::platform::is_executable(std::path::Path::new(&shell)),
            move |valid| {
                if let Some(this) = weak.upgrade().map(Self) {
                    if this.values()["terminal_shell"].as_str() != Some(&expected) {
                        return;
                    }
                    if autostart {
                        this.startup_shell_pending.set(false);
                        this.window.queue_draw();
                    }
                    for panel in this.panels() {
                        panel.set_shell(if valid { &expected } else { "" });
                    }
                    if !valid {
                        this.notify(
                            &format!(
                                "Shell {expected} is not executable; using the default shell."
                            ),
                            "info",
                        );
                    }
                    if autostart
                        && this.feature_enabled("terminal")
                        && this.terminal_bottom.pane_count() == 0
                    {
                        this.terminal_bottom.new_terminal();
                    }
                }
            },
        );
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
        crate::win::refresh_frame(&self.window);
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn stored(pairs: &[(&str, Value)]) -> settings::Values {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn without_system_hints_defaults_and_saved_values_are_unchanged() {
        let defaults = settings::defaults();
        let values = startup_values(stored(&[]), None, None);
        assert_eq!(values["theme"], defaults["theme"]);
        assert_eq!(values["reduce_motion"], defaults["reduce_motion"]);
        let saved = stored(&[("theme", json!("nord")), ("reduce_motion", json!(true))]);
        let values = startup_values(saved, None, None);
        assert_eq!(
            (&values["theme"], &values["reduce_motion"]),
            (&json!("nord"), &json!(true))
        );
        assert_eq!(values["graph"], Value::Null);
    }

    #[test]
    fn first_run_takes_the_light_system_theme_and_dark_keeps_the_default() {
        let light = startup_values(stored(&[]), Some(true), None);
        assert_eq!(light["theme"], json!("impro-light"));
        let dark = startup_values(stored(&[]), Some(false), None);
        assert_eq!(dark["theme"], settings::defaults()["theme"]);
        // A null entry is not a choice.
        let null = startup_values(stored(&[("theme", Value::Null)]), Some(true), None);
        assert_eq!(null["theme"], json!("impro-light"));
    }

    #[test]
    fn a_saved_theme_wins_over_the_system() {
        let values = startup_values(stored(&[("theme", json!("dracula"))]), Some(true), None);
        assert_eq!(values["theme"], json!("dracula"));
        let same = startup_values(stored(&[("theme", json!("impro-dark"))]), Some(true), None);
        assert_eq!(same["theme"], json!("impro-dark"));
    }

    #[test]
    fn disabled_system_animations_set_reduce_motion_only_without_a_saved_choice() {
        let values = startup_values(stored(&[]), None, Some(false));
        assert_eq!(values["reduce_motion"], json!(true));
        let enabled = startup_values(stored(&[]), None, Some(true));
        assert_eq!(
            enabled["reduce_motion"],
            settings::defaults()["reduce_motion"]
        );
        let explicit = startup_values(
            stored(&[("reduce_motion", json!(false))]),
            None,
            Some(false),
        );
        assert_eq!(explicit["reduce_motion"], json!(false));
    }

    #[test]
    fn every_system_choice_is_a_valid_setting() {
        let values = startup_values(stored(&[]), Some(true), Some(false));
        for key in ["theme", "reduce_motion"] {
            assert_eq!(settings::normalize(key, &values[key]).unwrap(), values[key]);
        }
    }
}
