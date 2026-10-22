//! Page builders and row factories of the settings window.
use crate::{
    dialog::{class, label, upgrade, Group, Inner, Page, Setter, PAGES},
    graph_prefs::{graph_defaults, graph_limits, graph_step},
};
use atk::prelude::*;
use gtk::prelude::*;
use impro_core::{
    identity,
    settings::{self, FONT_KEYS},
    themes,
};
use impro_ui::widgets::{
    HotkeyChip, RowOptions, SearchField, SettingCard, SettingRow, SwatchPicker, Toggle, WrapBox,
};
use serde_json::Value;
use std::{path::Path, rc::Rc};

const LICENSE_NOTE: &str = "impro runs on GTK 3, VTE and WebKitGTK (LGPL-2.1-or-later libraries) \
through the gtk-rs bindings. Icons come from your system icon theme and the interface uses your \
system fonts; no third-party fonts or icons are bundled.";

/// Name, description and owning page of the three font-size rows.
fn font_row(key: &str) -> (&'static str, &'static str) {
    match key {
        "terminal_font_size" => (
            "Terminal text size",
            "Text size of every terminal, in points.",
        ),
        "editor_font_size" => (
            "Editor text size",
            "Default size of file content, in points. Ctrl+0 returns to it.",
        ),
        _ => (
            "Explorer text size",
            "Size of file names in the explorer, in points.",
        ),
    }
}

pub(crate) struct Builder {
    pub body: gtk::Box,
    pub groups: Vec<Group>,
}

fn limits(key: &str) -> (f64, f64) {
    settings::limits().get(key).copied().unwrap_or((8.0, 24.0))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(path.as_os_str().as_bytes())
        // SAFETY: a valid NUL-terminated path; access only reads file metadata.
        .is_ok_and(|c| unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0)
}

/// `shutil.which` or an existing executable file.
fn shell_found(text: &str) -> bool {
    let direct = Path::new(text);
    if direct.is_file() && is_executable(direct) {
        return true;
    }
    if text.contains('/') {
        return false;
    }
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let candidate = dir.join(text);
            candidate.is_file() && is_executable(&candidate)
        })
    })
}

fn ui_font() -> String {
    gtk::Settings::default()
        .and_then(|s| s.gtk_font_name())
        .and_then(|name| pango::FontDescription::from_string(&name).family())
        .map_or_else(|| "Sans".to_owned(), |f| f.to_string())
}

impl Inner {
    pub(crate) fn ensure_page(self: &Rc<Self>, index: usize) {
        if self.pages.borrow()[index].is_some() {
            return;
        }
        let previous = self.building.replace(index);
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let mut builder = Builder {
            body: body.clone(),
            groups: Vec::new(),
        };
        match index {
            0 => self.build_appearance(&mut builder),
            1 => self.build_editor(&mut builder),
            2 => self.build_files(&mut builder),
            3 => self.build_notes(&mut builder),
            4 => self.build_terminal(&mut builder),
            5 => self.build_graph(&mut builder),
            6 => self.build_gitgraph(&mut builder),
            7 => self.build_hotkeys(&mut builder),
            _ => self.build_about(&mut builder),
        }
        self.building.set(previous);
        let (scroll, column) = scrolled_page(&body);
        self.stack.add_named(&scroll, PAGES[index].0);
        scroll.show_all();
        for group in &builder.groups {
            group.widget.set_no_show_all(true);
        }
        if index == 7 {
            let extras = self.extras.borrow();
            for widget in [
                extras.hotkey_box.clone().map(|w| w.upcast::<gtk::Widget>()),
                extras
                    .hotkeys_empty
                    .clone()
                    .map(|w| w.upcast::<gtk::Widget>()),
            ]
            .into_iter()
            .flatten()
            {
                widget.set_no_show_all(true);
            }
            if let Some(empty) = &extras.hotkeys_empty {
                empty.hide();
            }
        }
        self.pages.borrow_mut()[index] = Some(Page {
            scroll,
            body,
            column,
            groups: builder.groups,
        });
        if self.narrow.get() {
            let page = self.pages.borrow()[index].clone();
            for group in page.into_iter().flat_map(|p| p.groups) {
                for row in group.card.rows() {
                    row.set_narrow(true);
                }
            }
        }
        self.sync_page(index);
    }

    fn make_card(self: &Rc<Self>) -> SettingCard {
        let card = SettingCard::new();
        let weak = Rc::downgrade(self);
        card.connect_key_press_event(move |_, event| {
            if upgrade(&weak, |i| i.row_key(event)).unwrap_or(false) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        card
    }

    fn group(
        self: &Rc<Self>,
        b: &mut Builder,
        heading: Option<&str>,
        rows: &[SettingRow],
    ) -> SettingCard {
        let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
        if !b.groups.is_empty() {
            group.set_margin_top(24);
        }
        if let Some(heading) = heading {
            let title = class(&label(heading, 0.0), "heading");
            title.set_margin_start(16);
            title.set_margin_end(16);
            title.set_margin_bottom(16);
            group.pack_start(&title, false, false, 0);
        }
        let card = self.make_card();
        for row in rows {
            card.add_row(row);
        }
        group.pack_start(card.widget(), false, false, 0);
        b.body.pack_start(&group, false, false, 0);
        b.groups.push(Group {
            widget: group,
            card: card.clone(),
        });
        card
    }

    // Row factories ------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn row(
        self: &Rc<Self>,
        key: &str,
        name: &str,
        description: &str,
        control: &gtk::Widget,
        setter: Setter,
        aliases: &[&str],
        reset: bool,
        extra: Vec<gtk::Widget>,
    ) -> SettingRow {
        let reset_cb: Option<Box<dyn Fn()>> = reset.then(|| {
            let weak = Rc::downgrade(self);
            let key = key.to_owned();
            Box::new(move || {
                upgrade(&weak, |i| {
                    let default = settings::defaults()
                        .get(&key)
                        .cloned()
                        .unwrap_or(Value::Null);
                    i.changed(&key, default)
                });
            }) as Box<dyn Fn()>
        });
        let row = SettingRow::new(
            name,
            description,
            Some(control),
            RowOptions {
                key: Some(key),
                aliases,
                reset: reset_cb,
                extra,
            },
        );
        self.register(key, &row, control, Some(setter));
        row
    }

    fn register(&self, key: &str, row: &SettingRow, control: &gtk::Widget, setter: Option<Setter>) {
        let mut reg = self.reg.borrow_mut();
        reg.rows.insert(key.to_owned(), row.clone());
        reg.controls.insert(key.to_owned(), control.clone());
        if let Some(setter) = setter {
            reg.setters.insert(key.to_owned(), setter);
            let index = self.building.get();
            reg.keys[index].push(key.to_owned());
        }
    }

    fn toggle(
        self: &Rc<Self>,
        key: &str,
        name: &str,
        description: &str,
        aliases: &[&str],
    ) -> SettingRow {
        let toggle = Toggle::new(false, Some(name));
        let (weak, owned) = (Rc::downgrade(self), key.to_owned());
        toggle.connect_toggled(move |t| {
            upgrade(&weak, |i| i.changed(&owned, Value::Bool(t.is_active())));
        });
        let target = toggle.clone();
        let setter: Setter = Rc::new(move |v| target.set_active(v.as_bool().unwrap_or(false)));
        self.row(
            key,
            name,
            description,
            toggle.upcast_ref(),
            setter,
            aliases,
            true,
            vec![],
        )
    }

    fn dropdown(
        self: &Rc<Self>,
        key: &str,
        name: &str,
        description: &str,
        options: &[(&str, &str)],
        aliases: &[&str],
    ) -> SettingRow {
        let combo = gtk::ComboBoxText::new();
        for (value, text) in options {
            combo.append(Some(value), text);
        }
        let (weak, owned) = (Rc::downgrade(self), key.to_owned());
        combo.connect_changed(move |c| {
            if let Some(id) = c.active_id().filter(|id| !id.is_empty()) {
                upgrade(&weak, |i| i.changed(&owned, Value::from(id.as_str())));
            }
        });
        let target = combo.clone();
        let setter: Setter = Rc::new(move |v| {
            target.set_active_id(v.as_str());
        });
        self.row(
            key,
            name,
            description,
            combo.upcast_ref(),
            setter,
            aliases,
            true,
            vec![],
        )
    }

    fn slider(
        self: &Rc<Self>,
        key: &str,
        name: &str,
        description: &str,
        range: (f64, f64),
        aliases: &[&str],
        reset: bool,
    ) -> (gtk::Scale, SettingRow) {
        let (low, high) = range;
        let adjustment = gtk::Adjustment::new(low, low, high, 1.0, 1.0, 0.0);
        let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
        scale.set_digits(0);
        scale.set_round_digits(0);
        scale.set_draw_value(false);
        scale.set_size_request(120, -1);
        let value = class(&label(&format!("{}", low as i64), 1.0), "value");
        value.style_context().add_class("impro-setting-value");
        value.set_width_chars(format!("{}", high as i64).len() as i32);
        let shown = value.clone();
        scale.connect_value_changed(move |s| {
            shown.set_text(&format!("{}", s.value().round_ties_even() as i64))
        });
        self.reg
            .borrow_mut()
            .value_labels
            .insert(key.to_owned(), value.clone());
        let target = scale.clone();
        let setter: Setter = Rc::new(move |v| target.set_value(v.as_f64().unwrap_or(0.0)));
        let row = self.row(
            key,
            name,
            description,
            scale.upcast_ref(),
            setter,
            aliases,
            reset,
            vec![value.upcast()],
        );
        (scale, row)
    }

    fn font_button(
        self: &Rc<Self>,
        key: &str,
        name: &str,
        description: &str,
        fallback: fn() -> String,
        aliases: &[&str],
    ) -> SettingRow {
        let button = gtk::FontButton::new();
        button.set_level(gtk::FontChooserLevel::FAMILY);
        button.set_show_size(false);
        button.set_show_style(false);
        button.set_use_font(false);
        button.set_title(name);
        let (weak, owned) = (Rc::downgrade(self), key.to_owned());
        button.connect_font_set(move |b| {
            let text = match b.font_family() {
                Some(family) => family.name().to_string(),
                None => {
                    pango::FontDescription::from_string(b.font().as_deref().unwrap_or_default())
                        .family()
                        .map(|f| f.to_string())
                        .unwrap_or_default()
                }
            };
            upgrade(&weak, |i| i.changed(&owned, Value::from(text)));
        });
        let target = button.clone();
        let setter: Setter = Rc::new(move |v| {
            let text = v
                .as_str()
                .filter(|s| !s.is_empty())
                .map_or_else(fallback, str::to_owned);
            target.set_font(&text);
        });
        self.row(
            key,
            name,
            description,
            button.upcast_ref(),
            setter,
            aliases,
            true,
            vec![],
        )
    }

    fn font_size_row(self: &Rc<Self>, key: &str) -> SettingRow {
        let (name, description) = font_row(key);
        let (scale, row) = self.slider(
            key,
            name,
            description,
            limits(key),
            &["font text zoom"],
            true,
        );
        if let Some(accessible) = scale.accessible() {
            accessible.set_name(name);
        }
        let (weak, owned) = (Rc::downgrade(self), key.to_owned());
        scale.connect_value_changed(move |s| {
            upgrade(&weak, |i| i.size_changed(&owned, s));
        });
        self.reg.borrow_mut().scales.insert(key.to_owned(), scale);
        row
    }

    fn spin(
        self: &Rc<Self>,
        key: &str,
        name: &str,
        description: &str,
        step: f64,
        aliases: &[&str],
    ) -> SettingRow {
        let (low, high) = limits(key);
        let spin = gtk::SpinButton::with_range(low, high, step);
        spin.set_numeric(true);
        spin.set_width_chars(6);
        let (weak, owned) = (Rc::downgrade(self), key.to_owned());
        spin.connect_value_changed(move |s| {
            upgrade(&weak, |i| i.changed(&owned, Value::from(s.value_as_int())));
        });
        let target = spin.clone();
        let setter: Setter = Rc::new(move |v| target.set_value(v.as_f64().unwrap_or(0.0)));
        self.row(
            key,
            name,
            description,
            spin.upcast_ref(),
            setter,
            aliases,
            true,
            vec![],
        )
    }

    fn feature(self: &Rc<Self>, b: &mut Builder, key: &str, name: &str, description: &str) {
        self.reg
            .borrow_mut()
            .features
            .push((self.building.get(), key.to_owned()));
        let row = self.toggle(
            key,
            &format!("Enable {name}"),
            description,
            &["feature enabled disable"],
        );
        self.group(b, None, &[row]);
    }

    fn graph_row(
        self: &Rc<Self>,
        key: &str,
        name: &str,
        description: &str,
        aliases: &[&str],
    ) -> SettingRow {
        let row_key = format!("graph.{key}");
        let default = graph_defaults().get(key).cloned().unwrap_or(Value::Null);
        let weak = Rc::downgrade(self);
        let owned = key.to_owned();
        let (control, setter): (gtk::Widget, Setter) = if default.is_boolean() {
            let toggle = Toggle::new(false, Some(name));
            toggle.connect_toggled(move |t| {
                upgrade(&weak, |i| {
                    i.graph_changed(&owned, Value::Bool(t.is_active()))
                });
            });
            let target = toggle.clone();
            (
                toggle.upcast_ref::<gtk::Widget>().clone(),
                Rc::new(move |v: &Value| target.set_active(v.as_bool().unwrap_or(false))),
            )
        } else {
            let (low, high) = graph_limits(key).unwrap_or((0.0, 1.0));
            let spin = gtk::SpinButton::with_range(low, high, graph_step(key).unwrap_or(0.1));
            spin.set_digits(2);
            spin.set_numeric(true);
            spin.set_width_chars(5);
            spin.connect_value_changed(move |s| {
                upgrade(&weak, |i| i.graph_changed(&owned, Value::from(s.value())));
            });
            let target = spin.clone();
            (
                spin.upcast(),
                Rc::new(move |v: &Value| target.set_value(v.as_f64().unwrap_or(0.0))),
            )
        };
        let (weak, owned) = (Rc::downgrade(self), key.to_owned());
        let reset: Box<dyn Fn()> = Box::new(move || {
            upgrade(&weak, |i| i.graph_changed(&owned, default.clone()));
        });
        let row = SettingRow::new(
            name,
            description,
            Some(&control),
            RowOptions {
                key: Some(&row_key),
                aliases,
                reset: Some(reset),
                extra: vec![],
            },
        );
        self.register(&row_key, &row, &control, Some(setter));
        row
    }

    // Pages --------------------------------------------------------------------

    fn build_appearance(self: &Rc<Self>, b: &mut Builder) {
        let combo = gtk::ComboBoxText::new();
        for theme in themes::themes() {
            combo.append(Some(&theme.id), &theme.name);
        }
        let weak = Rc::downgrade(self);
        combo.connect_changed(move |c| {
            upgrade(&weak, |i| i.theme_changed(c));
        });
        let preview = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        preview.set_valign(gtk::Align::Center);
        if let Some(accessible) = preview.accessible() {
            accessible.set_name("Theme preview");
        }
        let mut providers = Vec::new();
        for _ in 0..6 {
            let swatch = class(&gtk::Box::new(gtk::Orientation::Horizontal, 0), "swatch");
            swatch.set_size_request(10, 16);
            let provider = gtk::CssProvider::new();
            swatch
                .style_context()
                .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
            providers.push(provider);
            preview.pack_start(&swatch, false, false, 0);
        }
        {
            let mut extras = self.extras.borrow_mut();
            extras.theme_combo = Some(combo.clone());
            extras.theme_preview = Some(preview.clone());
            extras.preview_providers = providers;
        }
        let theme_row = SettingRow::new(
            "Color theme",
            "Colours for the whole app; applies as you choose.",
            Some(combo.upcast_ref()),
            RowOptions {
                key: Some("theme"),
                aliases: &["color colour scheme dark light mode appearance base"],
                reset: None,
                extra: vec![preview.upcast()],
            },
        );
        self.register("theme", &theme_row, combo.upcast_ref(), None);

        let picker = SwatchPicker::default();
        let weak = Rc::downgrade(self);
        picker.connect_value_changed(move |_, value| {
            upgrade(&weak, |i| {
                i.changed("accent", value.map_or(Value::Null, Value::from))
            });
        });
        self.extras.borrow_mut().accent_picker = Some(picker.clone());
        let target = picker.clone();
        let setter: Setter = Rc::new(move |v| target.set_value(v.as_str(), false));
        let accent = self.row(
            "accent",
            "Accent color",
            "Colour of toggles, sliders and highlights. Restore to use the theme's own.",
            picker.upcast_ref(),
            setter,
            &["highlight brand primary colour"],
            true,
            vec![],
        );
        let motion = self.toggle(
            "reduce_motion",
            "Reduce motion",
            "Turn off interface animations such as sliding toggles.",
            &["animation accessibility"],
        );
        self.group(b, None, &[theme_row, accent, motion]);
        let font = self.font_button(
            "ui_font",
            "Interface font",
            "Font for menus, sidebars and dialogs. Restore to use the system font.",
            ui_font,
            &["typeface family"],
        );
        self.group(b, Some("Font"), &[font]);
    }

    fn build_editor(self: &Rc<Self>, b: &mut Builder) {
        let font = self.font_button(
            "editor_font",
            "Editor font",
            "Font for file content. Restore to use the default monospace font.",
            || "Monospace".to_owned(),
            &["typeface family monospace"],
        );
        let size = self.font_size_row("editor_font_size");
        self.group(b, None, &[size, font]);
        let numbers = self.toggle(
            "line_numbers",
            "Line numbers",
            "Show line numbers in the editor gutter.",
            &["gutter"],
        );
        let highlight = self.toggle(
            "highlight_line",
            "Highlight current line",
            "Tint the line that contains the cursor.",
            &["active cursor line"],
        );
        self.group(b, Some("Display"), &[numbers, highlight]);
        let (scale, indent) = self.slider(
            "indent_width",
            "Indent width",
            "Number of columns per indentation level.",
            limits("indent_width"),
            &["tab size spaces"],
            false,
        );
        let weak = Rc::downgrade(self);
        scale.connect_value_changed(move |s| {
            upgrade(&weak, |i| {
                i.changed(
                    "indent_width",
                    Value::from(s.value().round_ties_even() as i64),
                )
            });
        });
        let tabs = self.toggle(
            "indent_tabs",
            "Indent using tabs",
            "Insert tab characters instead of spaces when indenting.",
            &["spaces"],
        );
        let pair = self.toggle(
            "auto_pair",
            "Auto-pair brackets",
            "Insert the closing bracket or quote when typing an opening one.",
            &["brackets quotes close"],
        );
        self.group(b, Some("Behavior"), &[tabs, indent, pair]);
    }

    fn build_files(self: &Rc<Self>, b: &mut Builder) {
        self.feature(
            b,
            "feature_explorer",
            "file explorer",
            "Browse and manage project files.",
        );
        let rows = [
            self.toggle(
                "explorer_git_colors",
                "Git status colours",
                "Colour file names by their Git status.",
                &["version control colors"],
            ),
            self.toggle(
                "explorer_folders_first",
                "Folders first",
                "List folders before files.",
                &["directory sort order"],
            ),
            self.toggle(
                "explorer_compact",
                "Compact rows",
                "Use less space between file names.",
                &["density spacing"],
            ),
            self.font_size_row("explorer_font_size"),
        ];
        self.group(b, None, &rows);
        let hidden = self.toggle(
            "show_hidden",
            "Show hidden files",
            "Show files and folders whose names start with a dot in the explorer.",
            &["dotfiles"],
        );
        self.group(b, None, &[hidden]);
        let confirm = self.toggle(
            "confirm_delete",
            "Confirm before deleting",
            "Ask before deleting files and folders from the explorer.",
            &["delete"],
        );
        let mode = self.dropdown(
            "delete_mode",
            "Deleted files",
            "What happens to files deleted from the explorer.",
            &[
                ("trash", "Move to system trash"),
                ("permanent", "Delete permanently"),
            ],
            &["trash bin remove"],
        );
        self.group(b, Some("Trash"), &[confirm, mode]);
        let mut aliases = vec!["excluded exclude search"];
        aliases.extend(self.ignored_folders.iter().map(String::as_str));
        let ignored = SettingRow::new(
            "Ignored folders",
            "Folders skipped by Search in Files.",
            None,
            RowOptions {
                aliases: &aliases,
                ..Default::default()
            },
        );
        let chips = WrapBox::new(0, 0);
        chips.set_margin_top(10);
        for name in &self.ignored_folders {
            chips.add(HotkeyChip::new(name).widget());
        }
        if let Some(info) = ignored
            .desc_label
            .as_ref()
            .and_then(|l| l.parent())
            .and_then(|p| p.downcast::<gtk::Box>().ok())
        {
            info.pack_start(&chips, false, false, 0);
        }
        self.reg
            .borrow_mut()
            .rows
            .insert("ignored_folders".into(), ignored.clone());
        self.group(b, Some("Search"), &[ignored]);
    }

    fn notes_path_text(&self) -> String {
        match self.project_root.borrow().as_deref() {
            Some(root) => impro_core::notes::notes_dir(root).display().to_string(),
            None => format!("<project>/{}", identity::NOTES_DIR),
        }
    }

    pub(crate) fn sync_notes_folder(&self) {
        let row = self.extras.borrow().notes_folder.clone();
        if let Some(label) = row.and_then(|r| r.desc_label.clone()) {
            label.set_text(&self.notes_path_text());
        }
    }

    fn build_notes(self: &Rc<Self>, b: &mut Builder) {
        self.feature(
            b,
            "feature_notes",
            "notes",
            "Write and organise project notes.",
        );
        let rows = [
            self.toggle(
                "md_live_preview",
                "Preview project Markdown by default",
                "Default for project Markdown files. Notes always use editable live preview.",
                &["markdown source reading view"],
            ),
            self.toggle(
                "md_readable_width",
                "Readable line length",
                "Keep Markdown lines at a comfortable reading width.",
                &["markdown readable width"],
            ),
            self.toggle(
                "notes_slash_commands",
                "Slash commands",
                "Type / for a command menu in notes.",
                &["slash insert menu"],
            ),
            self.toggle(
                "notes_toolbar",
                "Insert toolbar",
                "Show the formatting and insert toolbar in notes.",
                &["formatting buttons"],
            ),
            self.toggle(
                "notes_selection_toolbar",
                "Selection toolbar",
                "Show floating formatting buttons when text is selected in notes.",
                &["selection formatting popover"],
            ),
            self.dropdown(
                "notes_sort",
                "Sort notes by",
                "Order notes in the explorer.",
                &[("name", "Name"), ("modified", "Last modified")],
                &["order recent"],
            ),
            self.toggle(
                "notes_open_after_create",
                "Open a note after creating it",
                "Start editing each newly created note.",
                &["new note focus"],
            ),
        ];
        self.group(b, None, &rows);
        let row = SettingRow::new(
            "Notes folder",
            &self.notes_path_text(),
            None,
            RowOptions {
                aliases: &["notes directory path location"],
                ..Default::default()
            },
        );
        if let Some(label) = &row.desc_label {
            label.set_selectable(true);
        }
        self.reg
            .borrow_mut()
            .rows
            .insert("notes_folder".into(), row.clone());
        self.extras.borrow_mut().notes_folder = Some(row.clone());
        self.group(b, Some("Location"), &[row]);
    }

    fn build_terminal(self: &Rc<Self>, b: &mut Builder) {
        self.feature(
            b,
            "feature_terminal",
            "terminal",
            "Run shells in project terminal docks.",
        );
        let entry = gtk::Entry::new();
        entry.set_width_chars(18);
        entry.set_placeholder_text(Some("Login shell"));
        let hint = gtk::Label::new(None);
        hint.set_xalign(0.0);
        hint.set_line_wrap(true);
        hint.style_context().add_class("impro-setting-desc");
        fn show_hint(hint: &gtk::Label, text: &str) {
            hint.set_text(if text.is_empty() {
                ""
            } else if shell_found(text) {
                "Shell will be checked when a terminal opens."
            } else {
                "Shell was not found; check its name or executable path."
            });
        }
        let (weak, shown) = (Rc::downgrade(self), hint.clone());
        entry.connect_changed(move |e| {
            let text = e.text().to_string();
            show_hint(&shown, &text);
            upgrade(&weak, |i| i.changed("terminal_shell", Value::from(text)));
        });
        let (target, shown) = (entry.clone(), hint.clone());
        let setter: Setter = Rc::new(move |v| {
            let text = v.as_str().unwrap_or_default();
            target.set_text(text);
            show_hint(&shown, text);
        });
        let shell = self.row(
            "terminal_shell",
            "Shell",
            "Executable name or path. Leave empty to use your login shell.",
            entry.upcast_ref(),
            setter,
            &["bash zsh command login executable"],
            true,
            vec![],
        );
        if let Some(info) = shell
            .desc_label
            .as_ref()
            .and_then(|l| l.parent())
            .and_then(|p| p.downcast::<gtk::Box>().ok())
        {
            info.pack_start(&hint, false, false, 0);
        }
        self.extras.borrow_mut().shell_hint = Some(hint);
        let rows = [
            self.spin(
                "terminal_max_panes",
                "Maximum terminals per dock",
                "Counts every pane of a dock: terminals, graph and Git graph.",
                1.0,
                &["limit split panes"],
            ),
            shell,
            self.font_size_row("terminal_font_size"),
            self.dropdown(
                "terminal_cursor",
                "Cursor shape",
                "Shape of the cursor in terminals.",
                &[
                    ("block", "Block"),
                    ("ibeam", "I-beam"),
                    ("underline", "Underline"),
                ],
                &["caret"],
            ),
            self.toggle(
                "terminal_blink",
                "Blinking cursor",
                "Blink the cursor in terminals.",
                &["caret blink"],
            ),
            self.spin(
                "terminal_scrollback",
                "Scrollback lines",
                "Lines each terminal keeps for scrolling back (1,000 to 100,000).",
                1000.0,
                &["history buffer"],
            ),
            self.toggle(
                "terminal_copy_on_select",
                "Copy on select",
                "Copy selected terminal text to the clipboard.",
                &["selection clipboard"],
            ),
            self.toggle(
                "terminal_bell",
                "Audible bell",
                "Play a sound when a terminal rings its bell.",
                &["sound beep"],
            ),
            self.toggle(
                "terminal_autostart",
                "Open bottom terminal at start-up",
                "Show the bottom terminal dock when impro starts.",
                &["dock startup launch"],
            ),
        ];
        self.group(b, None, &rows);
    }

    fn build_graph(self: &Rc<Self>, b: &mut Builder) {
        self.feature(
            b,
            "feature_graph",
            "graph view",
            "Explore project files and their references.",
        );
        let rows = [
            self.dropdown(
                "graph_default_type",
                "Default graph type",
                "Scope of newly opened graphs.",
                &[("global", "Global"), ("local", "Local"), ("note", "Note")],
                &["scope neighbourhood"],
            ),
            self.graph_row(
                "showTags",
                "Show tags",
                "Include tag markers.",
                &["tag markers"],
            ),
            self.graph_row(
                "showAttachments",
                "Show attachments",
                "Include referenced assets.",
                &["images assets"],
            ),
            self.graph_row(
                "showOrphans",
                "Show orphans",
                "Include unlinked files.",
                &["unlinked files"],
            ),
            self.graph_row(
                "showArrow",
                "Show arrows",
                "Show the direction of references.",
                &["direction"],
            ),
            self.graph_row(
                "nodeSizeMultiplier",
                "Node size",
                "Size of file markers.",
                &["marker size"],
            ),
            self.graph_row(
                "lineSizeMultiplier",
                "Link thickness",
                "Width of connections.",
                &["connection width"],
            ),
            self.graph_row(
                "textFadeMultiplier",
                "Label fade",
                "Detail level of file labels.",
                &["label detail text"],
            ),
            self.graph_row(
                "reduceMotion",
                "Reduce graph motion",
                "Skip graph animation and easing.",
                &["animate on open animation"],
            ),
        ];
        self.group(b, None, &rows);
    }

    fn build_gitgraph(self: &Rc<Self>, b: &mut Builder) {
        self.feature(
            b,
            "feature_gitgraph",
            "Git graph",
            "Browse branches and commit history.",
        );
        let rows = [
            self.dropdown(
                "gitgraph_orientation",
                "Default orientation",
                "Direction of commit history.",
                &[("horizontal", "Horizontal"), ("vertical", "Vertical")],
                &["layout direction"],
            ),
            self.spin(
                "gitgraph_max_commits",
                "Maximum commits to load",
                "Limit the amount of commit history loaded (100 to 10,000).",
                100.0,
                &["history limit"],
            ),
            self.toggle(
                "gitgraph_show_remotes",
                "Show remote branches",
                "Include remote branch references.",
                &["remote refs"],
            ),
            self.toggle(
                "gitgraph_show_tags",
                "Show tags",
                "Include Git tags.",
                &["release refs"],
            ),
            self.dropdown(
                "gitgraph_time_format",
                "Time format in tooltips",
                "How commit timestamps appear.",
                &[("relative", "Relative"), ("absolute", "Absolute")],
                &["date timestamp"],
            ),
        ];
        self.group(b, None, &rows);
    }

    fn build_hotkeys(self: &Rc<Self>, b: &mut Builder) {
        let filter = SearchField::new("Filter hotkeys…");
        filter.set_margin_bottom(16);
        let weak = Rc::downgrade(self);
        filter.connect_changed({
            let weak = weak.clone();
            move |_| {
                upgrade(&weak, |i| i.filter_hotkeys());
            }
        });
        filter.connect_key_press_event(move |_, event| {
            if upgrade(&weak, |i| i.search_key(event, false)).unwrap_or(false) {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        let hotkey_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        hotkey_box.pack_start(filter.widget(), false, false, 0);
        b.body.pack_start(&hotkey_box, false, false, 0);
        let card = self.group(b, None, &[]);
        let empty = class(&label("No hotkeys match.", 0.5), "empty");
        empty.set_margin_top(24);
        empty.set_no_show_all(true);
        b.body.pack_start(&empty, false, false, 0);
        {
            let mut extras = self.extras.borrow_mut();
            extras.hotkey_filter = Some(filter);
            extras.hotkey_box = Some(hotkey_box);
            extras.hotkeys_empty = Some(empty);
        }
        self.add_hotkey_rows(&card);
    }

    fn add_hotkey_rows(&self, card: &SettingCard) {
        let narrow = self.narrow.get();
        let hotkeys = self.hotkeys.borrow().clone();
        for (name, accel) in hotkeys {
            let chips = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            for part in accel.split(" / ").map(str::trim).filter(|p| !p.is_empty()) {
                chips.pack_start(HotkeyChip::new(part).widget(), false, false, 0);
            }
            let row = SettingRow::new(
                &name,
                "",
                Some(chips.upcast_ref()),
                RowOptions {
                    aliases: &[&accel, "hotkey shortcut"],
                    ..Default::default()
                },
            );
            row.set_narrow(narrow);
            row.show_all();
            card.add_row(&row);
        }
    }

    /// Replace the shortcut rows: `SettingCard` cannot drop rows, so the card is swapped.
    pub(crate) fn fill_hotkeys(self: &Rc<Self>) {
        let page = self.pages.borrow()[7].clone();
        let Some(group) = page.and_then(|p| p.groups.last().cloned()) else {
            return;
        };
        let card = self.make_card();
        self.add_hotkey_rows(&card);
        group.widget.remove(group.card.widget());
        group.widget.pack_start(card.widget(), false, false, 0);
        card.widget().show_all();
        if let Some(Some(page)) = self.pages.borrow_mut().get_mut(7) {
            if let Some(last) = page.groups.last_mut() {
                last.card = card;
            }
        }
    }

    fn build_about(self: &Rc<Self>, b: &mut Builder) {
        let path = settings::settings_path().display().to_string();
        let version = class(&label(identity::VERSION, 0.5), "value");
        version.style_context().add_class("impro-setting-value");
        version.set_selectable(true);
        let copy = gtk::Button::with_label("Copy path");
        copy.connect_clicked(|button| {
            let path = settings::settings_path().display().to_string();
            gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&path);
            button.set_label("Copied");
            let button = button.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(1500), move || {
                button.set_label("Copy path");
            });
        });
        let file_row = SettingRow::new(
            "Settings file",
            &path,
            Some(copy.upcast_ref()),
            RowOptions {
                aliases: &["config configuration json location"],
                ..Default::default()
            },
        );
        if let Some(label) = &file_row.desc_label {
            label.set_selectable(true);
            self.extras.borrow_mut().settings_path_label = Some(label.clone());
        }
        self.extras.borrow_mut().copy_button = Some(copy);
        self.reg
            .borrow_mut()
            .rows
            .insert("settings_file".into(), file_row.clone());
        let rows = [
            SettingRow::new(
                "impro version",
                "The version of impro that is running.",
                Some(version.upcast_ref()),
                RowOptions {
                    aliases: &["about release"],
                    ..Default::default()
                },
            ),
            file_row,
            SettingRow::new(
                "Licenses",
                LICENSE_NOTE,
                None,
                RowOptions {
                    aliases: &["license licence legal credits"],
                    ..Default::default()
                },
            ),
        ];
        self.group(b, None, &rows);
    }
}

fn scrolled_page(body: &gtk::Box) -> (gtk::ScrolledWindow, impro_ui::widgets::CenteredColumn) {
    let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let column = impro_ui::widgets::CenteredColumn::default();
    column.set_focus_vadjustment(&scroll.vadjustment());
    column.add(body);
    scroll.add(&column);
    (scroll, column)
}

#[allow(dead_code)]
const _: () = {
    let _ = FONT_KEYS;
};
