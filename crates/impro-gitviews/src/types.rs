//! Platform-independent value types shared by the WebKit host (Unix) and the Windows stub.
use impro_core::settings;
use serde_json::{json, Value};
use std::time::Duration;

/// Validated settings shared by all graph panes. Keys match the settings dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphSettings {
    pub orientation: String,
    pub max_commits: i64,
    pub show_remotes: bool,
    pub show_tags: bool,
    pub time_format: String,
}
impl Default for GraphSettings {
    fn default() -> Self {
        Self {
            orientation: "horizontal".into(),
            max_commits: 1500,
            show_remotes: true,
            show_tags: true,
            time_format: "relative".into(),
        }
    }
}
impl GraphSettings {
    /// Accept either the raw Git graph blob or the settings dialog's flat values.
    pub fn from_value(value: &Value) -> Self {
        let mut s = Self::default();
        let get = |k: &str| {
            value
                .get(format!("gitgraph_{k}"))
                .or_else(|| value.get(k))
                .unwrap_or(&Value::Null)
        };
        if get("orientation") == "vertical" {
            s.orientation = "vertical".into();
        }
        s.max_commits = get("max_commits")
            .as_i64()
            .unwrap_or(1500)
            .clamp(100, 10000);
        s.show_remotes = get("show_remotes").as_bool().unwrap_or(true);
        s.show_tags = get("show_tags").as_bool().unwrap_or(true);
        if get("time_format") == "absolute" {
            s.time_format = "absolute".into();
        }
        s
    }
    pub fn to_value(&self) -> Value {
        json!({"orientation":self.orientation,"max_commits":self.max_commits,"show_remotes":self.show_remotes,"show_tags":self.show_tags,"time_format":self.time_format})
    }
    /// Persist opaque settings on a worker; the callback reports whether saving succeeded.
    pub fn save(&self, done: impl FnOnce(bool) + 'static) {
        let value = self.to_value();
        #[allow(deprecated)]
        let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
        let mut done = Some(done);
        rx.attach(None, move |ok| {
            if let Some(done) = done.take() {
                done(ok);
            }
            glib::ControlFlow::Break
        });
        std::thread::spawn(move || {
            let _ = tx.send(settings::set_gitgraph(&value));
        });
    }
}

/// Timings from construction/data delivery through two browser frame boundaries.
#[derive(Clone, Copy, Debug, Default)]
pub struct PaintTimings {
    pub open_to_first_paint: Option<Duration>,
    pub data_to_paint: Option<Duration>,
}
