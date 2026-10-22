//! Style sheet of the panel, menus and overlays.

use std::cell::RefCell;
use std::collections::BTreeMap;

use gtk::prelude::*;
use impro_core::themes::UiColors;

use super::palette::PanelColors;

/// `graph.css` rendered for a theme. The accent comes from `ui["accent"]`.
pub fn css(ui: &UiColors) -> String {
    let accent = ui
        .get("accent")
        .map(String::as_str)
        .unwrap_or(super::palette::FALLBACK_ACCENT);
    css_with(&PanelColors::from_theme(ui, accent))
}

pub(crate) fn css_with(c: &PanelColors) -> String {
    let mut roles: BTreeMap<String, String> = BTreeMap::new();
    for (k, v) in [
        ("bg", &c.bg),
        ("text", &c.text),
        ("muted", &c.muted),
        ("accent", &c.accent),
        ("panel", &c.panel),
        ("border", &c.border),
        ("hover", &c.hover),
        ("track", &c.track),
        ("input", &c.input),
    ] {
        roles.insert(k.to_string(), v.clone());
    }
    roles.insert("shadow".to_string(), c.shadow.to_string());
    impro_ui::render_css(include_str!("graph.css"), &roles)
}

thread_local! {
    static PROVIDER: RefCell<Option<(gtk::CssProvider, String)>> = const { RefCell::new(None) };
}

/// Applies the style sheet for these colours to the default screen. The
/// application registers [`css`] with `impro_ui` as well; both render the same
/// text, this keeps views usable without the full application style.
pub fn install(ui: &UiColors, accent: &str) {
    let colors = PanelColors::from_theme(ui, accent);
    let text = css_with(&colors);
    PROVIDER.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_some_and(|(_, t)| *t == text) {
            return;
        }
        let provider = match slot.take() {
            Some((p, _)) => p,
            None => {
                let p = gtk::CssProvider::new();
                if let Some(screen) = gdk::Screen::default() {
                    gtk::StyleContext::add_provider_for_screen(
                        &screen,
                        &p,
                        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                    );
                }
                p
            }
        };
        if let Err(e) = provider.load_from_data(text.as_bytes()) {
            eprintln!("graph view style error: {e}");
        }
        *slot = Some((provider, text));
    });
}

/// True once some theme has been installed for this process.
pub fn is_installed() -> bool {
    PROVIDER.with(|s| s.borrow().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_placeholder_is_replaced() {
        for id in ["impro-dark", "impro-light"] {
            let theme = impro_core::themes::get_theme(Some(id));
            let text = css(&theme.ui);
            assert!(!text.contains("{bg}") && !text.contains("{accent}"), "{id}");
            assert!(
                !text.contains("{panel}") && !text.contains("{shadow}"),
                "{id}"
            );
            assert!(text.contains(&format!(
                "@define-color impro_graph_accent {};",
                theme.ui["accent"]
            )));
        }
    }
}
