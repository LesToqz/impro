//! Write-behind settings persistence: changes are merged into a pending map on the
//! main thread and written by one worker thread, coalescing bursts (a slider drag
//! is one write). Failures are reported on the main thread through the handler set
//! with [`set_save_failed_handler`]; the in-memory preference is never rolled back.
use impro_core::settings::{Store, Values};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        mpsc::{channel, RecvTimeoutError, Sender},
        Arc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

/// A burst is written once its last change is this old...
const QUIET: Duration = Duration::from_millis(25);
/// ...or at the latest this long after its first change.
const MAX_DELAY: Duration = Duration::from_millis(250);

type Pending = HashMap<PathBuf, Values>;

enum Msg {
    Wake,
    Flush(Sender<()>),
}

#[derive(Default)]
struct Shared {
    pending: Pending,
    inflight: Pending,
}

struct Writer {
    shared: Arc<Mutex<Shared>>,
    tx: Sender<Msg>,
}

fn lock(shared: &Mutex<Shared>) -> std::sync::MutexGuard<'_, Shared> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_all(shared: &Mutex<Shared>) {
    let batch = {
        let mut guard = lock(shared);
        let batch = std::mem::take(&mut guard.pending);
        guard.inflight = batch.clone();
        batch
    };
    for (path, values) in batch {
        if Store::new(&path).save(&values).is_err() {
            let reported = path.clone();
            glib::MainContext::default().invoke(move || report_failure(&reported));
        }
    }
    lock(shared).inflight.clear();
}

fn writer() -> &'static Writer {
    static WRITER: OnceLock<Writer> = OnceLock::new();
    WRITER.get_or_init(|| {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let (tx, rx) = channel::<Msg>();
        let worker = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("impro-settings-writer".into())
            .spawn(move || {
                let mut acks: Vec<Sender<()>> = Vec::new();
                while let Ok(first) = rx.recv() {
                    let started = Instant::now();
                    match first {
                        Msg::Flush(ack) => acks.push(ack),
                        Msg::Wake => loop {
                            let remaining = MAX_DELAY.saturating_sub(started.elapsed());
                            match rx.recv_timeout(QUIET.min(remaining)) {
                                Ok(Msg::Wake) => {}
                                Ok(Msg::Flush(ack)) => {
                                    acks.push(ack);
                                    break;
                                }
                                Err(RecvTimeoutError::Timeout)
                                | Err(RecvTimeoutError::Disconnected) => break,
                            }
                        },
                    }
                    write_all(&worker);
                    for ack in acks.drain(..) {
                        let _ = ack.send(());
                    }
                }
            });
        if let Err(error) = spawned {
            eprintln!("impro-settings-ui: cannot start the settings writer: {error}");
        }
        Writer { shared, tx }
    })
}

/// Queue `values` for `path`; later values for the same key win. Returns at once.
pub fn save_async_to(path: &Path, values: Values) {
    let writer = writer();
    {
        let mut guard = lock(&writer.shared);
        guard
            .pending
            .entry(path.to_path_buf())
            .or_default()
            .extend(values);
    }
    if writer.tx.send(Msg::Wake).is_err() {
        // The worker could not start: write synchronously rather than lose the change.
        write_all(&writer.shared);
    }
}

/// Queue `values` for the current settings file ([`impro_core::settings::settings_path`]).
pub fn save_async(values: Values) {
    save_async_to(&impro_core::settings::settings_path(), values);
}

/// Queue one key.
pub fn save_value_async(key: &str, value: Value) {
    let mut values = Values::new();
    values.insert(key.to_owned(), value);
    save_async(values);
}

/// A value of `key` queued or being written for `path` but possibly not yet on disk.
pub fn unsaved_value(path: &Path, key: &str) -> Option<Value> {
    let guard = lock(&writer().shared);
    guard
        .pending
        .get(path)
        .and_then(|values| values.get(key))
        .or_else(|| guard.inflight.get(path).and_then(|values| values.get(key)))
        .cloned()
}

/// Block until everything queued so far is on disk. Call at application shutdown.
pub fn flush() {
    let writer = writer();
    let (ack, done) = channel();
    if writer.tx.send(Msg::Flush(ack)).is_ok() {
        let _ = done.recv_timeout(Duration::from_secs(10));
    } else {
        write_all(&writer.shared);
    }
}

type FailureHandler = Rc<dyn Fn(&Path)>;

thread_local! {
    static HANDLER: RefCell<Option<FailureHandler>> = const { RefCell::new(None) };
}

/// Report failed writes (the path that could not be written) on the main thread.
/// Replaces any earlier handler; call from the GTK thread.
pub fn set_save_failed_handler(handler: impl Fn(&Path) + 'static) {
    HANDLER.with(|slot| *slot.borrow_mut() = Some(Rc::new(handler)));
}

fn report_failure(path: &Path) {
    let handler = HANDLER.with(|slot| slot.borrow().clone());
    match handler {
        Some(handler) => handler(path),
        None => eprintln!(
            "impro-settings-ui: could not save settings to {}",
            path.display()
        ),
    }
}
