//! Status texts, motion and pausing, persistence, theme, replay and lifecycle.

mod gtk_common;

use std::time::Duration;

use gtk::prelude::*;
use gtk_common::*;
use impro_graph::prefs::{self, GraphPrefs};
use impro_graph::view::testing as t;
use impro_graph::GraphType;
use serde_json::{json, Value};

#[test]
fn the_canvas_is_labelled_and_reports_a_status_before_data_arrives() {
    run(|| {
        let project = tempfile::tempdir().expect("dir");
        write(project.path(), "a.md", "x");
        let f = Fixture::in_dir(project, json!({}), false);
        assert_eq!(
            f.view.status_text(),
            ("Mapping this project…".to_string(), true)
        );
        let name = f
            .view
            .canvas()
            .accessible()
            .and_then(|a| atk::prelude::AtkObjectExt::name(&a));
        assert_eq!(name.as_deref(), Some("Project files and references"));
        f.view.rebuild();
        assert!(t::wait_for(5_000, || f.view.summary().nodes == 1));
        assert!(
            !f.view.status_text().1,
            "status hides once there is a graph"
        );
        // an empty project
        let empty = Fixture::in_dir(tempfile::tempdir().expect("dir"), json!({}), true);
        assert!(t::wait_for(5_000, || !empty
            .view
            .status_text()
            .0
            .starts_with("Mapping")));
        assert_eq!(
            empty.view.status_text(),
            ("No files match this view".to_string(), true)
        );
    });
}

#[test]
fn reopening_at_a_saved_zoom_extreme_keeps_markers_and_controls_visible() {
    run(|| {
        for zoom in [0.04, 6.0] {
            let f = Fixture::with(
                &[("a.md", "[[b]]\n"), ("b.md", "[[a]]\n")],
                json!({"scale":zoom,"close":true,"linkDistance":480,"centerStrength":0,"repelStrength":24}),
            );
            assert!(
                t::nodes_on_screen(&f.view) > 0,
                "saved zoom {zoom}: {:?}",
                t::info(&f.view)
            );
            assert!(f.view.panel().chip_visible());
            if zoom == 0.04 {
                assert_eq!(
                    t::info(&f.view).zoom,
                    zoom,
                    "a visible saved zoom is retained"
                );
                f.view.rebuild();
                f.wait_ready();
                assert_eq!(t::info(&f.view).zoom, zoom, "rebuild retains zoom");
            }
        }
    });
}

#[test]
fn the_first_frame_is_the_status_text_and_costs_almost_nothing() {
    run(|| {
        let project = tempfile::tempdir().expect("dir");
        for i in 0..800 {
            write(project.path(), &format!("n/{i}.md"), "[[1]]");
        }
        let t0 = std::time::Instant::now();
        let view = impro_graph::GraphView::with_options(
            project.path(),
            impro_graph::ViewOptions {
                store: std::sync::Arc::new(impro_graph::view::MemoryStore::new(json!({}))),
                scan: true,
            },
        );
        let constructed = t0.elapsed();
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.add(&*view);
        window.show_all();
        assert_eq!(view.summary().nodes, 0, "the scan runs off the main thread");
        assert_eq!(
            view.status_text(),
            ("Mapping this project…".to_string(), true)
        );
        assert!(t::wait_for(3_000, || view.frame_stats().frames >= 1));
        let first = t0.elapsed();
        println!("constructed in {constructed:?}");
        println!("open-to-first-frame: {first:?}");
        assert!(first < Duration::from_millis(250), "{first:?}");
        assert!(t::wait_for(10_000, || view.summary().nodes > 0));
        view.shutdown();
        window.close();
    });
}

#[test]
fn a_settled_graph_runs_no_frames_and_a_hidden_one_pauses() {
    run(|| {
        let f = Fixture::new();
        assert!(!f.view.frame_stats().animating);
        let ticks = t::info(&f.view).ticks;
        t::pump(400);
        assert_eq!(
            t::info(&f.view).ticks,
            ticks,
            "settled: the simulation does not run"
        );
        assert!(
            !f.view.frame_stats().animating,
            "settled: no frame callback"
        );
        // a structural edit wakes it, hiding pauses it mid-way
        write(f.root(), "wake.md", "[[a]] [[b]] [[c]]");
        f.view.file_saved(&f.path("wake.md"));
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "wake.md")));
        assert!(f.view.frame_stats().animating || t::needs_frames(&f.view));
        f.window.hide();
        t::pump(100);
        assert!(!f.view.frame_stats().animating, "hidden: no frame callback");
        let paused_at = t::info(&f.view).ticks;
        t::pump(300);
        assert_eq!(
            t::info(&f.view).ticks,
            paused_at,
            "hidden: the layout is not advanced"
        );
        f.window.show();
        assert!(
            t::wait_for(3_000, || t::info(&f.view).ticks > paused_at),
            "showing resumes the layout"
        );
        f.settle();
        assert!(!f.view.frame_stats().animating);
    });
}

#[test]
fn reduce_motion_skips_visual_easing() {
    run(|| {
        let f = Fixture::new();
        f.view.set_reduce_motion(true);
        assert!(f.view.prefs().reduce_motion);
        let (x, y, _) = f.screen("c.md");
        t::motion(&f.view, x, y);
        // one frame later everything is at its target
        assert!(t::wait_for(2_000, || !t::needs_frames(&f.view)));
        let alpha = t::node_look(&f.view, "orphan.md").expect("node").0;
        assert!((alpha - 0.28).abs() < 1e-3, "{alpha}");
        // zoom jumps too
        t::wheel(&f.view, 300.0, 200.0, -480.0, false);
        assert!(t::wait_for(2_000, || !t::needs_frames(&f.view)));
        assert!((t::info(&f.view).zoom - t::info(&f.view).target_zoom).abs() < 1e-12);
        assert!(saved(&f).iter().any(|b| b["reduceMotion"] == json!(true)));
    });
}

#[test]
fn garbage_in_the_saved_blob_is_normalised_and_saved_values_are_normalised() {
    run(|| {
        let blob = json!({
            "centerStrength": 99, "repelStrength": "lots", "linkDistance": 1, "scale": 100,
            "localDepth": 9, "showTags": "yes", "colorGroups": [{"query": 1}, {"query": "q", "color": "#abcdef"}],
            "unknown": {"x": 1}, "nodeSizeMultiplier": 2.5
        });
        let f = Fixture::in_dir(tempfile::tempdir().expect("dir"), blob.clone(), false);
        let p = f.view.prefs();
        assert_eq!(p, prefs::normalize(&blob));
        assert_eq!(p.center_strength, 0.6);
        assert_eq!(p.repel_strength, 6.0);
        assert_eq!(p.link_distance, 48.0);
        assert_eq!(p.local_depth, 6);
        assert!(!p.show_tags);
        assert_eq!(p.color_groups.len(), 1);
        assert_eq!(t::info(&f.view).zoom, 6.0, "the saved zoom is clamped");
        // whatever the panel saves is a fixed point of normalize
        f.view
            .panel()
            .scale("linkStrength")
            .expect("slider")
            .emit_by_name::<bool>("change-value", &[&gtk::ScrollType::Jump, &0.5f64]);
        for saved in saved(&f) {
            assert_eq!(prefs::normalize(&saved).to_value(), saved);
            assert!(saved.get("unknown").is_none());
        }
    });
}

#[test]
fn preferences_changed_elsewhere_update_the_panel_without_being_saved_again() {
    run(|| {
        let f = Fixture::new();
        let saves = f.store.saves().len();
        let mut p = f.view.prefs();
        p.show_tags = true;
        p.link_distance = 240.0;
        f.view.apply_prefs(&p.to_value());
        assert!(f
            .view
            .panel()
            .switch("showTags")
            .expect("switch")
            .is_active());
        assert_eq!(
            f.view
                .panel()
                .scale("linkDistance")
                .expect("slider")
                .value(),
            240.0
        );
        assert!(f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "#topic")));
        p.search = "path:hub".into();
        f.view.apply_prefs(&p.to_value());
        assert_eq!(f.view.panel().search_entry().text(), "path:hub");
        assert!(
            f.wait_ids(5_000, |ids| ids.iter().any(|i| i == "hub/Hub.md")
                && !ids.iter().any(|i| i == "a.md"))
        );
        t::pump(500);
        assert_eq!(f.store.saves().len(), saves, "applying is not saving");
        assert!(
            f.seen().settings.is_empty(),
            "and no settings-changed signal"
        );
    });
}

#[test]
fn a_failing_store_is_reported_and_the_session_keeps_its_changes() {
    run(|| {
        let f = Fixture::new();
        f.store.set_failing(true);
        f.view.panel().switch("showTags").expect("switch").toggle();
        assert!(t::wait_for(3_000, || f.view.save_failed()));
        assert!(!f.seen().errors.is_empty());
        assert!(f.view.prefs().show_tags, "the change stays in memory");
        f.store.set_failing(false);
    });
}

#[test]
fn flush_writes_pending_changes_immediately() {
    run(|| {
        let f = Fixture::new();
        f.view.panel().switch("showCode").expect("switch").toggle();
        assert!(
            f.store.saves().is_empty()
                || f.store
                    .saves()
                    .last()
                    .map(|s| s["showCode"] != json!(false))
                    .unwrap_or(true)
        );
        f.view.flush_prefs();
        assert_eq!(
            f.store.saves().last().expect("saved")["showCode"],
            json!(false)
        );
    });
}

#[test]
fn theme_font_and_accent_apply_to_the_canvas() {
    run(|| {
        let f = Fixture::new();
        let dark = impro_core::themes::get_theme(Some("impro-dark"));
        let light = impro_core::themes::get_theme(Some("impro-light"));
        f.view.set_theme(&dark.ui, &dark.ui["accent"]);
        f.settle();
        assert!(t::palette_dark(&f.view));
        assert_eq!(t::background_hex(&f.view), dark.ui["bg"].to_lowercase());
        f.view.set_theme(&light.ui, "#e5484d");
        assert!(t::wait_for(3_000, || !t::palette_dark(&f.view)));
        f.settle();
        assert_eq!(t::background_hex(&f.view), light.ui["bg"].to_lowercase());
        assert_eq!(t::accent_hex(&f.view), "#e5484d");
        // hovering uses the accent
        let (x, y, _) = f.screen("a.md");
        t::motion(&f.view, x, y);
        f.settle();
        assert_eq!(t::node_look(&f.view, "a.md").expect("node").1, "#e5484d");
        f.view.set_font(Some("monospace"));
        f.view.canvas().queue_draw();
        t::pump(100);
        assert!(t::label_cache_len(&f.view) > 0);
        // the panel follows the theme through the style sheet
        let css = impro_graph::css(&light.ui);
        assert!(css.contains(&format!("@define-color impro_graph_bg {};", light.ui["bg"])));
    });
}

#[test]
fn replay_reveals_the_history_in_order_and_ends_with_the_full_graph() {
    run(|| {
        let project = tempfile::tempdir().expect("dir");
        for i in 0..30 {
            write(
                project.path(),
                &format!("n{i:02}.md"),
                &format!("[[n{:02}]]", (i + 1) % 30),
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let f = Fixture::in_dir(project, json!({}), true);
        let full = f.view.summary();
        f.view.replay_history();
        assert!(f.view.replaying());
        assert!(f.view.panel().replay_visible());
        // the reveal rate is clamp(items/12, 8, 72) per second: 60 items -> 8/s
        t::pump(250);
        let early = t::info(&f.view).nodes;
        assert!(early < full.nodes, "{early} of {}", full.nodes);
        let mut last = early;
        assert!(
            t::wait_for(15_000, || {
                let n = t::info(&f.view).nodes;
                assert!(n + 1 >= last, "files only appear: {last} -> {n}");
                last = last.max(n);
                !f.view.replaying()
            }),
            "replay finishes"
        );
        assert!(t::wait_for(5_000, || t::info(&f.view).nodes == full.nodes));
        // not available in the local graph
        f.view.open_local(&f.path("n00.md"));
        f.view.replay_history();
        assert!(!f.view.replaying());
    });
}

#[test]
fn shutdown_is_idempotent_and_stops_everything() {
    run(|| {
        let f = Fixture::new();
        let stop = f.view.dock_shutdown();
        f.view.panel().switch("showTags").expect("switch").toggle();
        stop();
        f.view.shutdown();
        let saves = f.store.saves().len();
        assert!(saves >= 1, "pending preferences are written at shutdown");
        t::pump(300);
        // calls after shutdown are harmless
        f.view.rebuild();
        f.view.file_saved(&f.path("a.md"));
        f.view.set_active_file(None);
        t::pump(100);
        assert!(!f.view.frame_stats().animating);
    });
}

#[test]
fn the_default_graph_type_matches_the_setting_values() {
    run(|| {
        for t in GraphType::ALL {
            assert!(impro_core::settings::choices()["graph_default_type"]
                .iter()
                .any(|c| c == t.as_str()));
        }
        let d = GraphPrefs::default();
        let v: Value = d.to_value();
        assert_eq!(v["scale"], json!(1));
    });
}
