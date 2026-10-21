//! Bounded worker pool and cancellation tokens.
//!
//! Every directory scan, Git status, note listing, filesystem action and search run is
//! queued here, so the GTK main thread never touches the disk or spawns a process. The pool
//! has at most `min(cores - 1, 4)` threads (at least one); idle workers sleep on a condition
//! variable, so an idle explorer costs no CPU. Interactive jobs (listings, Git, actions)
//! are always dequeued before background jobs (searches).
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};

/// Shared cancellation flag; clones observe the same state.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Scheduling lane of a pooled job.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lane {
    /// Short jobs a user is waiting for: listings, Git, filesystem actions.
    Interactive,
    /// Long scans such as searches; they never delay interactive jobs for long.
    Background,
}

type Job = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Queues {
    interactive: VecDeque<Job>,
    background: VecDeque<Job>,
}

struct Shared {
    queues: Mutex<Queues>,
    ready: Condvar,
}

fn shared() -> &'static Shared {
    static POOL: OnceLock<&'static Shared> = OnceLock::new();
    POOL.get_or_init(|| {
        let shared: &'static Shared = Box::leak(Box::new(Shared {
            queues: Mutex::new(Queues::default()),
            ready: Condvar::new(),
        }));
        for index in 0..worker_count() {
            let spawned = std::thread::Builder::new()
                .name(format!("impro-explorer-{index}"))
                .spawn(move || worker(shared));
            if spawned.is_err() && index == 0 {
                eprintln!("impro-explorer: cannot start worker threads");
            }
        }
        shared
    })
}

/// Number of pooled threads: cores - 1, between one and four.
pub fn worker_count() -> usize {
    std::thread::available_parallelism()
        .map_or(1, |n| n.get().saturating_sub(1))
        .clamp(1, 4)
}

fn worker(shared: &'static Shared) {
    loop {
        let job = {
            let mut queues = shared.queues.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(job) = queues
                    .interactive
                    .pop_front()
                    .or_else(|| queues.background.pop_front())
                {
                    break job;
                }
                queues = shared.ready.wait(queues).unwrap_or_else(|e| e.into_inner());
            }
        };
        // A panicking job must not take the worker thread down with it.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
    }
}

/// Queue a job on the shared pool.
pub fn spawn(lane: Lane, job: impl FnOnce() + Send + 'static) {
    let shared = shared();
    {
        let mut queues = shared.queues.lock().unwrap_or_else(|e| e.into_inner());
        match lane {
            Lane::Interactive => queues.interactive.push_back(Box::new(job)),
            Lane::Background => queues.background.push_back(Box::new(job)),
        }
    }
    shared.ready.notify_one();
}
