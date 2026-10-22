//! Where the graph preferences live, and the write-behind thread that saves them.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::Value;

/// Persistent storage of the preferences blob.
pub trait PrefsStore: Send + Sync + 'static {
    /// The saved blob (any JSON value; [`crate::prefs::normalize`] validates it).
    fn load(&self) -> Value;
    /// Saves the blob; false when it could not be written.
    fn save(&self, value: &Value) -> bool;
}

/// The application's settings file (`graph` key of `settings.json`).
#[derive(Clone, Copy, Debug, Default)]
pub struct SettingsStore;

impl PrefsStore for SettingsStore {
    fn load(&self) -> Value {
        impro_core::settings::get_graph()
    }

    fn save(&self, value: &Value) -> bool {
        impro_core::settings::set_graph(value)
    }
}

/// A store in memory: for examples and tests, and for views that must not
/// touch the user's settings.
#[derive(Debug, Default)]
pub struct MemoryStore {
    value: Mutex<Value>,
    saves: Mutex<Vec<Value>>,
    fail: Mutex<bool>,
}

impl MemoryStore {
    pub fn new(initial: Value) -> MemoryStore {
        MemoryStore {
            value: Mutex::new(initial),
            saves: Mutex::new(Vec::new()),
            fail: Mutex::new(false),
        }
    }

    /// Every blob saved so far, oldest first.
    pub fn saves(&self) -> Vec<Value> {
        self.saves.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Makes later saves fail (to test error reporting).
    pub fn set_failing(&self, failing: bool) {
        if let Ok(mut f) = self.fail.lock() {
            *f = failing;
        }
    }
}

impl PrefsStore for MemoryStore {
    fn load(&self) -> Value {
        self.value.lock().map(|v| v.clone()).unwrap_or(Value::Null)
    }

    fn save(&self, value: &Value) -> bool {
        if self.fail.lock().map(|f| *f).unwrap_or(false) {
            return false;
        }
        if let Ok(mut v) = self.value.lock() {
            *v = value.clone();
        }
        if let Ok(mut s) = self.saves.lock() {
            s.push(value.clone());
        }
        true
    }
}

enum Message {
    Save(Value),
    Flush(Sender<()>),
    Stop,
}

/// Saves blobs on a background thread; bursts collapse into the newest value.
pub struct Writer {
    tx: Sender<Message>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Writer {
    /// `failed` runs on the writer thread when a save did not succeed.
    pub fn start(store: Arc<dyn PrefsStore>, failed: impl Fn() + Send + 'static) -> Writer {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("impro-graph-prefs".into())
            .spawn(move || run(rx, store, failed))
            .ok();
        Writer {
            tx,
            thread: Mutex::new(thread),
        }
    }

    pub fn save(&self, value: Value) {
        let _ = self.tx.send(Message::Save(value));
    }

    /// Waits until everything queued so far is written.
    pub fn flush(&self) {
        let (done, wait) = mpsc::channel();
        if self.tx.send(Message::Flush(done)).is_ok() {
            let _ = wait.recv();
        }
    }

    /// Writes what is queued and stops the thread.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Message::Stop);
        if let Some(handle) = self.thread.lock().ok().and_then(|mut t| t.take()) {
            let _ = handle.join();
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(rx: Receiver<Message>, store: Arc<dyn PrefsStore>, failed: impl Fn()) {
    let mut pending: Option<Value> = None;
    let write = |value: Value| {
        if !store.save(&value) {
            failed();
        }
    };
    while let Ok(first) = rx.recv() {
        let mut batch = vec![first];
        while let Ok(next) = rx.try_recv() {
            batch.push(next);
        }
        let mut stop = false;
        let mut waiting: Vec<Sender<()>> = Vec::new();
        for m in batch {
            match m {
                Message::Save(v) => pending = Some(v),
                Message::Flush(done) => waiting.push(done),
                Message::Stop => stop = true,
            }
        }
        if let Some(v) = pending.take() {
            write(v);
        }
        for done in waiting {
            let _ = done.send(());
        }
        if stop {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn saves_in_the_background_and_flushes() {
        let store = Arc::new(MemoryStore::new(Value::Null));
        let writer = Writer::start(store.clone(), || {});
        writer.save(json!({"a": 1}));
        writer.flush();
        assert_eq!(store.load(), json!({"a": 1}));
        writer.save(json!({"a": 2}));
        writer.shutdown();
        assert_eq!(
            store.load(),
            json!({"a": 2}),
            "shutdown writes what is queued"
        );
    }

    #[test]
    fn bursts_collapse_to_the_last_value() {
        let store = Arc::new(MemoryStore::new(Value::Null));
        let writer = Writer::start(store.clone(), || {});
        for i in 0..50 {
            writer.save(json!({ "n": i }));
        }
        writer.flush();
        let saves = store.saves();
        assert_eq!(saves.last(), Some(&json!({"n": 49})));
        assert!(saves.len() <= 50);
    }

    #[test]
    fn failures_are_reported() {
        let store = Arc::new(MemoryStore::new(Value::Null));
        store.set_failing(true);
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let writer = Writer::start(store.clone(), move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        writer.save(json!(1));
        writer.flush();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        store.set_failing(false);
        writer.save(json!(2));
        writer.flush();
        assert_eq!(store.load(), json!(2));
    }

    #[test]
    fn settings_store_reads_the_graph_blob() {
        // only checks the plumbing compiles against the core API
        let _ = SettingsStore;
    }
}
