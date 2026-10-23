//! "Replay project history": files appear in creation order, each followed by
//! its outgoing references, at [`crate::geometry::timeline_speed`] items/second.

use crate::model::{Edge, Graph, Node};

/// The reveal order of one graph.
#[derive(Clone, Debug)]
pub struct Plan {
    /// Node indices, oldest first (nodes without a timestamp last).
    order: Vec<u32>,
    /// Edge indices of each rank, in edge order.
    out: Vec<Vec<u32>>,
    show_nodes: bool,
    total: usize,
}

impl Plan {
    /// `show_nodes` is false when unlinked files are hidden: then only
    /// references count as items.
    pub fn new(graph: &Graph, show_nodes: bool) -> Plan {
        let n = graph.nodes.len();
        let mut order: Vec<u32> = (0..n as u32).collect();
        if graph.nodes.iter().any(|nd| nd.ctime.is_some()) {
            let t = |nd: &Node| nd.ctime.filter(|t| t.is_finite()).unwrap_or(f64::INFINITY);
            order.sort_by(|&a, &b| {
                t(&graph.nodes[a as usize])
                    .total_cmp(&t(&graph.nodes[b as usize]))
                    .then(a.cmp(&b))
            });
        }
        let mut rank = vec![0u32; n];
        for (r, &i) in order.iter().enumerate() {
            rank[i as usize] = r as u32;
        }
        let mut out: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (k, e) in graph.edges.iter().enumerate() {
            out[rank[e.src as usize] as usize].push(k as u32);
        }
        let total = out.iter().map(Vec::len).sum::<usize>() + if show_nodes { n } else { 0 };
        Plan {
            order,
            out,
            show_nodes,
            total,
        }
    }

    /// Number of reveal steps (nodes when shown, plus references).
    pub fn total(&self) -> usize {
        self.total
    }

    /// Number of items due at this elapsed playback time.
    pub fn progress(&self, elapsed: f64) -> usize {
        // Reveal is sampled at 30 Hz even on a faster display. Quantising the
        // playback time keeps item boundaries independent of display refresh.
        let sampled = (elapsed.max(0.0) * 30.0).floor() / 30.0;
        1usize.saturating_add(
            (crate::geometry::timeline_speed(self.total) * sampled).floor() as usize,
        )
    }

    /// The graph after the first `progress` items.
    pub fn subset(&self, graph: &Graph, progress: usize) -> Graph {
        let n = self.order.len();
        let mut shown = 0usize;
        let mut cut = n;
        let mut kept_edges: Vec<u32> = Vec::new();
        'outer: for r in 0..n {
            if self.show_nodes {
                if shown == progress {
                    cut = r;
                    break;
                }
                shown += 1;
            }
            for &k in &self.out[r] {
                if shown < progress {
                    kept_edges.push(k);
                } else {
                    cut = r + 1;
                    break 'outer;
                }
                shown += 1;
            }
        }
        let mut remap = vec![u32::MAX; n];
        let mut keep: Vec<usize> = self.order[..cut].iter().map(|&i| i as usize).collect();
        keep.sort_unstable();
        for (new, &old) in keep.iter().enumerate() {
            remap[old] = new as u32;
        }
        let nodes: Vec<Node> = keep.iter().map(|&i| graph.nodes[i].clone()).collect();
        let mut edges: Vec<Edge> = kept_edges
            .into_iter()
            .map(|k| graph.edges[k as usize])
            .filter(|e| remap[e.src as usize] != u32::MAX && remap[e.dst as usize] != u32::MAX)
            .map(|e| Edge {
                src: remap[e.src as usize],
                dst: remap[e.dst as usize],
                ..e
            })
            .collect();
        edges.sort_unstable_by_key(|e| (e.src, e.dst, e.kind));
        let total = nodes.len();
        Graph::assemble(nodes, edges, false, total, Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeKind;
    use std::sync::Arc;

    fn graph(times: &[Option<f64>], pairs: &[(u32, u32)]) -> Graph {
        let mut g = Graph::from_pairs(times.len(), pairs);
        for (n, t) in g.nodes.iter_mut().zip(times) {
            n.ctime = *t;
            n.kind = NodeKind::Note;
            n.id = Arc::from(&*n.id);
        }
        g
    }

    #[test]
    fn counts_nodes_and_references() {
        let g = graph(
            &[Some(1.0), Some(2.0), Some(3.0)],
            &[(0, 1), (1, 2), (2, 0)],
        );
        assert_eq!(Plan::new(&g, true).total(), 6);
        assert_eq!(Plan::new(&g, false).total(), 3);
    }

    #[test]
    fn replay_reveal_clock_polls_at_thirty_hz() {
        let g = graph(&[Some(1.0), Some(2.0)], &[(0, 1)]);
        let plan = Plan::new(&g, true);
        assert_eq!(plan.progress(0.0), 1);
        assert_eq!(plan.progress(0.250), 2);
        assert_eq!(plan.progress(0.255), 2);
        assert_eq!(plan.progress(0.270), 3);
        assert_eq!(plan.progress(1.0), 9);
    }

    #[test]
    fn endpoints_are_empty_and_full() {
        let g = graph(&[Some(3.0), Some(1.0), Some(2.0)], &[(0, 1), (1, 2)]);
        let p = Plan::new(&g, true);
        assert!(p.subset(&g, 0).nodes.is_empty());
        let full = p.subset(&g, p.total());
        assert_eq!(full.nodes.len(), 3);
        assert_eq!(full.pairs.len(), 2);
    }

    #[test]
    fn files_appear_oldest_first_and_references_follow_their_source() {
        // n0 is newest, n1 oldest; n1 -> n2 is revealed once n2 exists
        let g = graph(&[Some(3.0), Some(1.0), Some(2.0)], &[(1, 2), (0, 1)]);
        let p = Plan::new(&g, true);
        let one = p.subset(&g, 1);
        assert_eq!(one.nodes.len(), 1);
        assert_eq!(&*one.nodes[0].id, "n000001.md");
        // item 2 is the reference n1 -> n2, whose target is not there yet
        let two = p.subset(&g, 2);
        assert!(two.pairs.is_empty());
        let mut last = 0;
        for k in 0..=p.total() {
            let s = p.subset(&g, k);
            assert!(s.nodes.len() >= last, "monotonic");
            last = s.nodes.len();
        }
    }

    #[test]
    fn missing_timestamps_keep_index_order() {
        let g = graph(&[None, None, None], &[(0, 1)]);
        let p = Plan::new(&g, true);
        let s = p.subset(&g, 3);
        assert_eq!(s.nodes.len(), 2);
        assert_eq!(&*s.nodes[0].id, "n000000.md");
    }

    #[test]
    fn hidden_orphans_still_reveal_nodes_with_their_links() {
        let g = graph(&[Some(1.0), Some(2.0), Some(3.0)], &[(0, 1)]);
        let p = Plan::new(&g, false);
        assert_eq!(p.total(), 1);
        assert!(p.subset(&g, 0).nodes.is_empty() || p.subset(&g, 0).pairs.is_empty());
        assert_eq!(p.subset(&g, 1).pairs.len(), 1);
    }
}
