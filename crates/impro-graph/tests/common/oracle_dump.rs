//! Canonical JSON answers of the engine, shared by `examples/graph_dump.rs`
//! (the differential harness in `scripts/graph-oracle`) and the offline golden
//! tests. A request names a mode (`graph`, `text`, `live`, `prefs`); the answer
//! has the shape the reference-side script prints.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use impro_graph::parse::{extract_imports, extract_links, extract_tags, frontmatter, CodeKind};
use impro_graph::parse::{LinkKind, LinkSyntax, PropValue};
use impro_graph::{
    normalize, CancelToken, Graph, GraphOptions, NodeKind, ProjectIndex, ScanConfig,
};
use serde_json::{json, Map, Value};

fn str_of<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

fn options_of(spec: Option<&Value>, max_nodes: usize) -> GraphOptions {
    let mut o = GraphOptions {
        max_nodes,
        ..GraphOptions::default()
    };
    let Some(spec) = spec else { return o };
    let flag = |key: &str, slot: &mut bool| {
        if let Some(b) = spec.get(key).and_then(Value::as_bool) {
            *slot = b;
        }
    };
    if let Some(q) = spec.get("query").and_then(Value::as_str) {
        o.query = q.to_string();
    }
    flag("show_tags", &mut o.show_tags);
    flag("show_attachments", &mut o.show_attachments);
    flag("existing_only", &mut o.existing_only);
    flag("show_orphans", &mut o.show_orphans);
    flag("show_code", &mut o.show_code);
    flag("strict_filters", &mut o.strict_filters);
    if let Some(groups) = spec.get("groups").and_then(Value::as_array) {
        o.groups = groups
            .iter()
            .filter_map(|g| g.as_str().map(str::to_string))
            .collect();
    }
    o
}

fn dump_graph(index: &ProjectIndex, g: &Graph, focus: Option<&str>) -> Value {
    let nodes: Vec<Value> = g
        .nodes
        .iter()
        .map(|n| {
            let tags: Vec<String> = match (n.kind, n.file.and_then(|f| index.get(f))) {
                (NodeKind::Tag, _) => vec![n.label().to_string()],
                (_, Some(f)) => f.tags.iter().map(|t| t.to_string()).collect(),
                _ => Vec::new(),
            };
            json!({
                "id": &*n.id, "label": n.label(), "kind": n.kind.as_str(),
                "path": n.path(), "folder": n.folder(), "tags": tags,
                "in": n.in_degree, "out": n.out_degree, "size_hint": n.size_hint(),
                "group": n.group, "ctime": n.ctime,
            })
        })
        .collect();
    let links: Vec<Value> = g
        .edges
        .iter()
        .map(|e| {
            json!({
                "source": &*g.nodes[e.src as usize].id,
                "target": &*g.nodes[e.dst as usize].id,
                "kind": e.kind.as_str(), "resolved": e.resolved,
                "count": e.count, "self": e.is_self(),
            })
        })
        .collect();
    let mut counts = Map::new();
    for kind in NodeKind::ALL {
        counts.insert(kind.as_str().into(), json!(g.stats.count(kind)));
    }
    let ambiguous: Vec<Value> = g
        .stats
        .ambiguous
        .iter()
        .map(|a| {
            json!({"source": &*a.source, "target": &*a.target,
                   "candidates": a.candidates.iter().map(|c| &**c).collect::<Vec<_>>(),
                   "resolved": &*a.resolved})
        })
        .collect();
    json!({
        "nodes": nodes, "links": links, "truncated": g.truncated, "focus": focus,
        "stats": {
            "counts": counts, "orphans": g.stats.orphans,
            "unresolved": g.stats.unresolved.iter().map(|u| &**u).collect::<Vec<_>>(),
            "ambiguous": ambiguous, "components": g.stats.components,
        },
    })
}

fn scan(root: &Path) -> ProjectIndex {
    ProjectIndex::scan(root, ScanConfig::default(), &CancelToken::new()).expect("scan")
}

pub fn run_graph(req: &Value) -> Value {
    let root = PathBuf::from(str_of(req, "root"));
    let max = req.get("max_nodes").and_then(Value::as_u64).unwrap_or(3000) as usize;
    let mut index = scan(&root);
    if let Some(over) = req.get("overrides").and_then(Value::as_object) {
        for (path, text) in over {
            index.update_file(Path::new(path), text.as_str());
        }
    }
    let runs: Vec<Value> = req["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .map(|run| {
            let opts = options_of(run.get("options"), max);
            let result = match str_of(run, "mode") {
                "build" => dump_graph(&index, &index.build_graph(&opts), None),
                "local" => {
                    let depth = run.get("depth").and_then(Value::as_u64).unwrap_or(1) as u32;
                    let flag = |k: &str| run.get(k).and_then(Value::as_bool).unwrap_or(true);
                    let local = index.local_graph(
                        &root.join(str_of(run, "path")),
                        depth,
                        flag("incoming"),
                        flag("outgoing"),
                        &opts,
                    );
                    let focus = local.focus.map(|i| local.graph.nodes[i].id.to_string());
                    dump_graph(&index, &local.graph, focus.as_deref())
                }
                "folder" => dump_graph(
                    &index,
                    &index.folder_graph(str_of(run, "prefix"), &opts),
                    None,
                ),
                other => panic!("unknown run mode {other}"),
            };
            json!({"name": str_of(run, "name"), "result": result})
        })
        .collect();
    json!({"runs": runs})
}

fn code_class(kind: CodeKind) -> String {
    match kind {
        CodeKind::Python => "python".into(),
        CodeKind::Script => "script".into(),
        CodeKind::Style => "style".into(),
        CodeKind::Html => "html".into(),
        #[allow(unreachable_patterns)]
        other => format!("{other:?}").to_lowercase(),
    }
}

pub fn run_text(req: &Value) -> Value {
    let out: Vec<Value> = req["snippets"]
        .as_array()
        .expect("snippets")
        .iter()
        .map(|item| {
            let (path, text) = (str_of(item, "path"), str_of(item, "text"));
            let fm = frontmatter::parse(text);
            let props: Map<String, Value> = fm
                .props
                .iter()
                .map(|(k, v)| {
                    let v = match v {
                        PropValue::Text(t) => json!(t),
                        PropValue::List(l) => json!(l),
                    };
                    (k.clone(), v)
                })
                .collect();
            let links: Vec<Value> = extract_links(text)
                .iter()
                .map(|l| {
                    json!({
                        "target": l.target,
                        "kind": match l.kind { LinkKind::Wiki => "wiki", LinkKind::Embed => "embed", LinkKind::Markdown => "md" },
                        "alias": l.alias, "heading": l.heading, "block": l.block,
                        "line": l.line, "start": l.start,
                        "syntax": match l.syntax { LinkSyntax::Wiki => "wiki", LinkSyntax::Markdown => "md" },
                    })
                })
                .collect();
            let code: Vec<Value> = extract_imports(path, text)
                .iter()
                .map(|c| json!({"spec": c.spec, "kind": code_class(c.kind), "line": c.line}))
                .collect();
            json!({
                "name": str_of(item, "name"), "props": props,
                "body_start": text[..fm.body_start].chars().count(),
                "links": links, "tags": extract_tags(text), "code": code,
            })
        })
        .collect();
    json!({"snippets": out})
}

pub fn run_live(req: &Value) -> Value {
    let root = std::fs::canonicalize(str_of(req, "root")).expect("root");
    let max = req.get("max_nodes").and_then(Value::as_u64).unwrap_or(3000) as usize;
    let opts = options_of(req.get("options"), max);
    let mut index = scan(&root);
    let mut steps =
        vec![json!({"op": "scan", "result": dump_graph(&index, &index.build_graph(&opts), None)})];
    for op in req["ops"].as_array().expect("ops") {
        let abs = |k: &str| root.join(str_of(op, k));
        match str_of(op, "op") {
            "write" => {
                let path = abs("path");
                std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
                std::fs::write(&path, str_of(op, "text")).expect("write");
                index.update_file(&path, None);
            }
            "set_text" => {
                index.update_file(&abs("path"), op["text"].as_str());
            }
            "delete" => {
                let path = abs("path");
                if path.is_dir() {
                    std::fs::remove_dir_all(&path).expect("rm");
                } else {
                    std::fs::remove_file(&path).expect("rm");
                }
                index.remove(&path);
            }
            "rename" => {
                let (from, to) = (abs("from"), abs("to"));
                std::fs::create_dir_all(to.parent().expect("parent")).expect("mkdir");
                std::fs::rename(&from, &to).expect("mv");
                index.rename(&from, &to);
            }
            other => panic!("unknown op {other}"),
        }
        assert!(
            index.resolution_is_consistent(),
            "incremental resolution diverged after {op}"
        );
        steps
            .push(json!({"op": op, "result": dump_graph(&index, &index.build_graph(&opts), None)}));
    }
    json!({"steps": steps})
}

pub fn run_prefs(req: &Value) -> Value {
    let values: Vec<Value> = req["values"]
        .as_array()
        .expect("values")
        .iter()
        .map(|v| normalize(v).to_value())
        .collect();
    json!({"values": values})
}

/// Answers any request.
pub fn run(req: &Value) -> Value {
    match str_of(req, "mode") {
        "graph" => run_graph(req),
        "text" => run_text(req),
        "live" => run_live(req),
        "prefs" => run_prefs(req),
        other => panic!("unknown mode {other}"),
    }
}

/// Key-sorted compact JSON without the creation times, which depend on the
/// machine: the text hashed by [`digest`].
pub fn canonical(v: &Value) -> String {
    fn walk(v: &Value, out: &mut String) {
        match v {
            Value::Object(m) => {
                let mut keys: Vec<&String> = m.keys().filter(|k| *k != "ctime").collect();
                keys.sort();
                out.push('{');
                for (i, k) in keys.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&Value::String(k.clone()).to_string());
                    out.push(':');
                    walk(&m[k], out);
                }
                out.push('}');
            }
            Value::Array(a) => {
                out.push('[');
                for (i, x) in a.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    walk(x, out);
                }
                out.push(']');
            }
            other => out.push_str(&other.to_string()),
        }
    }
    let mut out = String::new();
    walk(v, &mut out);
    out
}

/// 64-bit FNV-1a of the canonical text, as 16 hex digits.
pub fn digest(v: &Value) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in canonical(v).bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}
