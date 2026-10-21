//! Shared cached CSS provider, extensible without dependencies on feature crates.
use gtk::prelude::*;
use impro_core::themes::{self, Theme, UiColors};
use serde_json::Value;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::LazyLock};
pub static TOKENS: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("tokens.json")).expect("embedded tokens"));
pub type CssBuilder = fn(&UiColors) -> String;
#[derive(Default)]
struct State {
    builders: BTreeMap<String, CssBuilder>,
    cache: BTreeMap<String, Rc<String>>,
    provider: Option<gtk::CssProvider>,
    current: Option<Rc<Theme>>,
    revision: u64,
}
thread_local! {static STATE:RefCell<State>=RefCell::new(State::default());}
/// Register once from a crate's init(). Re-registering an identical function is harmless.
pub fn register_css(name: &str, builder: CssBuilder) {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        if s.builders
            .get(name)
            .is_some_and(|b| std::ptr::fn_addr_eq(*b, builder))
        {
            return;
        }
        s.builders.insert(name.into(), builder);
        s.revision += 1;
        s.cache.clear();
    });
}
/// Copy Python f-string CSS verbatim, or use plain {bg}/{accent} placeholders.
/// Literal CSS braces remain intact; doubled f-string braces are unescaped.
pub fn render_css(template: &str, ui: &UiColors) -> String {
    let mut css = template.to_owned();
    for (key, value) in ui {
        for placeholder in [
            format!("{{{key}}}"),
            format!("{{ui['{key}']}}"),
            format!("{{ui[\"{key}\"]}}"),
        ] {
            css = css.replace(&placeholder, value);
        }
    }
    css.replace("{{", "{").replace("}}", "}")
}
pub fn build_css(theme: &Theme) -> String {
    let mut roles = theme.ui.clone();
    for (group, values) in TOKENS.as_object().expect("token object") {
        for (key, value) in values.as_object().expect("token group") {
            roles.insert(
                format!("{group}.{key}"),
                value
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| value.to_string()),
            );
        }
    }
    roles.insert(
        "easing".into(),
        TOKENS["easings"]["state"]
            .as_str()
            .expect("state easing")
            .into(),
    );
    roles.insert(
        "selection_alpha".into(),
        if theme.dark { "0.26" } else { "0.19" }.into(),
    );
    roles.insert(
        "popup_shadow".into(),
        TOKENS["shadows"][if theme.dark { "dark" } else { "light" }]
            .as_str()
            .expect("shadow")
            .into(),
    );
    let mut css = render_css(include_str!("style.css"), &roles);
    // Clone registry functions before invoking any third-party code.
    let builders: Vec<_> = STATE.with(|s| s.borrow().builders.values().copied().collect());
    for builder in builders {
        css.push_str(&builder(&theme.ui));
    }
    css
}
pub fn font_family(name: Option<&str>) -> String {
    let name: String = name
        .unwrap_or("")
        .trim()
        .chars()
        .filter(|c| impro_core::text::is_printable(*c) && !['"', '\\', ';', '{', '}'].contains(c))
        .collect();
    if name.is_empty() {
        name
    } else {
        format!("\"{name}\"")
    }
}
pub fn build_overrides(ui_font: Option<&str>, editor_font: Option<&str>) -> String {
    let mut css = String::new();
    let ui = font_family(ui_font);
    if !ui.is_empty() {
        css.push_str(&format!("* {{ font-family: {ui}; }}\n"));
    }
    let editor = font_family(editor_font);
    if !editor.is_empty() {
        css.push_str(&format!(
            ".impro-editor textview, .impro-editor textview text {{ font-family: {editor}; }}\n"
        ));
    }
    css
}
pub fn with_accent(theme: &Theme, accent: Option<&str>) -> Theme {
    let Some(accent) = accent.filter(|s| themes::valid_color(s)) else {
        return theme.clone();
    };
    let accent = accent.to_ascii_lowercase();
    let mut theme = theme.clone();
    let old = theme.ui["accent"].clone();
    let status = theme.ui["status_bg"] == old;
    for value in theme.ui.values_mut() {
        if *value == old {
            value.clone_from(&accent);
        }
    }
    let fg = if themes::contrast("#ffffff", &accent) >= themes::contrast("#111111", &accent) {
        "#ffffff"
    } else {
        "#111111"
    };
    if status {
        theme.ui.insert("status_fg".into(), fg.into());
    }
    theme.ui.insert("accent_fg".into(), fg.into());
    theme.ui.insert(
        "accent_text".into(),
        themes::legible(&accent, &theme.ui["bg"], 3.0),
    );
    theme.ui.insert(
        "accent_hover".into(),
        themes::mix_color(&accent, &theme.ui["foreground"], 0.2),
    );
    theme
}
#[derive(Clone, Debug)]
pub struct StyleError(pub String);
impl std::fmt::Display for StyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for StyleError {}
pub fn create_provider(css: &str) -> Result<gtk::CssProvider, StyleError> {
    let provider = gtk::CssProvider::new();
    let errors = Rc::new(RefCell::new(Vec::new()));
    let captured = errors.clone();
    let handler = provider
        .connect_parsing_error(move |_, _, error| captured.borrow_mut().push(error.to_string()));
    let result = provider.load_from_data(css.as_bytes());
    provider.disconnect(handler);
    let errors = errors.borrow();
    if let Err(error) = result {
        return Err(StyleError(format!(
            "impro theme CSS failed to parse: {error}"
        )));
    }
    if !errors.is_empty() {
        return Err(StyleError(format!(
            "impro theme CSS failed to parse: {}",
            errors.join("; ")
        )));
    }
    Ok(provider)
}
pub fn current_theme() -> Option<Rc<Theme>> {
    STATE.with(|s| s.borrow().current.clone())
}
pub fn provider() -> Option<gtk::CssProvider> {
    STATE.with(|s| s.borrow().provider.clone())
}
pub fn apply_theme(
    theme: &Theme,
    accent: Option<&str>,
    ui_font: Option<&str>,
    editor_font: Option<&str>,
) -> Result<(), StyleError> {
    if !gtk::is_initialized_main_thread() {
        return Err(StyleError(
            "impro theme requires an initialized GTK display".into(),
        ));
    }
    let screen = gdk::Screen::default()
        .ok_or_else(|| StyleError("impro theme requires an initialized GTK display".into()))?;
    let settings = gtk::Settings::default()
        .ok_or_else(|| StyleError("impro theme requires an initialized GTK display".into()))?;
    let themed = with_accent(theme, accent);
    let key = format!(
        "{}:{ui_font:?}:{editor_font:?}",
        serde_json::to_string(&themed).expect("theme serialization")
    );
    let cached = STATE.with(|s| s.borrow().cache.get(&key).cloned());
    let css = cached.unwrap_or_else(|| {
        let css = Rc::new(build_css(&themed) + &build_overrides(ui_font, editor_font));
        STATE.with(|s| {
            s.borrow_mut().cache.insert(key, css.clone());
        });
        css
    });
    // Stage strict parsing first. No RefCell borrow spans GTK calls or CSS builders.
    let validated = create_provider(&css)?;
    let previous = provider();
    if let Some(provider) = previous {
        provider
            .load_from_data(css.as_bytes())
            .map_err(|e| StyleError(e.to_string()))?;
    } else {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &validated,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        STATE.with(|s| s.borrow_mut().provider = Some(validated));
    }
    settings.set_gtk_application_prefer_dark_theme(themed.dark);
    STATE.with(|s| s.borrow_mut().current = Some(Rc::new(themed)));
    Ok(())
}
pub fn apply_style(theme: Option<&Theme>) -> Result<(), StyleError> {
    apply_theme(
        theme.unwrap_or_else(|| themes::get_theme(None)),
        None,
        None,
        None,
    )
}
