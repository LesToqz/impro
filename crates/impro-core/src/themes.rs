//! Immutable catalogue: exact resolved palettes, terminal ANSI colours and syntax roles.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

pub type UiColors = BTreeMap<String, String>;
pub type SyntaxColors = BTreeMap<String, SyntaxStyle>;
pub const DEFAULT_THEME_ID: &str = "impro-dark";
pub const KINDS: [&str; 26] = [
    "comment",
    "doc",
    "string",
    "escape",
    "regex",
    "number",
    "keyword",
    "control",
    "operator",
    "constant",
    "builtin",
    "function",
    "class",
    "decorator",
    "variable",
    "property",
    "tag",
    "attribute",
    "selector",
    "heading",
    "strong",
    "emphasis",
    "link",
    "preproc",
    "punctuation",
    "error",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntaxStyle {
    pub foreground: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalColors {
    pub foreground: String,
    pub background: String,
    pub cursor: String,
    pub ansi: [String; 16],
}
impl TerminalColors {
    pub fn new(
        foreground: String,
        background: String,
        cursor: String,
        ansi: Vec<String>,
    ) -> Result<Self, &'static str> {
        if ansi.len() != 16
            || ansi
                .iter()
                .any(|c| !valid_color(c) || *c != c.to_ascii_lowercase())
        {
            return Err("A terminal palette requires exactly 16 #rrggbb colors");
        }
        Ok(Self {
            foreground,
            background,
            cursor,
            ansi: ansi.try_into().map_err(|_| "Invalid ANSI length")?,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub dark: bool,
    pub ui: UiColors,
    pub syntax: SyntaxColors,
    pub terminal: TerminalColors,
    pub chrome: UiColors,
}

pub fn themes() -> &'static [Theme] {
    static THEMES: OnceLock<Vec<Theme>> = OnceLock::new();
    THEMES.get_or_init(|| {
        serde_json::from_str(include_str!("themes.json"))
            .expect("embedded theme catalogue is validated by tests")
    })
}
pub fn get_theme(theme_id: Option<&str>) -> &'static Theme {
    themes()
        .iter()
        .find(|t| Some(t.id.as_str()) == theme_id)
        .unwrap_or_else(|| &themes()[12])
}
pub fn theme_ids() -> Vec<&'static str> {
    themes().iter().map(|t| t.id.as_str()).collect()
}
pub fn valid_color(color: &str) -> bool {
    color.len() == 7
        && color.starts_with('#')
        && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}
fn rgb(color: &str) -> [f64; 3] {
    if !valid_color(color) {
        return [0.0; 3];
    }
    [1, 3, 5].map(|i| u8::from_str_radix(&color[i..i + 2], 16).unwrap_or(0) as f64)
}
pub fn luminance(color: &str) -> f64 {
    rgb(color)
        .into_iter()
        .zip([0.2126, 0.7152, 0.0722])
        .map(|(v, w)| {
            let v = v / 255.0;
            w * if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        })
        .sum()
}
pub fn contrast(a: &str, b: &str) -> f64 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn hex(rgb: [f64; 3]) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        rgb[0].round_ties_even() as u8,
        rgb[1].round_ties_even() as u8,
        rgb[2].round_ties_even() as u8
    )
}
pub fn mix_color(background: &str, foreground: &str, amount: f64) -> String {
    let (a, b) = (rgb(background), rgb(foreground));
    hex(std::array::from_fn(|i| {
        a[i] * (1.0 - amount) + b[i] * amount
    }))
}
pub fn legible(color: &str, background: &str, minimum: f64) -> String {
    let target = if luminance(background) < 0.3 {
        255.0
    } else {
        0.0
    };
    let rgb = rgb(color);
    let mut result = String::new();
    for step in 0..=100 {
        result = hex(rgb.map(|v| v + (target - v) * step as f64 / 100.0));
        if contrast(&result, background) >= minimum {
            break;
        }
    }
    result
}
