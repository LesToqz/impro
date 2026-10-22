//! Graph preference validation (port of the graph cases of test_settings.py) and the
//! hotkey merge helper; no display needed.
use impro_settings_ui::{
    graph_defaults, graph_limits, graph_values_equal, merge_hotkeys, normalize_graph,
    DEFAULT_HOTKEYS,
};
use serde_json::{json, Value};

#[test]
fn test_defaults_are_independent_and_not_user_tuned() {
    let graph = normalize_graph(&Value::Null);
    assert_eq!(graph["textFadeMultiplier"], 0.2);
    assert_eq!(graph["showTags"], false);
    assert_eq!(graph["scale"], 1);
    assert_eq!(graph["showCode"], true);
    assert_eq!(graph, graph_defaults());
    let mut other = normalize_graph(&Value::Null);
    other["colorGroups"]
        .as_array_mut()
        .unwrap()
        .push(json!({"query": "x", "color": "#ffffff"}));
    assert_eq!(normalize_graph(&Value::Null)["colorGroups"], json!([]));
    assert_eq!(graph_defaults().len(), 25);
}

#[test]
fn test_graph_validation() {
    let raw = json!({"search": 12, "showTags": "true", "showCode": false, "showOrphans": false,
        "localDepth": 99, "scale": -1, "centerStrength": null, "repelStrength": 100,
        "linkStrength": true, "linkDistance": -4, "unknown": 3,
        "colorGroups": [{"query": "tag:a", "color": "#ABCDEF", "junk": 1},
                        {"query": "bad", "color": "red"}, null]});
    let result = normalize_graph(&raw);
    assert_eq!(result["search"], "");
    assert_eq!(result["showTags"], false);
    assert_eq!(result["showCode"], false);
    assert_eq!(result["showOrphans"], false);
    assert_eq!(
        result["localDepth"],
        graph_limits("localDepth").unwrap().1 as i64
    );
    assert_eq!(result["repelStrength"], 24);
    assert_eq!(result["linkDistance"], 48);
    assert_eq!(result["scale"], 0.04);
    assert_eq!(result["linkStrength"], 0.35);
    assert_eq!(result["centerStrength"], graph_defaults()["centerStrength"]);
    assert_eq!(
        result["colorGroups"],
        json!([{"query": "tag:a", "color": "#abcdef"}])
    );
    assert!(!result.contains_key("unknown"));
}

#[test]
fn test_graph_group_color_forms_and_limits() {
    let groups = json!([{"query": "tag:a", "color": {"a": 1, "rgb": 16711680}},
                        {"query": "path:b", "color": "#ABCDEF"}]);
    let expected = json!([{"query": "tag:a", "color": {"a": 1, "rgb": 16711680}},
                          {"query": "path:b", "color": "#abcdef"}]);
    let result = normalize_graph(&json!({"colorGroups": groups, "close": true}));
    assert_eq!(result["colorGroups"], expected);
    assert_eq!(result["close"], true);
    for color in [
        json!({"a": 1, "rgb": true}),
        json!({"a": -1, "rgb": 1}),
        json!({"a": 1, "rgb": 0x100_0000}),
        json!({"rgb": 1}),
        json!({"a": "1", "rgb": 1}),
    ] {
        let result = normalize_graph(&json!({"colorGroups": [{"query": "x", "color": color}]}));
        assert_eq!(result["colorGroups"], json!([]), "{color}");
    }
    let many: Vec<Value> = (0..20)
        .map(|n| json!({"query": n.to_string(), "color": "#000000"}))
        .collect();
    assert_eq!(
        normalize_graph(&json!({ "colorGroups": many }))["colorGroups"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
}

#[test]
fn test_corrupted_graph_falls_back_to_defaults() {
    for corrupt in [Value::Null, json!("broken"), json!([]), json!(42)] {
        assert_eq!(normalize_graph(&corrupt), graph_defaults());
    }
    let graph = normalize_graph(&json!({"showTags": true, "unknown": 3, "scale": -10}));
    assert_eq!(graph["showTags"], true);
    assert_eq!(graph["scale"], 0.04);
}

#[test]
fn test_number_spelling_is_ignored_by_comparison() {
    assert!(graph_values_equal(&json!(6), &json!(6.0)));
    assert!(!graph_values_equal(&json!(6), &json!(7)));
    assert!(graph_values_equal(&json!("a"), &json!("a")));
}

#[test]
fn test_merge_hotkeys_keeps_commands_then_unused_defaults() {
    let commands = vec![
        (
            "Settings: Open settings".to_owned(),
            vec!["Ctrl+,".to_owned()],
        ),
        ("No keys".to_owned(), vec![]),
        (
            "Next tab".to_owned(),
            vec!["Ctrl+Tab".to_owned(), "Ctrl+PageDown".to_owned()],
        ),
    ];
    let merged = merge_hotkeys(&commands);
    assert_eq!(
        merged[0],
        ("Settings: Open settings".to_owned(), "Ctrl+,".to_owned())
    );
    assert_eq!(
        merged[1],
        ("Next tab".to_owned(), "Ctrl+Tab / Ctrl+PageDown".to_owned())
    );
    assert!(!merged.iter().any(|(l, _)| l == "Open settings"));
    assert!(!merged.iter().any(|(l, _)| l == "Next editor tab"));
    assert!(merged.iter().any(|(l, _)| l == "Save file"));
    assert_eq!(merge_hotkeys(&[]).len(), DEFAULT_HOTKEYS.len());
}
