//! Portable Windows engine policy. No native objects or environment mutations here.
use std::{ffi::OsStr, path::PathBuf};

pub(crate) const RUNTIME_DOWNLOAD: &str =
    "https://developer.microsoft.com/microsoft-edge/webview2/";
pub(crate) const BRIDGE: &str = r#"(() => {
  if (window.top !== window) return;
  const post = message => window.chrome.webview.postMessage(message);
  Object.defineProperty(window, 'webkit', {value: Object.freeze({messageHandlers:
    Object.freeze({impro: Object.freeze({postMessage: post})})}), configurable: false});
  window.addEventListener('error', e => post({type:'error', message:e.message || 'Git page error'}));
  window.addEventListener('unhandledrejection', e => post({type:'error', message:String(e.reason)}));
  for (const key of ['localStorage', 'sessionStorage', 'indexedDB', 'openDatabase']) {
    try { Object.defineProperty(window, key, {get: () => undefined}); } catch (_) {}
  }
  window.open = () => null;
})();"#;

// No network, media, WebGL, background services, or persistent page cache is needed.
// Storage APIs are disabled by BRIDGE; requests are also blocked by the host.
pub(crate) const BROWSER_ARGUMENTS: &str = "--disable-features=msEdgeSidebarV2,BackForwardCache,WebPayments,AutofillServerCommunication --disable-background-networking --disable-component-update --disable-domain-reliability --disable-sync --disable-webgl --disable-accelerated-2d-canvas --autoplay-policy=user-gesture-required --no-pings";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    pub devtools: bool,
    pub context_menus: bool,
    pub browser_accelerators: bool,
    pub status_bar: bool,
    pub zoom: bool,
    pub error_page: bool,
    pub passwords: bool,
    pub autofill: bool,
    pub swipe: bool,
}
impl Settings {
    pub(crate) fn new(devtools: Option<&str>) -> Self {
        Self {
            devtools: devtools == Some("1"),
            context_menus: false,
            browser_accelerators: false,
            status_bar: false,
            zoom: false,
            error_page: false,
            passwords: false,
            autofill: false,
            swipe: false,
        }
    }
}

pub(crate) fn forced_notice(value: Option<&str>) -> bool {
    value == Some("notice")
}

pub(crate) fn user_data_folder(local: Option<&OsStr>) -> Result<PathBuf, &'static str> {
    let local = local
        .filter(|s| !s.is_empty())
        .ok_or("LOCALAPPDATA is unavailable")?;
    // Check Windows syntax on every platform so portable tests cover the actual rule.
    let text = local.to_string_lossy();
    let bytes = text.as_bytes();
    let absolute = text.starts_with(r"\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'));
    if !absolute || text.contains('\0') {
        return Err("LOCALAPPDATA must be an absolute Windows directory");
    }
    Ok(PathBuf::from(local).join("impro").join("WebView2"))
}

pub(crate) fn physical_size(width: i32, height: i32, scale: i32) -> (i32, i32) {
    (
        width.max(0).saturating_mul(scale.max(1)),
        height.max(0).saturating_mul(scale.max(1)),
    )
}

/// Win32 virtual key -> GDK key name. Only application shortcuts leave the page.
pub(crate) fn shortcut(key: u32, ctrl: bool, shift: bool, alt: bool) -> Option<&'static str> {
    if ctrl {
        match (key, shift, alt) {
            (0x09, _, false) => Some(if shift { "ISO_Left_Tab" } else { "Tab" }),
            (0x21, false, false) => Some("Page_Up"),
            (0x22, false, false) => Some("Page_Down"),
            (0x50, _, false) => Some("p"),
            (0x53, _, false) => Some("s"),
            (0x47, _, true) => Some("g"),
            (0x48, false, true) => Some("h"),
            (0x54, false, true) => Some("t"),
            (0x57, false, false) => Some("w"),
            (0x4f, false, false) => Some("o"),
            (0x51, false, false) => Some("q"),
            (0x42, false, false) => Some("b"),
            (0x46, true, false) => Some("f"),
            (0x4d, true, false) => Some("m"),
            (0x35, true, false) => Some("percent"),
            (0x7a, true, false) => Some("F11"),
            (0xc0, false, _) => Some("grave"),
            (0xbc, false, false) => Some("comma"),
            (0xbf, false, false) => Some("slash"),
            (0xbb, false, false) => Some("equal"),
            (0xbd, false, false) => Some("minus"),
            (0x30, false, false) => Some("0"),
            _ => None,
        }
    } else if !alt {
        match (key, shift) {
            (0x1b, false) => Some("Escape"),
            (0x70, false) => Some("F1"),
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn privacy_settings_are_explicit_and_devtools_opt_in() {
        let policy = Settings::new(None);
        assert_eq!(
            policy,
            Settings {
                devtools: false,
                context_menus: false,
                browser_accelerators: false,
                status_bar: false,
                zoom: false,
                error_page: false,
                passwords: false,
                autofill: false,
                swipe: false
            }
        );
        assert!(Settings::new(Some("1")).devtools);
        assert!(!Settings::new(Some("true")).devtools);
        assert!(BROWSER_ARGUMENTS.contains("BackForwardCache"));
    }
    #[test]
    fn bridge_preserves_object_messages_and_reports_script_errors() {
        assert!(BRIDGE.contains("postMessage(message)"));
        assert!(!BRIDGE.contains("JSON.stringify"));
        assert!(BRIDGE.contains("messageHandlers"));
        assert!(BRIDGE.contains("unhandledrejection"));
        assert!(RUNTIME_DOWNLOAD.starts_with("https://developer.microsoft.com/"));
    }
    #[test]
    fn data_folder_never_falls_back_to_the_executable_directory() {
        assert!(user_data_folder(None).is_err());
        for invalid in ["", "relative", r"C:relative", r"\rooted", "C:\\bad\0path"] {
            assert!(
                user_data_folder(Some(OsStr::new(invalid))).is_err(),
                "{invalid:?}"
            );
        }
        for local in [
            r"C:\Users\Public\AppData\Local",
            r"\\server\profile\Local",
            "D:/Local",
        ] {
            let path = user_data_folder(Some(OsStr::new(local))).unwrap();
            assert_eq!(path, PathBuf::from(local).join("impro").join("WebView2"));
        }
    }
    #[test]
    fn fallback_is_an_exact_opt_in() {
        assert!(forced_notice(Some("notice")));
        assert!(!forced_notice(None));
        assert!(!forced_notice(Some("anything")));
    }
    #[test]
    fn shortcuts_leave_page_navigation_and_editing_keys_alone() {
        for key in [0x57, 0x09, 0x50, 0x42] {
            assert!(shortcut(key, true, false, false).is_some());
        }
        assert_eq!(shortcut(0x09, true, true, false), Some("ISO_Left_Tab"));
        assert_eq!(shortcut(0x50, true, true, false), Some("p"));
        assert_eq!(shortcut(0x48, true, false, true), Some("h"));
        assert_eq!(shortcut(0x1b, false, false, false), Some("Escape"));
        for key in [0x09, 0x0d, 0x21, 0x22, 0x25, 0x26, 0x27, 0x28, 0xbf, 0x7a] {
            assert!(shortcut(key, false, false, false).is_none());
        }
        for key in [0x41, 0x43, 0x56, 0x58, 0x5a] {
            assert!(shortcut(key, true, false, false).is_none());
        }
        assert!(
            shortcut(0x50, true, false, true).is_none(),
            "AltGr must stay in the page"
        );
    }
    #[test]
    fn dpi_bounds_use_physical_pixels_without_double_scaling() {
        assert_eq!(physical_size(800, 600, 1), (800, 600));
        assert_eq!(physical_size(800, 600, 2), (1600, 1200));
        assert_eq!(physical_size(-1, 600, 0), (0, 600));
        assert_eq!(physical_size(i32::MAX, 1, 2), (i32::MAX, 2));
    }
}
