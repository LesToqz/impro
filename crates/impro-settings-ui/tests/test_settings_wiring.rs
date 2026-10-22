//! Window wiring contract that can run against a stand-in owner (the cases of
//! test_settings_wiring.py that need the real window are listed in VERIFICATION.md).
mod common;
use common::*;
use gtk::prelude::*;
use impro_core::settings;
use impro_settings_ui::{flush, SettingsDialog};
use serde_json::json;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

/// What the window does with the dialog: lazy construction, signal forwarding, persistence of
/// theme and font sizes by the owner, and `open_settings(page)`.
struct Owner {
    window: gtk::Window,
    dialog: RefCell<Option<SettingsDialog>>,
    sizes: Rc<RefCell<BTreeMap<String, i32>>>,
    theme: Rc<RefCell<String>>,
    created: std::cell::Cell<u32>,
}

impl Owner {
    fn new() -> Self {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.show_all();
        Self {
            window,
            dialog: RefCell::default(),
            sizes: Rc::new(RefCell::new(
                settings::get_font_sizes().into_iter().collect(),
            )),
            theme: Rc::new(RefCell::new(settings::get_theme_id())),
            created: Default::default(),
        }
    }

    fn open_settings(&self, page: Option<&str>) -> SettingsDialog {
        if self.dialog.borrow().is_none() {
            self.created.set(self.created.get() + 1);
            let dialog = SettingsDialog::new(
                Some(&self.window),
                Some(&self.theme.borrow()),
                Some(&self.sizes.borrow()),
            );
            let theme = self.theme.clone();
            dialog.connect_theme_selected(move |id| {
                *theme.borrow_mut() = id.to_owned();
                settings::set_theme_id(Some(id));
            });
            let sizes = self.sizes.clone();
            dialog.connect_font_size_changed(move |key, size| {
                sizes.borrow_mut().insert(key.to_owned(), size);
                settings::set_font_size(key, &json!(size)).unwrap();
            });
            *self.dialog.borrow_mut() = Some(dialog);
        }
        let dialog = self.dialog.borrow().clone().unwrap();
        let mut live = settings::get_values();
        live.insert("theme".into(), json!(self.theme.borrow().clone()));
        for (key, size) in self.sizes.borrow().iter() {
            live.insert(key.clone(), json!(size));
        }
        dialog.set_values(&live);
        dialog.set_hotkeys(&impro_settings_ui::default_hotkeys());
        dialog.present(page);
        dialog
    }
}

#[test]
fn test_gear_accelerator_lazy_reusable_nonmodal_dialog() {
    run(|| {
        let _fx = Fixture::new();
        let owner = Owner::new();
        assert!(owner.dialog.borrow().is_none());
        let dialog = owner.open_settings(None);
        assert_eq!(dialog.transient_for().as_ref(), Some(&owner.window));
        assert!(!dialog.is_modal());
        dialog.response();
        assert!(!dialog.is_visible());
        let again = owner.open_settings(None);
        assert_eq!(owner.created.get(), 1);
        assert!(again.is_visible());
        assert_eq!(dialog.page_id().as_deref(), Some("appearance"));
        unsafe { owner.window.destroy() };
        pump(10);
    });
}

#[test]
fn test_open_on_hotkeys_page_for_f1() {
    run(|| {
        let _fx = Fixture::new();
        let owner = Owner::new();
        let dialog = owner.open_settings(Some("hotkeys"));
        assert_eq!(dialog.page_id().as_deref(), Some("hotkeys"));
        assert_eq!(visible_page(&dialog).as_deref(), Some("hotkeys"));
        let row = dialog.nav_list().rows()["hotkeys"].clone();
        assert_eq!(dialog.focused_widget().as_ref(), Some(row.upcast_ref()));
        let reopened = owner.open_settings(Some("terminal"));
        assert_eq!(visible_page(&reopened).as_deref(), Some("terminal"));
        unsafe { owner.window.destroy() };
        pump(10);
    });
}

#[test]
fn test_live_sizes_future_content_reset_and_reopen() {
    run(|| {
        let _fx = Fixture::new();
        let owner = Owner::new();
        let dialog = owner.open_settings(None);
        let want = [
            ("terminal_font_size", 17),
            ("editor_font_size", 18),
            ("explorer_font_size", 16),
        ];
        for (key, value) in want {
            dialog.size_scale(key).unwrap().set_value(value as f64);
        }
        let sizes: BTreeMap<String, i32> = want.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        assert_eq!(*owner.sizes.borrow(), sizes);
        assert_eq!(settings::get_font_sizes(), sizes);
        // A second window reads the stored sizes and shows them.
        let other = Owner::new();
        assert_eq!(*other.sizes.borrow(), sizes);
        assert_eq!(other.open_settings(None).sizes(), sizes);
        for key in settings::FONT_KEYS {
            dialog.reset_button(key).unwrap().clicked();
        }
        let defaults: BTreeMap<String, i32> = settings::default_font_sizes()
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        assert_eq!(*owner.sizes.borrow(), defaults);
        assert_eq!(settings::get_font_sizes(), defaults);
        unsafe {
            owner.window.destroy();
            other.window.destroy();
        }
        pump(10);
    });
}

#[test]
fn test_invalid_saved_sizes_are_safe_before_first_frame() {
    run(|| {
        let fx = Fixture::new();
        let target = settings::settings_path();
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        let cases = [
            (
                json!({"terminal_font_size": "bad", "editor_font_size": "bad", "explorer_font_size": "bad"}),
                [11, 11, 10],
            ),
            (
                json!({"terminal_font_size": -100, "editor_font_size": 9999, "explorer_font_size": null}),
                [7, 32, 10],
            ),
        ];
        for (raw, expected) in cases {
            std::fs::write(&target, raw.to_string()).unwrap();
            let owner = Owner::new();
            let dialog = owner.open_settings(None);
            let want: BTreeMap<String, i32> = settings::FONT_KEYS
                .iter()
                .map(|k| k.to_string())
                .zip(expected)
                .collect();
            assert_eq!(dialog.sizes(), want, "{raw}");
            unsafe { owner.window.destroy() };
            pump(10);
        }
        drop(fx);
    });
}

#[test]
fn test_settings_changes_are_forwarded_and_saved_without_blocking() {
    run(|| {
        let fx = Fixture::new();
        let changes = fx.changes();
        click(&fx.dialog.control("show_hidden").unwrap());
        assert_eq!(changes.borrow().len(), 1); // Signalled before any disk work is awaited.
        flush();
        assert_eq!(settings::get_value("show_hidden").unwrap(), json!(false));
    });
}
