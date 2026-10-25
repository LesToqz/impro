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
  for (const key of ['localStorage', 'sessionStorage', 'indexedDB', 'openDatabase', 'AudioContext', 'webkitAudioContext', 'RTCPeerConnection']) {
    try { Object.defineProperty(window, key, {get: () => undefined}); } catch (_) {}
  }
  window.open = () => null;
})();"#;

// No network, media, WebGL, background services, or persistent page cache is needed.
// Storage APIs are disabled by BRIDGE; requests are also blocked by the host.
pub(crate) const BROWSER_ARGUMENTS: &str = "--disable-features=msEdgeSidebarV2,BackForwardCache,WebPayments,AutofillServerCommunication --dns-prefetch-disable --disable-background-networking --disable-component-update --disable-domain-reliability --disable-sync --disable-webgl --disable-accelerated-2d-canvas --autoplay-policy=user-gesture-required --no-pings";

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

/// What the controller's single `NavigateToString` call may navigate to.
///
/// The WebView2 runtime does not navigate to `about:blank` for `NavigateToString`; it
/// issues a `data:text/html...` navigation (the page's own origin is `about:blank`).
/// Older documentation and the first host revision assumed `about:blank` here, which
/// cancelled the document itself. Both spellings are accepted, but only for the one
/// navigation the host armed; everything else is cancelled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavigationVerdict {
    /// The armed document navigation: let it run.
    Document,
    /// A harmless blank page while the document is still pending (not the document).
    Blank,
    Cancel(&'static str),
}

fn has_scheme(uri: &str, scheme: &str) -> bool {
    uri.get(..scheme.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(scheme))
}

fn navigation_verdict(armed: bool, uri: &str, redirected: bool) -> NavigationVerdict {
    if !armed {
        return NavigationVerdict::Cancel("no document navigation is pending");
    }
    if redirected {
        return NavigationVerdict::Cancel("redirected navigation");
    }
    if has_scheme(uri, "data:text/html") {
        NavigationVerdict::Document
    } else if uri.eq_ignore_ascii_case("about:blank") {
        NavigationVerdict::Blank
    } else {
        NavigationVerdict::Cancel("unexpected navigation target")
    }
}

/// How a `NavigationCompleted` event must be treated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Completion {
    DocumentLoaded,
    DocumentFailed,
    /// Completion of a navigation this host cancelled on purpose.
    PolicyCancelled,
    /// Success or cancellation of something that is not the document.
    Unrelated,
    /// A non-cancel failure of a navigation the host never saw start.
    UnknownFailure,
}

/// Navigation bookkeeping for one controller: one armed document navigation, identified
/// by its navigation id, and the ids this host cancelled.
#[derive(Default)]
pub(crate) struct Navigations {
    armed: bool,
    document: Option<u64>,
    cancelled: Vec<u64>,
}
impl Navigations {
    const CANCELLED_KEPT: usize = 64;
    /// Called right before `NavigateToString`.
    pub(crate) fn arm(&mut self) {
        *self = Self {
            armed: true,
            ..Self::default()
        };
    }
    /// The page said `ready`: no further document navigation may be accepted.
    pub(crate) fn disarm(&mut self) {
        self.armed = false;
    }
    pub(crate) fn document(&self) -> Option<u64> {
        self.document
    }
    pub(crate) fn starting(&mut self, id: u64, uri: &str, redirected: bool) -> NavigationVerdict {
        let verdict = navigation_verdict(self.armed, uri, redirected);
        match verdict {
            NavigationVerdict::Document => {
                self.armed = false;
                self.document = Some(id);
            }
            NavigationVerdict::Blank => {}
            NavigationVerdict::Cancel(_) => {
                if self.cancelled.len() == Self::CANCELLED_KEPT {
                    self.cancelled.remove(0);
                }
                self.cancelled.push(id);
            }
        }
        verdict
    }
    pub(crate) fn completed(&self, id: u64, success: bool, canceled_status: bool) -> Completion {
        if self.document == Some(id) {
            if success {
                Completion::DocumentLoaded
            } else {
                Completion::DocumentFailed
            }
        } else if self.cancelled.contains(&id) {
            Completion::PolicyCancelled
        } else if success || canceled_status {
            Completion::Unrelated
        } else {
            Completion::UnknownFailure
        }
    }
}

/// External resource policy: only inline schemes may load; everything else gets HTTP 403.
/// The document navigation itself (`data:`) must never be answered by the filter.
pub(crate) fn blocks_resource(uri: &str) -> bool {
    !["data:", "about:", "blob:"]
        .iter()
        .any(|scheme| has_scheme(uri, scheme))
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
    fn only_the_armed_document_navigation_is_allowed() {
        use NavigationVerdict::*;
        let data = "data:text/html;charset=utf-8;base64,PGh0bWw+";
        // (armed, uri, redirected) -> verdict
        let table = [
            (true, data, false, Document),
            (true, "DATA:TEXT/HTML,<p>", false, Document),
            (true, "about:blank", false, Blank),
            (true, "ABOUT:BLANK", false, Blank),
            (true, data, true, Cancel("redirected navigation")),
            (
                true,
                "https://example.invalid/",
                false,
                Cancel("unexpected navigation target"),
            ),
            (
                true,
                "file:///C:/Windows/win.ini",
                false,
                Cancel("unexpected navigation target"),
            ),
            (
                true,
                "data:text/plain,hi",
                false,
                Cancel("unexpected navigation target"),
            ),
            (
                true,
                "javascript:alert(1)",
                false,
                Cancel("unexpected navigation target"),
            ),
            (
                true,
                "about:blank#x",
                false,
                Cancel("unexpected navigation target"),
            ),
            (true, "", false, Cancel("unexpected navigation target")),
            (
                false,
                data,
                false,
                Cancel("no document navigation is pending"),
            ),
            (
                false,
                "about:blank",
                false,
                Cancel("no document navigation is pending"),
            ),
        ];
        for (armed, uri, redirected, expected) in table {
            assert_eq!(
                navigation_verdict(armed, uri, redirected),
                expected,
                "{uri}"
            );
        }
    }

    #[test]
    fn exactly_one_data_navigation_is_accepted_per_arming() {
        let data = "data:text/html;charset=utf-8;base64,PGh0bWw+";
        let mut n = Navigations::default();
        assert!(matches!(
            n.starting(1, data, false),
            NavigationVerdict::Cancel(_)
        ));
        n.arm();
        // An unrelated blank page neither consumes the arming nor becomes the document.
        assert_eq!(
            n.starting(2, "about:blank", false),
            NavigationVerdict::Blank
        );
        assert_eq!(n.document(), None);
        assert_eq!(n.starting(3, data, false), NavigationVerdict::Document);
        assert_eq!(n.document(), Some(3));
        // A second data: navigation (for example from page script) is cancelled.
        assert!(matches!(
            n.starting(4, data, false),
            NavigationVerdict::Cancel(_)
        ));
        assert!(matches!(
            n.starting(5, "about:blank", false),
            NavigationVerdict::Cancel(_)
        ));
        assert_eq!(n.document(), Some(3));
        // Re-arming (reload) forgets the previous document.
        n.arm();
        assert_eq!(n.document(), None);
        assert_eq!(n.starting(9, data, false), NavigationVerdict::Document);
        n.disarm();
        assert!(matches!(
            n.starting(10, data, false),
            NavigationVerdict::Cancel(_)
        ));
    }

    #[test]
    fn completion_is_classified_by_navigation_id() {
        use Completion::*;
        let data = "data:text/html,x";
        let mut n = Navigations::default();
        n.arm();
        n.starting(1, "https://example.invalid/", false);
        n.starting(2, data, false);
        // Our own cancellation never fails the page; the document failing always does.
        assert_eq!(n.completed(1, false, true), PolicyCancelled);
        assert_eq!(n.completed(2, false, true), DocumentFailed);
        assert_eq!(n.completed(2, false, false), DocumentFailed);
        assert_eq!(n.completed(2, true, false), DocumentLoaded);
        // Navigations never seen starting: success/cancel is ignored, other failure surfaces.
        assert_eq!(n.completed(7, true, false), Unrelated);
        assert_eq!(n.completed(7, false, true), Unrelated);
        assert_eq!(n.completed(7, false, false), UnknownFailure);
    }

    #[test]
    fn cancelled_ids_are_bounded() {
        let mut n = Navigations::default();
        for id in 0..200 {
            n.starting(id, "https://example.invalid/", false);
        }
        assert_eq!(n.cancelled.len(), Navigations::CANCELLED_KEPT);
        assert_eq!(n.completed(199, false, true), Completion::PolicyCancelled);
        assert_eq!(n.completed(0, false, true), Completion::Unrelated);
    }

    #[test]
    fn external_requests_are_blocked_but_inline_loads_pass() {
        for blocked in [
            "https://example.invalid/a.js",
            "http://127.0.0.1:8080/",
            "ftp://example.invalid/",
            "file:///C:/secret.txt",
            "wss://example.invalid/",
            "ms-appx://x/y",
            "",
        ] {
            assert!(blocks_resource(blocked), "{blocked}");
        }
        for inline in [
            "data:text/html;charset=utf-8;base64,PGh0bWw+",
            "DATA:image/png;base64,AAAA",
            "about:blank",
            "blob:null/1234",
        ] {
            assert!(!blocks_resource(inline), "{inline}");
        }
    }

    #[test]
    fn dpi_bounds_use_physical_pixels_without_double_scaling() {
        assert_eq!(physical_size(800, 600, 1), (800, 600));
        assert_eq!(physical_size(800, 600, 2), (1600, 1200));
        assert_eq!(physical_size(-1, 600, 0), (0, 600));
        assert_eq!(physical_size(i32::MAX, 1, 2), (i32::MAX, 2));
    }
}
