//! Theme application: the per-view stylesheet, the style tags and accent sync.
use super::{Inner, StyleState};
use gtk::prelude::*;
use impro_core::themes::{SyntaxColors, UiColors, KINDS};

/// Pixels the first line of a block takes above itself, by style tag.
pub(super) fn above_of(name: &str) -> i32 {
    match name {
        "body" => 2,
        "blank" => 0,
        "h1" => 18,
        "h2" => 14,
        "h3" | "h4" | "h5" | "h6" => 12,
        "quote" => 10,
        "code-opening" => 34,
        _ => 0,
    }
}

fn font_families(view: &gtk::TextView) -> Vec<String> {
    view.pango_context()
        .list_families()
        .iter()
        .map(|f| f.name().to_string())
        .collect()
}

fn valid_color(value: &str) -> bool {
    gdk::RGBA::parse(value).is_ok()
}

/// Stylesheet of a live view: the template filled with the editor palette.
pub fn view_css(style: &StyleState, tables: &str) -> String {
    let mut map = UiColors::new();
    for (key, value) in [
        ("bg", style.bg.clone()),
        ("fg", style.fg.clone()),
        ("border", style.border.clone()),
        ("muted", style.muted.clone()),
        ("overlay", style.overlay.clone()),
        ("surface", style.surface.clone()),
        ("size", style.size.to_string()),
        ("body_font", style.body_font.clone()),
        ("code_font", style.code_font.clone()),
        ("pill_size", 7.max(style.size - 2).to_string()),
        ("placeholder", format!("alpha({}, 0.55)", style.muted)),
        ("tables", tables.to_string()),
    ] {
        map.insert(key.into(), value);
    }
    impro_ui::render_css(include_str!("../livemd.css"), &map)
}

/// Registered with the shared theme registry: the stylesheet with default
/// colours, so a live view looks right before its own `set_style` runs.
pub fn css(ui: &UiColors) -> String {
    let get = |key: &str| ui.get(key).cloned().unwrap_or_default();
    let style = StyleState {
        size: 11,
        bg: get("bg"),
        fg: get("editor_fg"),
        muted: get("muted"),
        border: get("border"),
        overlay: get("overlay"),
        surface: get("sidebar_bg"),
        body_font: "sans-serif".into(),
        code_font: "monospace".into(),
        ..Default::default()
    };
    view_css(&style, "")
}

impl Inner {
    /// Get or create a registry tag (`live-md-<name>`).
    pub(super) fn tag(&self, name: &str) -> gtk::TextTag {
        if let Some(tag) = self.tags.borrow().get(name) {
            return tag.clone();
        }
        let full = format!("live-md-{name}");
        let table = self.buffer.tag_table().expect("buffer tag table");
        let tag = table.lookup(&full).unwrap_or_else(|| {
            let tag = gtk::TextTag::new(Some(&full));
            table.add(&tag);
            tag
        });
        self.tags.borrow_mut().insert(name.to_string(), tag.clone());
        tag
    }
    pub(super) fn tag_hidden(&self) {
        self.tag("hidden").set_property("invisible", true);
    }
    /// `pad-N`: created after every style tag so it outranks them for space above.
    pub(super) fn pad_tag(&self, above: i32) -> gtk::TextTag {
        let name = format!("pad-{above}");
        if let Some(tag) = self.tags.borrow().get(&name) {
            return tag.clone();
        }
        let tag = self.tag(&name);
        tag.set_property("pixels-above-lines", above);
        tag.set_property("pixels-below-lines", 0);
        tag
    }

    pub(super) fn set_style(&self, syntax: &SyntaxColors, chrome: &UiColors, size: i32) {
        let theme = impro_ui::current_theme();
        let ui = theme.as_ref().map(|t| t.ui.clone()).unwrap_or_default();
        let chrome_get = |key: &str| chrome.get(key).cloned().unwrap_or_default();
        let color = |kind: &str| -> String {
            match syntax.get(kind).or_else(|| syntax.get("operator")) {
                Some(style) if valid_color(&style.foreground) => style.foreground.clone(),
                _ => chrome_get("gutter_fg_active"),
            }
        };
        let bg = chrome_get("gutter_bg");
        let fg = color("operator");
        let pick = |key: &str, fallback: String| ui.get(key).cloned().unwrap_or(fallback);
        let border = pick("border", chrome_get("gutter_fg"));
        let muted = pick("muted", chrome_get("gutter_fg"));
        let overlay = pick("overlay", fg.clone());
        let surface = pick("sidebar_bg", chrome_get("current_line_bg"));
        let families = font_families(&self.view);
        let body_font = if families.iter().any(|f| f == "Inter") {
            "Inter"
        } else {
            "sans-serif"
        };
        let code_font = if families.iter().any(|f| f == "IBM Plex Mono") {
            "IBM Plex Mono"
        } else {
            "monospace"
        };
        let state = StyleState {
            chrome: chrome.clone(),
            size,
            bg: bg.clone(),
            fg: fg.clone(),
            muted: muted.clone(),
            border,
            overlay: overlay.clone(),
            surface: surface.clone(),
            body_font: body_font.into(),
            code_font: code_font.into(),
        };
        self.style.replace(state.clone());
        let mut palette = ui.clone();
        for (key, value) in [
            ("foreground", &fg),
            ("overlay", &overlay),
            ("muted", &muted),
        ] {
            palette.insert(key.into(), value.clone());
        }
        for key in ["accent", "accent_text", "border", "sidebar_bg"] {
            palette
                .entry(key.into())
                .or_insert_with(|| chrome_get("gutter_fg_active"));
        }
        let tables = crate::livetable::styled_css(&palette, &bg, size, body_font);
        let css = view_css(&state, &tables);
        if let Err(error) = self.provider.load_from_data(css.as_bytes()) {
            eprintln!("live markdown stylesheet: {error}");
        }
        let set = |name: &str, props: &[(&str, glib::Value)]| {
            let tag = self.tag(name);
            for (key, value) in props {
                tag.set_property_from_value(key, value);
            }
        };
        use glib::value::ToValue;
        set(
            "body",
            &[
                ("family", body_font.to_value()),
                ("pixels-above-lines", 2.to_value()),
                ("pixels-below-lines", 0.to_value()),
                ("pixels-inside-wrap", 7.to_value()),
            ],
        );
        set(
            "blank",
            &[
                ("size-points", 4.0f64.to_value()),
                ("pixels-above-lines", 0.to_value()),
                ("pixels-below-lines", 0.to_value()),
            ],
        );
        for (level, pixels) in [28, 26, 24, 22, 20, 18].into_iter().enumerate() {
            let level = level + 1;
            set(
                &format!("h{level}"),
                &[
                    ("foreground", fg.to_value()),
                    ("weight", (if level == 1 { 700 } else { 600 }).to_value()),
                    ("scale", (f64::from(pixels) / 15.0).to_value()),
                    ("font-features", "kern=1".to_value()),
                    ("stretch", pango::Stretch::SemiCondensed.to_value()),
                    (
                        "pixels-above-lines",
                        above_of(&format!("h{level}")).to_value(),
                    ),
                    ("pixels-below-lines", 0.to_value()),
                ],
            );
        }
        set(
            "strong",
            &[("foreground", fg.to_value()), ("weight", 700.to_value())],
        );
        set(
            "emphasis",
            &[
                ("foreground", fg.to_value()),
                ("style", pango::Style::Italic.to_value()),
            ],
        );
        set("strike", &[("strikethrough", true.to_value())]);
        set(
            "checked",
            &[
                ("foreground", muted.to_value()),
                ("strikethrough", true.to_value()),
            ],
        );
        set(
            "code",
            &[
                ("family", code_font.to_value()),
                ("size-points", f64::from(7.max(size - 2)).to_value()),
                ("background", surface.to_value()),
                ("foreground", fg.to_value()),
            ],
        );
        set(
            "codeblock",
            &[
                ("family", code_font.to_value()),
                (
                    "size-points",
                    (8.0f64).max(f64::from(size) - 1.25).to_value(),
                ),
                ("paragraph-background", surface.to_value()),
                ("foreground", fg.to_value()),
                ("pixels-above-lines", 3.to_value()),
                ("pixels-below-lines", 0.to_value()),
                ("pixels-inside-wrap", 4.to_value()),
            ],
        );
        set(
            "code-opening",
            &[
                ("size-points", 1.0f64.to_value()),
                ("paragraph-background", bg.to_value()),
                ("pixels-above-lines", 34.to_value()),
                ("pixels-below-lines", 0.to_value()),
            ],
        );
        set(
            "code-closing",
            &[
                ("size-points", 1.0f64.to_value()),
                ("paragraph-background", bg.to_value()),
                ("pixels-above-lines", 12.to_value()),
                ("pixels-below-lines", 0.to_value()),
            ],
        );
        set(
            "quote",
            &[
                ("pixels-above-lines", 10.to_value()),
                ("pixels-below-lines", 0.to_value()),
                ("pixels-inside-wrap", 7.to_value()),
            ],
        );
        set(
            "link",
            &[
                ("foreground", color("link").to_value()),
                ("underline", pango::Underline::Single.to_value()),
            ],
        );
        set(
            "image",
            &[
                ("foreground", muted.to_value()),
                ("background", surface.to_value()),
            ],
        );
        for kind in KINDS {
            let style = syntax.get(kind);
            let valid = style.is_some_and(|s| valid_color(&s.foreground));
            set(
                &format!("syntax-{kind}"),
                &[
                    ("foreground", color(kind).to_value()),
                    ("foreground-set", valid.to_value()),
                    (
                        "weight",
                        (if style.is_some_and(|s| s.bold) {
                            700
                        } else {
                            400
                        })
                        .to_value(),
                    ),
                    (
                        "style",
                        (if style.is_some_and(|s| s.italic) {
                            pango::Style::Italic
                        } else {
                            pango::Style::Normal
                        })
                        .to_value(),
                    ),
                ],
            );
        }
        if let (Some(table), Some(checked)) =
            (self.buffer.tag_table(), self.tags.borrow().get("checked"))
        {
            checked.set_priority(table.size() - 1);
            if std::env::var_os("DBG_WARM").is_some() && self.char_count() > 0 {
                let t = std::time::Instant::now();
                self.buffer.apply_tag(checked, &self.iter(0), &self.iter(1));
                self.slow("warm-apply", t);
                self.buffer.remove_tag(checked, &self.iter(0), &self.iter(1));
                self.slow("warm-remove", t);
            }
        }
        self.sync_accent();
        self.resize();
        if self.enabled.get() {
            let private: Vec<gtk::TextTag> = self
                .blocks
                .borrow()
                .items
                .iter()
                .flat_map(|b| b.tags.iter().cloned())
                .collect();
            for tag in private {
                tag.set_property("foreground", bg.as_str());
            }
            self.queue_layout();
        }
    }

    /// Follow the theme accent (task ticks, link colour, focused table cell).
    pub(super) fn sync_accent(&self) {
        let Some(theme) = impro_ui::current_theme() else {
            return;
        };
        let link = self.tags.borrow().get("link").cloned();
        let Some(link) = link else {
            return;
        };
        let accent = (
            theme.ui["accent"].clone(),
            theme.ui["accent_text"].clone(),
            theme.ui["bg"].clone(),
        );
        link.set_property("foreground", accent.1.as_str());
        if self.accent.borrow().as_ref() != Some(&accent) {
            let css = format!(
                ".live-md-task check:checked {{background:{0}; border-color:{0}; color:{2};}}\n\
                 .live-md-cell link {{color:{1};}}\n\
                 .live-table-cell.focused > .live-table-inner {{border-color:alpha({0},0.7); background-color:alpha({0},0.07);}}\n\
                 .live-table-label link {{color:{1};}}\n",
                accent.0, accent.1, accent.2
            );
            if let Err(error) = self.accent_provider.load_from_data(css.as_bytes()) {
                eprintln!("live markdown accent: {error}");
            }
            self.accent.replace(Some(accent));
        }
    }

    /// Attach the view stylesheets to `widget` and its current children.
    pub(super) fn provide(&self, widget: &gtk::Widget) {
        let context = widget.style_context();
        context.add_provider(&self.provider, gtk::STYLE_PROVIDER_PRIORITY_USER + 1);
        context.add_provider(&self.accent_provider, gtk::STYLE_PROVIDER_PRIORITY_USER + 2);
        if let Some(container) = widget.downcast_ref::<gtk::Container>() {
            for child in container.children() {
                self.provide(&child);
            }
        }
    }
}
