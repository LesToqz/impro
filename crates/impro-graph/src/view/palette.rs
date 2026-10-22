//! Colour roles of the canvas and the panel, derived from the UI theme.

use std::collections::BTreeMap;

use impro_core::themes::{legible, luminance, mix_color};

/// Straight-alpha colour, channels in `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Rgba { r, g, b, a }
    }

    /// Parses `#rgb`, `#rrggbb` or `#rrggbbaa`; `None` for anything else.
    pub fn parse(text: &str) -> Option<Rgba> {
        let hex = text.trim().strip_prefix('#')?;
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        let nibble = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
        let (r, g, b, a) = match hex.len() {
            3 => (nibble(0)?, nibble(1)?, nibble(2)?, 255),
            6 => (byte(0)?, byte(2)?, byte(4)?, 255),
            8 => (byte(0)?, byte(2)?, byte(4)?, byte(6)?),
            _ => return None,
        };
        Some(Rgba::from_u8(r, g, b, a as f32 / 255.0))
    }

    pub fn from_u8(r: u8, g: u8, b: u8, a: f32) -> Rgba {
        Rgba::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a)
    }

    pub fn from_rgb(rgb: u32) -> Rgba {
        Rgba::from_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 1.0)
    }

    pub fn with_alpha(self, a: f32) -> Rgba {
        Rgba { a, ..self }
    }

    /// Linear interpolation of all four channels.
    pub fn mix(self, other: Rgba, t: f32) -> Rgba {
        Rgba {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    /// `#rrggbb` (alpha is dropped).
    pub fn to_hex(self) -> String {
        let c = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
        format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
    }

    pub fn rgb(self) -> [f32; 3] {
        [self.r, self.g, self.b]
    }
}

/// Everything the renderer needs, resolved from a theme.
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub background: Rgba,
    pub fill: Rgba,
    pub focused: Rgba,
    pub tag: Rgba,
    pub attachment: Rgba,
    pub unresolved: Rgba,
    pub code: Rgba,
    pub line: Rgba,
    pub line_highlight: Rgba,
    pub highlight: Rgba,
    pub ring: Rgba,
    pub text: Rgba,
    pub text_muted: Rgba,
    pub arrow: Rgba,
    pub accent: Rgba,
    pub groups: Vec<Rgba>,
    pub dark: bool,
    pub font: String,
}

/// Fallback roles for a view created without a theme.
pub const FALLBACK_BG: &str = "#17212b";
pub const FALLBACK_TEXT: &str = "#e4edf5";
pub const FALLBACK_ACCENT: &str = "#63b3ed";

fn rgba(text: &str, fallback: &str) -> Rgba {
    Rgba::parse(text)
        .or_else(|| Rgba::parse(fallback))
        .unwrap_or(Rgba::new(0.0, 0.0, 0.0, 1.0))
}

impl Palette {
    /// Resolves canvas roles from UI colours (`bg`, `foreground`, `muted`) and
    /// the accent, keeping marker colours legible against the background.
    pub fn from_theme(ui: &BTreeMap<String, String>, accent: &str, presets: &[String]) -> Palette {
        let get = |k: &str, d: &str| ui.get(k).map(String::as_str).unwrap_or(d).to_string();
        let background = get("bg", FALLBACK_BG);
        let text = get("foreground", "#cccccc");
        let muted = get("muted", &text);
        let groups: Vec<String> = presets
            .iter()
            .map(|c| legible(c, &background, 3.0))
            .collect();
        let group = |i: usize| groups.get(i).cloned().unwrap_or_else(|| accent.to_string());
        let node = legible(&muted, &background, 3.0);
        let line = mix_color(&background, &text, 0.25);
        let text_c = rgba(&text, FALLBACK_TEXT);
        let accent_c = rgba(accent, FALLBACK_ACCENT);
        let mut unresolved = rgba(&node, "#7d8f9e");
        unresolved.a *= 0.65;
        Palette {
            background: rgba(&background, FALLBACK_BG),
            fill: rgba(&node, "#adc2d4"),
            focused: accent_c,
            tag: rgba(&group(3), "#70d6a0"),
            attachment: rgba(&group(2), "#e7c66b"),
            unresolved,
            code: rgba(&group(1), "#65d3df"),
            line: rgba(&line, "#506779"),
            line_highlight: accent_c,
            highlight: accent_c,
            ring: accent_c,
            text: text_c,
            text_muted: rgba(&node, "#a1b5c6"),
            arrow: text_c.with_alpha(text_c.a * 0.7),
            accent: accent_c,
            groups: groups.iter().map(|g| rgba(g, "#63b3ed")).collect(),
            dark: luminance(&background) < 0.3,
            font: "sans".to_string(),
        }
    }

    /// The standalone slate theme (dark), used until the host supplies one.
    pub fn fallback() -> Palette {
        let mut ui = BTreeMap::new();
        ui.insert("bg".to_string(), FALLBACK_BG.to_string());
        ui.insert("foreground".to_string(), FALLBACK_TEXT.to_string());
        ui.insert("muted".to_string(), "#a1b5c6".to_string());
        let presets: Vec<String> = ["#63b3ed", "#65d3df", "#e7c66b", "#70d6a0"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        Palette::from_theme(&ui, FALLBACK_ACCENT, &presets)
    }

    /// Colour of the `k`-th colour rule when it carries no colour of its own.
    pub fn group(&self, k: usize) -> Rgba {
        if self.groups.is_empty() {
            self.fill
        } else {
            self.groups[k % self.groups.len()]
        }
    }

    /// Channel-wise blend of two palettes (theme change easing). Text and the
    /// group list switch halfway.
    pub fn blend(&self, to: &Palette, t: f32) -> Palette {
        let m = |a: Rgba, b: Rgba| a.mix(b, t);
        let late = t >= 0.5;
        Palette {
            background: m(self.background, to.background),
            fill: m(self.fill, to.fill),
            focused: m(self.focused, to.focused),
            tag: m(self.tag, to.tag),
            attachment: m(self.attachment, to.attachment),
            unresolved: m(self.unresolved, to.unresolved),
            code: m(self.code, to.code),
            line: m(self.line, to.line),
            line_highlight: m(self.line_highlight, to.line_highlight),
            highlight: m(self.highlight, to.highlight),
            ring: m(self.ring, to.ring),
            text: if late { to.text } else { self.text },
            text_muted: m(self.text_muted, to.text_muted),
            arrow: m(self.arrow, to.arrow),
            accent: m(self.accent, to.accent),
            groups: if late {
                to.groups.clone()
            } else {
                self.groups.clone()
            },
            dark: to.dark,
            font: to.font.clone(),
        }
    }
}

/// Panel colours derived from the same roles (the surfaces of the controls).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelColors {
    pub bg: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
    pub panel: String,
    pub border: String,
    pub hover: String,
    pub track: String,
    pub input: String,
    pub shadow: &'static str,
}

impl PanelColors {
    pub fn from_theme(ui: &BTreeMap<String, String>, accent: &str) -> PanelColors {
        let get = |k: &str, d: &str| ui.get(k).map(String::as_str).unwrap_or(d).to_string();
        let bg = get("bg", FALLBACK_BG);
        let text = get("foreground", FALLBACK_TEXT);
        let muted = legible(&get("muted", &text), &bg, 3.0);
        let dark = luminance(&bg) < 0.3;
        let mix = |t_dark: f64, t_light: f64| mix_color(&bg, &text, if dark { t_dark } else { t_light });
        PanelColors {
            panel: mix(0.06, 0.035),
            border: mix(0.13, 0.11),
            hover: mix(0.11, 0.07),
            track: mix(0.24, 0.18),
            input: mix(0.02, 0.0),
            shadow: if dark {
                "rgba(0,0,0,0.4)"
            } else {
                "rgba(0,0,0,0.12)"
            },
            bg,
            text,
            muted,
            accent: accent.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_forms() {
        assert_eq!(Rgba::parse("#fff"), Some(Rgba::new(1.0, 1.0, 1.0, 1.0)));
        assert_eq!(Rgba::parse("#000000").map(|c| c.to_hex()), Some("#000000".into()));
        assert_eq!(Rgba::parse("#ff000080").map(|c| c.a > 0.49 && c.a < 0.51), Some(true));
        assert_eq!(Rgba::parse("red"), None);
        assert_eq!(Rgba::parse("#12"), None);
        assert_eq!(Rgba::parse("#ggg"), None);
        assert_eq!(Rgba::parse("#éé"), None);
    }

    #[test]
    fn hex_round_trips() {
        for c in ["#17212b", "#63b3ed", "#ffffff", "#010203"] {
            assert_eq!(Rgba::parse(c).map(|c| c.to_hex()).as_deref(), Some(c));
        }
    }

    #[test]
    fn fallback_is_the_slate_dark_palette() {
        let p = Palette::fallback();
        assert!(p.dark);
        assert_eq!(p.background.to_hex(), "#17212b");
        assert_eq!(p.accent.to_hex(), "#63b3ed");
        assert!((p.unresolved.a - 0.65).abs() < 1e-3);
        assert!((p.arrow.a - 0.7).abs() < 1e-3);
    }

    #[test]
    fn theme_roles_drive_the_palette() {
        let theme = impro_core::themes::get_theme(Some("impro-light"));
        let presets = impro_core::settings::accent_presets();
        let p = Palette::from_theme(&theme.ui, &theme.ui["accent"], presets);
        assert!(!p.dark);
        assert_eq!(p.groups.len(), presets.len());
        let dark = impro_core::themes::get_theme(Some("impro-dark"));
        let q = Palette::from_theme(&dark.ui, &dark.ui["accent"], presets);
        assert!(q.dark);
        assert_ne!(p.fill, q.fill);
        assert_eq!(q.group(presets.len() + 1), q.groups[1]);
    }

    #[test]
    fn blending_interpolates_and_ends_on_target() {
        let a = Palette::fallback();
        let mut b = a.clone();
        b.background = Rgba::new(1.0, 1.0, 1.0, 1.0);
        b.dark = false;
        assert_eq!(a.blend(&b, 1.0).background, b.background);
        assert_eq!(a.blend(&b, 0.0).background, a.background);
        let half = a.blend(&b, 0.5);
        assert!(half.background.r > a.background.r && half.background.r < 1.0);
    }

    #[test]
    fn panel_surfaces_follow_the_contrast_direction() {
        let theme = impro_core::themes::get_theme(Some("impro-dark"));
        let p = PanelColors::from_theme(&theme.ui, "#63b3ed");
        assert_ne!(p.panel, p.bg);
        assert_eq!(p.shadow, "rgba(0,0,0,0.4)");
        let light = impro_core::themes::get_theme(Some("impro-light"));
        let l = PanelColors::from_theme(&light.ui, "#3b82f6");
        assert_eq!(l.shadow, "rgba(0,0,0,0.12)");
        assert_eq!(l.input, l.bg);
    }
}
