//! Fractional interface scale for displays that GTK 3 only scales in whole steps.
//!
//! GTK 3 scales windows by whole numbers only. On Windows at 125, 150 or 175 percent GDK keeps the
//! window scale at 1 and raises the font resolution instead (144 dpi at 150 percent). Text sized in
//! points grows with it, while everything sized in CSS pixels (most of impro's chrome, many of its
//! font sizes included), every icon and every size set from code stays at its 100 percent size. The
//! result was a window whose explorer and terminal text was half as large again as its menus,
//! headers, buttons and icons.
//!
//! [`factor`] is the ratio GTK applies to point sizes. The helpers here apply the same ratio to CSS
//! pixel lengths, icon sizes and sizes chosen in code, so the whole window scales together the way
//! native Windows applications do. At 100 percent, at 200 percent and above (where GDK scales whole
//! windows itself and keeps the font resolution at 96 dpi) and on Linux the factor is 1 and nothing
//! changes.
use gtk::prelude::*;
use std::cell::Cell;

/// Overrides the factor on Windows (`1` turns the scaling off); accepted range 0.5 to 4.
pub const ENV: &str = "IMPRO_UI_SCALE";

/// The resolution at which a CSS pixel is one device pixel.
const BASE_DPI: f64 = 96.0;

thread_local! {
    static FACTOR: Cell<Option<f64>> = const { Cell::new(None) };
}

/// The factor for a font resolution in dpi (`<= 0` when unknown) and an optional override, as
/// decided on Windows; every other platform uses 1.
pub fn decide(resolution: f64, env: Option<&str>, windows: bool) -> f64 {
    if !windows {
        return 1.0;
    }
    if let Some(value) = env
        .and_then(|text| text.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value > 0.0)
    {
        return value.clamp(0.5, 4.0);
    }
    if !resolution.is_finite() || resolution <= BASE_DPI {
        return 1.0;
    }
    // Two decimals: 120 dpi is exactly 1.25, 144 dpi exactly 1.5.
    ((resolution / BASE_DPI).min(4.0) * 100.0).round() / 100.0
}

/// The process's interface scale. Fixed at the first call made after GTK has a display, so every
/// size computed from it agrees; before that it is 1 and nothing is remembered.
pub fn factor() -> f64 {
    if let Some(value) = FACTOR.with(Cell::get) {
        return value;
    }
    if !gtk::is_initialized_main_thread() {
        return 1.0;
    }
    let Some(screen) = gdk::Screen::default() else {
        return 1.0;
    };
    let env = std::env::var(ENV).ok();
    let value = decide(screen.resolution(), env.as_deref(), cfg!(windows));
    FACTOR.with(|cell| cell.set(Some(value)));
    if std::env::var_os(impro_core::identity::QA_TIMING_ENV).is_some() {
        eprintln!(
            "impro: ui-scale={value} font-resolution={}",
            screen.resolution()
        );
    }
    value
}

/// A length in CSS pixels at 100 percent, scaled. 0, 1 and -1 (hairlines and "unset") stay.
pub fn px(value: i32) -> i32 {
    px_with(value, factor())
}

pub fn px_with(value: i32, factor: f64) -> i32 {
    if value.abs() <= 1 || factor == 1.0 {
        return value;
    }
    (f64::from(value) * factor).round() as i32
}

/// Scale every pixel length in a style sheet. Hairlines (at most 1px) stay crisp; font sizes keep
/// one decimal, every other length is rounded to whole pixels as GTK lays them out.
pub fn css(text: &str) -> String {
    css_with(text, factor())
}

pub fn css_with(text: &str, factor: f64) -> String {
    if factor == 1.0 || !text.contains("px") {
        return text.to_owned();
    }
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        let Some((start, end)) = number_at(bytes, i) else {
            i += 1;
            continue;
        };
        let unit_end = end + 2;
        let is_px = bytes.get(end..unit_end) == Some(b"px".as_slice())
            && !bytes.get(unit_end).is_some_and(|b| is_ident(*b));
        if !is_px {
            i = end.max(i + 1);
            continue;
        }
        let Ok(value) = text[start..end].parse::<f64>() else {
            i = unit_end;
            continue;
        };
        if value.abs() > 1.0 {
            let scaled = value * factor;
            let rendered = if in_font_size(&text[..start]) {
                let tenth = (scaled * 10.0).round() / 10.0;
                format!("{tenth}")
            } else {
                format!("{}", scaled.round())
            };
            out.push_str(&text[copied..start]);
            out.push_str(&rendered);
            copied = end;
        }
        i = unit_end;
    }
    out.push_str(&text[copied..]);
    out
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')
}

/// A CSS number starting at `i` that is not part of an identifier, a colour or another number:
/// its byte range, sign included.
fn number_at(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    if i > 0 && (is_ident(bytes[i - 1]) || matches!(bytes[i - 1], b'.' | b'#')) {
        return None;
    }
    let mut j = i;
    if bytes[j] == b'-' {
        j += 1;
    }
    let digits = j;
    while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
        j += 1;
    }
    let number = &bytes[digits..j];
    (number.iter().any(u8::is_ascii_digit) && number.iter().filter(|b| **b == b'.').count() <= 1)
        .then_some((i, j))
}

/// Whether the declaration that the text ends inside is `font-size`.
fn in_font_size(before: &str) -> bool {
    let declaration = before
        .rfind([';', '{', '}'])
        .map_or(before, |at| &before[at + 1..]);
    declaration
        .split(':')
        .next()
        .is_some_and(|name| name.trim() == "font-size")
}

/// Pixels at 100 percent of a `GtkIconSize` value (the `icon-size` property of an image).
pub fn icon_size_px(size: i32) -> i32 {
    match size {
        3 => 24, // GTK_ICON_SIZE_LARGE_TOOLBAR
        5 => 32, // GTK_ICON_SIZE_DND
        6 => 48, // GTK_ICON_SIZE_DIALOG
        _ => 16, // menu, small toolbar, button and invalid
    }
}

thread_local! {
    static ICONS_INSTALLED: Cell<bool> = const { Cell::new(false) };
}

/// Key of the pixel size this module gave an image, so its own change is not scaled again.
const IMAGE_KEY: &str = "impro-ui-scale-px";

/// Scale every themed icon image of the process by [`factor`]. Images are caught when they are
/// added to a container (GTK's `parent-set` signal, through an emission hook), which is before
/// their first size request, and again whenever code changes their pixel size. Images showing a
/// pixbuf or surface are not affected by the pixel size and stay as they are. No-op at factor 1.
pub fn install_icon_scaling() {
    if factor() == 1.0 || ICONS_INSTALLED.with(|done| done.replace(true)) {
        return;
    }
    use glib::translate::IntoGlib;
    let Some(signal) =
        glib::subclass::signal::SignalId::lookup("parent-set", gtk::Widget::static_type())
    else {
        return;
    };
    // SAFETY: the hook is a plain function without data; GLib keeps it for the process.
    unsafe {
        gobject_sys::g_signal_add_emission_hook(
            signal.into_glib(),
            0,
            Some(parent_set_hook),
            std::ptr::null_mut(),
            None,
        );
    }
}

unsafe extern "C" fn parent_set_hook(
    _hint: *mut gobject_sys::GSignalInvocationHint,
    count: std::os::raw::c_uint,
    params: *const gobject_sys::GValue,
    _data: glib::ffi::gpointer,
) -> glib::ffi::gboolean {
    if count >= 1 && !params.is_null() {
        // SAFETY: the first parameter of a signal emission is the instance; `glib::Value` is a
        // transparent wrapper of `GValue`, borrowed here for the duration of the hook only.
        let instance = unsafe { &*params.cast::<glib::Value>() };
        if let Ok(object) = instance.get::<glib::Object>() {
            if let Some(image) = object.downcast_ref::<gtk::Image>() {
                scale_image(image);
            }
        }
    }
    glib::ffi::GTRUE
}

fn scaled_size(image: &gtk::Image) -> Option<i32> {
    // SAFETY: only this module stores under the key, always an `i32`.
    unsafe { image.data::<i32>(IMAGE_KEY).map(|value| *value.as_ref()) }
}

fn scale_image(image: &gtk::Image) {
    if scaled_size(image).is_none() {
        image.connect_pixel_size_notify(rescale_image);
    }
    rescale_image(image);
}

fn rescale_image(image: &gtk::Image) {
    let current = image.pixel_size();
    if scaled_size(image) == Some(current) {
        return;
    }
    let base = if current > 0 {
        current
    } else {
        icon_size_px(image.property::<i32>("icon-size"))
    };
    let scaled = px(base);
    // SAFETY: see `scaled_size`.
    unsafe { image.set_data(IMAGE_KEY, scaled) };
    if scaled != current {
        image.set_pixel_size(scaled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_windows_scales_and_the_resolution_decides() {
        assert_eq!(decide(144.0, None, false), 1.0);
        assert_eq!(decide(96.0, None, true), 1.0);
        assert_eq!(decide(120.0, None, true), 1.25);
        assert_eq!(decide(144.0, None, true), 1.5);
        assert_eq!(decide(168.0, None, true), 1.75);
        assert_eq!(decide(0.0, None, true), 1.0);
        assert_eq!(decide(-1.0, None, true), 1.0);
        assert_eq!(decide(f64::NAN, None, true), 1.0);
        assert_eq!(decide(2000.0, None, true), 4.0);
    }

    #[test]
    fn the_environment_overrides_within_bounds() {
        assert_eq!(decide(144.0, Some("1"), true), 1.0);
        assert_eq!(decide(96.0, Some(" 1.25 "), true), 1.25);
        assert_eq!(decide(96.0, Some("9"), true), 4.0);
        assert_eq!(decide(96.0, Some("0.1"), true), 0.5);
        assert_eq!(decide(144.0, Some("auto"), true), 1.5);
        assert_eq!(decide(144.0, Some("0"), true), 1.5);
        assert_eq!(decide(144.0, Some("2"), false), 1.0);
    }

    #[test]
    fn lengths_scale_but_hairlines_and_unset_stay() {
        assert_eq!(px_with(300, 1.5), 450);
        assert_eq!(px_with(13, 1.25), 16);
        assert_eq!(px_with(-1, 1.5), -1);
        assert_eq!(px_with(1, 1.5), 1);
        assert_eq!(px_with(0, 1.5), 0);
        assert_eq!(px_with(-8, 1.5), -12);
        assert_eq!(px_with(300, 1.0), 300);
    }

    #[test]
    fn style_sheets_scale_pixel_lengths_only() {
        let css = "button { padding: 6px 9px; border: 1px solid #12px; min-height: 22px; \
                   font-size: 13px; outline-offset: -1px; margin: -4px; }";
        assert_eq!(
            css_with(css, 1.5),
            "button { padding: 9px 14px; border: 1px solid #12px; min-height: 33px; \
             font-size: 19.5px; outline-offset: -1px; margin: -6px; }"
        );
    }

    #[test]
    fn style_sheets_leave_other_units_identifiers_and_hairlines() {
        let css = ".impro-8px { font-size: 11pt; width: 50%; margin: 0.5px; \
                   transition: 128ms; padding: 2em; box-shadow: 0 5px 17px alpha(#11191d, 0.38); }";
        assert_eq!(
            css_with(css, 1.5),
            ".impro-8px { font-size: 11pt; width: 50%; margin: 0.5px; \
             transition: 128ms; padding: 2em; box-shadow: 0 8px 26px alpha(#11191d, 0.38); }"
        );
    }

    #[test]
    fn font_sizes_keep_a_decimal_and_other_lengths_round() {
        assert_eq!(
            css_with("a { font-size: 12.5px; padding: 3px; }", 1.25),
            "a { font-size: 15.6px; padding: 4px; }"
        );
        assert_eq!(
            css_with("a{font-size:14px}b{min-width:14px}", 1.5),
            "a{font-size:21px}b{min-width:21px}"
        );
    }

    #[test]
    fn identity_factor_returns_the_text_unchanged() {
        let css = "a { padding: 3px; }";
        assert_eq!(css_with(css, 1.0), css);
        assert_eq!(css_with("a { color: red; }", 1.5), "a { color: red; }");
    }

    #[test]
    fn units_that_merely_start_with_px_are_not_pixels() {
        assert_eq!(
            css_with("a { x: 3pxl; y: 4px; }", 2.0),
            "a { x: 3pxl; y: 8px; }"
        );
    }
}
