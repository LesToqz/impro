//! Wire protocol shared by the WebKitGTK and WebView2 hosts.
use serde_json::Value;

pub(crate) fn parse_message(raw: &str) -> Option<Value> {
    serde_json::from_str(raw).ok()
}

/// Commands issued during engine creation are delivered in order at the page handshake.
#[derive(Default)]
pub(crate) struct ScriptQueue(Vec<String>);
impl ScriptQueue {
    pub(crate) fn push(&mut self, script: String) {
        self.0.push(script);
    }
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
    pub(crate) fn ready_script(&mut self, api: &str, active: bool) -> String {
        let queue = std::mem::take(&mut self.0);
        let lifecycle = if active { "resume" } else { "pause" };
        format!("window.__improActive={active};if(typeof window.{api}.{lifecycle}==='function')window.{api}.{lifecycle}();\n{}", queue.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_payloads_preserve_the_page_protocol() {
        for value in [
            json!({"type":"ready"}),
            json!({"type":"painted","generation":9}),
            json!({"type":"open-commit","hash":"abc123"}),
            json!({"type":"open-file","path":"src/a b.rs","line":3}),
            json!({"type":"orientation","value":"vertical"}),
            json!({"type":"refresh"}),
            json!({"type":"expand","path":"a.rs","start":3,"end":9}),
            json!({"type":"copy","text":"a\n\"b"}),
            json!({"type":"error","message":"日本語"}),
        ] {
            assert_eq!(parse_message(&value.to_string()), Some(value));
        }
        assert!(parse_message("{broken").is_none());
        // Strings stay strings, matching WebKit's to_json; the dispatcher rejects them.
        assert_eq!(
            parse_message(r#""{\"type\":\"ready\"}""#),
            Some(json!(r#"{"type":"ready"}"#))
        );
    }

    #[test]
    fn ready_flushes_ordered_commands_once_after_lifecycle() {
        let mut queue = ScriptQueue::default();
        queue.push("theme();".into());
        queue.push("data();".into());
        let first = queue.ready_script("ImproGit", false);
        assert!(first.starts_with("window.__improActive=false;"));
        assert!(first.ends_with("theme();\ndata();"));
        assert!(first.find("pause").unwrap() < first.find("theme").unwrap());
        let next = queue.ready_script("ImproGit", true);
        assert!(next.contains("resume"));
        assert!(!next.contains("data();"));
        queue.push("stale();".into());
        queue.clear();
        assert!(!queue.ready_script("ImproCommit", true).contains("stale"));
    }
}
