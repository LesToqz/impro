//! Canonical JSON dump of the engine, for the differential harness in
//! `scripts/graph-oracle`.
//!
//! `cargo run -p impro-graph --example graph_dump -- request.json` reads the
//! same request files as `oracle.py` and prints the same canonical shape.

#[path = "../tests/common/oracle_dump.rs"]
mod oracle_dump;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: graph_dump request.json");
    let req: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("read")).expect("json");
    println!(
        "{}",
        serde_json::to_string(&oracle_dump::run(&req)).expect("serialise")
    );
}
