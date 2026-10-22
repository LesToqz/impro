//! Controls panel: sections, defaults, round trip with the preferences, search,
//! colour rules, reset and local-mode controls.

mod gtk_common;

use gtk::prelude::*;
use gtk_common::*;
use impro_graph::prefs::{self, GraphPrefs, LIMITS, STEPS};
use impro_graph::view::testing as t;
use impro_graph::GraphType;
use serde_json::{json, Value};

#[test]
fn sections_are_in_the_documented_order_with_the_default_collapse_state() {
    run(|| {
        let f = Fixture::new();
        let p = f.view.panel();
        let ids: Vec<&str> = impro_graph::view::panel::SECTIONS
            .iter()
            .map(|s| s.0)
            .collect();
        assert_eq!(ids, ["filters", "groups", "display", "forces"]);
        let titles: Vec<&str> = impro_graph::view::panel::SECTIONS
            .iter()
            .map(|s| s.2)
            .collect();
        assert_eq!(
            titles,
            [
                "Project scope",
                "Colour rules",
                "Visual detail",
                "Layout tuning"
            ]
        );
        // the scope section starts expanded, the others collapsed
        assert_eq!(p.collapsed("filters"), Some(false));
        for id in ["groups", "display", "forces"] {
            assert_eq!(p.collapsed(id), Some(true), "{id}");
        }
        assert!(f.view.panel_open());
    });
}

#[test]
fn every_control_shows_its_default_range_and_step() {
    run(|| {
        let f = Fixture::new();
        let p = f.view.panel();
        let d = GraphPrefs::default();
        let defaults = d.to_value();
        for (key, lo, hi) in LIMITS.iter().filter(|(k, _, _)| *k != "scale") {
            let scale = p.scale(key).unwrap_or_else(|| panic!("slider {key}"));
            let adj = scale.adjustment();
            assert_eq!((adj.lower(), adj.upper()), (*lo, *hi), "{key} range");
            let step = STEPS.iter().find(|(k, _)| k == key).and_then(|s| s.1);
            assert_eq!(p.step_of(key), step, "{key} step");
            assert_eq!(adj.step_increment(), step.unwrap_or(0.0), "{key} increment");
            let want = defaults[*key].as_f64().expect("number");
            assert!(
                (scale.value() - want).abs() < 1e-9,
                "{key}: {} vs {want}",
                scale.value()
            );
        }
        for (key, text) in [
            ("textFadeMultiplier", "0.2"),
            ("nodeSizeMultiplier", "1.15"),
            ("lineSizeMultiplier", "1.25"),
            ("centerStrength", "0.12"),
            ("repelStrength", "6.0"),
            ("linkStrength", "0.35"),
            ("linkDistance", "160"),
            ("localDepth", "2"),
        ] {
            assert_eq!(p.slider_text(key).as_deref(), Some(text), "{key}");
        }
        for key in [
            "showTags",
            "showAttachments",
            "hideUnresolved",
            "showOrphans",
            "showCode",
            "showArrow",
            "localIncoming",
            "localOutgoing",
        ] {
            let on = p
                .switch(key)
                .unwrap_or_else(|| panic!("switch {key}"))
                .is_active();
            assert_eq!(on, defaults[key].as_bool().expect("bool"), "{key}");
        }
        assert_eq!(p.search_entry().text(), "");
        assert!(p.rule_entries().is_empty());
        // the global graph hides the local-mode rows and keeps the unlinked-files switch
        assert_eq!(p.row_visible("localDepth"), Some(false));
        assert_eq!(p.row_visible("localIncoming"), Some(false));
        assert_eq!(p.row_visible("showOrphans"), Some(true));
        assert!(p.replay_visible());
    });
}

#[test]
fn a_switch_flows_to_the_graph_the_preferences_and_the_saved_blob() {
    run(|| {
        let f = Fixture::new();
        assert!(!f.has("#topic"), "tags start hidden");
        f.view.panel().switch("showTags").expect("switch").toggle();
        assert!(f.view.prefs().show_tags);
        assert!(
            f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "#topic")),
            "tag node appears"
        );
        let blobs = saved(&f);
        let last = blobs.last().expect("saved");
        assert_eq!(last["showTags"], json!(true));
        assert_eq!(
            prefs::normalize(last).to_value(),
            *last,
            "the saved blob is normalised"
        );
        // a new view created from the store restores the switch
        let again = Fixture::in_dir(tempfile::tempdir().expect("dir"), last.clone(), false);
        assert!(again
            .view
            .panel()
            .switch("showTags")
            .expect("switch")
            .is_active());
        assert!(f
            .seen()
            .settings
            .iter()
            .any(|s| s["showTags"] == json!(true)));
    });
}

#[test]
fn every_filter_switch_changes_the_graph_as_documented() {
    run(|| {
        let f = Fixture::new();
        let toggle = |key: &str| f.view.panel().switch(key).expect("switch").toggle();
        assert!(f.has("?Missing Note"), "missing targets show by default");
        assert!(f.has("src/main.py") && f.has("orphan.md"));
        toggle("hideUnresolved");
        assert!(f.wait_ids(5_000, |ids| !ids.iter().any(|i| i == "?Missing Note")));
        toggle("showAttachments");
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "pic.png")));
        toggle("showOrphans");
        assert!(f.wait_ids(5_000, |ids| !ids.iter().any(|i| i == "orphan.md")));
        toggle("showCode");
        assert!(f.wait_ids(5_000, |ids| !ids.iter().any(|i| i == "src/main.py")));
        // everything back
        for key in [
            "hideUnresolved",
            "showAttachments",
            "showOrphans",
            "showCode",
        ] {
            toggle(key);
        }
        assert!(
            f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "?Missing Note")
                && ids.iter().any(|i| i == "src/main.py"))
        );
        assert!(!f.has("pic.png"));
    });
}

#[test]
fn sliders_snap_to_their_step_and_update_preferences_and_display() {
    run(|| {
        let f = Fixture::new();
        let p = f.view.panel();
        let scale = p.scale("linkDistance").expect("slider");
        scale.emit_by_name::<bool>("change-value", &[&gtk::ScrollType::Jump, &171.0f64]);
        assert_eq!(scale.value(), 168.0, "snaps to a multiple of 8 from 48");
        assert_eq!(f.view.prefs().link_distance, 168.0);
        assert_eq!(p.slider_text("linkDistance").as_deref(), Some("168"));
        let size = p.scale("nodeSizeMultiplier").expect("slider");
        size.emit_by_name::<bool>("change-value", &[&gtk::ScrollType::Jump, &2.04f64]);
        assert!((size.value() - 2.05).abs() < 1e-9);
        assert_eq!(p.slider_text("nodeSizeMultiplier").as_deref(), Some("2.05"));
        // the marker grows (eased) with the multiplier
        let before = t::node_radius(&f.view, "hub/Hub.md").expect("radius");
        f.settle();
        let after = t::node_radius(&f.view, "hub/Hub.md").expect("radius");
        assert!(after > before * 1.5, "{before} -> {after}");
        // forces reheat the layout
        let ticks = t::info(&f.view).ticks;
        p.scale("repelStrength")
            .expect("slider")
            .emit_by_name::<bool>("change-value", &[&gtk::ScrollType::Jump, &10.0f64]);
        assert!(t::wait_for(3_000, || t::info(&f.view).ticks > ticks));
        assert_eq!(f.view.prefs().repel_strength, 10.0);
        assert_eq!(f.view.prefs().forces().repel, -1200.0);
    });
}

#[test]
fn search_filters_with_the_query_language() {
    run(|| {
        let f = Fixture::new();
        let entry = f.view.panel().search_entry().clone();
        entry.set_text("path:hub");
        assert!(
            f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "hub/Hub.md")
                && !ids.iter().any(|i| i == "a.md"))
        );
        assert_eq!(f.view.prefs().search, "path:hub");
        entry.set_text("tag:#topic");
        assert!(f.wait_ids(5_000, |ids| ids == ["hub/Hub.md"]
            || (ids.contains(&"hub/Hub.md".to_string())
                && !ids.contains(&"c.md".to_string()))));
        entry.set_text("\"lonely\" OR -a");
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "orphan.md")));
        // missing targets stay until their references are gone ("strict" filtering)
        entry.set_text("zzzz-nothing");
        assert!(f.wait_ids(5_000, |ids| !ids.is_empty()
            && ids.iter().all(|i| i.starts_with('?'))));
        f.view
            .panel()
            .switch("hideUnresolved")
            .expect("switch")
            .toggle();
        assert!(f.wait_ids(5_000, |ids| ids.is_empty()));
        assert_eq!(
            f.view.status_text(),
            ("No files match this view".to_string(), true)
        );
        f.view
            .panel()
            .switch("hideUnresolved")
            .expect("switch")
            .toggle();
        entry.set_text("");
        assert!(f.wait_ids(5_000, |ids| ids.len() > 5));
        assert!(!f.view.status_text().1);
        // saved with the preferences (the debounce keeps only the newest value)
        entry.set_text("keep me");
        assert_eq!(saved(&f).last().expect("saved")["search"], json!("keep me"));
    });
}

#[test]
fn colour_rules_are_limited_to_twelve_and_colour_the_first_match() {
    run(|| {
        let f = Fixture::new();
        let p = f.view.panel();
        let dark = f.view.panel().is_local(); // false; the theme is dark by default
        let _ = dark;
        p.add_rule_button().clicked();
        assert_eq!(p.rule_entries().len(), 1);
        let rule = f.view.prefs().color_groups[0].clone();
        assert_eq!(rule.query, "");
        assert_eq!(rule.color.to_hex_string(), prefs::new_rule_color(0, true));
        p.rule_entries()[0].set_text("path:hub");
        t::pump(300);
        f.settle();
        let (_, color) = t::node_look(&f.view, "hub/Hub.md").expect("node");
        assert_eq!(color, prefs::new_rule_color(0, true));
        let (_, other) = t::node_look(&f.view, "a.md").expect("node");
        assert_ne!(other, color);
        // a second rule only colours what the first one did not
        p.add_rule_button().clicked();
        p.rule_entries()[1].set_text("path:a");
        t::pump(300);
        f.settle();
        assert_eq!(
            t::node_look(&f.view, "a.md").expect("node").1,
            prefs::new_rule_color(1, true)
        );
        // the limit
        for _ in 0..20 {
            p.add_rule_button().clicked();
        }
        assert_eq!(f.view.prefs().color_groups.len(), 12);
        assert_eq!(p.rule_entries().len(), 12);
        assert!(!p.add_rule_button().is_sensitive());
        // removing a rule frees a slot
        let remove_count = f.view.prefs().color_groups.len();
        let mut p2 = f.view.prefs();
        p2.color_groups.remove(0);
        f.view.apply_prefs(&p2.to_value());
        assert_eq!(f.view.panel().rule_entries().len(), remove_count - 1);
        assert!(f.view.panel().add_rule_button().is_sensitive());
    });
}

#[test]
fn reset_restores_defaults_but_keeps_search_zoom_and_section_state() {
    run(|| {
        let f = Fixture::new();
        let p = f.view.panel();
        p.switch("showTags").expect("switch").toggle();
        p.switch("showArrow").expect("switch").toggle();
        p.scale("centerStrength")
            .expect("slider")
            .emit_by_name::<bool>("change-value", &[&gtk::ScrollType::Jump, &0.4f64]);
        p.add_rule_button().clicked();
        p.search_entry().set_text("hub");
        p.click_section("display");
        assert_eq!(p.collapsed("display"), Some(false));
        t::wheel(&f.view, 450.0, 300.0, -480.0, false);
        t::pump(600);
        let zoom = t::info(&f.view).target_zoom;
        assert!(zoom > 2.0);
        f.view.panel().reset_button().clicked();
        let now = f.view.prefs();
        let d = GraphPrefs::default();
        assert_eq!(now.show_tags, d.show_tags);
        assert_eq!(now.show_arrow, d.show_arrow);
        assert_eq!(now.center_strength, d.center_strength);
        assert!(now.color_groups.is_empty());
        assert_eq!(now.search, "hub", "search text stays");
        assert!(!now.collapse_display, "section visibility stays");
        assert!(
            (t::info(&f.view).target_zoom - zoom).abs() < 1e-9,
            "zoom stays"
        );
        assert!(f.view.panel().rule_entries().is_empty());
        assert!(f
            .view
            .panel()
            .switch("showArrow")
            .expect("switch")
            .is_active());
    });
}

#[test]
fn the_local_graph_swaps_in_its_traversal_controls() {
    run(|| {
        let f = Fixture::new();
        let hub = f.path("hub/Hub.md");
        f.view.open_local(&hub);
        assert_eq!(f.view.graph_type(), GraphType::Local);
        let p = f.view.panel();
        for key in ["localDepth", "localIncoming", "localOutgoing"] {
            assert_eq!(p.row_visible(key), Some(true), "{key}");
        }
        assert_eq!(p.row_visible("showOrphans"), Some(false));
        assert!(!p.replay_visible());
        // one hop reaches the neighbours, not the targets' targets
        let depth = p.scale("localDepth").expect("slider");
        depth.emit_by_name::<bool>("change-value", &[&gtk::ScrollType::Jump, &1.0f64]);
        assert!(
            f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "hub/Hub.md")
                && !ids.iter().any(|i| i == "?Missing Note"))
        );
        depth.emit_by_name::<bool>("change-value", &[&gtk::ScrollType::Jump, &2.0f64]);
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "?Missing Note")));
        // traversal directions
        f.view
            .panel()
            .switch("localOutgoing")
            .expect("switch")
            .toggle();
        assert!(f.wait_ids(5_000, |ids| ids == ["hub/Hub.md"]));
        assert!(!f.view.prefs().local_outgoing);
        // reset also restores the traversal in local mode
        f.view.panel().reset_button().clicked();
        assert!(f.view.prefs().local_outgoing);
        assert_eq!(f.view.prefs().local_depth, 2);
        // back to the global graph
        f.view.open_global();
        assert_eq!(f.view.panel().row_visible("localDepth"), Some(false));
        assert_eq!(f.view.panel().row_visible("showOrphans"), Some(true));
    });
}

#[test]
fn section_and_panel_state_are_persisted() {
    run(|| {
        let f = Fixture::new();
        let p = f.view.panel();
        p.click_section("forces");
        p.click_section("filters");
        let blobs = saved(&f);
        let last: &Value = blobs.last().expect("saved");
        assert_eq!(last["collapse-forces"], json!(false));
        assert_eq!(last["collapse-filter"], json!(true));
        assert_eq!(last["collapse-display"], json!(true));
        f.view.set_panel_open(false);
        assert!(!f.view.panel_open());
        let blobs = saved(&f);
        assert_eq!(blobs.last().expect("saved")["close"], json!(true));
        f.view.set_panel_open(true);
        assert!(f.view.panel_open());
        assert_eq!(saved(&f).last().expect("saved")["close"], json!(false));
    });
}

#[test]
fn saved_state_restores_into_the_panel() {
    run(|| {
        let blob = json!({
            "showTags": true, "collapse-filter": true, "collapse-forces": false,
            "linkDistance": 200, "colorGroups": [{"query": "path:hub", "color": "#112233"}],
            "close": true, "search": "hub", "scale": 2.5
        });
        let f = Fixture::in_dir(tempfile::tempdir().expect("dir"), blob, false);
        let p = f.view.panel();
        assert!(p.switch("showTags").expect("switch").is_active());
        assert_eq!(p.collapsed("filters"), Some(true));
        assert_eq!(p.collapsed("forces"), Some(false));
        assert_eq!(p.scale("linkDistance").expect("slider").value(), 200.0);
        assert_eq!(p.rule_entries()[0].text(), "path:hub");
        assert_eq!(p.search_entry().text(), "hub");
        assert!(!f.view.panel_open());
        assert!((t::info(&f.view).zoom - 2.5).abs() < 1e-9);
    });
}
