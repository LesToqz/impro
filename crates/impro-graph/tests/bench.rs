//! Timing benchmarks (run with `cargo test --release -- --ignored --nocapture`).

mod common;

use std::time::Instant;

use common::*;
use impro_graph::filter::GraphOptions;
use impro_graph::layout::{Layout, Rng};
use impro_graph::model::Graph;
use impro_graph::query::Query;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn big_graph(n: usize, edges: usize, seed: u64) -> Graph {
    let mut rng = Rng::new(seed);
    let clusters = (n as f64 / 12.0).sqrt() as usize;
    let mut pairs = Vec::with_capacity(edges);
    for i in clusters..n {
        pairs.push((i as u32, (i % clusters) as u32));
    }
    while pairs.len() < edges {
        let a = (rng.next_u64() % n as u64) as u32;
        // mostly within the same cluster
        let b = if rng.unit() < 0.85 {
            ((rng.next_u64() % (n / clusters) as u64) as usize * clusters + a as usize % clusters)
                .min(n - 1) as u32
        } else {
            (rng.next_u64() % n as u64) as u32
        };
        pairs.push((a, b));
    }
    Graph::from_pairs(n, &pairs)
}

#[test]
#[ignore]
fn bench_layout_tick() {
    let graph = big_graph(3000, 8000, 7);
    println!(
        "graph: {} nodes, {} distinct pairs",
        graph.nodes.len(),
        graph.pairs.len()
    );
    let mut layout = Layout::new();
    let t = Instant::now();
    layout.set_graph(&graph);
    println!("set_graph: {:.2} ms", ms(t));
    let mut times = Vec::new();
    for _ in 0..241 {
        let t = Instant::now();
        layout.step();
        times.push(ms(t));
    }
    let mut sorted = times.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mean = times.iter().sum::<f64>() / times.len() as f64;
    println!(
        "tick (3000 nodes): mean {:.3} ms, median {:.3} ms, p95 {:.3} ms, max {:.3} ms",
        mean,
        sorted[sorted.len() / 2],
        sorted[sorted.len() * 95 / 100],
        sorted[sorted.len() - 1]
    );
    assert!(layout.settled());
    assert!(
        sorted[sorted.len() * 95 / 100] < 2.5,
        "p95 tick above 2.5 ms"
    );
}

#[test]
#[ignore]
fn bench_index_5k_files() {
    let p = Project::new();
    for i in 0..5000 {
        p.write(
            &format!("dir{}/n{i}.md", i % 50),
            &format!(
                "# Note {i}\n[[n{}]] [[n{}]] #tag{} some body text for the note\n",
                (i + 1) % 5000,
                (i * 7) % 5000,
                i % 40
            ),
        );
    }
    let t = Instant::now();
    let idx = p.scan();
    println!("scan 5000 files: {:.1} ms", ms(t));
    let t = Instant::now();
    let g = idx.build_graph(&GraphOptions {
        show_tags: true,
        ..GraphOptions::default()
    });
    println!(
        "build graph ({} nodes, {} edges): {:.2} ms",
        g.nodes.len(),
        g.edges.len(),
        ms(t)
    );
    let mut idx = idx;
    let path = p.path("dir0/n0.md");
    std::fs::write(&path, "# Note 0\n[[n2]] changed #tag0\n").unwrap();
    let t = Instant::now();
    let change = idx.update_file(&path, None);
    println!(
        "update_file (one note): {:.3} ms, {} modified, {} rerouted",
        ms(t),
        change.modified.len(),
        change.rerouted.len()
    );
    let t = Instant::now();
    idx.update_file(&path, Some("# Note 0\n[[n3]] buffer edit\n"));
    println!("update_file (buffer text): {:.3} ms", ms(t));
    let t = Instant::now();
    let _ = idx.build_graph(&GraphOptions::default());
    println!("rebuild graph after edit: {:.2} ms", ms(t));
    // a new file whose name is referenced by many notes
    std::fs::write(p.path("dir1/popular.md"), "x").unwrap();
    let t = Instant::now();
    idx.update_file(&p.path("dir1/popular.md"), None);
    println!("update_file (new file): {:.3} ms", ms(t));
}

#[test]
#[ignore]
fn bench_query_5k_nodes() {
    let p = Project::new();
    for i in 0..5000 {
        p.write(
            &format!("q{}/note-{i}.md", i % 50),
            &format!(
                "---\ntags: [t{}]\n---\nlorem ipsum dolor sit amet {i} consectetur {} adipiscing\n",
                i % 30,
                (i * 31) % 977
            ),
        );
    }
    let idx = p.scan();
    let docs: Vec<_> = idx.iter().map(|(_, f)| f).collect();
    for q in [
        "lorem",
        "content:consectetur 97",
        "\"dolor sit\" -amet",
        "/note-4[0-9]+/",
        "tag:t7 OR path:q1/",
        "absent-term",
    ] {
        let query = Query::parse(q);
        let t = Instant::now();
        let mut hits = 0;
        for f in &docs {
            let doc = impro_graph::query::Doc {
                path: &f.path,
                label: f.label(),
                content: &f.text,
                tags: impro_graph::query::Tags::Many(&f.tags),
            };
            hits += query.matches(&doc) as usize;
        }
        println!(
            "query {q:<28} {:>7.3} ms over {} files ({} hits)",
            ms(t),
            docs.len(),
            hits
        );
    }
    let t = Instant::now();
    let g = idx.build_graph(&GraphOptions {
        query: "lorem".into(),
        strict_filters: true,
        ..GraphOptions::default()
    });
    println!(
        "build with query: {:.2} ms ({} nodes)",
        ms(t),
        g.nodes.len()
    );
}
