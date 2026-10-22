//! Page palettes derived from the same roles as the native editor.
use impro_core::themes::{legible, luminance, Theme, UiColors};
use serde_json::{json, Value};

fn role<'a>(ui: &'a UiColors, key: &str, fallback: &'a str) -> &'a str {
    ui.get(key)
        .map(String::as_str)
        .filter(|v| !v.is_empty())
        .unwrap_or(fallback)
}
fn rgb(s: &str) -> [f64; 3] {
    let n = u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0);
    [
        ((n >> 16) & 255) as f64 / 255.,
        ((n >> 8) & 255) as f64 / 255.,
        (n & 255) as f64 / 255.,
    ]
}
fn hsv(s: &str) -> (f64, f64, f64) {
    let [r, g, b] = rgb(s);
    let v = r.max(g).max(b);
    let lo = r.min(g).min(b);
    let d = v - lo;
    let h = if d == 0. {
        0.
    } else if v == r {
        ((g - b) / d).rem_euclid(6.) / 6.
    } else if v == g {
        ((b - r) / d + 2.) / 6.
    } else {
        ((r - g) / d + 4.) / 6.
    };
    (h, if v == 0. { 0. } else { d / v }, v)
}
fn from_hsv(h: f64, s: f64, v: f64) -> String {
    let h = h.rem_euclid(1.) * 6.;
    let f = h - h.floor();
    let p = v * (1. - s);
    let q = v * (1. - s * f);
    let t = v * (1. - s * (1. - f));
    let (r, g, b) = match h as u32 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    format!(
        "#{:02x}{:02x}{:02x}",
        (r * 255.).round_ties_even() as u8,
        (g * 255.).round_ties_even() as u8,
        (b * 255.).round_ties_even() as u8
    )
}
fn family(ui: &UiColors, key: &str, fallback: &str) -> String {
    match ui.get(key).filter(|s| !s.is_empty()) {
        Some(s) => format!("{}, {fallback}", json!(s)),
        None => fallback.into(),
    }
}
/// Eight readable branch lanes, reserving the accent for the default branch.
pub fn git_colors(ui: &UiColors, accent: &str) -> Value {
    let bg = role(ui, "bg", "#1e1e1e");
    let mut branches: Vec<String> = Vec::new();
    for base in [
        "#5588ee", "#a477e8", "#48b86b", "#e79a39", "#e45c68", "#33b4af", "#d56fbd", "#c5ad35",
    ] {
        let (h, s, v) = hsv(base);
        let mut candidate = String::new();
        for step in 0..36 {
            candidate = legible(&from_hsv(h + step as f64 / 36., s, v), bg, 3.);
            if std::iter::once(accent)
                .chain(branches.iter().map(String::as_str))
                .all(|other| {
                    rgb(&candidate)
                        .iter()
                        .zip(rgb(other))
                        .map(|(a, b)| (a - b).powi(2))
                        .sum::<f64>()
                        .sqrt()
                        >= 0.22
                })
            {
                break;
            }
        }
        branches.push(candidate);
    }
    json!({"background":bg,"surface":role(ui,"sidebar_bg",bg),"surface_alt":role(ui,"surface_alt",bg),"border":role(ui,"border","#555555"),"text":role(ui,"foreground","#cccccc"),"text_muted":legible(role(ui,"muted","#888888"),bg,3.),"accent":accent,"accent_fg":role(ui,"accent_fg","#ffffff"),"head":accent,"branch_colors":branches,"mono_font":family(ui,"editor_font","monospace"),"ui_font":family(ui,"ui_font","system-ui, sans-serif"),"dark":luminance(bg)<0.3})
}
/// Commit palette, including the host's exact syntax colours and live font size.
pub fn commit_colors(theme: &Theme, ui: &UiColors, accent: &str, size: i32) -> Value {
    let c = git_colors(ui, accent);
    let bg = c["background"].as_str().unwrap_or("#1e1e1e");
    let add = role(ui, "git_new", "#48b86b");
    let mut del = role(ui, "git_conflict", role(ui, "danger", "#e45c68"));
    let [r, g, b] = rgb(del);
    if r <= g.max(b) {
        del = role(ui, "danger", "#e45c68");
    }
    let tint = |s: &str| {
        let [r, g, b] = rgb(s);
        format!(
            "rgba({}, {}, {}, 0.17)",
            (r * 255.).round_ties_even(),
            (g * 255.).round_ties_even(),
            (b * 255.).round_ties_even()
        )
    };
    let syntax: serde_json::Map<String, Value> = theme
        .syntax
        .iter()
        .map(|(k, v)| (k.clone(), json!(v.foreground)))
        .collect();
    json!({"bg":bg,"fg":role(ui,"editor_fg",c["text"].as_str().unwrap_or("#cccccc")),"muted":c["text_muted"],"surface":c["surface"],"surface2":c["surface_alt"],"border":c["border"],"hover":role(ui,"hover",c["surface"].as_str().unwrap_or(bg)),"accent":accent,"accent_fg":c["accent_fg"],"add":add,"del":del,"add_bg":tint(add),"del_bg":tint(del),"mono":c["mono_font"],"ui":c["ui_font"],"size":size,"dark":c["dark"],"syntax":syntax})
}
