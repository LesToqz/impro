//! Secondary: force-layout engines on the 3000-node fixture graph (edges
//! only; every engine starts from its own default placement). Compares the
//! app's `impro_graph::Layout` with `fjadra` (d3-force port) and
//! `force_graph`, with `petgraph` as the structure. Reports ms per tick,
//! ticks to settle, spacing/overlap/edge-length quality and determinism.

use std::time::Instant;

use graph_lib_bench::fixture::{self, Fixture};
use impro_graph::{Graph, Layout};

struct Outcome {
    name: &'static str,
    ticks: usize,
    ms_per_tick: Vec<f64>,
    x: Vec<f32>,
    y: Vec<f32>,
}

fn dedup(fx: &Fixture) -> Vec<(u32, u32)> {
    let mut e: Vec<(u32, u32)> = fx.edges.iter().map(|&(a, b)| (a.min(b), a.max(b))).collect();
    e.sort_unstable();
    e.dedup();
    e
}

fn app(fx: &Fixture, edges: &[(u32, u32)]) -> Outcome {
    let graph = Graph::from_pairs(fx.x.len(), edges);
    let mut layout = Layout::new();
    layout.set_graph(&graph);
    let mut ms = Vec::new();
    while !layout.settled() && ms.len() < 2000 {
        let t = Instant::now();
        layout.step();
        ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    Outcome { name: "impro-graph Layout", ticks: ms.len(), ms_per_tick: ms, x: layout.x().to_vec(), y: layout.y().to_vec() }
}

fn fjadra_run(fx: &Fixture, edges: &[(u32, u32)]) -> Outcome {
    use fjadra::{Center, Collide, Link, ManyBody, Node, SimulationBuilder};
    let n = fx.x.len();
    let radius: Vec<f64> = fx.radius.iter().map(|r| *r as f64).collect();
    let mut sim = SimulationBuilder::default()
        .build((0..n).map(|_| Node::default()))
        .add_force("link", Link::new(edges.iter().map(|&(a, b)| (a as usize, b as usize))))
        .add_force("charge", ManyBody::new())
        .add_force("center", Center::new())
        .add_force("collide", Collide::new().radius(move |i| radius[i]));
    let mut ms = Vec::new();
    while !sim.is_finished() && ms.len() < 2000 {
        let t = Instant::now();
        sim.tick(1);
        ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let pos: Vec<[f64; 2]> = sim.positions().collect();
    Outcome {
        name: "fjadra (link+charge+center+collide)",
        ticks: ms.len(),
        ms_per_tick: ms,
        x: pos.iter().map(|p| p[0] as f32).collect(),
        y: pos.iter().map(|p| p[1] as f32).collect(),
    }
}

fn force_graph_run(fx: &Fixture, edges: &[(u32, u32)], ticks: usize) -> Outcome {
    use force_graph::{EdgeData, ForceGraph, NodeData, SimulationParameters};
    let n = fx.x.len();
    let mut g: ForceGraph<(), ()> = ForceGraph::new(SimulationParameters::default());
    let ids: Vec<_> = (0..n)
        .map(|i| {
            let r = 10.0 * (i as f32 + 0.5).sqrt();
            let a = i as f32 * 2.399_963;
            g.add_node(NodeData { x: r * a.cos(), y: r * a.sin(), mass: 10.0, is_anchor: false, user_data: () })
        })
        .collect();
    for &(a, b) in edges {
        g.add_edge(ids[a as usize], ids[b as usize], EdgeData::default());
    }
    let mut ms = Vec::new();
    for _ in 0..ticks {
        let t = Instant::now();
        g.update(0.016);
        ms.push(t.elapsed().as_secs_f64() * 1e3);
    }
    let mut x = vec![0.0; n];
    let mut y = vec![0.0; n];
    g.visit_nodes(|node| {
        let i = ids.iter().position(|id| *id == node.index()).unwrap_or(0);
        x[i] = node.x();
        y[i] = node.y();
    });
    Outcome { name: "force_graph (O(n²) repulsion)", ticks, ms_per_tick: ms, x, y }
}

fn pct(v: &[f64], p: f64) -> f64 {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    s[((s.len() - 1) as f64 * p) as usize]
}

fn quality(o: &Outcome, fx: &Fixture, edges: &[(u32, u32)]) -> String {
    let n = o.x.len();
    // spatial hash for pair checks
    let cell = 40.0f32;
    let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::new();
    for i in 0..n {
        grid.entry(((o.x[i] / cell).floor() as i32, (o.y[i] / cell).floor() as i32)).or_default().push(i);
    }
    let (mut min_gap, mut overlaps) = (f32::MAX, 0usize);
    for i in 0..n {
        let (cx, cy) = ((o.x[i] / cell).floor() as i32, (o.y[i] / cell).floor() as i32);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &j in grid.get(&(cx + dx, cy + dy)).map_or(&[][..], |v| &v[..]) {
                    if j > i {
                        let d = ((o.x[i] - o.x[j]).powi(2) + (o.y[i] - o.y[j]).powi(2)).sqrt();
                        let gap = d - fx.radius[i] - fx.radius[j];
                        min_gap = min_gap.min(gap);
                        if gap < 0.0 {
                            overlaps += 1;
                        }
                    }
                }
            }
        }
    }
    let lens: Vec<f32> = edges
        .iter()
        .map(|&(a, b)| ((o.x[a as usize] - o.x[b as usize]).powi(2) + (o.y[a as usize] - o.y[b as usize]).powi(2)).sqrt())
        .collect();
    let mean = lens.iter().sum::<f32>() / lens.len().max(1) as f32;
    let sd = (lens.iter().map(|l| (l - mean).powi(2)).sum::<f32>() / lens.len().max(1) as f32).sqrt();
    let (mn, mx) = lens.iter().fold((f32::MAX, 0f32), |(a, b), l| (a.min(*l), b.max(*l)));
    let mut rad: Vec<f32> = (0..n).map(|i| (o.x[i].powi(2) + o.y[i].powi(2)).sqrt()).collect();
    rad.sort_by(|a, b| a.total_cmp(b));
    format!(
        "min gap {min_gap:.1}, overlapping pairs {overlaps}, edge len mean {mean:.0} sd {sd:.0} (cv {:.2}) min {mn:.0} max {mx:.0}, 98 % of nodes within r={:.0}",
        sd / mean.max(1e-6),
        rad[(n as f32 * 0.98) as usize]
    )
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| "app".into());
    let fx = fixture::by_name(&name);
    let edges = dedup(&fx);
    // petgraph as the structure
    let t = Instant::now();
    let mut pg = petgraph::graph::UnGraph::<(), ()>::with_capacity(fx.x.len(), edges.len());
    let idx: Vec<_> = (0..fx.x.len()).map(|_| pg.add_node(())).collect();
    for &(a, b) in &edges {
        pg.add_edge(idx[a as usize], idx[b as usize], ());
    }
    let build = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let comps = petgraph::algo::connected_components(&pg);
    println!(
        "fixture `{name}`: {} nodes, {} edges; petgraph UnGraph build {build:.2} ms, connected_components = {comps} in {:.3} ms",
        fx.x.len(),
        edges.len(),
        t.elapsed().as_secs_f64() * 1e3
    );
    println!("\n| engine | ticks | ms/tick p50 | p95 | settle total ms | deterministic | quality |\n|---|---:|---:|---:|---:|---|---|");
    type Run<'a> = Box<dyn Fn() -> Outcome + 'a>;
    let runs: Vec<Run<'_>> = vec![
        Box::new(|| app(&fx, &edges)),
        Box::new(|| fjadra_run(&fx, &edges)),
        Box::new(|| force_graph_run(&fx, &edges, 30)),
    ];
    for run in runs {
        let a = run();
        let b = run();
        let det = a.x == b.x && a.y == b.y;
        println!(
            "| {} | {} | {:.2} | {:.2} | {:.0} | {} | {} |",
            a.name,
            a.ticks,
            pct(&a.ms_per_tick, 0.5),
            pct(&a.ms_per_tick, 0.95),
            a.ms_per_tick.iter().sum::<f64>(),
            if det { "yes" } else { "no" },
            quality(&a, &fx, &edges)
        );
    }
}
