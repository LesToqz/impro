//! The indexing worker: owns the project index, applies live updates and builds
//! graphs off the main thread. Requests coalesce — a burst of changes produces
//! one build — and a new scan cancels the one in flight.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use super::scene::Built;
use crate::filter::{self, GraphOptions};
use crate::index::{CancelToken, ProjectIndex, ScanConfig};
use crate::model::Graph;

/// Which graph to build.
#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    /// Every file of the project.
    Global,
    /// Only the notes folder.
    Notes,
    /// The neighbourhood of one file.
    Local {
        path: PathBuf,
        depth: u32,
        incoming: bool,
        outgoing: bool,
    },
    /// A local graph without a file: nothing to show.
    Empty,
}

#[derive(Clone, Debug)]
pub struct BuildRequest {
    pub generation: u64,
    pub mode: Mode,
    pub options: GraphOptions,
}

/// A change of the files, in the order it happened.
#[derive(Debug)]
pub enum Update {
    /// An unsaved buffer (`Some`) or a re-read from disk (`None`).
    Text(PathBuf, Option<String>),
    Remove(PathBuf),
    Rename(PathBuf, PathBuf),
}

enum Command {
    /// Index `root` from scratch (or re-scan the current root).
    Scan(PathBuf, CancelToken),
    Update(Update),
    Build(BuildRequest),
    Stop,
}

/// The reply to a [`BuildRequest`].
#[derive(Debug)]
pub struct Output {
    pub generation: u64,
    pub result: Result<Built, String>,
    /// The files indexed so far.
    pub files: usize,
    /// Worker time spent (scan, updates and build), in milliseconds.
    pub elapsed_ms: f64,
}

/// Handle to the worker thread.
pub struct Host {
    tx: Sender<Command>,
    scan_token: Mutex<CancelToken>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Host {
    /// Starts the worker; every [`Output`] goes to `deliver`.
    pub fn start(deliver: impl Fn(Output) + Send + 'static) -> Host {
        let (tx, rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("impro-graph-index".into())
            .spawn(move || worker(rx, deliver))
            .ok();
        Host {
            tx,
            scan_token: Mutex::new(CancelToken::new()),
            thread: Mutex::new(thread),
        }
    }

    /// Replaces the index with a fresh scan of `root`; the scan in flight stops.
    pub fn scan(&self, root: &Path) {
        let token = CancelToken::new();
        if let Ok(mut current) = self.scan_token.lock() {
            current.cancel();
            *current = token.clone();
        }
        let _ = self.tx.send(Command::Scan(root.to_path_buf(), token));
    }

    pub fn update(&self, update: Update) {
        let _ = self.tx.send(Command::Update(update));
    }

    pub fn build(&self, request: BuildRequest) {
        let _ = self.tx.send(Command::Build(request));
    }

    /// Stops the worker and waits for it (a running scan is cancelled).
    pub fn shutdown(&self) {
        if let Ok(token) = self.scan_token.lock() {
            token.cancel();
        }
        let _ = self.tx.send(Command::Stop);
        if let Some(handle) = self.thread.lock().ok().and_then(|mut t| t.take()) {
            let _ = handle.join();
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn apply_update(index: &mut ProjectIndex, update: Update) {
    match update {
        Update::Text(path, text) => {
            index.update_file(&path, text.as_deref());
        }
        Update::Remove(path) => {
            index.remove(&path);
        }
        Update::Rename(from, to) => {
            index.rename(&from, &to);
        }
    }
}

/// Builds the requested graph.
pub fn build_graph(index: &ProjectIndex, request: &BuildRequest) -> Built {
    match &request.mode {
        Mode::Global => Built::global(filter::build(index, &request.options)),
        Mode::Notes => Built::global(filter::folder(
            index,
            &impro_core::notes::NOTES_DIR.replace(std::path::MAIN_SEPARATOR, "/"),
            &request.options,
        )),
        Mode::Local {
            path,
            depth,
            incoming,
            outgoing,
        } => {
            let local = filter::local(index, path, *depth, *incoming, *outgoing, &request.options);
            Built {
                focus: local.focus,
                layers: Some(local.layers),
                graph: local.graph,
            }
        }
        Mode::Empty => Built::global(Graph::empty()),
    }
}

fn worker(rx: Receiver<Command>, deliver: impl Fn(Output)) {
    let mut index: Option<ProjectIndex> = None;
    let mut root: Option<PathBuf> = None;
    while let Ok(first) = rx.recv() {
        let mut batch = vec![first];
        while let Ok(next) = rx.try_recv() {
            batch.push(next);
        }
        let started = Instant::now();
        let mut request: Option<BuildRequest> = None;
        let mut stop = false;
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            for command in batch {
                match command {
                    Command::Stop => {
                        stop = true;
                        break;
                    }
                    Command::Scan(path, token) => {
                        if token.is_cancelled() {
                            continue;
                        }
                        if root.as_deref() == Some(path.as_path()) && index.is_some() {
                            if let Some(i) = index.as_mut() {
                                let _ = i.rescan(&token);
                            }
                        } else {
                            match ProjectIndex::scan(&path, ScanConfig::default(), &token) {
                                Ok(fresh) => {
                                    index = Some(fresh);
                                    root = Some(path);
                                }
                                Err(_) => {
                                    // cancelled: a newer scan follows
                                }
                            }
                        }
                    }
                    Command::Update(u) => {
                        if let Some(i) = index.as_mut() {
                            apply_update(i, u);
                        }
                    }
                    Command::Build(r) => request = Some(r),
                }
            }
        }));
        if stop {
            return;
        }
        let Some(request) = request else {
            continue;
        };
        let result = match outcome {
            Err(_) => Err("the graph indexer stopped unexpectedly".to_string()),
            Ok(()) => match index.as_ref() {
                None => Ok(Built::default()),
                Some(i) => catch_unwind(AssertUnwindSafe(|| build_graph(i, &request)))
                    .map_err(|_| "building the graph failed".to_string()),
            },
        };
        deliver(Output {
            generation: request.generation,
            result,
            files: index.as_ref().map_or(0, ProjectIndex::len),
            elapsed_ms: started.elapsed().as_secs_f64() * 1e3,
        });
    }
}

/// Shared handle used by tests to wait for outputs.
pub type Inbox = Arc<Mutex<Vec<Output>>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("a.md"), "[[b]] and #topic").expect("write");
        fs::write(dir.path().join("b.md"), "[[c]]").expect("write");
        fs::write(dir.path().join("c.md"), "").expect("write");
        fs::create_dir(dir.path().join(".impro")).expect("dir");
        fs::create_dir(dir.path().join(".impro/Notes")).expect("dir");
        fs::write(dir.path().join(".impro/Notes/n.md"), "[[a]]").expect("write");
        dir
    }

    fn start() -> (Host, mpsc::Receiver<Output>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let host = Host::start(move |o| {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(o);
            }
        });
        (host, rx)
    }

    fn request(generation: u64, mode: Mode) -> BuildRequest {
        BuildRequest {
            generation,
            mode,
            options: GraphOptions {
                strict_filters: true,
                ..GraphOptions::default()
            },
        }
    }

    fn wait(rx: &mpsc::Receiver<Output>) -> Output {
        rx.recv_timeout(Duration::from_secs(10)).expect("output")
    }

    #[test]
    fn scans_and_builds_off_thread() {
        let dir = vault();
        let (host, rx) = start();
        host.scan(dir.path());
        host.build(request(1, Mode::Global));
        let out = wait(&rx);
        assert_eq!(out.generation, 1);
        let built = out.result.expect("graph");
        assert!(built.graph.index_of("a.md").is_some());
        assert!(out.files >= 4);
        host.shutdown();
    }

    #[test]
    fn bursts_coalesce_into_one_build() {
        let dir = vault();
        let (host, rx) = start();
        host.scan(dir.path());
        for g in 1..=5 {
            host.build(request(g, Mode::Global));
        }
        let first = wait(&rx);
        assert!(first.generation >= 1);
        // whatever came first, the last request is always answered eventually
        let mut last = first.generation;
        while last < 5 {
            last = wait(&rx).generation;
        }
        assert_eq!(last, 5);
        host.shutdown();
    }

    #[test]
    fn live_updates_change_the_graph() {
        let dir = vault();
        let (host, rx) = start();
        host.scan(dir.path());
        host.build(request(1, Mode::Global));
        wait(&rx);
        // an unsaved buffer introduces a new link and a new file
        host.update(Update::Text(
            dir.path().join("c.md"),
            Some("[[new-note]]".into()),
        ));
        host.update(Update::Text(
            dir.path().join("new-note.md"),
            Some("hello".into()),
        ));
        host.build(request(2, Mode::Global));
        let out = wait(&rx);
        let g = out.result.expect("graph").graph;
        assert!(g.index_of("new-note.md").is_some());
        // removal and rename follow the file system
        fs::remove_file(dir.path().join("b.md")).expect("remove");
        host.update(Update::Remove(dir.path().join("b.md")));
        fs::rename(dir.path().join("c.md"), dir.path().join("d.md")).expect("rename");
        host.update(Update::Rename(
            dir.path().join("c.md"),
            dir.path().join("d.md"),
        ));
        host.build(request(3, Mode::Global));
        let g = wait(&rx).result.expect("graph").graph;
        assert!(g.index_of("b.md").is_none());
        assert!(g.index_of("c.md").is_none());
        assert!(g.index_of("d.md").is_some());
        host.shutdown();
    }

    #[test]
    fn local_notes_and_empty_modes() {
        let dir = vault();
        let (host, rx) = start();
        host.scan(dir.path());
        host.build(request(
            1,
            Mode::Local {
                path: dir.path().join("a.md"),
                depth: 1,
                incoming: true,
                outgoing: true,
            },
        ));
        let local = wait(&rx).result.expect("graph");
        let focus = local.focus.expect("focus");
        assert_eq!(&*local.graph.nodes[focus].id, "a.md");
        assert!(local.graph.index_of("c.md").is_none(), "two hops away");
        assert_eq!(local.layers.map(|l| l.len()), Some(local.graph.nodes.len()));
        host.build(request(2, Mode::Notes));
        let notes = wait(&rx).result.expect("graph").graph;
        assert!(notes
            .nodes
            .iter()
            .all(|n| n.id.starts_with(".impro/Notes/")));
        host.build(request(3, Mode::Empty));
        assert!(wait(&rx).result.expect("graph").graph.is_empty());
        host.shutdown();
    }

    #[test]
    fn a_new_scan_replaces_the_index_and_cancels_the_old_one() {
        let one = vault();
        let two = tempfile::tempdir().expect("tempdir");
        fs::write(two.path().join("only.md"), "").expect("write");
        let (host, rx) = start();
        host.scan(one.path());
        host.scan(two.path());
        host.build(request(1, Mode::Global));
        let g = wait(&rx).result.expect("graph").graph;
        assert_eq!(g.nodes.len(), 1);
        assert!(g.index_of("only.md").is_some());
        host.shutdown();
    }

    #[test]
    fn building_before_any_scan_yields_an_empty_graph() {
        let (host, rx) = start();
        host.build(request(1, Mode::Global));
        assert!(wait(&rx).result.expect("graph").graph.is_empty());
        host.shutdown();
    }

    #[test]
    fn shutdown_is_idempotent_and_joins() {
        let dir = vault();
        let (host, _rx) = start();
        host.scan(dir.path());
        host.shutdown();
        host.shutdown();
        host.build(request(1, Mode::Global));
    }
}
