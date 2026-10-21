use impro_core::settings::*;
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt};
fn store() -> (tempfile::TempDir, Store) {
    let d = tempfile::tempdir().unwrap();
    let s = Store::new(d.path().join("impro/settings.json"));
    (d, s)
}
#[test]
fn test_round_trip_private_and_unknown_keys() {
    let (_d, s) = store();
    assert!(s.load().is_empty());
    s.save(
        json!({"unrelated":{"size":12},"unicode":"λ"})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    assert!(s.set_value("theme", &json!("nord")).unwrap());
    assert_eq!(s.get_value("theme").unwrap(), "nord");
    assert_eq!(s.load()["unrelated"], json!({"size":12}));
    assert_eq!(s.load()["unicode"], "λ");
    assert_eq!(
        fs::metadata(s.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
}
#[test]
fn test_invalid_files() {
    let (_d, s) = store();
    fs::create_dir_all(s.path().parent().unwrap()).unwrap();
    for payload in ["", "{broken", "[]", "null", "42", "\"text\""] {
        fs::write(s.path(), payload).unwrap();
        assert!(s.load().is_empty());
    }
    fs::write(s.path(), [255]).unwrap();
    assert!(s.load().is_empty());
    fs::remove_file(s.path()).unwrap();
    fs::create_dir(s.path()).unwrap();
    assert!(s.save(&Values::new()).is_err());
}
#[test]
fn test_default_lookup_does_not_create_or_rewrite_settings() {
    let (_d, s) = store();
    assert_eq!(s.get_values(), *defaults());
    assert!(!s.path().exists());
    s.save(json!({"theme":"dracula","keep":42}).as_object().unwrap())
        .unwrap();
    let original = fs::read(s.path()).unwrap();
    assert_eq!(s.get_value("theme").unwrap(), "dracula");
    assert_eq!(fs::read(s.path()).unwrap(), original);
}
#[test]
fn test_atomic_replace_and_cleanup_on_failure() {
    let (_d, s) = store();
    s.save(json!({"original":true}).as_object().unwrap())
        .unwrap();
    s.save(json!({"new":7}).as_object().unwrap()).unwrap();
    assert_eq!(
        s.load(),
        *json!({"original":true,"new":7}).as_object().unwrap()
    );
    let blocked = s.path().with_file_name("blocked");
    fs::create_dir(&blocked).unwrap();
    let bad = Store::new(blocked);
    assert!(bad.save(&Values::new()).is_err());
    assert_eq!(fs::read_dir(s.path().parent().unwrap()).unwrap().count(), 2);
}
#[test]
fn test_font_defaults_and_unknown_keys() {
    assert_eq!(default_font_sizes().values().copied().sum::<i32>(), 32);
    assert!(normalize("other", &json!(12)).is_err());
    for k in FONT_KEYS {
        assert_eq!(normalize(k, &Value::Null).unwrap(), defaults()[k]);
    }
}
#[test]
fn test_font_clamping_rounding_and_reset_preserve_keys() {
    let (_d, s) = store();
    s.save(
        json!({"future":{"enabled":true},"theme":"nord"})
            .as_object()
            .unwrap(),
    )
    .unwrap();
    for key in FONT_KEYS {
        let (lo, hi) = limits()[key];
        for (raw, want) in [
            (-100.0, lo as i64),
            (100.0, hi as i64),
            (12.7, 13),
            (12.2, 12),
            (12.5, 12),
            (13.5, 14),
        ] {
            s.set_value(key, &json!(raw)).unwrap();
            assert_eq!(s.get_value(key).unwrap(), want);
        }
        s.set_value(key, &defaults()[key]).unwrap();
    }
    assert_eq!(s.load()["future"], json!({"enabled":true}));
    assert_eq!(s.get_value("theme").unwrap(), "nord");
}
#[test]
fn test_junk_font_values() {
    for k in FONT_KEYS {
        for raw in [
            Value::Null,
            json!(true),
            json!(false),
            json!("14"),
            json!([]),
            json!({}),
        ] {
            assert_eq!(normalize(k, &raw).unwrap(), defaults()[k]);
        }
    }
}
#[test]
fn test_defaults_cover_spec_keys() {
    assert_eq!(defaults().len(), 45);
    assert!(!defaults().contains_key("md_default_preview"));
    assert!(!defaults().contains_key("graph"));
    assert_eq!(defaults()["delete_mode"], "permanent");
}
#[test]
fn test_clamping_and_invalid_values_fall_back() {
    for (key, raw, want) in [
        ("indent_width", json!(99), json!(8)),
        ("line_numbers", json!(0), json!(true)),
        ("accent", json!("#3B82F6"), json!("#3b82f6")),
        ("terminal_cursor", json!("BLOCK"), json!("block")),
        ("ui_font", json!("bad\nname"), json!("")),
        ("ui_font", json!(" Inter "), json!("Inter")),
    ] {
        assert_eq!(normalize(key, &raw).unwrap(), want);
    }
}
#[test]
fn test_contract_keys_validation_roundtrip_and_reset() {
    let (_d, s) = store();
    for (key, default) in defaults() {
        assert_eq!(normalize(key, &Value::Null).unwrap(), *default);
        s.set_value(key, default).unwrap();
    }
    assert_eq!(s.get_values(), *defaults());
    assert!(s
        .set_values(
            json!({"line_numbers":false,"unknown":7})
                .as_object()
                .unwrap()
        )
        .is_err());
    assert_eq!(s.get_value("line_numbers").unwrap(), true);
}
#[test]
fn test_graph_survives_theme_and_font_save() {
    let (_d, s) = store();
    let graph = json!({"unknown":7,"nested":[{"scale":-100}],"showTags":"opaque"});
    assert!(s.set_blob("graph", &graph));
    s.set_value("theme", &json!("nord")).unwrap();
    s.set_value("editor_font_size", &json!(16)).unwrap();
    assert_eq!(s.get_blob("graph"), graph);
    assert!(s.set_blob("gitgraph", &json!([1, 2])));
    assert_eq!(s.get_blob("gitgraph"), json!([1, 2]));
}
#[test]
fn test_arbitrarily_large_json_integers_clamp_and_preserve_unknown_keys() {
    let (_d, s) = store();
    let huge = "9".repeat(1000);
    let payload =
        format!("{{\"editor_font_size\":{huge},\"explorer_font_size\":-{huge},\"future\":{huge}}}");
    fs::create_dir_all(s.path().parent().unwrap()).unwrap();
    fs::write(s.path(), payload).unwrap();
    assert_eq!(s.get_value("editor_font_size").unwrap(), 32);
    assert_eq!(s.get_value("explorer_font_size").unwrap(), 8);
    s.set_value("theme", &json!("nord")).unwrap();
    assert_eq!(s.load()["future"].to_string(), huge);
    assert_eq!(
        normalize(
            "editor_font_size",
            &serde_json::from_str::<Value>("1e400").unwrap()
        )
        .unwrap(),
        11
    );
}
#[test]
fn test_printable_font_families_reject_unicode_format_characters() {
    for raw in [
        "bad\u{200b}name",
        "bad\u{e000}name",
        "bad\u{a0}name",
        "bad\u{2028}name",
    ] {
        assert_eq!(normalize("ui_font", &json!(raw)).unwrap(), "");
    }
    assert_eq!(normalize("ui_font", &json!("café λ")).unwrap(), "café λ");
}
