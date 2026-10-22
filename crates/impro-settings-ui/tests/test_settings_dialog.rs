//! Settings window contract (port of test_settings_dialog.py); run with GDK_BACKEND=x11 under xvfb.
mod common;
use common::*;
use gdk::keys::constants as k;
use gtk::prelude::*;
use impro_core::{settings, themes};
use impro_settings_ui::{
    flush, graph_defaults, graph_limits, graph_values_equal, SettingsDialog, DEFAULT_HOTKEYS, PAGES,
};
use impro_ui::widgets::SettingRow;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

fn visible_hotkeys(dialog: &SettingsDialog) -> Vec<SettingRow> {
    let query = dialog.hotkey_filter().text().to_string();
    dialog
        .hotkey_card()
        .rows()
        .into_iter()
        .filter(|r| r.get_visible() && r.matches(&query))
        .collect()
}

fn hex_rgb(color: &str) -> [u8; 3] {
    [1, 3, 5].map(|i| u8::from_str_radix(&color[i..i + 2], 16).unwrap())
}

fn font_limits(key: &str) -> (f64, f64) {
    let (low, high) = settings::font_size_limits()[key];
    (low as f64, high as f64)
}

fn default_size(key: &str) -> i32 {
    settings::default_font_sizes()[key]
}

fn default_of(key: &str) -> Value {
    settings::defaults()[key].clone()
}

fn all_graph_keys() -> Vec<&'static str> {
    vec![
        "showTags",
        "showAttachments",
        "showOrphans",
        "showArrow",
        "nodeSizeMultiplier",
        "lineSizeMultiplier",
        "textFadeMultiplier",
        "reduceMotion",
    ]
}

fn window_pixel(window: &gdk::Window, x: i32, y: i32) -> [u8; 3] {
    use glib::translate::*;
    // SAFETY: the window is realized; the call returns a new reference or NULL.
    let pixbuf: Option<gdk_pixbuf::Pixbuf> = unsafe {
        from_glib_full(gdk::ffi::gdk_pixbuf_get_from_window(
            window.to_glib_none().0,
            x,
            y,
            1,
            1,
        ))
    };
    let pixels = pixbuf.expect("pixel").read_pixel_bytes();
    [pixels[0], pixels[1], pixels[2]]
}

// Existing API -------------------------------------------------------------

#[test]
fn test_window_defaults_and_theme_order() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        assert_eq!(dialog.title().as_deref(), Some("Settings"));
        assert_eq!(dialog.default_size(), (900, 700));
        assert_eq!(dialog.size_request(), (600, 400));
        assert_eq!(dialog.theme_id(), settings::get_theme_id());
        let defaults: BTreeMap<String, i32> = settings::default_font_sizes()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        assert_eq!(dialog.sizes(), defaults);
        let model = dialog.theme_combo().model().unwrap();
        let mut ids = Vec::new();
        model.foreach(|m, _, iter| {
            ids.push(m.value(iter, 1).get::<String>().unwrap());
            false
        });
        let expected: Vec<_> = themes::themes().iter().map(|t| t.id.clone()).collect();
        assert_eq!(ids, expected);
        dialog.build_all_pages();
        for key in settings::new_keys()
            .into_iter()
            .chain(settings::FONT_KEYS)
            .chain(all_graph_keys().into_iter().map(|_| ""))
        {
            if let Some(button) = dialog.reset_button(key) {
                assert!(!button.is_sensitive(), "{key}");
            }
        }
        for name in all_graph_keys() {
            let button = dialog.reset_button(&format!("graph.{name}")).unwrap();
            assert!(!button.is_sensitive(), "{name}");
        }
        let mut shown = dialog.values();
        let mut want = settings::defaults().clone();
        want.insert("graph".into(), Value::Object(graph_defaults()));
        // The stored theme is the default one in a clean directory.
        shown.insert("theme".into(), want["theme"].clone());
        assert_eq!(shown, want);
        assert_eq!(
            dialog.stack().transition_type(),
            gtk::StackTransitionType::None
        );
        let nav = dialog.nav_list();
        let rows = nav.rows();
        let order: Vec<String> = nav
            .children()
            .iter()
            .map(|child| {
                rows.values()
                    .find(|r| r.upcast_ref::<gtk::Widget>() == child)
                    .unwrap()
                    .page_id
                    .clone()
            })
            .collect();
        assert_eq!(
            order,
            PAGES.iter().map(|p| p.0.to_owned()).collect::<Vec<_>>()
        );
    });
}

#[test]
fn test_theme_selection_live_and_silent_setter() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        let events: Rc<RefCell<Vec<String>>> = Rc::default();
        let sink = events.clone();
        dialog.connect_theme_selected(move |id| sink.borrow_mut().push(id.to_owned()));
        dialog.theme_combo().set_active_id(Some("nord"));
        assert_eq!(*events.borrow(), ["nord"]);
        dialog.theme_combo().set_active_id(Some("dracula"));
        assert_eq!(*events.borrow(), ["nord", "dracula"]);
        dialog.set_theme_id(Some("light-plus"));
        assert_eq!(dialog.theme_id(), "light-plus");
        dialog.set_theme_id(Some("unknown"));
        assert_eq!(dialog.theme_id(), themes::DEFAULT_THEME_ID);
        dialog.set_values(&obj(json!({"theme": "monokai"})));
        assert_eq!(dialog.theme_id(), "monokai");
        assert_eq!(*events.borrow(), ["nord", "dracula"]);
        assert!(dialog
            .theme_preview()
            .tooltip_text()
            .unwrap()
            .contains("Monokai"));
    });
}

#[test]
fn test_font_sliders_reset_limits_and_silent_setter() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        let events: Rc<RefCell<Vec<(String, i32)>>> = Rc::default();
        let sink = events.clone();
        dialog.connect_font_size_changed(move |k, v| sink.borrow_mut().push((k.to_owned(), v)));
        let changes = fx.changes();
        let last_event =
            |events: &Rc<RefCell<Vec<(String, i32)>>>| events.borrow().last().cloned().unwrap();
        for key in settings::FONT_KEYS {
            let scale = dialog.size_scale(key).unwrap();
            let (low, high) = font_limits(key);
            let adjustment = scale.adjustment();
            assert_eq!((adjustment.lower(), adjustment.upper()), (low, high));
            scale.set_value(18.0);
            assert_eq!(last_event(&events), (key.to_owned(), 18));
            assert_eq!(dialog.value_label(key).unwrap().text(), "18");
            assert!(dialog.reset_button(key).unwrap().is_sensitive());
            dialog.reset_button(key).unwrap().clicked();
            assert_eq!(last_event(&events), (key.to_owned(), default_size(key)));
            assert!(!dialog.reset_button(key).unwrap().is_sensitive());
            scale.set_value(1000.0);
            assert_eq!(last_event(&events), (key.to_owned(), high as i32));
            scale.set_value(-1000.0);
            assert_eq!(last_event(&events), (key.to_owned(), low as i32));
        }
        let before = events.borrow().clone();
        dialog.set_sizes(
            &settings::FONT_KEYS
                .iter()
                .map(|k| (k.to_string(), 13))
                .collect(),
        );
        assert_eq!(*events.borrow(), before);
        let want: BTreeMap<String, i32> = settings::FONT_KEYS
            .iter()
            .map(|k| (k.to_string(), 13))
            .collect();
        assert_eq!(dialog.sizes(), want);
        let mut snapshot = dialog.sizes();
        snapshot.insert("terminal_font_size".into(), 25);
        assert_eq!(dialog.sizes()["terminal_font_size"], 13);
        flush();
        assert!(settings::load().is_empty()); // The owner persists font sizes and the theme.
        assert!(changes.borrow().is_empty()); // Existing keys keep their own signals only.
    });
}

#[test]
fn test_initial_persistence_partial_arguments_and_parent() {
    run(|| {
        let fx = Fixture::new();
        settings::set_theme_id(Some("nord"));
        settings::set_font_size("terminal_font_size", &json!(17)).unwrap();
        settings::set_values(&obj(
            json!({"indent_width": 2, "terminal_cursor": "underline", "accent": "#e5484d"}),
        ))
        .unwrap();
        let dialog = SettingsDialog::new(Some(&fx.dialog), None, None);
        assert_eq!(dialog.transient_for().as_ref(), Some(&*fx.dialog));
        assert!(dialog.must_destroy_with_parent());
        assert!(!dialog.is_modal());
        assert_eq!(dialog.theme_id(), "nord");
        assert_eq!(dialog.sizes()["terminal_font_size"], 17);
        assert_eq!(dialog.values()["indent_width"], 2);
        let combo = dialog.control("terminal_cursor").unwrap();
        assert_eq!(
            combo
                .downcast_ref::<gtk::ComboBoxText>()
                .unwrap()
                .active_id()
                .as_deref(),
            Some("underline")
        );
        assert_eq!(dialog.accent_picker().value().as_deref(), Some("#e5484d"));
        let sizes: BTreeMap<String, i32> = [("editor_font_size".to_owned(), 19)].into();
        let other = SettingsDialog::new(None, Some("dracula"), Some(&sizes));
        let mut want: BTreeMap<String, i32> = settings::default_font_sizes()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        want.insert("editor_font_size".into(), 19);
        assert_eq!(other.sizes(), want);
        assert_eq!(other.theme_id(), "dracula");
        unsafe {
            dialog.destroy();
            other.destroy();
        }
    });
}

#[test]
fn test_close_escape_window_control_and_reuse() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        fx.show();
        key(&**dialog, k::Escape);
        assert!(!dialog.is_visible());
        fx.show();
        dialog.response();
        assert!(!dialog.is_visible());
        fx.show();
        let handled = dialog
            .emit_by_name::<bool>("delete-event", &[&gdk::Event::new(gdk::EventType::Delete)]);
        assert!(handled);
        assert!(!dialog.is_visible());
        fx.show(); // Reusable: still working after hiding.
        let changes = fx.changes();
        click(&dialog.control("line_numbers").unwrap());
        assert_eq!(
            *changes.borrow(),
            [("line_numbers".to_owned(), json!(false))]
        );
        assert!(dialog.is_visible());
    });
}

// New keys -----------------------------------------------------------------

#[test]
fn test_every_new_key_persists_clamps_and_emits() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        let changes = fx.changes();
        let mut expected = BTreeMap::new();
        for key in settings::new_keys() {
            let control = dialog.control(key).unwrap();
            let want: Value = if key == "accent" {
                dialog.accent_picker().buttons()["#22a55b"].clicked();
                json!("#22a55b")
            } else if control.is::<gtk::ToggleButton>() {
                click(&control);
                json!(!default_of(key).as_bool().unwrap())
            } else if let Some(combo) = control.downcast_ref::<gtk::ComboBoxText>() {
                let value = settings::choices()[key]
                    .iter()
                    .find(|v| Some(v.as_str()) != default_of(key).as_str())
                    .unwrap()
                    .clone();
                combo.set_active_id(Some(&value));
                json!(value)
            } else if control.is::<gtk::Scale>() || control.is::<gtk::SpinButton>() {
                let (_, high) = settings::limits()[key];
                if let Some(scale) = control.downcast_ref::<gtk::Scale>() {
                    scale.set_value(high + 1000.0);
                } else {
                    control
                        .downcast_ref::<gtk::SpinButton>()
                        .unwrap()
                        .set_value(high + 1000.0);
                }
                json!(high as i64)
            } else if let Some(entry) = control.downcast_ref::<gtk::Entry>() {
                entry.set_text("/bin/bash");
                json!("/bin/bash")
            } else if let Some(button) = control.downcast_ref::<gtk::FontButton>() {
                button.set_font("Serif 12");
                button.emit_by_name::<()>("font-set", &[]);
                json!("Serif")
            } else {
                panic!("untested control for {key}");
            };
            expected.insert(key, want.clone());
            assert_eq!(last(&changes), (key.to_owned(), want), "{key}");
        }
        let order: Vec<String> = changes.borrow().iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(order, settings::new_keys());
        flush();
        let stored = settings::get_values();
        for (key, want) in &expected {
            assert_eq!(&stored[*key], want, "{key}");
            assert_eq!(&dialog.values()[*key], want, "{key}");
        }
        // Reset extra buttons restore defaults (accent back to the theme's own).
        for key in ["accent", "ui_font", "editor_font"] {
            assert!(dialog.reset_button(key).unwrap().is_sensitive(), "{key}");
            dialog.reset_button(key).unwrap().clicked();
            assert_eq!(last(&changes), (key.to_owned(), default_of(key)));
            flush();
            assert_eq!(settings::get_value(key).unwrap(), default_of(key));
            assert!(!dialog.reset_button(key).unwrap().is_sensitive(), "{key}");
        }
        assert_eq!(dialog.accent_picker().value(), None);
        let indent = dialog.control("indent_width").unwrap();
        let indent = indent.downcast_ref::<gtk::Scale>().unwrap();
        indent.set_value(-5.0);
        assert_eq!(last(&changes), ("indent_width".to_owned(), json!(2)));
        let count = changes.borrow().len();
        indent.set_value(2.0); // No change, no emission.
        assert_eq!(changes.borrow().len(), count);
    });
}

#[test]
fn test_feature_page_order_sensitivity_search_and_keyboard() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        let titles: Vec<_> = PAGES.iter().map(|p| p.1).collect();
        assert_eq!(
            titles,
            [
                "Appearance",
                "Editor",
                "File explorer",
                "Notes",
                "Terminal",
                "Graph view",
                "Git graph",
                "Hotkeys",
                "About"
            ]
        );
        fx.show();
        let changes = fx.changes();
        for (page, feature) in [
            ("files", "feature_explorer"),
            ("notes", "feature_notes"),
            ("terminal", "feature_terminal"),
            ("graph", "feature_graph"),
            ("gitgraph", "feature_gitgraph"),
        ] {
            let rows: Vec<SettingRow> = dialog
                .groups(page)
                .iter()
                .flat_map(|(_, card)| card.rows())
                .collect();
            assert_eq!(rows[0].key.as_deref(), Some(feature));
            dialog.select_page(page);
            assert!(dialog.focus_content());
            assert_eq!(dialog.focused_widget().as_ref(), Some(rows[0].upcast_ref()));
            key(&**dialog, k::Down);
            assert_eq!(dialog.focused_widget().as_ref(), Some(rows[1].upcast_ref()));
            click(&dialog.control(feature).unwrap());
            assert_eq!(last(&changes), (feature.to_owned(), json!(false)));
            assert!(rows[1..].iter().all(|r| !r.is_sensitive()));
            assert!(rows[1..].iter().all(|r| r.opacity() < 0.5));
            assert!(rows[0].is_sensitive());
            dialog.search(&rows[1].name);
            assert!(rows[1].get_visible());
            assert!(dialog.visible_rows().iter().all(|r| r.name != rows[1].name)); // Insensitive rows are skipped.
            dialog.search("");
            dialog.set_values(&obj(json!({ feature: true })));
            assert!(rows.iter().all(|r| r.is_sensitive()));
        }
        for (query, key) in [
            ("slash", "notes_slash_commands"),
            ("selection", "notes_selection_toolbar"),
            ("login executable", "terminal_shell"),
            ("connection width", "graph.lineSizeMultiplier"),
            ("remote refs", "gitgraph_show_remotes"),
            ("density", "explorer_compact"),
        ] {
            dialog.search(query);
            let row = dialog.row(key).unwrap();
            assert!(row.get_visible());
            assert!(
                dialog
                    .visible_rows()
                    .iter()
                    .any(|r| r.key.as_deref() == Some(key)),
                "{query}"
            );
        }
        dialog.search("");
    });
}

#[test]
fn test_all_controls_silent_initialization_and_graph_roundtrip() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        let changes = fx.changes();
        let mut values = settings::Values::new();
        for key in settings::new_keys() {
            let default = default_of(key);
            let value = if default.is_boolean() {
                json!(!default.as_bool().unwrap())
            } else if let Some(options) = settings::choices().get(key) {
                json!(options.last().unwrap())
            } else if let Some(&(_, high)) = settings::limits().get(key) {
                json!(high as i64)
            } else {
                default
            };
            values.insert(key.to_owned(), value);
        }
        dialog.set_values(&values);
        assert!(changes.borrow().is_empty());
        let shown = dialog.values();
        for (key, value) in &values {
            assert_eq!(&shown[key], value, "{key}");
            let control = dialog.control(key).unwrap();
            if control.is::<gtk::ToggleButton>() {
                assert_eq!(
                    control
                        .downcast_ref::<gtk::ToggleButton>()
                        .unwrap()
                        .is_active(),
                    value.as_bool().unwrap()
                );
            } else if let Some(combo) = control.downcast_ref::<gtk::ComboBoxText>() {
                assert_eq!(combo.active_id().as_deref(), value.as_str());
            } else if let Some(scale) = control.downcast_ref::<gtk::Scale>() {
                assert_eq!(scale.value(), value.as_f64().unwrap());
            } else if let Some(spin) = control.downcast_ref::<gtk::SpinButton>() {
                assert_eq!(spin.value(), value.as_f64().unwrap());
            }
        }
        dialog.set_values(&obj(json!({
            "feature_explorer": true, "feature_notes": true, "feature_terminal": true,
            "feature_graph": true, "feature_gitgraph": true})));
        let groups = json!([{"query": "tag:a", "color": "#abcdef"}]);
        let mut graph = graph_defaults();
        graph.insert("search".into(), json!("path:src"));
        graph.insert("colorGroups".into(), groups.clone());
        graph.insert("future".into(), json!({"x": 2}));
        dialog.set_values(&obj(json!({ "graph": graph })));
        assert!(changes.borrow().is_empty());
        assert!(dialog.values()["graph"].get("future").is_none());
        for name in all_graph_keys() {
            let row_key = format!("graph.{name}");
            let control = dialog.control(&row_key).unwrap();
            if control.is::<gtk::ToggleButton>() {
                click(&control);
            } else {
                let (_, high) = graph_limits(name).unwrap();
                control
                    .downcast_ref::<gtk::SpinButton>()
                    .unwrap()
                    .set_value(high);
            }
            let (kind, value) = last(&changes);
            assert_eq!(kind, "graph");
            flush();
            let stored = Value::Object(impro_settings_ui::normalize_graph(&settings::get_graph()));
            assert_eq!(value, stored, "{name}");
            assert!(value.get("future").is_none());
            assert_eq!(value["search"], "path:src");
            assert_eq!(value["colorGroups"], groups);
            dialog.reset_button(&row_key).unwrap().clicked();
            let shown = &dialog.values()["graph"][name];
            assert!(
                graph_values_equal(shown, &graph_defaults()[name]),
                "{name}: {shown}"
            );
        }
        let mut snapshot = dialog.values();
        snapshot["graph"]["colorGroups"][0]["query"] = json!("changed");
        assert_eq!(dialog.values()["graph"]["colorGroups"], groups);
        dialog
            .control("terminal_shell")
            .unwrap()
            .downcast_ref::<gtk::Entry>()
            .unwrap()
            .set_text("/missing/impro-shell");
        assert!(dialog.shell_hint().text().contains("not found"));
        assert_eq!(
            last(&changes),
            ("terminal_shell".to_owned(), json!("/missing/impro-shell"))
        );
    });
}

#[test]
fn test_graph_edit_retains_preferences_changed_in_an_open_graph() {
    run(|| {
        let fx = Fixture::new();
        settings::set_graph(&json!({"linkDistance": 220, "search": "new query", "future": 7}));
        click(&fx.dialog.control("graph.showTags").unwrap());
        flush();
        let graph = settings::get_graph();
        assert_eq!(graph["linkDistance"], 220);
        assert_eq!(graph["search"], "new query");
        assert!(graph.get("future").is_none());
        assert_eq!(graph["showTags"], true);
    });
}

#[test]
fn test_two_quick_graph_edits_keep_both() {
    run(|| {
        let fx = Fixture::new();
        click(&fx.dialog.control("graph.showTags").unwrap());
        click(&fx.dialog.control("graph.showArrow").unwrap()); // Before the first write lands.
        flush();
        let graph = settings::get_graph();
        assert_eq!(
            (&graph["showTags"], &graph["showArrow"]),
            (&json!(true), &json!(false))
        );
    });
}

#[test]
fn test_set_values_is_silent_and_validated() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        let changes = fx.changes();
        dialog.set_values(&obj(json!({
            "line_numbers": false, "indent_width": 99, "delete_mode": "bogus", "accent": "#3B82F6",
            "terminal_scrollback": 5, "unknown": 1, "explorer_font_size": 20, "ui_font": "Cantarell"})));
        assert!(changes.borrow().is_empty());
        let values = dialog.values();
        assert_eq!(
            (
                &values["line_numbers"],
                &values["indent_width"],
                &values["delete_mode"]
            ),
            (&json!(false), &json!(8), &json!("permanent"))
        );
        assert_eq!(
            (&values["accent"], &values["terminal_scrollback"]),
            (&json!("#3b82f6"), &json!(1000))
        );
        assert_eq!(values["explorer_font_size"], 20);
        let toggle = dialog.control("line_numbers").unwrap();
        assert!(!toggle
            .downcast_ref::<gtk::ToggleButton>()
            .unwrap()
            .is_active());
        let font = dialog.control("ui_font").unwrap();
        assert_eq!(
            font.downcast_ref::<gtk::FontButton>()
                .unwrap()
                .font()
                .as_deref(),
            Some("Cantarell")
        );
        assert!(!values.contains_key("unknown"));
        flush();
        assert!(settings::load().is_empty());
    });
}

// Layout and geometry ------------------------------------------------------

#[test]
fn test_nav_width_clamp_rows_and_column() {
    run(|| {
        let fx = Fixture::new();
        fx.show();
        let dialog = &fx.dialog;
        assert_eq!(dialog.nav_pane().allocated_width(), 225);
        for row in dialog.nav_list().children() {
            let height = row.allocated_height() - 2;
            assert!((24..=26).contains(&height), "{height}");
        }
        let page = dialog.page_scroll("appearance").unwrap();
        let column = dialog.page_body("appearance").unwrap();
        let (x, y) = column.translate_coordinates(&page, 0, 0).unwrap();
        assert!(column.allocated_width() <= 700);
        assert_eq!(x, 32);
        assert_eq!(y, 48);
        assert!(!dialog.row("theme").unwrap().narrow());
        for (width, expected) in [(600, 200), (1400, 250)] {
            dialog.resize(width, 700);
            pump(150);
            assert_eq!(dialog.nav_pane().allocated_width(), expected, "{width}");
            if width == 600 {
                // 336 px column: controls stack under the text, toggles stay.
                let theme = dialog.row("theme").unwrap();
                assert!(theme.narrow());
                assert!(dialog.hotkey_card().rows()[0].narrow());
                assert!(!dialog.row("reduce_motion").unwrap().narrow());
                let controls = theme
                    .controls
                    .translate_coordinates(&*theme, 0, 0)
                    .unwrap()
                    .1;
                let name = theme
                    .name_label
                    .translate_coordinates(&*theme, 0, 0)
                    .unwrap()
                    .1;
                assert!(controls > name);
            }
        }
        assert!(!dialog.row("theme").unwrap().narrow());
        let page_width = page.allocated_width();
        let x = column.translate_coordinates(&page, 0, 0).unwrap().0;
        assert_eq!(column.allocated_width(), 700);
        assert!((x - (page_width - 700 - x)).abs() <= 1); // Centred.
    });
}

#[test]
fn test_card_radius_and_row_metrics() {
    run(|| {
        let fx = Fixture::new();
        fx.show();
        let ui = &themes::get_theme(Some("impro-dark")).ui;
        let dialog = &fx.dialog;
        let card = dialog.groups("appearance")[0].1.clone();
        let (x, y) = card.translate_coordinates(&**dialog, 0, 0).unwrap();
        let window = dialog.window().unwrap();
        let pixel = |px, py| window_pixel(&window, px, py);
        assert_eq!(pixel(x, y), hex_rgb(&ui["bg"])); // Rounded corner shows the page.
        assert_eq!(pixel(x + 2, y + 2), hex_rgb(&ui["bg"]));
        assert_eq!(pixel(x + 12, y + 12), hex_rgb(&ui["surface_alt"]));
        assert_eq!(
            pixel(x + card.allocated_width() / 2, y),
            hex_rgb(&ui["surface_alt"])
        );
        let row = dialog.row("reduce_motion").unwrap();
        assert!(
            (70..=82).contains(&row.allocated_height()),
            "{}",
            row.allocated_height()
        );
        let toggle = dialog.control("reduce_motion").unwrap();
        let rx = row.translate_coordinates(&**dialog, 0, 0).unwrap().0;
        let tx = toggle.translate_coordinates(&**dialog, 0, 0).unwrap().0;
        assert_eq!(
            rx + row.allocated_width() - 20,
            tx + toggle.allocated_width()
        );
        assert_eq!(
            (toggle.allocated_width(), toggle.allocated_height()),
            (40, 22)
        );
    });
}

// Keyboard -----------------------------------------------------------------

#[test]
fn test_keyboard_navigation_model() {
    run(|| {
        let fx = Fixture::new();
        fx.show();
        let dialog = &fx.dialog;
        let nav = dialog.nav_list();
        let rows = nav.rows();
        let focused = |w: &gtk::Widget| dialog.focused_widget().as_ref() == Some(w);
        rows["appearance"].grab_focus();
        key(&*nav, k::Down);
        assert_eq!(nav.page_id().as_deref(), Some("editor"));
        assert_eq!(
            dialog.stack().visible_child_name().as_deref(),
            Some("editor")
        );
        key(&*nav, k::k);
        assert_eq!(
            dialog.stack().visible_child_name().as_deref(),
            Some("appearance")
        );
        key(&*nav, k::Return);
        assert!(focused(dialog.row("theme").unwrap().upcast_ref()));
        key(dialog.deref_window(), k::Down);
        assert!(focused(dialog.row("accent").unwrap().upcast_ref()));
        key(dialog.deref_window(), k::Down);
        key(dialog.deref_window(), k::Down); // Across cards: into the "Font" group.
        assert!(focused(dialog.row("ui_font").unwrap().upcast_ref()));
        key(dialog.deref_window(), k::Up);
        assert!(focused(dialog.row("reduce_motion").unwrap().upcast_ref()));
        let changes = fx.changes();
        key(dialog.deref_window(), k::space);
        key(dialog.deref_window(), k::Return);
        assert_eq!(
            *changes.borrow(),
            [
                ("reduce_motion".to_owned(), json!(true)),
                ("reduce_motion".to_owned(), json!(false))
            ]
        );
        key(dialog.deref_window(), k::Left);
        assert!(focused(rows["appearance"].upcast_ref()));
        send_key(&*nav, k::ISO_Left_Tab, gdk::ModifierType::SHIFT_MASK);
        assert!(focused(dialog.search_entry().upcast_ref()));
        rows["appearance"].grab_focus();
        key(&*nav, k::Tab);
        assert!(focused(dialog.row("theme").unwrap().upcast_ref()));
        key(dialog.deref_window(), k::Tab); // Tab moves from the row into its controls.
        let focus = dialog.focused_widget().unwrap();
        assert!(focus.is_ancestor(&*dialog.row("theme").unwrap()));
        send_key(dialog.deref_window(), k::f, gdk::ModifierType::CONTROL_MASK);
        assert!(focused(dialog.search_entry().upcast_ref()));
        rows["appearance"].grab_focus();
        key(dialog.deref_window(), k::slash);
        assert!(focused(dialog.search_entry().upcast_ref()));
    });
}

trait DerefWindow {
    fn deref_window(&self) -> &gtk::Window;
}
impl DerefWindow for SettingsDialog {
    fn deref_window(&self) -> &gtk::Window {
        self
    }
}

#[test]
fn test_toggle_space_and_focus_ring_rule() {
    run(|| {
        let fx = Fixture::new();
        fx.show();
        let toggle = fx.dialog.control("reduce_motion").unwrap();
        toggle.grab_focus();
        let changes = fx.changes();
        key(&toggle, k::space);
        pump(300); // GtkButton keyboard activation clicks after a short press.
        assert_eq!(
            *changes.borrow(),
            [("reduce_motion".to_owned(), json!(true))]
        );
        let css = impro_ui::widgets::css(&themes::get_theme(Some("impro-dark")).ui);
        assert!(css.contains("list.impro-card > row:focus"));
        let flat = css.split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(flat.contains("button.impro-toggle:focus { outline-width: 2px"));
    });
}

// Search -------------------------------------------------------------------

#[test]
fn test_search_filters_groups_and_escape_order() {
    run(|| {
        let fx = Fixture::new();
        fx.show();
        let dialog = &fx.dialog;
        let entry = dialog.search_entry();
        entry.grab_focus();
        entry.set_text("cursor");
        pump(0);
        assert_eq!(
            dialog.stack().visible_child_name().as_deref(),
            Some("results")
        );
        let visible: Vec<_> = dialog
            .visible_rows()
            .iter()
            .map(|r| r.key.clone().unwrap())
            .collect();
        assert_eq!(
            visible,
            ["highlight_line", "terminal_cursor", "terminal_blink"]
        );
        assert_eq!(dialog.nav_list().rows()["terminal"].count_label.text(), "2");
        assert!(!dialog.nav_list().rows()["files"].is_child_visible());
        assert!(dialog.section("terminal").unwrap().get_visible());
        assert!(!dialog.section("files").unwrap().get_visible());
        for (query, key) in [
            ("brand", Some("accent")),
            ("gutter", Some("line_numbers")),
            ("trash", Some("delete_mode")),
            ("Ctrl+Shift+S", None),
        ] {
            entry.set_text(query);
            let rows = dialog.visible_rows();
            match key {
                Some(key) => assert!(
                    rows.iter().any(|r| r.key.as_deref() == Some(key)),
                    "{query}"
                ),
                None => assert_eq!(
                    rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
                    ["Save all files"]
                ),
            }
        }
        entry.set_text("zzzz");
        dialog.show_all(); // e.g. MainWindow.open_settings() while a search is active.
        let empty = dialog.empty_label().unwrap();
        assert!(empty.get_visible());
        assert!(!dialog.section("terminal").unwrap().get_visible());
        assert!(!dialog.hotkey_box().get_visible());
        assert!(empty.text().contains("zzzz"));
        assert!(dialog.visible_rows().is_empty());
        key(&**dialog, k::Escape); // Clears first...
        assert_eq!(entry.text(), "");
        assert!(dialog.is_visible());
        assert_eq!(
            dialog.stack().visible_child_name().as_deref(),
            Some("appearance")
        );
        assert!(!empty.get_visible());
        let body = dialog.page_body("appearance").unwrap();
        assert_eq!(
            body.parent().as_ref(),
            Some(dialog.page_column("appearance").unwrap().upcast_ref())
        );
        assert_eq!(dialog.visible_rows().len(), 4);
        dialog.show_all(); // show_all() must not resurrect filtered widgets.
        assert!(!empty.get_visible());
        key(&**dialog, k::Escape); // ...then closes.
        assert!(!dialog.is_visible());
    });
}

#[test]
fn test_hotkeys_page_list_filter_and_feed() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        let names: Vec<_> = dialog
            .hotkey_card()
            .rows()
            .iter()
            .map(|r| r.name.clone())
            .collect();
        assert_eq!(
            names,
            DEFAULT_HOTKEYS
                .iter()
                .map(|h| h.0.to_owned())
                .collect::<Vec<_>>()
        );
        assert!(DEFAULT_HOTKEYS.contains(&("Open settings", "Ctrl+,")));
        let rows = dialog.hotkey_card().rows();
        let row = &rows[names.iter().position(|n| n == "Next editor tab").unwrap()];
        let chips: Vec<_> = row
            .control
            .as_ref()
            .unwrap()
            .downcast_ref::<gtk::Container>()
            .unwrap()
            .children()
            .iter()
            .map(|c| c.downcast_ref::<gtk::Label>().unwrap().text().to_string())
            .collect();
        assert_eq!(chips, ["Ctrl+Tab", "Ctrl+PageDown"]);
        fx.show();
        dialog.select_page("hotkeys");
        dialog.hotkey_filter().set_text("terminal");
        assert!(visible_hotkeys(dialog)
            .iter()
            .all(|r| r.name.to_lowercase().contains("terminal")));
        dialog.hotkey_filter().set_text("nothing-here");
        assert!(dialog.hotkeys_empty().get_visible());
        dialog.hotkey_filter().grab_focus();
        key(&**dialog, k::Escape);
        assert_eq!(dialog.hotkey_filter().text(), "");
        assert!(dialog.is_visible());
        dialog.set_hotkeys(&[("Do things", "Ctrl+D"), ("Other", "F9 / F10")]);
        let names: Vec<_> = dialog
            .hotkey_card()
            .rows()
            .iter()
            .map(|r| r.name.clone())
            .collect();
        assert_eq!(names, ["Do things", "Other"]);
        assert_eq!(visible_hotkeys(dialog).len(), 2);
    });
}

#[test]
fn test_about_page_and_copy_path() {
    run(|| {
        let fx = Fixture::new();
        let dialog = &fx.dialog;
        let path = settings::settings_path().display().to_string();
        assert_eq!(dialog.settings_path_label().text(), path);
        dialog.copy_button().clicked();
        let text = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).wait_for_text();
        assert_eq!(text.as_deref(), Some(path.as_str()));
        assert!(dialog.row("settings_file").is_some());
    });
}

#[test]
fn test_css_parses_for_every_theme() {
    run(|| {
        impro_settings_ui::init();
        for theme in themes::themes() {
            let provider = gtk::CssProvider::new();
            let errors: Rc<RefCell<Vec<String>>> = Rc::default();
            let sink = errors.clone();
            provider.connect_parsing_error(move |_, _, e| sink.borrow_mut().push(e.to_string()));
            let css = impro_settings_ui::css(&theme.ui);
            provider.load_from_data(css.as_bytes()).unwrap();
            assert!(
                errors.borrow().is_empty(),
                "{}: {:?}",
                theme.id,
                errors.borrow()
            );
            let widgets = impro_ui::widgets::css(&theme.ui);
            assert!(widgets.contains("button.impro-toggle"));
            let all = impro_ui::style::build_css(theme);
            assert!(all.contains(&css), "{}", theme.id);
            assert!(all.contains(&widgets), "{}", theme.id);
        }
    });
}
