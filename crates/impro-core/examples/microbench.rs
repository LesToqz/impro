//! Criterion-free smoke microbenchmarks; cargo run --release -p impro-core --example microbench.
use impro_core::{notes, themes};
use std::{hint::black_box, time::Instant};
fn main() {
    let start = Instant::now();
    for _ in 0..100_000 {
        black_box(themes::get_theme(Some(black_box("impro-dark"))));
    }
    println!(
        "100000 theme lookups: {:.3} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    let document = "café 🌿 a line of text\n".repeat(40_000);
    let offset = document.chars().count() as isize;
    let start = Instant::now();
    for _ in 0..100 {
        let _ = black_box(notes::apply_command(
            "bold",
            black_box(&document),
            offset - 5,
            Some(offset),
        ));
    }
    println!(
        "100 Unicode edits on {} bytes: {:.3} ms",
        document.len(),
        start.elapsed().as_secs_f64() * 1000.0
    );
}
