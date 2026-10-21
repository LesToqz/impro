//! The graph handed to the view: index-based, sorted by node id.

use std::sync::Arc;

use crate::index::{EdgeKind, FileId};
use crate::parse::text::{basename, dirname, stem};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NodeKind {
    Note,
    Code,
    Attachment,
    Tag,
    Unresolved,
}

impl NodeKind {
    pub const ALL: [NodeKind; 5] = [
        NodeKind::Note,
        NodeKind::Code,
        NodeKind::Attachment,
        NodeKind::Tag,
        NodeKind::Unresolved,
    ];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Note => "note",
            NodeKind::Code => "code",
            NodeKind::Attachment => "attachment",
            NodeKind::Tag => "tag",
            NodeKind::Unresolved => "unresolved",
        }
    }

    /// Kinds that are searched themselves; the others only follow them.
    pub fn is_primary(self) -> bool {
        matches!(self, NodeKind::Note | NodeKind::Code)
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    /// Relative path for files, `#tag` for tags, `?target` for missing targets.
    pub id: Arc<str>,
    pub kind: NodeKind,
    pub file: Option<FileId>,
    /// Distinct directed endpoint pairs ending / starting here.
    pub in_degree: u32,
    pub out_degree: u32,
    /// Index of the first colour rule matching this node.
    pub group: Option<u8>,
    pub ctime: Option<f64>,
}

impl Node {
    /// Display name: file stem, tag text or missing target text.
    pub fn label(&self) -> &str {
        match self.kind {
            NodeKind::Tag | NodeKind::Unresolved => &self.id[1..],
            _ => stem(&self.id),
        }
    }

    /// Project path (empty for tags and missing targets).
    pub fn path(&self) -> &str {
        match self.kind {
            NodeKind::Tag | NodeKind::Unresolved => "",
            _ => &self.id,
        }
    }

    pub fn folder(&self) -> &str {
        match self.kind {
            NodeKind::Tag | NodeKind::Unresolved => "",
            _ => dirname(&self.id),
        }
    }

    pub fn file_name(&self) -> &str {
        basename(self.path())
    }

    /// In plus out degree; a self reference counts twice.
    pub fn degree(&self) -> u32 {
        self.in_degree + self.out_degree
    }

    pub fn size_hint(&self) -> u32 {
        1 + self.degree()
    }
}

/// Edges with the same endpoints and kind collapse into one with a count.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Edge {
    pub src: u32,
    pub dst: u32,
    pub kind: EdgeKind,
    pub resolved: bool,
    pub count: u32,
}

impl Edge {
    pub fn is_self(&self) -> bool {
        self.src == self.dst
    }
}

/// A bare name that matched several files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ambiguity {
    pub source: Arc<str>,
    pub target: Box<str>,
    pub candidates: Vec<Arc<str>>,
    pub resolved: Arc<str>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Indexed by [`NodeKind::index`].
    pub counts: [usize; 5],
    pub orphans: usize,
    pub unresolved: Vec<Arc<str>>,
    pub ambiguous: Vec<Ambiguity>,
    pub components: usize,
}

impl Stats {
    pub fn count(&self, kind: NodeKind) -> usize {
        self.counts[kind.index()]
    }
}

/// One drawn line between two nodes (`a < b`) with the directions present.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Line {
    pub a: u32,
    pub b: u32,
    pub a_to_b: bool,
    pub b_to_a: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Graph {
    /// Sorted by id.
    pub nodes: Vec<Node>,
    /// Sorted by (source, target, kind).
    pub edges: Vec<Edge>,
    /// Distinct directed endpoint pairs (self pairs included), sorted.
    pub pairs: Vec<(u32, u32)>,
    /// The node cap dropped nodes.
    pub truncated: bool,
    /// Node count before the cap was applied.
    pub total_nodes: usize,
    pub stats: Stats,
}

/// Compressed adjacency for traversals.
#[derive(Clone, Debug, Default)]
pub struct Adjacency {
    pub out_start: Vec<u32>,
    pub out_to: Vec<u32>,
    pub in_start: Vec<u32>,
    pub in_from: Vec<u32>,
}

impl Adjacency {
    pub fn outgoing(&self, n: usize) -> &[u32] {
        &self.out_to[self.out_start[n] as usize..self.out_start[n + 1] as usize]
    }

    pub fn incoming(&self, n: usize) -> &[u32] {
        &self.in_from[self.in_start[n] as usize..self.in_start[n + 1] as usize]
    }
}

impl Graph {
    pub fn empty() -> Graph {
        Graph::default()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Index of the node with this id.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.nodes.binary_search_by(|n| (*n.id).cmp(id)).ok()
    }

    /// Assembles a graph from sorted nodes and sorted, collapsed edges, then
    /// derives pairs, degrees and statistics.
    pub(crate) fn assemble(
        mut nodes: Vec<Node>,
        edges: Vec<Edge>,
        truncated: bool,
        total_nodes: usize,
        ambiguous: Vec<Ambiguity>,
    ) -> Graph {
        let mut pairs: Vec<(u32, u32)> = Vec::with_capacity(edges.len());
        for e in &edges {
            if pairs.last() != Some(&(e.src, e.dst)) {
                pairs.push((e.src, e.dst));
            }
        }
        for n in &mut nodes {
            n.in_degree = 0;
            n.out_degree = 0;
        }
        for &(s, t) in &pairs {
            nodes[s as usize].out_degree += 1;
            nodes[t as usize].in_degree += 1;
        }
        let mut stats = Stats::default();
        for n in &nodes {
            stats.counts[n.kind.index()] += 1;
            if n.degree() == 0 {
                stats.orphans += 1;
            }
            if n.kind == NodeKind::Unresolved {
                stats.unresolved.push(n.id.clone());
            }
        }
        stats.components = components(nodes.len(), &pairs);
        stats.ambiguous = ambiguous;
        stats
            .ambiguous
            .sort_by(|a, b| (&a.source, &a.target).cmp(&(&b.source, &b.target)));
        Graph {
            nodes,
            edges,
            pairs,
            truncated,
            total_nodes,
            stats,
        }
    }

    /// Nodes where `keep` is true, with every edge between them.
    pub fn subgraph(&self, keep: &[bool]) -> Graph {
        let mut remap = vec![u32::MAX; self.nodes.len()];
        let mut nodes = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if keep.get(i).copied().unwrap_or(false) {
                remap[i] = nodes.len() as u32;
                nodes.push(n.clone());
            }
        }
        let edges: Vec<Edge> = self
            .edges
            .iter()
            .filter(|e| remap[e.src as usize] != u32::MAX && remap[e.dst as usize] != u32::MAX)
            .map(|e| Edge {
                src: remap[e.src as usize],
                dst: remap[e.dst as usize],
                ..*e
            })
            .collect();
        let alive: std::collections::HashSet<&str> = nodes.iter().map(|n| &*n.id).collect();
        let ambiguous = self
            .stats
            .ambiguous
            .iter()
            .filter(|a| alive.contains(&*a.source) && alive.contains(&*a.resolved))
            .cloned()
            .collect();
        let total = nodes.len();
        Graph::assemble(nodes, edges, self.truncated, total.max(self.total_nodes.min(total)), ambiguous)
    }

    pub fn adjacency(&self) -> Adjacency {
        let n = self.nodes.len();
        let mut out_start = vec![0u32; n + 1];
        let mut in_start = vec![0u32; n + 1];
        for &(s, t) in &self.pairs {
            out_start[s as usize + 1] += 1;
            in_start[t as usize + 1] += 1;
        }
        for i in 0..n {
            out_start[i + 1] += out_start[i];
            in_start[i + 1] += in_start[i];
        }
        let mut out_to = vec![0u32; self.pairs.len()];
        let mut in_from = vec![0u32; self.pairs.len()];
        let mut oc = out_start.clone();
        let mut ic = in_start.clone();
        for &(s, t) in &self.pairs {
            out_to[oc[s as usize] as usize] = t;
            oc[s as usize] += 1;
            in_from[ic[t as usize] as usize] = s;
            ic[t as usize] += 1;
        }
        Adjacency {
            out_start,
            out_to,
            in_start,
            in_from,
        }
    }

    /// One line per unordered non-self pair, with the directions it carries.
    pub fn lines(&self) -> Vec<Line> {
        let mut v: Vec<Line> = self
            .pairs
            .iter()
            .filter(|(s, t)| s != t)
            .map(|&(s, t)| Line {
                a: s.min(t),
                b: s.max(t),
                a_to_b: s < t,
                b_to_a: s > t,
            })
            .collect();
        v.sort_unstable_by_key(|l| (l.a, l.b));
        let mut out: Vec<Line> = Vec::with_capacity(v.len());
        for l in v {
            match out.last_mut() {
                Some(last) if last.a == l.a && last.b == l.b => {
                    last.a_to_b |= l.a_to_b;
                    last.b_to_a |= l.b_to_a;
                }
                _ => out.push(l),
            }
        }
        out
    }
}

fn find(parent: &mut [u32], mut x: u32) -> u32 {
    while parent[x as usize] != x {
        parent[x as usize] = parent[parent[x as usize] as usize];
        x = parent[x as usize];
    }
    x
}

/// Connected components, ignoring direction.
fn components(n: usize, pairs: &[(u32, u32)]) -> usize {
    let mut parent: Vec<u32> = (0..n as u32).collect();
    let mut count = n;
    for &(s, t) in pairs {
        let (a, b) = (find(&mut parent, s), find(&mut parent, t));
        if a != b {
            parent[a as usize] = b;
            count -= 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, kind: NodeKind) -> Node {
        Node {
            id: Arc::from(id),
            kind,
            file: None,
            in_degree: 0,
            out_degree: 0,
            group: None,
            ctime: None,
        }
    }

    fn edge(s: u32, t: u32, kind: EdgeKind) -> Edge {
        Edge {
            src: s,
            dst: t,
            kind,
            resolved: true,
            count: 1,
        }
    }

    fn sample() -> Graph {
        // 0 <-> 1 (two kinds one way), 2 self, 3 isolated, 4 tag
        Graph::assemble(
            vec![
                node("a.md", NodeKind::Note),
                node("b.md", NodeKind::Note),
                node("c.md", NodeKind::Note),
                node("d.md", NodeKind::Note),
                node("#t", NodeKind::Tag),
            ],
            vec![
                edge(0, 1, EdgeKind::Embed),
                edge(0, 1, EdgeKind::Link),
                edge(0, 4, EdgeKind::Tag),
                edge(1, 0, EdgeKind::Link),
                edge(2, 2, EdgeKind::Link),
            ],
            false,
            5,
            Vec::new(),
        )
    }

    #[test]
    fn degrees_use_distinct_directed_pairs() {
        let g = sample();
        assert_eq!(g.pairs, [(0, 1), (0, 4), (1, 0), (2, 2)]);
        assert_eq!((g.nodes[0].in_degree, g.nodes[0].out_degree), (1, 2));
        assert_eq!((g.nodes[1].in_degree, g.nodes[1].out_degree), (1, 1));
        // a self pair counts twice
        assert_eq!(g.nodes[2].degree(), 2);
        assert_eq!(g.nodes[3].degree(), 0);
        assert_eq!(g.nodes[3].size_hint(), 1);
        assert_eq!(g.stats.orphans, 1);
        assert_eq!(g.stats.components, 3);
        assert_eq!(g.stats.count(NodeKind::Tag), 1);
    }

    #[test]
    fn lines_merge_mutual_pairs_and_drop_self() {
        let l = sample().lines();
        assert_eq!(
            l,
            [
                Line { a: 0, b: 1, a_to_b: true, b_to_a: true },
                Line { a: 0, b: 4, a_to_b: true, b_to_a: false },
            ]
        );
    }

    #[test]
    fn adjacency_matches_pairs() {
        let g = sample();
        let a = g.adjacency();
        assert_eq!(a.outgoing(0), [1, 4]);
        assert_eq!(a.incoming(0), [1]);
        assert_eq!(a.outgoing(3), [] as [u32; 0]);
        assert_eq!(a.incoming(2), [2]);
    }

    #[test]
    fn labels_and_paths() {
        assert_eq!(node("dir/My Note.md", NodeKind::Note).label(), "My Note");
        assert_eq!(node("dir/My Note.md", NodeKind::Note).folder(), "dir");
        assert_eq!(node("#a/b", NodeKind::Tag).label(), "a/b");
        assert_eq!(node("#a/b", NodeKind::Tag).path(), "");
        assert_eq!(node("?Missing", NodeKind::Unresolved).label(), "Missing");
    }

    #[test]
    fn subgraph_keeps_inner_edges_and_recomputes_degrees() {
        let g = sample();
        let s = g.subgraph(&[true, true, false, false, false]);
        assert_eq!(s.nodes.len(), 2);
        assert_eq!(s.edges.len(), 3);
        assert_eq!(s.nodes[0].out_degree, 1);
        assert_eq!(g.index_of("c.md"), Some(2));
        assert_eq!(g.index_of("zz"), None);
    }
}
