//! Preferences normalisation on randomly generated JSON.
// `u64::is_multiple_of` needs Rust 1.90; the workspace minimum is 1.88.
#![allow(clippy::manual_is_multiple_of)]

mod common;

use impro_graph::layout::Rng;
use impro_graph::prefs::{normalize, normalize_value, GraphPrefs, LIMITS};
use serde_json::{json, Map, Value};

const KEYS: &[&str] = &[
    "search",
    "showTags",
    "showAttachments",
    "hideUnresolved",
    "showOrphans",
    "showCode",
    "showArrow",
    "textFadeMultiplier",
    "nodeSizeMultiplier",
    "lineSizeMultiplier",
    "centerStrength",
    "repelStrength",
    "linkStrength",
    "linkDistance",
    "colorGroups",
    "collapse-filter",
    "collapse-display",
    "collapse-forces",
    "collapse-color-groups",
    "scale",
    "localDepth",
    "localIncoming",
    "localOutgoing",
    "close",
    "reduceMotion",
    "unknownKey",
    "other",
];

fn random_value(rng: &mut Rng, depth: u32) -> Value {
    match rng.next_u64() % 9 {
        0 => Value::Null,
        1 => Value::Bool(rng.next_u64() % 2 == 0),
        2 => json!((rng.next_u64() % 40) as i64 - 10),
        3 => json!((rng.unit() as f64 - 0.3) * 1000.0),
        4 => json!(["#a1B2c3", "#12", "text", ""][(rng.next_u64() % 4) as usize]),
        5 if depth < 2 => {
            let n = (rng.next_u64() % 16) as usize;
            Value::Array((0..n).map(|_| random_rule(rng)).collect())
        }
        6 if depth < 2 => random_object(rng, depth + 1),
        7 => json!(rng.unit() as f64 * 8.0),
        _ => json!("query"),
    }
}

fn random_rule(rng: &mut Rng) -> Value {
    let color = match rng.next_u64() % 4 {
        0 => json!("#ABCDEF"),
        1 => json!({"rgb": (rng.next_u64() % 20_000_000) as i64, "a": rng.unit()}),
        2 => json!({"rgb": 1.5, "a": 0.5}),
        _ => json!(42),
    };
    json!({"query": if rng.next_u64() % 5 == 0 { json!(7) } else { json!("tag:x") }, "color": color})
}

fn random_object(rng: &mut Rng, depth: u32) -> Value {
    let mut m = Map::new();
    for _ in 0..(rng.next_u64() % 20) {
        let k = KEYS[(rng.next_u64() % KEYS.len() as u64) as usize];
        m.insert(k.to_string(), random_value(rng, depth));
    }
    Value::Object(m)
}

#[test]
fn normalisation_is_idempotent_bounded_and_closed() {
    let mut rng = Rng::new(31);
    for _ in 0..5000 {
        let input = if rng.next_u64() % 10 == 0 {
            random_value(&mut rng, 0)
        } else {
            random_object(&mut rng, 0)
        };
        let once = normalize(&input);
        let blob = once.to_value();
        assert_eq!(normalize(&blob), once);
        assert_eq!(normalize_value(&blob), blob);
        let obj = blob.as_object().expect("object");
        assert_eq!(obj.len(), 25, "exactly the known keys survive");
        for (key, lo, hi) in LIMITS {
            let v = obj[key].as_f64().expect("number");
            assert!(v >= lo && v <= hi, "{key}={v}");
        }
        assert!(once.color_groups.len() <= 12);
        assert!(once.local_depth >= 1 && once.local_depth <= 6);
        assert_eq!(once.forces().repel, (-120.0 * once.repel_strength) as f32);
    }
    assert_eq!(normalize(&Value::Null), GraphPrefs::default());
}

#[test]
fn hostile_preferences_match_the_frozen_reference() {
    common::golden::check_prefs();
}
