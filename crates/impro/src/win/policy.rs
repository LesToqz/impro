//! Window-frame decisions for the native Windows frame, as pure functions.
//!
//! This file is compiled on every host so the decisions can be unit-tested on Linux;
//! the operating-system calls that act on them live in the Windows-only siblings.
use impro_core::{
    identity,
    themes::{self, Theme, UiColors},
};
use std::path::Path;

/// The light theme chosen on a first run when Windows apps use the light colour mode.
/// It is the light sibling of the default `impro-dark` theme.
pub const FIRST_RUN_LIGHT_THEME: &str = "impro-light";
/// Lowest Windows build whose DWM honours caption, text and border colours (Windows 11 21H2).
pub const CAPTION_COLOR_MIN_BUILD: u32 = 22000;

// DWMWINDOWATTRIBUTE values (dwmapi.h). The Windows module asserts they match windows-sys.
pub const DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1: i32 = 19;
pub const DWMWA_USE_IMMERSIVE_DARK_MODE: i32 = 20;
pub const DWMWA_WINDOW_CORNER_PREFERENCE: i32 = 33;
pub const DWMWA_BORDER_COLOR: i32 = 34;
pub const DWMWA_CAPTION_COLOR: i32 = 35;
pub const DWMWA_TEXT_COLOR: i32 = 36;
/// DWM_WINDOW_CORNER_PREFERENCE: DWMWCP_ROUND.
pub const DWMWCP_ROUND: u32 = 2;

/// True when the menu/toggle bar is packed into the window content instead of being the
/// client-side header bar: always on Windows (the native caption stays), and on any other
/// platform in debug builds with `IMPRO_DOCKED_TITLEBAR=1`, to preview the Windows layout.
pub fn docked_titlebar() -> bool {
    docked_titlebar_for(
        cfg!(windows),
        cfg!(debug_assertions),
        std::env::var(identity::DOCKED_TITLEBAR_ENV).ok().as_deref(),
    )
}

pub fn docked_titlebar_for(windows: bool, debug_build: bool, env: Option<&str>) -> bool {
    windows || (debug_build && env == Some("1"))
}

/// Last path component, accepting both separators so Windows paths work on any host.
/// A bare drive root such as `C:\` yields `C:`; a filesystem root yields `None`.
pub fn folder_name(root: &Path) -> Option<String> {
    let text = root.to_string_lossy();
    let name = text
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

/// Caption text of the native Windows frame: `<folder> - impro`.
pub fn caption_title(root: &Path) -> String {
    match folder_name(root) {
        Some(name) => format!("{name} - {}", identity::WINDOW_TITLE),
        None => identity::WINDOW_TITLE.to_owned(),
    }
}

/// Window title for the current platform; Linux keeps the plain application name.
pub fn window_title(root: &Path) -> String {
    window_title_for(cfg!(windows), root)
}

pub fn window_title_for(native_caption: bool, root: &Path) -> String {
    if native_caption {
        caption_title(root)
    } else {
        identity::WINDOW_TITLE.to_owned()
    }
}

/// The theme to use when settings hold no `theme` yet and Windows apps use the light mode.
/// `None` keeps the stored or default theme (dark mode, an unreadable registry, or a stored choice).
pub fn initial_theme(
    has_stored_theme: bool,
    apps_use_light: Option<bool>,
    catalogue: &[Theme],
) -> Option<&str> {
    if has_stored_theme || apps_use_light != Some(true) {
        return None;
    }
    catalogue
        .iter()
        .find(|t| t.id == FIRST_RUN_LIGHT_THEME && !t.dark)
        .or_else(|| catalogue.iter().find(|t| !t.dark))
        .map(|t| t.id.as_str())
}

/// Reduce motion when the OS has client-area animations off and the user never chose.
/// `None` leaves the stored or default preference untouched.
pub fn initial_reduce_motion(
    has_stored_choice: bool,
    client_area_animations: Option<bool>,
) -> Option<bool> {
    (!has_stored_choice && client_area_animations == Some(false)).then_some(true)
}

/// `#rrggbb` as a Win32 COLORREF (`0x00BBGGRR`).
pub fn colorref(color: &str) -> Option<u32> {
    if !themes::valid_color(color) {
        return None;
    }
    let channel = |at: usize| u32::from_str_radix(&color[at..at + 2], 16).ok();
    Some(channel(1)? | channel(3)? << 8 | channel(5)? << 16)
}

/// Colours the native caption takes from the theme so it blends with the docked bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaptionStyle {
    /// Light caption glyphs and system menu (DWM immersive dark mode).
    pub dark: bool,
    pub caption: Option<u32>,
    pub text: Option<u32>,
    pub border: Option<u32>,
}

/// The docked bar paints `sidebar_bg` with `titlebar_fg` text and a `border` rule
/// (see titlebar.css), so the caption must use exactly those keys to meet it seamlessly.
pub fn caption_style(ui: &UiColors) -> CaptionStyle {
    let color = |key: &str| ui.get(key).map(String::as_str);
    let caption = color("sidebar_bg");
    CaptionStyle {
        // Whichever of white and black glyphs contrasts better with the caption colour.
        dark: caption
            .is_none_or(|c| themes::contrast("#ffffff", c) >= themes::contrast("#000000", c)),
        caption: caption.and_then(colorref),
        text: color("titlebar_fg").and_then(colorref),
        border: color("border").and_then(colorref),
    }
}

/// `(attribute, value)` pairs applied only where `supports_caption_colors` holds.
/// A colour missing from the theme is skipped rather than guessed.
pub fn caption_attributes(style: &CaptionStyle) -> Vec<(i32, u32)> {
    let mut list = vec![(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND)];
    for (attribute, color) in [
        (DWMWA_BORDER_COLOR, style.border),
        (DWMWA_CAPTION_COLOR, style.caption),
        (DWMWA_TEXT_COLOR, style.text),
    ] {
        list.extend(color.map(|c| (attribute, c)));
    }
    list
}

pub fn supports_caption_colors(build: Option<u32>) -> bool {
    build.is_some_and(|b| b >= CAPTION_COLOR_MIN_BUILD)
}

/// `CurrentBuildNumber` is a decimal string such as `22631`.
pub fn parse_build_number(text: &str) -> Option<u32> {
    text.trim_matches(|c: char| c.is_whitespace() || c == '\0')
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docked_on_windows_always_and_elsewhere_only_for_the_debug_switch() {
        assert!(docked_titlebar_for(true, false, None));
        assert!(docked_titlebar_for(true, true, Some("0")));
        assert!(!docked_titlebar_for(false, true, None));
        assert!(!docked_titlebar_for(false, true, Some("0")));
        assert!(docked_titlebar_for(false, true, Some("1")));
        assert!(!docked_titlebar_for(false, false, Some("1")));
    }

    #[test]
    fn folder_name_handles_windows_and_unix_paths() {
        for (path, expected) in [
            (r"C:\Users\me\proj", Some("proj")),
            (r"C:\Users\me\proj\", Some("proj")),
            (r"\\?\C:\Users\me\My Notes", Some("My Notes")),
            (r"\\server\share", Some("share")),
            (r"C:\", Some("C:")),
            (r"\\?\C:\", Some("C:")),
            ("/home/me/proj", Some("proj")),
            ("/home/me/proj/", Some("proj")),
            ("C:/Users/me/proj", Some("proj")),
            ("proj", Some("proj")),
            ("/", None),
            ("", None),
        ] {
            assert_eq!(folder_name(Path::new(path)).as_deref(), expected, "{path}");
        }
    }

    #[test]
    fn caption_is_folder_dash_application() {
        assert_eq!(
            caption_title(Path::new(r"C:\Users\me\proj")),
            "proj - impro"
        );
        assert_eq!(caption_title(Path::new("/")), "impro");
        assert_eq!(caption_title(Path::new("")), "impro");
    }

    #[test]
    fn linux_title_stays_the_application_name() {
        let root = Path::new("/home/me/proj");
        assert_eq!(window_title_for(false, root), identity::WINDOW_TITLE);
        assert_eq!(window_title_for(true, root), "proj - impro");
        if !cfg!(windows) {
            assert_eq!(window_title(root), "impro");
        }
    }

    #[test]
    fn first_run_follows_the_windows_colour_mode_only_without_a_stored_theme() {
        let catalogue = themes::themes();
        assert_eq!(
            initial_theme(false, Some(true), catalogue),
            Some("impro-light")
        );
        assert_eq!(initial_theme(false, Some(false), catalogue), None);
        assert_eq!(initial_theme(false, None, catalogue), None);
        assert_eq!(initial_theme(true, Some(true), catalogue), None);
        assert_eq!(initial_theme(true, Some(false), catalogue), None);
    }

    #[test]
    fn light_choice_exists_is_light_and_has_a_fallback() {
        let catalogue = themes::themes();
        let id = initial_theme(false, Some(true), catalogue).unwrap();
        assert!(!themes::get_theme(Some(id)).dark);
        assert_eq!(themes::get_theme(Some(id)).id, id);
        // Without impro-light the first light theme of the catalogue is used.
        let reduced: Vec<Theme> = catalogue
            .iter()
            .filter(|t| t.id != "impro-light")
            .cloned()
            .collect();
        let fallback = initial_theme(false, Some(true), &reduced).unwrap();
        assert!(reduced.iter().any(|t| t.id == fallback && !t.dark));
        // A catalogue without any light theme keeps the default.
        let dark: Vec<Theme> = catalogue.iter().filter(|t| t.dark).cloned().collect();
        assert_eq!(initial_theme(false, Some(true), &dark), None);
        assert_eq!(initial_theme(false, Some(true), &[]), None);
    }

    #[test]
    fn reduce_motion_follows_the_os_unless_the_user_chose() {
        assert_eq!(initial_reduce_motion(false, Some(false)), Some(true));
        assert_eq!(initial_reduce_motion(false, Some(true)), None);
        assert_eq!(initial_reduce_motion(false, None), None);
        assert_eq!(initial_reduce_motion(true, Some(false)), None);
        assert_eq!(initial_reduce_motion(true, Some(true)), None);
    }

    #[test]
    fn colorref_is_blue_green_red() {
        assert_eq!(colorref("#000000"), Some(0));
        assert_eq!(colorref("#ffffff"), Some(0x00ff_ffff));
        assert_eq!(colorref("#ff0000"), Some(0x0000_00ff));
        assert_eq!(colorref("#00ff00"), Some(0x0000_ff00));
        assert_eq!(colorref("#0000ff"), Some(0x00ff_0000));
        assert_eq!(colorref("#202c32"), Some(0x0032_2c20));
        for bad in [
            "",
            "#fff",
            "202c32",
            "#12345g",
            "#1234567",
            "alpha(#fff, 0.1)",
        ] {
            assert_eq!(colorref(bad), None, "{bad}");
        }
    }

    #[test]
    fn caption_matches_the_docked_bar_colours_for_every_theme() {
        for theme in themes::themes() {
            let style = caption_style(&theme.ui);
            let id = &theme.id;
            assert_eq!(style.caption, colorref(&theme.ui["sidebar_bg"]), "{id}");
            assert_eq!(style.text, colorref(&theme.ui["titlebar_fg"]), "{id}");
            assert_eq!(style.border, colorref(&theme.ui["border"]), "{id}");
            assert!(style.caption.is_some() && style.text.is_some() && style.border.is_some());
            // Light glyphs exactly for the themes the catalogue calls dark.
            assert_eq!(style.dark, theme.dark, "{id}");
        }
    }

    #[test]
    fn caption_style_survives_a_theme_without_colours() {
        let style = caption_style(&UiColors::new());
        assert!(style.dark);
        assert_eq!(
            (style.caption, style.text, style.border),
            (None, None, None)
        );
        assert_eq!(
            caption_attributes(&style),
            vec![(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND)]
        );
    }

    #[test]
    fn caption_attributes_are_round_corners_then_border_caption_text() {
        let style = CaptionStyle {
            dark: true,
            caption: Some(1),
            text: Some(2),
            border: Some(3),
        };
        assert_eq!(
            caption_attributes(&style),
            vec![(33, 2), (34, 3), (35, 1), (36, 2)]
        );
    }

    #[test]
    fn caption_colours_need_windows_11() {
        assert!(!supports_caption_colors(None));
        assert!(!supports_caption_colors(Some(19045)));
        assert!(!supports_caption_colors(Some(21999)));
        assert!(supports_caption_colors(Some(22000)));
        assert!(supports_caption_colors(Some(26100)));
    }

    #[test]
    fn build_number_parses_the_registry_string() {
        assert_eq!(parse_build_number("22631"), Some(22631));
        assert_eq!(parse_build_number(" 19045\0"), Some(19045));
        assert_eq!(parse_build_number(""), None);
        assert_eq!(parse_build_number("22631.1"), None);
        assert_eq!(parse_build_number("abc"), None);
    }
}
