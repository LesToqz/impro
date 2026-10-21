use super::*;
use crate::model::Graph;

fn chain(n: usize) -> Graph {
    let pairs: Vec<_> = (0..n as u32 - 1).map(|i| (i, i + 1)).collect();
    Graph::from_pairs(n, &pairs)
}

fn ring(n: usize) -> Graph {
    let mut pairs: Vec<_> = (0..n as u32 - 1).map(|i| (i, i + 1)).collect();
    pairs.push((n as u32 - 1, 0));
    Graph::from_pairs(n, &pairs)
}

fn hub(n: usize) -> Graph {
    let pairs: Vec<_> = (1..n as u32).map(|i| (0, i)).collect();
    Graph::from_pairs(n, &pairs)
}

/// Clustered pseudo-random graph.
fn clustered(n: usize, seed: u64) -> Graph {
    let mut rng = Rng::new(seed);
    let clusters = ((n as f32 / 12.0).sqrt().round() as usize).max(1);
    let mut pairs = Vec::new();
    for i in clusters..n {
        let c = i % clusters;
        pairs.push((i as u32, c as u32));
        for _ in 0..1 + (rng.unit() * 3.0) as usize {
            let j = (rng.next_u64() % n as u64) as u32;
            pairs.push((i as u32, j));
        }
    }
    Graph::from_pairs(n, &pairs)
}

fn loaded(g: &Graph) -> Layout {
    let mut l = Layout::with_seed(17);
    l.set_graph(g);
    l
}

fn settle_checked(l: &mut Layout) -> usize {
    let ticks = l.settle(1200);
    assert!(l.settled(), "did not settle after {ticks} ticks");
    for i in 0..l.len() {
        assert!(
            l.x()[i].is_finite() && l.y()[i].is_finite(),
            "finite positions"
        );
        assert!(l.vx()[i].hypot(l.vy()[i]) < 4.0, "small residual velocity");
    }
    ticks
}

#[test]
fn forces_map_controls_linearly() {
    let f = ForceParams::default();
    assert_eq!(
        (f.center, f.link, f.distance, f.repel),
        (0.12, 0.35, 160.0, -720.0)
    );
    let off = ForceParams::from_controls(0.0, 0.0, 160.0, 0.0);
    assert_eq!((off.center, off.link, off.repel), (0.0, 0.0, 0.0));
    assert_eq!(
        ForceParams::from_controls(0.12, 0.35, 160.0, 12.0).repel,
        -1440.0
    );
    // clamped and sanitised
    let wild = ForceParams::from_controls(f64::NAN, 1e9, -5.0, f64::INFINITY);
    assert_eq!(wild.center, 0.12);
    assert_eq!(wild.link, 1.5);
    assert_eq!(wild.distance, 48.0);
    assert_eq!(wild.repel, -720.0);
    assert_eq!(
        ForceParams::from_controls(9.0, 0.0, 160.0, 100.0).center,
        0.6
    );
    assert_eq!(
        ForceParams::from_controls(0.0, 0.0, 160.0, 100.0).repel,
        -2880.0
    );
}

#[test]
fn isolated_link_pulls_endpoints_together_around_a_fixed_midpoint() {
    let mut l = loaded(&Graph::from_pairs(2, &[(0, 1)]));
    l.set_params(ForceParams {
        center: 0.0,
        link: 0.5,
        distance: 160.0,
        repel: 0.0,
    });
    l.set_position(0, -150.0, 4.0);
    l.set_position(1, 150.0, 4.0);
    // hold alpha at exactly 1 for one tick
    l.set_alpha_target(1.0);
    l.step();
    // correction: (300-160)/300 · 0.5 · 300 = 70, shared half and half, then
    // 45 % velocity decay
    assert!((l.vx()[0] - 19.25).abs() < 1e-3, "{}", l.vx()[0]);
    assert!((l.vx()[1] + 19.25).abs() < 1e-3, "{}", l.vx()[1]);
    assert!((l.x()[0] + l.x()[1]).abs() < 1e-3);
}

#[test]
fn cooling_takes_240_ticks_and_edits_reheat() {
    let mut l = loaded(&Graph::empty());
    let ticks = l.settle(1000);
    assert!((240..=241).contains(&ticks), "{ticks}");
    assert!(l.settled());
    assert!(!l.step(), "a settled layout does nothing");
    l.reheat();
    assert!((l.alpha() - 0.4).abs() < 1e-12);
    assert!(!l.settled());
}

#[test]
fn structural_edits_reheat_but_identical_graphs_do_not() {
    let g = chain(10);
    let mut l = loaded(&g);
    assert_eq!(l.alpha(), 1.0);
    l.settle(1000);
    assert!(l.settled());
    assert!(!l.set_graph(&g), "same graph is not an edit");
    assert!(l.settled());
    assert!(l.set_graph(&chain(11)));
    assert!((l.alpha() - 0.4).abs() < 1e-9);
}

#[test]
fn chain_ring_hub_and_clusters_settle_finitely() {
    for g in [
        chain(28),
        ring(36),
        hub(41),
        clustered(180, 31),
        Graph::from_pairs(30, &[]),
    ] {
        let mut l = loaded(&g);
        settle_checked(&mut l);
    }
}

#[test]
fn hub_stays_nearer_the_centre_than_its_dependents() {
    let mut l = loaded(&hub(41));
    settle_checked(&mut l);
    let n = l.len() as f32;
    let (cx, cy) = (l.x().iter().sum::<f32>() / n, l.y().iter().sum::<f32>() / n);
    let dist = |i: usize| (l.x()[i] - cx).hypot(l.y()[i] - cy);
    let leaves: f32 = (1..41).map(dist).sum::<f32>() / 40.0;
    assert!(dist(0) < leaves / 2.0, "hub {} leaves {}", dist(0), leaves);
}

#[test]
fn layout_is_deterministic() {
    let g = clustered(160, 37);
    let mut a = loaded(&g);
    let mut b = loaded(&g);
    for _ in 0..80 {
        a.step();
        b.step();
    }
    assert_eq!(a.x(), b.x());
    assert_eq!(a.y(), b.y());
    let c = {
        let mut c = Layout::with_seed(18);
        c.set_graph(&g);
        c
    };
    assert_ne!(a.x(), c.x(), "the seed changes the initial placement");
}

#[test]
fn updates_keep_state_of_surviving_nodes() {
    let g = chain(32);
    let mut l = loaded(&g);
    for _ in 0..40 {
        l.step();
    }
    let before: Vec<_> = (0..32)
        .map(|i| (l.ids()[i].clone(), l.x()[i], l.y()[i], l.vx()[i], l.vy()[i]))
        .collect();
    // drop the first four nodes, add four new ones
    let pairs: Vec<_> = (0..38u32)
        .filter(|i| *i >= 4 && *i < 37)
        .map(|i| (i, i + 1))
        .collect();
    let g2 = Graph::from_pairs(36, &pairs);
    assert!(l.set_graph(&g2));
    for (id, x, y, vx, vy) in &before {
        if let Some(i) = l.index_of(id) {
            assert_eq!(
                (l.x()[i], l.y()[i], l.vx()[i], l.vy()[i]),
                (*x, *y, *vx, *vy),
                "{id}"
            );
        }
    }
    assert!(l.index_of("n000000.md").is_some(), "ids are reused by name");
}

#[test]
fn barnes_hut_matches_the_exact_sum() {
    let mut l = loaded(&clustered(320, 43));
    for _ in 0..25 {
        l.step();
    }
    let exact = l.exact_repulsion(-288.0);
    let approx = l.approx_repulsion(-288.0);
    let (mut err, mut mag) = (0.0f64, 0.0f64);
    for (e, a) in exact.iter().zip(&approx) {
        err += ((e.0 - a.0) as f64).hypot((e.1 - a.1) as f64);
        mag += (e.0 as f64).hypot(e.1 as f64);
    }
    assert!(err / mag < 0.03, "relative error {}", err / mag);
}

#[test]
fn barnes_hut_is_exact_for_a_few_points_and_handles_coincident_points() {
    let xs = [0.0f32, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, -300.0];
    let ys = [0.0f32, 10.0, 10.0, 10.0, 10.0, 10.0, 10.0, 200.0];
    let mut t = QuadTree::default();
    t.build(&xs, &ys);
    assert_eq!(t.order().len(), xs.len());
    let exact = brute_acceleration(&xs, &ys, -50.0, MIN_DISTANCE2);
    for i in 0..xs.len() {
        let a = t.acceleration(
            i as u32,
            xs[i],
            ys[i],
            &xs,
            &ys,
            -50.0,
            1.0 / (THETA * THETA),
            MIN_DISTANCE2,
        );
        assert!(a.0.is_finite() && a.1.is_finite());
        assert!(
            (a.0 - exact[i].0).abs() < 0.2 * exact[i].0.abs() + 0.05,
            "{i}: {a:?} {:?}",
            exact[i]
        );
    }
    let mut empty = QuadTree::default();
    empty.build(&[], &[]);
    assert_eq!(
        empty.acceleration(0, 0.0, 0.0, &[], &[], -1.0, 2.0, 64.0),
        (0.0, 0.0)
    );
}

#[test]
fn collision_separates_crowded_nodes_without_other_forces() {
    let mut l = loaded(&Graph::from_pairs(24, &[]));
    l.set_params(ForceParams {
        center: 0.0,
        link: 0.0,
        distance: 160.0,
        repel: 0.0,
    });
    let mut rng = Rng::new(47);
    for i in 0..24 {
        l.set_position(i, rng.unit() * 40.0, rng.unit() * 40.0);
    }
    settle_checked(&mut l);
    for i in 0..24 {
        for j in i + 1..24 {
            let d = (l.x()[i] - l.x()[j]).hypot(l.y()[i] - l.y()[j]);
            assert!(d > 63.0, "{i} {j} {d}");
        }
    }
}

#[test]
fn dragged_node_stays_put_then_releases() {
    let mut l = loaded(&chain(16));
    l.begin_drag(0, 73.0, -29.0);
    for _ in 0..32 {
        l.step();
        assert!(l.is_pinned(0));
    }
    assert_eq!((l.x()[0], l.y()[0]), (73.0, -29.0));
    assert_eq!((l.vx()[0], l.vy()[0]), (0.0, 0.0));
    l.drag_to(0, 80.0, -20.0);
    l.step();
    assert_eq!((l.x()[0], l.y()[0]), (80.0, -20.0));
    assert!(!l.settled(), "dragging keeps the layout warm");
    l.end_drag(0);
    assert!(!l.is_pinned(0));
    settle_checked(&mut l);
}

#[test]
fn pins_survive_graph_updates() {
    let mut l = loaded(&chain(8));
    l.pin(3, 5.0, 6.0);
    l.set_graph(&chain(9));
    let i = l.index_of("n000003.md").expect("kept");
    assert!(l.is_pinned(i));
    l.step();
    assert_eq!((l.x()[i], l.y()[i]), (5.0, 6.0));
}

#[test]
fn new_nodes_start_in_a_bounded_disc() {
    let g = chain(128);
    let l = loaded(&g);
    let r = (72.0f32 * 72.0 * 128.0 / std::f32::consts::PI).sqrt();
    assert!(l
        .x()
        .iter()
        .zip(l.y())
        .all(|(x, y)| x.hypot(*y) <= r + 1e-3));
}

#[test]
fn new_neighbours_appear_beside_surviving_nodes() {
    let mut l = loaded(&Graph::from_pairs(1, &[]));
    l.set_position(0, 500.0, -300.0);
    // a second node linked to the first
    l.set_graph(&Graph::from_pairs(2, &[(0, 1)]));
    assert_eq!((l.x()[0], l.y()[0]), (500.0, -300.0));
    assert!((l.x()[1] - 500.0).abs() <= 24.0);
    assert!((l.y()[1] + 300.0).abs() <= 24.0);
}

#[test]
fn chains_of_new_nodes_follow_a_placed_one() {
    let mut l = loaded(&Graph::from_pairs(1, &[]));
    l.set_position(0, 1000.0, 1000.0);
    l.set_graph(&chain(5));
    for i in 1..5 {
        let d = (l.x()[i] - 1000.0).hypot(l.y()[i] - 1000.0);
        assert!(d < 24.0 * 1.5 * i as f32 + 1.0, "{i} {d}");
    }
}

#[test]
fn radii_follow_weights_and_multiplier() {
    let mut l = loaded(&hub(5));
    let hub_r = l.radius()[0];
    assert!(hub_r > l.radius()[1]);
    l.set_size_multiplier(2.0);
    assert!((l.radius()[0] - 2.0 * hub_r / 1.15).abs() < 1e-3);
    l.set_weights(&[12.0, 6.0, 4.0, 3.0, 12.0 / 5.0]);
    assert_eq!(l.weight()[1], 6.0);
    l.set_weights(&[1.0]); // wrong length ignored
    assert_eq!(l.weight().len(), 5);
    l.set_size_multiplier(f32::NAN);
    l.set_size_multiplier(-1.0);
    assert!(l.radius().iter().all(|r| r.is_finite() && *r > 0.0));
}

#[test]
fn degenerate_inputs_do_not_panic() {
    let mut l = loaded(&Graph::from_pairs(1, &[]));
    l.settle(300);
    l.pin(99, 0.0, 0.0);
    l.unpin(99);
    l.set_position(99, 1.0, 1.0);
    l.pin(0, f32::NAN, 0.0);
    assert!(!l.is_pinned(0));
    l.set_theta(5.0);
    l.set_alpha_target(f64::NAN);
    assert!(l.bounds().is_some());
    assert!(Layout::new().bounds().is_none());
    // coincident nodes
    let mut c = loaded(&Graph::from_pairs(6, &[(0, 1), (1, 2), (2, 0)]));
    for i in 0..6 {
        c.set_position(i, 3.0, 3.0);
    }
    settle_checked(&mut c);
    // self loops and duplicate pairs
    let g = Graph::from_pairs(3, &[(0, 0), (0, 1), (0, 1), (5, 6)]);
    settle_checked(&mut loaded(&g));
}
