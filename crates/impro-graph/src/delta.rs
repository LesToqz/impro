//! What changed between two graphs, so the view can patch itself in place.

use std::sync::Arc;

use crate::index::EdgeKind;
use crate::model::Graph;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeKey {
    pub src: Arc<str>,
    pub dst: Arc<str>,
    pub kind: EdgeKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphDelta {
    pub added_nodes: Vec<Arc<str>>,
    pub removed_nodes: Vec<Arc<str>>,
    /// Same node, different degree or colour rule.
    pub changed_nodes: Vec<Arc<str>>,
    pub added_edges: Vec<EdgeKey>,
    pub removed_edges: Vec<EdgeKey>,
    /// Same edge, different multiplicity or resolution.
    pub changed_edges: Vec<EdgeKey>,
}

impl GraphDelta {
    pub fn is_empty(&self) -> bool {
        self.added_nodes.is_empty()
            && self.removed_nodes.is_empty()
            && self.changed_nodes.is_empty()
            && self.added_edges.is_empty()
            && self.removed_edges.is_empty()
            && self.changed_edges.is_empty()
    }

    /// The set of nodes or connections changed (the layout must reheat).
    pub fn is_structural(&self) -> bool {
        !(self.added_nodes.is_empty()
            && self.removed_nodes.is_empty()
            && self.added_edges.is_empty()
            && self.removed_edges.is_empty())
    }
}

fn key(g: &Graph, e: &crate::model::Edge) -> (Arc<str>, Arc<str>, EdgeKind) {
    (
        g.nodes[e.src as usize].id.clone(),
        g.nodes[e.dst as usize].id.clone(),
        e.kind,
    )
}

/// Compares two graphs in O(nodes + edges); both are sorted by id.
pub fn diff(old: &Graph, new: &Graph) -> GraphDelta {
    let mut d = GraphDelta::default();
    let (mut i, mut j) = (0, 0);
    while i < old.nodes.len() || j < new.nodes.len() {
        let order = match (old.nodes.get(i), new.nodes.get(j)) {
            (Some(a), Some(b)) => a.id.cmp(&b.id),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, _) => std::cmp::Ordering::Greater,
        };
        match order {
            std::cmp::Ordering::Less => {
                d.removed_nodes.push(old.nodes[i].id.clone());
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                d.added_nodes.push(new.nodes[j].id.clone());
                j += 1;
            }
            std::cmp::Ordering::Equal => {
                let (a, b) = (&old.nodes[i], &new.nodes[j]);
                if a.in_degree != b.in_degree || a.out_degree != b.out_degree || a.group != b.group
                {
                    d.changed_nodes.push(b.id.clone());
                }
                i += 1;
                j += 1;
            }
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < old.edges.len() || j < new.edges.len() {
        let ka = old.edges.get(i).map(|e| key(old, e));
        let kb = new.edges.get(j).map(|e| key(new, e));
        let order = match (&ka, &kb) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, _) => std::cmp::Ordering::Greater,
        };
        let to_key = |k: (Arc<str>, Arc<str>, EdgeKind)| EdgeKey {
            src: k.0,
            dst: k.1,
            kind: k.2,
        };
        match (order, ka, kb) {
            (std::cmp::Ordering::Less, Some(k), _) => {
                d.removed_edges.push(to_key(k));
                i += 1;
            }
            (std::cmp::Ordering::Greater, _, Some(k)) => {
                d.added_edges.push(to_key(k));
                j += 1;
            }
            (_, _, Some(k)) => {
                let (a, b) = (&old.edges[i], &new.edges[j]);
                if a.count != b.count || a.resolved != b.resolved {
                    d.changed_edges.push(to_key(k));
                }
                i += 1;
                j += 1;
            }
            // unreachable: the loop condition guarantees one side exists
            _ => break,
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Rng;
    use std::collections::BTreeSet;

    fn random_graph(rng: &mut Rng, n: usize) -> Graph {
        let pairs: Vec<_> = (0..n * 2)
            .map(|_| {
                (
                    (rng.next_u64() % n as u64) as u32,
                    (rng.next_u64() % n as u64) as u32,
                )
            })
            .collect();
        Graph::from_pairs(n, &pairs)
    }

    fn edge_set(g: &Graph) -> BTreeSet<(String, String)> {
        g.pairs
            .iter()
            .map(|&(s, t)| {
                (
                    g.nodes[s as usize].id.to_string(),
                    g.nodes[t as usize].id.to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn applying_a_delta_reproduces_the_new_graph() {
        let mut rng = Rng::new(5);
        for _ in 0..100 {
            let (na, nb) = (
                3 + (rng.next_u64() % 30) as usize,
                3 + (rng.next_u64() % 30) as usize,
            );
            let a = random_graph(&mut rng, na);
            let b = random_graph(&mut rng, nb);
            let d = diff(&a, &b);
            let mut nodes: BTreeSet<String> = a.nodes.iter().map(|n| n.id.to_string()).collect();
            for r in &d.removed_nodes {
                assert!(nodes.remove(&**r));
            }
            for x in &d.added_nodes {
                assert!(nodes.insert(x.to_string()));
            }
            assert_eq!(nodes, b.nodes.iter().map(|n| n.id.to_string()).collect());
            let mut edges = edge_set(&a);
            for e in &d.removed_edges {
                assert!(edges.remove(&(e.src.to_string(), e.dst.to_string())));
            }
            for e in &d.added_edges {
                assert!(edges.insert((e.src.to_string(), e.dst.to_string())));
            }
            assert_eq!(edges, edge_set(&b));
            assert_eq!(
                d.is_empty(),
                a.nodes.len() == b.nodes.len()
                    && edge_set(&a) == edge_set(&b)
                    && d.changed_nodes.is_empty()
            );
        }
    }

    #[test]
    fn identical_graphs_have_an_empty_delta() {
        let g = Graph::from_pairs(5, &[(0, 1), (1, 2)]);
        let d = diff(&g, &g.clone());
        assert!(d.is_empty() && !d.is_structural());
        assert!(diff(&Graph::empty(), &Graph::empty()).is_empty());
        let grown = diff(&Graph::empty(), &g);
        assert_eq!(grown.added_nodes.len(), 5);
        assert!(grown.is_structural());
    }

    #[test]
    fn degree_changes_are_reported_without_being_structural() {
        let a = Graph::from_pairs(3, &[(0, 1)]);
        let mut b = a.clone();
        b.nodes[0].group = Some(2);
        let d = diff(&a, &b);
        assert_eq!(d.changed_nodes.len(), 1);
        assert!(!d.is_structural() && !d.is_empty());
    }
}
