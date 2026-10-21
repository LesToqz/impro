//! A layout tick must not allocate once the graph is loaded.

use std::alloc::{GlobalAlloc, Layout as AllocLayout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use impro_graph::layout::{Layout, Rng};
use impro_graph::model::Graph;

/// Test-only allocation counter that forwards to the system allocator.
struct Counting;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: AllocLayout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: AllocLayout) {
        System.dealloc(ptr, layout)
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: AllocLayout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn random_graph(n: usize, seed: u64) -> Graph {
    let mut rng = Rng::new(seed);
    let mut pairs = Vec::new();
    for i in 1..n as u32 {
        pairs.push((i, (rng.next_u64() % i as u64) as u32));
        if i % 3 == 0 {
            pairs.push((i, (rng.next_u64() % n as u64) as u32));
        }
    }
    Graph::from_pairs(n, &pairs)
}

#[test]
fn ticks_do_not_allocate() {
    for (n, seed) in [(600usize, 5u64), (2500, 6), (40, 7)] {
        let graph = random_graph(n, seed);
        let mut layout = Layout::new();
        layout.set_graph(&graph);
        layout.begin_drag(5.min(n - 1), 10.0, 10.0);
        // no warm-up: set_graph must have sized every buffer; the run covers
        // the whole cooling curve while the layout spreads out
        let before = ALLOCATIONS.load(Ordering::SeqCst);
        for k in 0..260 {
            layout.drag_to(5.min(n - 1), 10.0 + k as f32, 10.0);
            assert!(layout.step());
        }
        let after = ALLOCATIONS.load(Ordering::SeqCst);
        assert_eq!(
            after - before,
            0,
            "n={n}: ticks allocated {} times",
            after - before
        );
    }
}
