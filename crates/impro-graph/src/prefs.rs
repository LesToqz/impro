//! The persisted graph blob: defaults, bounds and normalisation.
//!
//! [`normalize`] accepts any JSON value and returns valid preferences:
//! unknown keys vanish, wrong types fall back to the default, numbers clamp
//! to their range, hop counts round to whole numbers and at most twelve colour
//! rules survive. `normalize(normalize(x).to_value()) == normalize(x)`.

use serde_json::{Map, Number, Value};

use crate::layout::ForceParams;

/// Most colour rules kept.
pub const MAX_COLOR_RULES: usize = 12;

/// `(key, min, max)` of every bounded number.
pub const LIMITS: [(&str, f64, f64); 9] = [
    ("textFadeMultiplier", -1.0, 2.0),
    ("nodeSizeMultiplier", 0.5, 3.0),
    ("lineSizeMultiplier", 0.5, 4.0),
    ("centerStrength", 0.0, 0.6),
    ("repelStrength", 0.0, 24.0),
    ("linkStrength", 0.0, 1.5),
    ("linkDistance", 48.0, 480.0),
    ("scale", 0.04, 6.0),
    ("localDepth", 1.0, 6.0),
];

/// Control step of each slider (`None` = continuous). Saved values are never
/// snapped to the step.
pub const STEPS: [(&str, Option<f64>); 9] = [
    ("textFadeMultiplier", Some(0.1)),
    ("nodeSizeMultiplier", Some(0.05)),
    ("lineSizeMultiplier", Some(0.25)),
    ("centerStrength", Some(0.02)),
    ("repelStrength", Some(0.5)),
    ("linkStrength", Some(0.05)),
    ("linkDistance", Some(8.0)),
    ("scale", None),
    ("localDepth", Some(1.0)),
];

pub fn limits(key: &str) -> Option<(f64, f64)> {
    LIMITS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, lo, hi)| (*lo, *hi))
}

/// Rule colour: `#rrggbb` text, or the legacy packed form `{rgb, a}`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuleColor {
    pub rgb: u32,
    pub alpha: f64,
    /// Persisted as `{"a":…, "rgb":…}` instead of a hex string.
    pub packed: bool,
}

impl RuleColor {
    pub fn hex(rgb: u32) -> Self {
        RuleColor {
            rgb,
            alpha: 1.0,
            packed: false,
        }
    }

    pub fn to_hex_string(&self) -> String {
        format!("#{:06x}", self.rgb & 0xff_ffff)
    }

    /// `(r, g, b, a)` with channels in `0..=255` and alpha in `0..=1`.
    pub fn rgba(&self) -> (u8, u8, u8, f64) {
        (
            (self.rgb >> 16 & 255) as u8,
            (self.rgb >> 8 & 255) as u8,
            (self.rgb & 255) as u8,
            self.alpha,
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColorRule {
    pub query: String,
    pub color: RuleColor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphPrefs {
    pub search: String,
    pub show_tags: bool,
    pub show_attachments: bool,
    pub hide_unresolved: bool,
    pub show_orphans: bool,
    pub show_code: bool,
    pub show_arrow: bool,
    pub text_fade_multiplier: f64,
    pub node_size_multiplier: f64,
    pub line_size_multiplier: f64,
    pub center_strength: f64,
    pub repel_strength: f64,
    pub link_strength: f64,
    pub link_distance: f64,
    pub color_groups: Vec<ColorRule>,
    pub collapse_filter: bool,
    pub collapse_display: bool,
    pub collapse_forces: bool,
    pub collapse_color_groups: bool,
    pub scale: f64,
    pub local_depth: u32,
    pub local_incoming: bool,
    pub local_outgoing: bool,
    pub close: bool,
    pub reduce_motion: bool,
}

impl Default for GraphPrefs {
    fn default() -> Self {
        GraphPrefs {
            search: String::new(),
            show_tags: false,
            show_attachments: false,
            hide_unresolved: false,
            show_orphans: true,
            show_code: true,
            show_arrow: true,
            text_fade_multiplier: 0.2,
            node_size_multiplier: 1.15,
            line_size_multiplier: 1.25,
            center_strength: 0.12,
            repel_strength: 6.0,
            link_strength: 0.35,
            link_distance: 160.0,
            color_groups: Vec::new(),
            collapse_filter: false,
            collapse_display: true,
            collapse_forces: true,
            collapse_color_groups: true,
            scale: 1.0,
            local_depth: 2,
            local_incoming: true,
            local_outgoing: true,
            close: false,
            reduce_motion: false,
        }
    }
}

/// A finite number from JSON, clamped; integers too large for `f64` clamp by
/// sign instead of falling back to the default.
fn number(v: Option<&Value>, key: &str, default: f64) -> f64 {
    let (lo, hi) = limits(key).unwrap_or((f64::MIN, f64::MAX));
    let n = match v {
        Some(Value::Number(n)) => n,
        _ => return default,
    };
    match n.as_f64() {
        Some(f) if f.is_finite() => f.clamp(lo, hi),
        _ => {
            let text = n.to_string();
            if text.contains(['.', 'e', 'E']) {
                default
            } else if text.starts_with('-') {
                lo
            } else {
                hi
            }
        }
    }
}

fn boolean(v: Option<&Value>, default: bool) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        _ => default,
    }
}

fn is_hex_color(s: &str) -> Option<u32> {
    let rest = s.strip_prefix('#')?;
    if rest.len() != 6 || !rest.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(rest, 16).ok()
}

fn rule_color(v: Option<&Value>) -> Option<RuleColor> {
    match v? {
        Value::String(s) => is_hex_color(s).map(RuleColor::hex),
        Value::Object(o) => {
            let rgb = match o.get("rgb")? {
                Value::Number(n) if n.is_i64() || n.is_u64() => n.as_u64()?,
                _ => return None,
            };
            if rgb > 0xff_ffff {
                return None;
            }
            let alpha = match o.get("a")? {
                Value::Number(n) => n.as_f64().filter(|a| a.is_finite())?,
                _ => return None,
            };
            if !(0.0..=1.0).contains(&alpha) {
                return None;
            }
            Some(RuleColor {
                rgb: rgb as u32,
                alpha,
                packed: true,
            })
        }
        _ => None,
    }
}

fn rules(v: Option<&Value>) -> Vec<ColorRule> {
    let list = match v {
        Some(Value::Array(a)) => a,
        _ => return Vec::new(),
    };
    let mut out = Vec::new();
    for item in list {
        let obj = match item {
            Value::Object(o) => o,
            _ => continue,
        };
        let query = match obj.get("query") {
            Some(Value::String(q)) => q.clone(),
            _ => continue,
        };
        let color = match rule_color(obj.get("color")) {
            Some(c) => c,
            None => continue,
        };
        out.push(ColorRule { query, color });
        if out.len() == MAX_COLOR_RULES {
            break;
        }
    }
    out
}

/// Validates any JSON value into preferences.
pub fn normalize(value: &Value) -> GraphPrefs {
    let d = GraphPrefs::default();
    let empty = Map::new();
    let o = value.as_object().unwrap_or(&empty);
    let get = |k: &str| o.get(k);
    GraphPrefs {
        search: match get("search") {
            Some(Value::String(s)) => s.clone(),
            _ => d.search,
        },
        show_tags: boolean(get("showTags"), d.show_tags),
        show_attachments: boolean(get("showAttachments"), d.show_attachments),
        hide_unresolved: boolean(get("hideUnresolved"), d.hide_unresolved),
        show_orphans: boolean(get("showOrphans"), d.show_orphans),
        show_code: boolean(get("showCode"), d.show_code),
        show_arrow: boolean(get("showArrow"), d.show_arrow),
        text_fade_multiplier: number(get("textFadeMultiplier"), "textFadeMultiplier", d.text_fade_multiplier),
        node_size_multiplier: number(get("nodeSizeMultiplier"), "nodeSizeMultiplier", d.node_size_multiplier),
        line_size_multiplier: number(get("lineSizeMultiplier"), "lineSizeMultiplier", d.line_size_multiplier),
        center_strength: number(get("centerStrength"), "centerStrength", d.center_strength),
        repel_strength: number(get("repelStrength"), "repelStrength", d.repel_strength),
        link_strength: number(get("linkStrength"), "linkStrength", d.link_strength),
        link_distance: number(get("linkDistance"), "linkDistance", d.link_distance),
        color_groups: rules(get("colorGroups")),
        collapse_filter: boolean(get("collapse-filter"), d.collapse_filter),
        collapse_display: boolean(get("collapse-display"), d.collapse_display),
        collapse_forces: boolean(get("collapse-forces"), d.collapse_forces),
        collapse_color_groups: boolean(get("collapse-color-groups"), d.collapse_color_groups),
        scale: number(get("scale"), "scale", d.scale),
        local_depth: number(get("localDepth"), "localDepth", d.local_depth as f64).round_ties_even() as u32,
        local_incoming: boolean(get("localIncoming"), d.local_incoming),
        local_outgoing: boolean(get("localOutgoing"), d.local_outgoing),
        close: boolean(get("close"), d.close),
        reduce_motion: boolean(get("reduceMotion"), d.reduce_motion),
    }
}

/// Normalises a JSON blob straight to its persisted form.
pub fn normalize_value(value: &Value) -> Value {
    normalize(value).to_value()
}

fn num(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        Value::from(v as i64)
    } else {
        Number::from_f64(v).map_or(Value::Null, Value::Number)
    }
}

impl GraphPrefs {
    /// The blob to persist (keys as in the saved file).
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("search".into(), Value::from(self.search.clone()));
        m.insert("showTags".into(), self.show_tags.into());
        m.insert("showAttachments".into(), self.show_attachments.into());
        m.insert("hideUnresolved".into(), self.hide_unresolved.into());
        m.insert("showOrphans".into(), self.show_orphans.into());
        m.insert("showCode".into(), self.show_code.into());
        m.insert("showArrow".into(), self.show_arrow.into());
        m.insert("textFadeMultiplier".into(), num(self.text_fade_multiplier));
        m.insert("nodeSizeMultiplier".into(), num(self.node_size_multiplier));
        m.insert("lineSizeMultiplier".into(), num(self.line_size_multiplier));
        m.insert("centerStrength".into(), num(self.center_strength));
        m.insert("repelStrength".into(), num(self.repel_strength));
        m.insert("linkStrength".into(), num(self.link_strength));
        m.insert("linkDistance".into(), num(self.link_distance));
        let groups = self
            .color_groups
            .iter()
            .map(|g| {
                let mut o = Map::new();
                o.insert("query".into(), Value::from(g.query.clone()));
                let color = if g.color.packed {
                    let mut c = Map::new();
                    c.insert("a".into(), num(g.color.alpha));
                    c.insert("rgb".into(), Value::from(g.color.rgb));
                    Value::Object(c)
                } else {
                    Value::from(g.color.to_hex_string())
                };
                o.insert("color".into(), color);
                Value::Object(o)
            })
            .collect();
        m.insert("colorGroups".into(), Value::Array(groups));
        m.insert("collapse-filter".into(), self.collapse_filter.into());
        m.insert("collapse-display".into(), self.collapse_display.into());
        m.insert("collapse-forces".into(), self.collapse_forces.into());
        m.insert("collapse-color-groups".into(), self.collapse_color_groups.into());
        m.insert("scale".into(), num(self.scale));
        m.insert("localDepth".into(), Value::from(self.local_depth));
        m.insert("localIncoming".into(), self.local_incoming.into());
        m.insert("localOutgoing".into(), self.local_outgoing.into());
        m.insert("close".into(), self.close.into());
        m.insert("reduceMotion".into(), self.reduce_motion.into());
        Value::Object(m)
    }

    /// Force-simulation parameters: centre and link strengths and the
    /// preferred distance are used directly, repulsion is `-120 × control`.
    pub fn forces(&self) -> ForceParams {
        ForceParams::from_controls(
            self.center_strength,
            self.link_strength,
            self.link_distance,
            self.repel_strength,
        )
    }

    /// "Reset graph preferences": filters, display, forces and colour rules go
    /// back to defaults (traversal too in local mode); search text, zoom,
    /// section visibility and the accessibility switches stay.
    pub fn reset(&mut self, local_mode: bool) {
        let d = GraphPrefs::default();
        self.show_tags = d.show_tags;
        self.show_attachments = d.show_attachments;
        self.hide_unresolved = d.hide_unresolved;
        self.show_orphans = d.show_orphans;
        self.show_code = d.show_code;
        self.show_arrow = d.show_arrow;
        self.text_fade_multiplier = d.text_fade_multiplier;
        self.node_size_multiplier = d.node_size_multiplier;
        self.line_size_multiplier = d.line_size_multiplier;
        self.center_strength = d.center_strength;
        self.repel_strength = d.repel_strength;
        self.link_strength = d.link_strength;
        self.link_distance = d.link_distance;
        self.color_groups.clear();
        if local_mode {
            self.local_depth = d.local_depth;
            self.local_incoming = d.local_incoming;
            self.local_outgoing = d.local_outgoing;
        }
    }

    /// Appends a rule with the next generated colour; false when full.
    pub fn add_color_rule(&mut self, query: &str, dark_theme: bool) -> bool {
        if self.color_groups.len() >= MAX_COLOR_RULES {
            return false;
        }
        let rgb = new_rule_rgb(self.color_groups.len(), dark_theme);
        self.color_groups.push(ColorRule {
            query: query.to_string(),
            color: RuleColor::hex(rgb),
        });
        true
    }
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> u32 {
    let (s, l) = (s / 100.0, l / 100.0);
    let k = |n: f64| (n + h / 30.0) % 12.0;
    let a = s * l.min(1.0 - l);
    let f = |n: f64| l - a * (-1.0f64).max((k(n) - 3.0).min((9.0 - k(n)).min(1.0)));
    let ch = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u32;
    ch(f(0.0)) << 16 | ch(f(8.0)) << 8 | ch(f(4.0))
}

/// Packed `0xRRGGBB` of the colour for the `index`-th rule: hue starts at 210°
/// and advances 137.5° per rule, saturation 65 %, lightness 64 % (dark) or
/// 42 % (light).
pub fn new_rule_rgb(index: usize, dark_theme: bool) -> u32 {
    let hue = (210.0 + 137.5 * index as f64) % 360.0;
    hsl_to_rgb(hue, 65.0, if dark_theme { 64.0 } else { 42.0 })
}

/// `#rrggbb` text of [`new_rule_rgb`].
pub fn new_rule_color(index: usize, dark_theme: bool) -> String {
    format!("#{:06x}", new_rule_rgb(index, dark_theme))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../tests/graph_params.json")).expect("fixture parses")
    }

    #[test]
    fn defaults_match_the_contract_fixture() {
        let fx = fixture();
        assert_eq!(GraphPrefs::default().to_value(), fx["defaults"]);
        assert_eq!(normalize(&Value::Null), GraphPrefs::default());
        assert_eq!(normalize(&json!({})).to_value(), fx["defaults"]);
    }

    #[test]
    fn limits_and_steps_match_the_contract_fixture() {
        let fx = fixture();
        for (key, lo, hi) in LIMITS {
            let pair = &fx["limits"][key];
            assert_eq!(pair[0].as_f64(), Some(lo), "{key}");
            assert_eq!(pair[1].as_f64(), Some(hi), "{key}");
        }
        assert_eq!(fx["limits"].as_object().map(|o| o.len()), Some(LIMITS.len()));
        for (key, step) in STEPS {
            assert_eq!(fx["steps"][key].as_f64(), step, "{key}");
        }
    }

    #[test]
    fn numbers_clamp_and_wrong_types_use_defaults() {
        let p = normalize(&json!({
            "centerStrength": 99, "repelStrength": -5, "linkDistance": "far",
            "scale": true, "textFadeMultiplier": null,
            "lineSizeMultiplier": 0.1, "linkStrength": 1.5
        }));
        assert_eq!(p.center_strength, 0.6);
        assert_eq!(p.repel_strength, 0.0);
        assert_eq!(p.link_distance, 160.0);
        assert_eq!(p.scale, 1.0);
        assert_eq!(p.text_fade_multiplier, 0.2);
        assert_eq!(p.line_size_multiplier, 0.5);
        assert_eq!(p.link_strength, 1.5);
        // out-of-range integers clamp by sign
        let big = serde_json::from_str::<Value>("{\"linkDistance\": 1000000000000000000000000000000, \"centerStrength\": -1000000000000000000000000000000}").unwrap();
        let p = normalize(&big);
        assert_eq!(p.link_distance, 480.0);
        assert_eq!(p.center_strength, 0.0);
    }

    #[test]
    fn hop_counts_round_half_to_even() {
        for (input, expect) in [(0, 1), (1, 1), (2, 2), (6, 6), (9, 6)] {
            assert_eq!(normalize(&json!({"localDepth": input})).local_depth, expect);
        }
        assert_eq!(normalize(&json!({"localDepth": 2.5})).local_depth, 2);
        assert_eq!(normalize(&json!({"localDepth": 3.5})).local_depth, 4);
        assert_eq!(normalize(&json!({"localDepth": 1.4})).local_depth, 1);
        assert_eq!(normalize(&json!({"localDepth": 5.6})).local_depth, 6);
        // other numbers stay continuous
        assert_eq!(normalize(&json!({"linkDistance": 161.3})).link_distance, 161.3);
    }

    #[test]
    fn unknown_keys_drop_and_booleans_need_booleans() {
        let p = normalize(&json!({"mystery": 1, "showTags": 1, "showCode": "yes", "close": true, "search": 3}));
        assert!(!p.show_tags);
        assert!(p.show_code);
        assert!(p.close);
        assert_eq!(p.search, "");
        let v = p.to_value();
        assert!(v.get("mystery").is_none());
        assert_eq!(v.as_object().map(|o| o.len()), Some(25));
    }

    #[test]
    fn colour_rules_validate_and_cap() {
        let p = normalize(&json!({"colorGroups": [
            {"query": "tag:a", "color": "#ABCDEF"},
            {"query": "x", "color": "#abc"},
            {"query": "y", "color": {"rgb": 16777215, "a": 0.5}},
            {"query": "z", "color": {"rgb": 16777216, "a": 0.5}},
            {"query": "w", "color": {"rgb": 1.0, "a": 0.5}},
            {"query": "v", "color": {"rgb": 5, "a": 2}},
            {"query": 3, "color": "#000000"},
            "junk",
            {"query": "ok", "color": {"rgb": 255, "a": 1}}
        ]}));
        let q: Vec<_> = p.color_groups.iter().map(|g| g.query.as_str()).collect();
        assert_eq!(q, ["tag:a", "y", "ok"]);
        assert_eq!(p.color_groups[0].color.to_hex_string(), "#abcdef");
        assert!(p.color_groups[1].color.packed);
        assert_eq!(p.color_groups[1].color.rgba(), (255, 255, 255, 0.5));
        let many: Vec<Value> = (0..20)
            .map(|i| json!({"query": format!("q{i}"), "color": "#000000"}))
            .collect();
        assert_eq!(normalize(&json!({"colorGroups": many})).color_groups.len(), 12);
        assert!(normalize(&json!({"colorGroups": "no"})).color_groups.is_empty());
    }

    #[test]
    fn normalisation_is_idempotent() {
        let messy = [
            json!(null),
            json!([1, 2]),
            json!({"localDepth": 3.5, "scale": 100, "colorGroups": [{"query": "a", "color": {"rgb": 7, "a": 0.25}}, {"query": "b", "color": "#00FF00"}], "search": "x"}),
            json!({"repelStrength": 12.25, "linkDistance": 48.0, "collapse-forces": false}),
        ];
        for m in messy {
            let once = normalize(&m);
            let twice = normalize(&once.to_value());
            assert_eq!(once, twice);
            assert_eq!(normalize_value(&once.to_value()), once.to_value());
        }
    }

    #[test]
    fn generated_rule_colours_step_around_the_hue_wheel() {
        // hue 210° at S 65 %, L 64 % / 42 %
        assert_eq!(new_rule_color(0, true), "#68a3df");
        assert_eq!(new_rule_color(0, false), "#256bb1");
        assert_ne!(new_rule_color(0, true), new_rule_color(1, true));
        let first = new_rule_rgb(0, true);
        assert!((first & 255) > (first >> 16 & 255), "blue dominates the first rule");
        let light = new_rule_rgb(0, false);
        let sum = |c: u32| (c >> 16 & 255) + (c >> 8 & 255) + (c & 255);
        assert!(sum(light) < sum(first));
        // 137.5° steps wrap around the wheel
        let hues: Vec<u32> = (0..12).map(|i| new_rule_rgb(i, true)).collect();
        let mut sorted = hues.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 12);
        let mut p = GraphPrefs::default();
        for _ in 0..12 {
            assert!(p.add_color_rule("q", true));
        }
        assert!(!p.add_color_rule("q", true));
        assert_eq!(p.color_groups[3].color.rgb, new_rule_rgb(3, true));
    }

    #[test]
    fn forces_map_linearly() {
        let f = GraphPrefs::default().forces();
        assert_eq!(f.center, 0.12);
        assert_eq!(f.link, 0.35);
        assert_eq!(f.distance, 160.0);
        assert_eq!(f.repel, -720.0);
        let mut p = GraphPrefs::default();
        p.repel_strength = 0.0;
        assert_eq!(p.forces().repel, 0.0);
        p.repel_strength = 12.0;
        assert_eq!(p.forces().repel, -1440.0);
    }

    #[test]
    fn reset_keeps_search_zoom_and_sections() {
        let mut p = normalize(&json!({
            "search": "keep", "scale": 3, "collapse-filter": true, "reduceMotion": true,
            "showTags": true, "linkDistance": 300, "localDepth": 5, "localOutgoing": false,
            "colorGroups": [{"query": "a", "color": "#000000"}]
        }));
        let before = p.clone();
        p.reset(false);
        assert_eq!(p.search, "keep");
        assert_eq!(p.scale, 3.0);
        assert!(p.collapse_filter && p.reduce_motion);
        assert!(!p.show_tags && p.color_groups.is_empty());
        assert_eq!(p.link_distance, 160.0);
        assert_eq!(p.local_depth, 5, "traversal is untouched outside local mode");
        assert!(!p.local_outgoing);
        let mut q = before;
        q.reset(true);
        assert_eq!(q.local_depth, 2);
        assert!(q.local_outgoing);
    }
}
