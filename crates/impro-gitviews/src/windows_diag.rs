//! Portable WebView2 diagnostics: HRESULT/URI/enum formatting and the per-host timeline.
//! No native objects here, so every format is unit-tested on every platform.
use std::time::{Duration, Instant};

/// Longest URI printed anywhere in diagnostics.
const URI_LIMIT: usize = 120;
/// Timeline lines kept per host; later lines only bump a counter.
const LINE_LIMIT: usize = 300;

/// `0x80070057`-style rendering of an HRESULT.
pub(crate) fn hresult(code: i32) -> String {
    format!("0x{:08X}", code as u32)
}

/// `text` cut to `limit` characters (with the number of dropped ones), controls replaced.
pub(crate) fn clip(text: &str, limit: usize) -> String {
    let total = text.chars().count();
    let mut out: String = text.chars().take(limit).map(printable).collect();
    if total > limit {
        out.push_str(&format!("...(+{} chars)", total - limit));
    }
    out
}

/// A URI that is safe to print: at most 120 characters, control characters replaced,
/// and `data:` documents reduced to their media type plus length (never the page html).
pub(crate) fn describe_uri(uri: &str) -> String {
    let total = uri.chars().count();
    if uri
        .get(..5)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("data:"))
    {
        let head: String = uri
            .chars()
            .take_while(|c| *c != ',')
            .take(URI_LIMIT)
            .map(printable)
            .collect();
        return format!("{head},<{total} chars total, content omitted>");
    }
    clip(uri, URI_LIMIT)
}
fn printable(c: char) -> char {
    if c.is_control() {
        '?'
    } else {
        c
    }
}

/// `COREWEBVIEW2_WEB_ERROR_STATUS` name (values 0..=18 in the SDK).
pub(crate) fn web_error_name(status: i32) -> &'static str {
    match status {
        0 => "UNKNOWN",
        1 => "CERTIFICATE_COMMON_NAME_IS_INCORRECT",
        2 => "CERTIFICATE_EXPIRED",
        3 => "CLIENT_CERTIFICATE_CONTAINS_ERRORS",
        4 => "CERTIFICATE_REVOKED",
        5 => "CERTIFICATE_IS_INVALID",
        6 => "SERVER_UNREACHABLE",
        7 => "TIMEOUT",
        8 => "ERROR_HTTP_INVALID_SERVER_RESPONSE",
        9 => "CONNECTION_ABORTED",
        10 => "CONNECTION_RESET",
        11 => "DISCONNECTED",
        12 => "CANNOT_CONNECT",
        13 => "HOST_NAME_NOT_RESOLVED",
        14 => "OPERATION_CANCELED",
        15 => "REDIRECT_FAILED",
        16 => "UNEXPECTED_ERROR",
        17 => "VALID_AUTHENTICATION_CREDENTIALS_REQUIRED",
        18 => "VALID_PROXY_AUTHENTICATION_REQUIRED",
        _ => "?",
    }
}

/// `COREWEBVIEW2_PROCESS_FAILED_KIND` name (values 0..=9 in the SDK).
pub(crate) fn process_failed_name(kind: i32) -> &'static str {
    match kind {
        0 => "BROWSER_PROCESS_EXITED",
        1 => "RENDER_PROCESS_EXITED",
        2 => "RENDER_PROCESS_UNRESPONSIVE",
        3 => "FRAME_RENDER_PROCESS_EXITED",
        4 => "UTILITY_PROCESS_EXITED",
        5 => "SANDBOX_HELPER_PROCESS_EXITED",
        6 => "GPU_PROCESS_EXITED",
        7 => "PPAPI_PLUGIN_PROCESS_EXITED",
        8 => "PPAPI_BROKER_PROCESS_EXITED",
        9 => "UNKNOWN_PROCESS_EXITED",
        _ => "?",
    }
}

/// Stderr echo is on for `IMPRO_GIT_DEBUG=1` (product) and `IMPRO_GIT_DIAG=1` (test harness).
pub(crate) fn echo_enabled(debug: Option<&str>, diag: Option<&str>) -> bool {
    debug == Some("1") || diag == Some("1")
}

/// Milestones of one page load, in the order they must happen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Phase {
    Environment,
    Controller,
    Shim,
    NavigationStarted,
    NavigationCompleted,
    Ready,
}
const PHASES: [(Phase, &str); 6] = [
    (Phase::Environment, "environment"),
    (Phase::Controller, "controller"),
    (Phase::Shim, "shim_registered"),
    (Phase::NavigationStarted, "navigation_started"),
    (Phase::NavigationCompleted, "navigation_completed"),
    (Phase::Ready, "ready"),
];
impl Phase {
    pub(crate) fn label(self) -> &'static str {
        PHASES[self as usize].1
    }
}

pub(crate) struct Diagnostics {
    id: usize,
    echo: bool,
    start: Instant,
    lines: Vec<String>,
    dropped: usize,
    marks: [Option<Duration>; 6],
    failure: Option<String>,
}
impl Diagnostics {
    pub(crate) fn new(id: usize, echo: bool) -> Self {
        Self {
            id,
            echo,
            start: Instant::now(),
            lines: Vec::new(),
            dropped: 0,
            marks: [None; 6],
            failure: None,
        }
    }
    pub(crate) fn note(&mut self, text: &str) {
        self.note_at(self.start.elapsed(), text);
    }
    pub(crate) fn note_at(&mut self, elapsed: Duration, text: &str) {
        let line = format!("+{}ms {text}", elapsed.as_millis());
        if self.echo {
            eprintln!("[impro-gitviews webview2#{}] {line}", self.id);
        }
        if self.lines.len() < LINE_LIMIT {
            self.lines.push(line);
        } else {
            self.dropped += 1;
        }
    }
    /// Record a milestone (first occurrence only) and a timeline line for it.
    pub(crate) fn mark(&mut self, phase: Phase, text: &str) {
        self.mark_at(self.start.elapsed(), phase, text);
    }
    pub(crate) fn mark_at(&mut self, elapsed: Duration, phase: Phase, text: &str) {
        self.marks[phase as usize].get_or_insert(elapsed);
        self.note_at(elapsed, &format!("[{}] {text}", phase.label()));
    }
    /// A failed call: logged, and kept as the concrete cause for the user-facing code line.
    pub(crate) fn fail(&mut self, text: &str) {
        self.failure = Some(text.to_owned());
        self.note(&format!("FAILED {text}"));
    }
    pub(crate) fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
    /// The last milestone reached, i.e. where a stall happened.
    pub(crate) fn last_phase(&self) -> Option<Phase> {
        PHASES
            .iter()
            .rev()
            .map(|(phase, _)| *phase)
            .find(|phase| self.marks[*phase as usize].is_some())
    }
    /// `environment=412ms controller=980ms ... navigation_completed=- ready=-`
    pub(crate) fn summary(&self) -> String {
        PHASES
            .iter()
            .map(|(phase, label)| match self.marks[*phase as usize] {
                Some(at) => format!("{label}={}ms", at.as_millis()),
                None => format!("{label}=-"),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
    /// Phase timing plus the full timeline.
    pub(crate) fn render(&self) -> String {
        let mut out = format!("phases: {}", self.summary());
        for line in &self.lines {
            out.push('\n');
            out.push_str(line);
        }
        if self.dropped > 0 {
            out.push_str(&format!("\n... {} more lines dropped", self.dropped));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hresults_are_fixed_width_hex() {
        assert_eq!(hresult(0), "0x00000000");
        assert_eq!(hresult(-2147024809), "0x80070057");
        assert_eq!(hresult(i32::MIN), "0x80000000");
    }

    #[test]
    fn uris_are_truncated_and_documents_never_printed() {
        assert_eq!(describe_uri("about:blank"), "about:blank");
        let long = format!("https://example.invalid/{}", "a".repeat(300));
        let shown = describe_uri(&long);
        assert!(shown.starts_with("https://example.invalid/aaa"));
        assert!(shown.ends_with("...(+204 chars)"), "{shown}");
        assert_eq!(shown.chars().count(), 120 + "...(+204 chars)".len());
        let page = format!(
            "data:text/html;charset=utf-8;base64,{}",
            "SECRETHTML".repeat(50)
        );
        let shown = describe_uri(&page);
        assert!(!shown.contains("SECRET"), "{shown}");
        assert!(
            shown.starts_with("data:text/html;charset=utf-8;base64,<"),
            "{shown}"
        );
        assert!(shown.contains(&format!("{} chars total", page.chars().count())));
        assert!(!describe_uri("DATA:text/html,<h1>x</h1>").contains("<h1>"));
        // Multi-byte input is cut on character boundaries; controls cannot forge lines.
        let wide = "é".repeat(200);
        assert_eq!(
            describe_uri(&wide).chars().filter(|c| *c == 'é').count(),
            120
        );
        assert_eq!(describe_uri("a\nb\r+1ms c"), "a?b?+1ms c");
        assert_eq!(describe_uri(""), "");
        assert_eq!(describe_uri("dat"), "dat");
    }

    #[test]
    fn sdk_enum_names_cover_the_documented_range() {
        assert_eq!(web_error_name(0), "UNKNOWN");
        assert_eq!(web_error_name(14), "OPERATION_CANCELED");
        assert_eq!(web_error_name(18), "VALID_PROXY_AUTHENTICATION_REQUIRED");
        assert_eq!(web_error_name(19), "?");
        assert_eq!(web_error_name(-1), "?");
        assert_eq!(process_failed_name(0), "BROWSER_PROCESS_EXITED");
        assert_eq!(process_failed_name(6), "GPU_PROCESS_EXITED");
        assert_eq!(process_failed_name(9), "UNKNOWN_PROCESS_EXITED");
        assert_eq!(process_failed_name(10), "?");
    }

    #[test]
    fn echo_needs_an_exact_opt_in() {
        assert!(echo_enabled(Some("1"), None));
        assert!(echo_enabled(None, Some("1")));
        assert!(!echo_enabled(None, None));
        assert!(!echo_enabled(Some("true"), Some("0")));
    }

    #[test]
    fn timeline_reports_phase_timings_and_the_stall_point() {
        let mut d = Diagnostics::new(7, false);
        assert_eq!(d.last_phase(), None);
        d.mark_at(Duration::from_millis(412), Phase::Environment, "ok");
        d.mark_at(Duration::from_millis(980), Phase::Controller, "ok");
        d.mark_at(Duration::from_millis(1002), Phase::Shim, "ok");
        d.mark_at(Duration::from_millis(9999), Phase::Shim, "again");
        assert_eq!(d.last_phase(), Some(Phase::Shim));
        assert_eq!(
            d.summary(),
            "environment=412ms controller=980ms shim_registered=1002ms \
             navigation_started=- navigation_completed=- ready=-"
        );
        d.note_at(Duration::from_millis(1010), "NavigationStarting id=2");
        let text = d.render();
        assert!(text.starts_with("phases: environment=412ms"));
        assert!(text.contains("\n+412ms [environment] ok"));
        assert!(text.ends_with("+1010ms NavigationStarting id=2"));
        assert_eq!(Phase::NavigationCompleted.label(), "navigation_completed");
    }

    #[test]
    fn failures_are_remembered_and_the_timeline_is_bounded() {
        let mut d = Diagnostics::new(1, false);
        assert_eq!(d.failure(), None);
        d.fail("SetIsMuted HRESULT 0x80004002");
        assert_eq!(d.failure(), Some("SetIsMuted HRESULT 0x80004002"));
        assert!(d.render().contains("FAILED SetIsMuted HRESULT 0x80004002"));
        for i in 0..400 {
            d.note(&format!("line {i}"));
        }
        let text = d.render();
        assert!(text.ends_with("... 101 more lines dropped"), "{text}");
        assert_eq!(text.lines().count(), 1 + LINE_LIMIT + 1);
    }
}
