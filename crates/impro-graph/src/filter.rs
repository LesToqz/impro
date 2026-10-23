//! From index to drawable graph: visibility toggles, search, colour rules,
//! the node cap and the local (neighbourhood) view.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::index::{
    EdgeKind, FileEntry, FileId, FileKind, ProjectIndex, CODE_EXTENSIONS, NOTE_EXTENSIONS,
};
use crate::model::{Ambiguity, Edge, Graph, Node, NodeKind};
use crate::parse::text::extension;
use crate::prefs::GraphPrefs;
use crate::query::{Doc, Query, Tags};

pub use crate::prefs::MAX_COLOR_RULES;
/// Default cap on drawn nodes.
pub const DEFAULT_MAX_NODES: usize = 3000;

#[derive(Clone, Debug)]
pub struct GraphOptions {
    pub query: String,
    pub show_tags: bool,
    pub show_attachments: bool,
    /// Hide references whose target does not exist ("Existing files only").
    pub existing_only: bool,
    /// Keep nodes without any surviving edge.
    pub show_orphans: bool,
    /// Show source files.
    pub show_code: bool,
    /// Search text of each colour rule, in priority order.
    pub groups: Vec<String>,
    /// UI mode: every file and tag is tested by the query. Data-client mode
    /// keeps non-file neighbours of matching files instead.
    pub strict_filters: bool,
    pub max_nodes: usize,
}

impl Default for GraphOptions {
    fn default() -> Self {
        GraphOptions {
            query: String::new(),
            show_tags: true,
            show_attachments: true,
            existing_only: true,
            show_orphans: true,
            show_code: true,
            groups: Vec::new(),
            strict_filters: false,
            max_nodes: DEFAULT_MAX_NODES,
        }
    }
}

impl GraphOptions {
    /// Options the panel derives from persisted preferences (strict mode).
    pub fn from_prefs(prefs: &GraphPrefs) -> Self {
        GraphOptions {
            query: prefs.search.clone(),
            show_tags: prefs.show_tags,
            show_attachments: prefs.show_attachments,
            existing_only: prefs.hide_unresolved,
            show_orphans: prefs.show_orphans,
            show_code: prefs.show_code,
            groups: prefs
                .color_groups
                .iter()
                .take(MAX_COLOR_RULES)
                .map(|g| g.query.clone())
                .collect(),
            strict_filters: true,
            max_nodes: DEFAULT_MAX_NODES,
        }
    }
}

const NONE: u32 = u32::MAX;

/// A node before filtering and sorting.
struct Draft {
    id: Arc<str>,
    kind: NodeKind,
    file: Option<FileId>,
    ctime: Option<f64>,
    /// Tag text or missing target text.
    text: Option<Box<str>>,
}

fn kind_of_file(f: &FileEntry) -> NodeKind {
    match f.kind {
        FileKind::Note => NodeKind::Note,
        FileKind::Code => NodeKind::Code,
        FileKind::Attachment => NodeKind::Attachment,
    }
}

fn file_node(drafts: &mut Vec<Draft>, node_of_file: &mut [u32], id: FileId, f: &FileEntry) -> u32 {
    let slot = &mut node_of_file[id.index()];
    if *slot == NONE {
        *slot = drafts.len() as u32;
        drafts.push(Draft {
            id: f.path.clone(),
            kind: kind_of_file(f),
            file: Some(id),
            ctime: f.ctime,
            text: None,
        });
    }
    *slot
}

fn doc_of<'a>(index: &'a ProjectIndex, d: &'a Draft) -> Doc<'a> {
    match (d.file.and_then(|f| index.get(f)), d.kind) {
        (Some(f), _) => Doc {
            path: &f.path,
            label: f.label(),
            content: &f.text,
            tags: Tags::Many(&f.tags),
        },
        (None, NodeKind::Tag) => Doc {
            path: &d.id,
            label: d.text.as_deref().unwrap_or(""),
            content: "",
            tags: Tags::One(d.text.as_deref().unwrap_or("")),
        },
        (None, _) => Doc {
            path: &d.id,
            label: d.text.as_deref().unwrap_or(""),
            content: "",
            tags: Tags::None,
        },
    }
}

/// Builds the global graph. `keep` is exempt from a strict search (the focus
/// of a local graph); the node cap applies to it like to any other node.
pub fn build_with_focus(index: &ProjectIndex, opts: &GraphOptions, keep: Option<FileId>) -> Graph {
    let mut drafts: Vec<Draft> = Vec::new();
    let mut node_of_file = vec![NONE; index.slots()];
    let mut tag_node: HashMap<&str, u32> = HashMap::new();
    let mut missing_node: HashMap<&str, u32> = HashMap::new();
    // (source node, target node, kind, resolved)
    let mut raw: Vec<(u32, u32, EdgeKind, bool)> = Vec::new();
    let mut ambiguous_refs: Vec<(FileId, usize)> = Vec::new();

    // every note and (optionally) source file is a node; attachments only
    // appear when referenced
    for (id, f) in index.iter() {
        match f.kind {
            FileKind::Note => {
                file_node(&mut drafts, &mut node_of_file, id, f);
            }
            FileKind::Code if opts.show_code => {
                file_node(&mut drafts, &mut node_of_file, id, f);
            }
            _ => {}
        }
    }

    let sources: Vec<(FileId, &FileEntry)> = index
        .iter()
        .filter(|(id, f)| f.kind != FileKind::Attachment && node_of_file[id.index()] != NONE)
        .collect();
    for (sid, f) in sources {
        let src = node_of_file[sid.index()];
        let code_source = f.kind == FileKind::Code;
        for (ri, (r, res)) in f.refs.iter().zip(&f.resolved).enumerate() {
            if res.ambiguous {
                ambiguous_refs.push((sid, ri));
            }
            let (dst, resolved) = match res.dest.and_then(|d| index.get(d).map(|e| (d, e))) {
                Some((did, dest)) => {
                    let hidden = match dest.kind {
                        FileKind::Code => !opts.show_code,
                        FileKind::Attachment => !opts.show_attachments,
                        FileKind::Note => false,
                    };
                    if hidden {
                        continue;
                    }
                    (file_node(&mut drafts, &mut node_of_file, did, dest), true)
                }
                None => {
                    if code_source || opts.existing_only {
                        continue;
                    }
                    let ext = extension(&r.target);
                    if !opts.show_attachments
                        && !ext.is_empty()
                        && !NOTE_EXTENSIONS.contains(&ext.as_str())
                        && !CODE_EXTENSIONS.contains(&ext.as_str())
                    {
                        continue;
                    }
                    let n = *missing_node.entry(&r.target).or_insert_with(|| {
                        drafts.push(Draft {
                            id: Arc::from(format!("?{}", r.target)),
                            kind: NodeKind::Unresolved,
                            file: None,
                            ctime: None,
                            text: Some(r.target.clone()),
                        });
                        drafts.len() as u32 - 1
                    });
                    (n, false)
                }
            };
            raw.push((src, dst, r.edge, resolved));
        }
        if opts.show_tags {
            for tag in &f.tags {
                let n = *tag_node.entry(tag).or_insert_with(|| {
                    drafts.push(Draft {
                        id: Arc::from(format!("#{tag}")),
                        kind: NodeKind::Tag,
                        file: None,
                        ctime: None,
                        text: Some(tag.clone()),
                    });
                    drafts.len() as u32 - 1
                });
                raw.push((src, n, EdgeKind::Tag, true));
            }
        }
    }

    // collapse duplicate (source, target, kind) edges into counts
    raw.sort_unstable_by_key(|e| (e.0, e.1, e.2));
    let mut edges: Vec<Edge> = Vec::with_capacity(raw.len());
    for (s, t, k, resolved) in raw {
        match edges.last_mut() {
            Some(l) if l.src == s && l.dst == t && l.kind == k => l.count += 1,
            _ => edges.push(Edge {
                src: s,
                dst: t,
                kind: k,
                resolved,
                count: 1,
            }),
        }
    }

    let keep_node = keep
        .map(|k| node_of_file.get(k.index()).copied().unwrap_or(NONE))
        .filter(|n| *n != NONE);
    let mut alive = vec![true; drafts.len()];

    // search
    let query = Query::parse(&opts.query);
    if !query.is_blank() {
        if opts.strict_filters {
            for (i, d) in drafts.iter().enumerate() {
                alive[i] = Some(i as u32) == keep_node
                    || d.kind == NodeKind::Unresolved
                    || query.matches(&doc_of(index, d));
            }
        } else {
            let selected: Vec<bool> = drafts
                .iter()
                .map(|d| d.kind.is_primary() && query.matches(&doc_of(index, d)))
                .collect();
            let mut keep_flags = selected.clone();
            for e in &edges {
                let (s, t) = (e.src as usize, e.dst as usize);
                if selected[s] && !drafts[t].kind.is_primary() {
                    keep_flags[t] = true;
                }
                if selected[t] && !drafts[s].kind.is_primary() {
                    keep_flags[s] = true;
                }
            }
            alive = keep_flags;
        }
        edges.retain(|e| alive[e.src as usize] && alive[e.dst as usize]);
    }

    // colour rules
    let rules: Vec<Query> = opts
        .groups
        .iter()
        .take(MAX_COLOR_RULES)
        .map(|g| Query::parse(g))
        .collect();
    let rule_blank: Vec<bool> = opts
        .groups
        .iter()
        .take(MAX_COLOR_RULES)
        .map(|g| g.trim().is_empty())
        .collect();
    let mut group: Vec<Option<u8>> = vec![None; drafts.len()];
    if !rules.is_empty() {
        for (i, d) in drafts.iter().enumerate() {
            if !alive[i] {
                continue;
            }
            let doc = doc_of(index, d);
            let strict_skip =
                opts.strict_filters && matches!(d.kind, NodeKind::Tag | NodeKind::Unresolved);
            group[i] = rules
                .iter()
                .enumerate()
                .find(|(j, q)| {
                    !(opts.strict_filters && (strict_skip || rule_blank[*j])) && q.matches(&doc)
                })
                .map(|(j, _)| j as u8);
        }
    }

    // orphans
    if !opts.show_orphans {
        let mut linked = vec![false; drafts.len()];
        for e in &edges {
            linked[e.src as usize] = true;
            linked[e.dst as usize] = true;
        }
        for (a, l) in alive.iter_mut().zip(&linked) {
            *a &= *l;
        }
    }

    // cap: keep the most connected nodes; every edge kind between two nodes
    // counts, a self reference twice
    let total_alive = alive.iter().filter(|a| **a).count();
    let truncated = total_alive > opts.max_nodes;
    if truncated {
        let mut degree = vec![0u32; drafts.len()];
        for e in edges
            .iter()
            .filter(|e| alive[e.src as usize] && alive[e.dst as usize])
        {
            degree[e.src as usize] += 1;
            degree[e.dst as usize] += 1;
        }
        let mut order: Vec<usize> = (0..drafts.len()).filter(|i| alive[*i]).collect();
        order.sort_unstable_by(|a, b| {
            degree[*b]
                .cmp(&degree[*a])
                .then_with(|| drafts[*a].id.cmp(&drafts[*b].id))
        });
        let survivors: std::collections::HashSet<usize> =
            order.into_iter().take(opts.max_nodes).collect();
        for (i, a) in alive.iter_mut().enumerate() {
            *a = *a && survivors.contains(&i);
        }
    }

    // compact, sort by id, remap
    let mut order: Vec<usize> = (0..drafts.len()).filter(|i| alive[*i]).collect();
    order.sort_unstable_by(|a, b| drafts[*a].id.cmp(&drafts[*b].id));
    let mut remap = vec![NONE; drafts.len()];
    for (new, old) in order.iter().enumerate() {
        remap[*old] = new as u32;
    }
    let nodes: Vec<Node> = order
        .iter()
        .map(|&i| {
            let d = &drafts[i];
            Node {
                id: d.id.clone(),
                kind: d.kind,
                file: d.file,
                in_degree: 0,
                out_degree: 0,
                group: group[i],
                ctime: d.ctime,
            }
        })
        .collect();
    let mut out_edges: Vec<Edge> = edges
        .iter()
        .filter(|e| remap[e.src as usize] != NONE && remap[e.dst as usize] != NONE)
        .map(|e| Edge {
            src: remap[e.src as usize],
            dst: remap[e.dst as usize],
            ..*e
        })
        .collect();
    out_edges.sort_unstable_by_key(|e| (e.src, e.dst, e.kind));

    let ambiguous: Vec<Ambiguity> = ambiguous_refs
        .into_iter()
        .filter_map(|(sid, ri)| {
            let f = index.get(sid)?;
            let r = f.refs.get(ri)?;
            let dest = f.resolved.get(ri)?.dest?;
            let (sn, dn) = (node_of_file[sid.index()], node_of_file[dest.index()]);
            if sn == NONE || dn == NONE || remap[sn as usize] == NONE || remap[dn as usize] == NONE
            {
                return None;
            }
            let target = r.target.split(['#', '^', '?']).next().unwrap_or("");
            let candidates = index
                .wiki_candidates(&f.path, target)
                .into_iter()
                .filter_map(|c| index.get(c).map(|e| e.path.clone()))
                .collect();
            Some(Ambiguity {
                source: f.path.clone(),
                target: r.target.clone(),
                candidates,
                resolved: index.get(dest)?.path.clone(),
            })
        })
        .collect();

    Graph::assemble(nodes, out_edges, truncated, total_alive, ambiguous)
}

/// Builds the global graph.
pub fn build(index: &ProjectIndex, opts: &GraphOptions) -> Graph {
    build_with_focus(index, opts, None)
}

/// Only the files below `prefix` and the links between them.
pub fn folder(index: &ProjectIndex, prefix: &str, opts: &GraphOptions) -> Graph {
    let g = build(index, opts);
    let prefix = format!("{}/", prefix.trim_matches('/'));
    let keep: Vec<bool> = g
        .nodes
        .iter()
        .map(|n| {
            !matches!(n.kind, NodeKind::Tag | NodeKind::Unresolved) && n.id.starts_with(&prefix)
        })
        .collect();
    g.subgraph(&keep)
}

/// A neighbourhood around one file.
#[derive(Clone, Debug, Default)]
pub struct LocalGraph {
    pub graph: Graph,
    /// Breadth-first layer of each node (focus = 0), aligned with `graph.nodes`.
    pub layers: Vec<u8>,
    /// Index of the focus node, if it is part of the graph.
    pub focus: Option<usize>,
}

impl LocalGraph {
    /// Marker weight of each node: `12 / (layer + 1)`.
    pub fn weights(&self) -> Vec<f32> {
        self.layers
            .iter()
            .map(|l| crate::geometry::local_weight(*l as u32))
            .collect()
    }
}

/// Breadth-first layers from `focus` over `graph`, up to `depth` hops, following
/// outgoing and/or incoming references. Unreached nodes get `u8::MAX`.
pub fn layers_from(
    graph: &Graph,
    focus: usize,
    depth: u32,
    incoming: bool,
    outgoing: bool,
) -> Vec<u8> {
    let mut layer = vec![u8::MAX; graph.nodes.len()];
    if focus >= graph.nodes.len() {
        return layer;
    }
    let adj = graph.adjacency();
    layer[focus] = 0;
    let mut frontier = vec![focus];
    let mut next = Vec::new();
    for step in 1..=depth.clamp(1, 250) {
        next.clear();
        for &u in &frontier {
            if outgoing {
                for &v in adj.outgoing(u) {
                    if layer[v as usize] == u8::MAX {
                        layer[v as usize] = step as u8;
                        next.push(v as usize);
                    }
                }
            }
            if incoming {
                for &v in adj.incoming(u) {
                    if layer[v as usize] == u8::MAX {
                        layer[v as usize] = step as u8;
                        next.push(v as usize);
                    }
                }
            }
        }
        if next.is_empty() {
            break;
        }
        std::mem::swap(&mut frontier, &mut next);
    }
    layer
}

/// The focus file and everything within `depth` hops. The focus survives a
/// strict search filter; like every node it can fall to the node cap, which
/// leaves an empty graph.
pub fn local(
    index: &ProjectIndex,
    focus: &Path,
    depth: u32,
    incoming: bool,
    outgoing: bool,
    opts: &GraphOptions,
) -> LocalGraph {
    let focus_id = index
        .rel_path(focus)
        .and_then(|rel| index.id_of_folded(&rel).or_else(|| index.id_of(&rel)));
    let full = build_with_focus(index, opts, focus_id);
    let focus_idx = focus_id
        .and_then(|id| index.get(id))
        .and_then(|f| full.index_of(&f.path));
    let focus_idx = match focus_idx {
        Some(i) => i,
        None => {
            // no focus in the graph: nothing to show, but the cap still counts
            let none = vec![false; full.nodes.len()];
            return LocalGraph {
                graph: full.subgraph(&none),
                ..LocalGraph::default()
            };
        }
    };
    let layers = layers_from(&full, focus_idx, depth, incoming, outgoing);
    let keep: Vec<bool> = layers.iter().map(|l| *l != u8::MAX).collect();
    let graph = full.subgraph(&keep);
    let layers: Vec<u8> = layers.into_iter().filter(|l| *l != u8::MAX).collect();
    let focus = graph.index_of(&full.nodes[focus_idx].id);
    LocalGraph {
        graph,
        layers,
        focus,
    }
}

impl ProjectIndex {
    pub fn build_graph(&self, opts: &GraphOptions) -> Graph {
        build(self, opts)
    }

    pub fn local_graph(
        &self,
        focus: &Path,
        depth: u32,
        incoming: bool,
        outgoing: bool,
        opts: &GraphOptions,
    ) -> LocalGraph {
        local(self, focus, depth, incoming, outgoing, opts)
    }

    pub fn folder_graph(&self, prefix: &str, opts: &GraphOptions) -> Graph {
        folder(self, prefix, opts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Rng;

    /// Independent reference: repeated relaxation instead of a queue.
    fn reference_layers(g: &Graph, focus: usize, depth: u32, inc: bool, out: bool) -> Vec<u8> {
        let n = g.nodes.len();
        let mut dist = vec![u32::MAX; n];
        dist[focus] = 0;
        loop {
            let mut changed = false;
            for &(s, t) in &g.pairs {
                let (s, t) = (s as usize, t as usize);
                if out && dist[s] != u32::MAX && dist[s] + 1 < dist[t] {
                    dist[t] = dist[s] + 1;
                    changed = true;
                }
                if inc && dist[t] != u32::MAX && dist[t] + 1 < dist[s] {
                    dist[s] = dist[t] + 1;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        dist.iter()
            .map(|d| if *d <= depth { *d as u8 } else { u8::MAX })
            .collect()
    }

    #[test]
    fn layers_match_a_reference_on_random_graphs() {
        let mut rng = Rng::new(11);
        for round in 0..200 {
            let n = 2 + (rng.next_u64() % 40) as usize;
            let m = (rng.next_u64() % (3 * n as u64)) as usize;
            let pairs: Vec<_> = (0..m)
                .map(|_| {
                    (
                        (rng.next_u64() % n as u64) as u32,
                        (rng.next_u64() % n as u64) as u32,
                    )
                })
                .collect();
            let g = Graph::from_pairs(n, &pairs);
            let focus = (rng.next_u64() % n as u64) as usize;
            for depth in 1..=6 {
                for (inc, out) in [(true, true), (true, false), (false, true)] {
                    assert_eq!(
                        layers_from(&g, focus, depth, inc, out),
                        reference_layers(&g, focus, depth, inc, out),
                        "round {round} depth {depth} inc {inc} out {out}"
                    );
                }
            }
        }
    }

    #[test]
    fn neither_direction_reaches_nothing_and_bad_focus_is_safe() {
        let g = Graph::from_pairs(3, &[(0, 1), (1, 2)]);
        assert_eq!(layers_from(&g, 1, 6, false, false), [u8::MAX, 0, u8::MAX]);
        assert_eq!(layers_from(&g, 9, 6, true, true), [u8::MAX; 3]);
        assert_eq!(
            layers_from(&g, 0, 0, true, true),
            [0, 1, u8::MAX],
            "depth is at least one hop"
        );
    }

    #[test]
    fn options_follow_the_preferences() {
        let mut prefs = GraphPrefs {
            search: "x".into(),
            show_tags: true,
            hide_unresolved: true,
            show_orphans: false,
            ..GraphPrefs::default()
        };
        for _ in 0..12 {
            prefs.add_color_rule("q", true);
        }
        let o = GraphOptions::from_prefs(&prefs);
        assert_eq!(o.query, "x");
        assert!(o.show_tags && o.existing_only && !o.show_orphans && o.strict_filters);
        assert!(!o.show_attachments && o.show_code);
        assert_eq!(o.groups.len(), MAX_COLOR_RULES);
        assert_eq!(o.max_nodes, 3000);
    }
}
