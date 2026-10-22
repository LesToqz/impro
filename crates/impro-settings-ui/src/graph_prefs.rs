//! Graph-view preferences shown on the Graph view page: defaults, limits, control
//! steps and validation. The core settings store keeps the `graph` blob opaque, so
//! this crate validates it itself (same rules as the graph view).
use serde_json::{json, Map, Value};

/// Every known graph key with its default, in the original order.
pub fn graph_defaults() -> Map<String, Value> {
    let defaults = json!({
        "search": "",
        "showTags": false,
        "showAttachments": false,
        "hideUnresolved": false,
        "showOrphans": true,
        "showCode": true,
        "showArrow": true,
        "textFadeMultiplier": 0.2,
        "nodeSizeMultiplier": 1.15,
        "lineSizeMultiplier": 1.25,
        "centerStrength": 0.12,
        "repelStrength": 6,
        "linkStrength": 0.35,
        "linkDistance": 160,
        "colorGroups": [],
        "collapse-filter": false,
        "collapse-display": true,
        "collapse-forces": true,
        "collapse-color-groups": true,
        "scale": 1,
        "localDepth": 2,
        "localIncoming": true,
        "localOutgoing": true,
        "close": false,
        "reduceMotion": false
    });
    match defaults {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

/// `(low, high)` of a numeric graph key.
pub fn graph_limits(key: &str) -> Option<(f64, f64)> {
    Some(match key {
        "textFadeMultiplier" => (-1.0, 2.0),
        "nodeSizeMultiplier" => (0.5, 3.0),
        "lineSizeMultiplier" => (0.5, 4.0),
        "centerStrength" => (0.0, 0.6),
        "repelStrength" => (0.0, 24.0),
        "linkStrength" => (0.0, 1.5),
        "linkDistance" => (48.0, 480.0),
        "scale" => (0.04, 6.0),
        "localDepth" => (1.0, 6.0),
        _ => return None,
    })
}

/// Control step of a numeric graph key (saved values are not snapped to it).
pub fn graph_step(key: &str) -> Option<f64> {
    Some(match key {
        "textFadeMultiplier" => 0.1,
        "nodeSizeMultiplier" => 0.05,
        "lineSizeMultiplier" => 0.25,
        "centerStrength" => 0.02,
        "repelStrength" => 0.5,
        "linkStrength" => 0.05,
        "linkDistance" => 8.0,
        "localDepth" => 1.0,
        _ => return None,
    })
}

fn number(value: f64, integral: bool) -> Value {
    if integral && value.fract() == 0.0 && value.abs() < 9.0e15 {
        Value::from(value as i64)
    } else {
        serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number)
    }
}

fn valid_hex(color: &str) -> bool {
    color.len() == 7 && color.starts_with('#') && color[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn color_group(group: &Value) -> Option<Value> {
    let group = group.as_object()?;
    let query = group.get("query")?.as_str()?;
    let color = group.get("color")?;
    let color = if let Some(text) = color.as_str().filter(|c| valid_hex(c)) {
        Value::from(text.to_ascii_lowercase())
    } else if let Some(object) = color.as_object() {
        let rgb = object.get("rgb").filter(|v| v.is_i64() || v.is_u64())?;
        let alpha = object.get("a")?.as_f64().filter(|a| a.is_finite())?;
        if !(0.0..=0xff_ffff as f64).contains(&rgb.as_f64()?) || !(0.0..=1.0).contains(&alpha) {
            return None;
        }
        json!({ "a": object["a"], "rgb": rgb })
    } else {
        return None;
    };
    Some(json!({ "query": query, "color": color }))
}

/// Independent, validated graph preferences; unknown keys are dropped.
pub fn normalize_graph(value: &Value) -> Map<String, Value> {
    let empty = Map::new();
    let value = value.as_object().unwrap_or(&empty);
    let mut result = Map::new();
    for (key, default) in graph_defaults() {
        let raw = value.get(&key).unwrap_or(&default);
        let normalized = if default.is_boolean() {
            if raw.is_boolean() {
                raw.clone()
            } else {
                default.clone()
            }
        } else if let Some((low, high)) = graph_limits(&key) {
            let value = raw
                .as_f64()
                .filter(|v| v.is_finite())
                .or_else(|| default.as_f64())
                .unwrap_or(low);
            let mut clamped = value.max(low).min(high);
            if key == "localDepth" {
                clamped = clamped.round_ties_even();
            }
            number(clamped, default.is_i64() || default.is_u64())
        } else if key == "colorGroups" {
            Value::Array(
                raw.as_array()
                    .map(|groups| groups.iter().filter_map(color_group).take(12).collect())
                    .unwrap_or_default(),
            )
        } else if raw.is_string() {
            raw.clone()
        } else {
            default.clone()
        };
        result.insert(key, normalized);
    }
    result
}

/// Equality that ignores the integer/float spelling of numbers.
pub fn graph_values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}
