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
