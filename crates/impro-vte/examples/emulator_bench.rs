//! Run with --release --features emulator --example emulator_bench.
#[cfg(feature = "emulator")]
fn main() {
    use std::time::{Duration, Instant};
    let mut screen = impro_vte::emu::Screen::new(120, 40, 10_000);
    let line = "\x1b[38;2;125;200;40mcompile\x1b[0m 界 e\u{301} \x1b[1mbold\x1b[0m\r\n";
    let chunk = line.repeat(2048 / line.len());
    let batches = (50 * 1024 * 1024) / chunk.len();
    let start = Instant::now();
    let mut maximum = Duration::ZERO;
    for _ in 0..batches {
        let batch = Instant::now();
        screen.feed(chunk.as_bytes());
        maximum = maximum.max(batch.elapsed());
    }
    let elapsed = start.elapsed();
    println!("bytes={} batches={} seconds={:.3} throughput_mib_s={:.2} mean_batch_us={:.2} max_batch_us={:.2}",batches*chunk.len(),batches,elapsed.as_secs_f64(),batches as f64*chunk.len() as f64/1048576./elapsed.as_secs_f64(),elapsed.as_secs_f64()*1e6/batches as f64,maximum.as_secs_f64()*1e6);
}
#[cfg(not(feature = "emulator"))]
fn main() {}
