//! Write-behind persistence: coalescing, flush, failure reporting and the dialog's
//! behaviour when the settings file cannot be written.
mod common;
use common::*;
use gtk::prelude::*;
use impro_core::settings;
use impro_settings_ui::{flush, save_async_to, save_value_async, set_save_failed_handler, writer};
use serde_json::{json, Value};
use std::{cell::RefCell, path::PathBuf, rc::Rc};

#[test]
fn test_coalesced_burst_lands_last_value_after_flush() {
    run(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("impro/settings.json");
        for n in 0..200 {
            save_async_to(&path, obj(json!({"indent_width": n % 8 + 2, "keep": "x"})));
        }
        assert!(writer::unsaved_value(&path, "indent_width").is_some() || path.exists());
        flush();
        assert!(writer::unsaved_value(&path, "indent_width").is_none());
        let stored = settings::Store::new(&path).load();
        assert_eq!(stored["indent_width"], 199 % 8 + 2);
        assert_eq!(stored["keep"], "x");
    });
}

#[test]
fn test_write_goes_to_the_path_current_at_queue_time() {
    run(|| {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        std::env::set_var("XDG_CONFIG_HOME", a.path());
        save_value_async("auto_pair", Value::Bool(false));
        std::env::set_var("XDG_CONFIG_HOME", b.path());
        save_value_async("line_numbers", Value::Bool(false));
        flush();
        let first = settings::Store::new(a.path().join("impro/settings.json")).load();
        let second = settings::Store::new(b.path().join("impro/settings.json")).load();
        assert_eq!(first["auto_pair"], false);
        assert!(!first.contains_key("line_numbers"));
        assert_eq!(second["line_numbers"], false);
    });
}

#[test]
fn test_failed_write_is_reported_on_the_main_thread() {
    run(|| {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "x").unwrap();
        let path = blocker.join("settings.json"); // The parent is a file: cannot be created.
        let failures: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
        let sink = failures.clone();
        set_save_failed_handler(move |p| sink.borrow_mut().push(p.to_path_buf()));
        save_async_to(&path, obj(json!({"auto_pair": false})));
        flush();
        pump(50);
        assert_eq!(*failures.borrow(), [path]);
    });
}

#[test]
fn test_failed_save_still_changes_live_and_reused_dialog() {
    run(|| {
        let fx = Fixture::new();
        let blocker = fx.root.path().join("blocked");
        std::fs::write(&blocker, "x").unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &blocker); // settings_path() now has a file as parent.
        let failures: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
        let sink = failures.clone();
        fx.dialog
            .connect_save_failed(move |p| sink.borrow_mut().push(p.to_path_buf()));
        let changes = fx.changes();
        click(&fx.dialog.control("line_numbers").unwrap());
        click(&fx.dialog.control("auto_pair").unwrap());
        flush();
        pump(50);
        assert!(!failures.borrow().is_empty());
        assert_eq!(failures.borrow()[0], settings::settings_path());
        // The session keeps the change; the reused dialog still shows it.
        assert_eq!(changes.borrow().len(), 2);
        assert_eq!(fx.dialog.values()["line_numbers"], false);
        fx.dialog.hide();
        fx.show();
        assert_eq!(fx.dialog.values()["auto_pair"], false);
        std::env::set_var("XDG_CONFIG_HOME", fx.root.path());
    });
}
