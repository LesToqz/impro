//! Offline checks against `tests/golden/*.json`: answers the reference gave on
//! synthetic trees, frozen by `scripts/graph-oracle/freeze.py`. The trees are
//! rebuilt here, so no Python and no reference checkout is needed.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::oracle_dump::{canonical, digest, run};
use super::Project;

fn load(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("golden json")
}

fn golden_names(prefix: &str) -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("golden dir")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix) && n.ends_with(".json"))
        .collect();
    names.sort();
    names
}

fn filler(size: usize, ch: char) -> String {
    let line = format!("{}\n", ch.to_string().repeat(79));
    line.repeat(size / 80 + 1)[..size].to_string()
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(path, bytes).expect("write");
}

fn chain(root: &Path, count: usize) {
    for i in 0..count {
        let mut body = vec![format!("note {i}")];
        for k in [1usize, 7, 31, 100] {
            if i >= k && (i + k) % 3 != 0 {
                body.push(format!("[[n{:05}]]", i - k));
            }
        }
        if i % 50 == 0 {
            body.push("#hub".into());
        }
        if i % 997 == 0 {
            body.push("[[never]]".into());
        }
        put(
            root,
            &format!("d{}/n{i:05}.md", i % 17),
            format!("{}\n", body.join(" ")).as_bytes(),
        );
    }
}

fn big_mixed(root: &Path) {
    for i in 0..3300usize {
        let mut body = vec![
            format!("[[m{:05}]]", (i * 7) % 3300),
            format!("#t{}", i % 40),
        ];
        if i % 11 == 0 {
            body.push(format!("[[ghost{}]]", i % 300));
        }
        if i % 13 == 0 {
            body.push(format!("![[img{}.png]]", i % 20));
        }
        put(
            root,
            &format!("g{}/m{i:05}.md", i % 9),
            format!("{}\n", body.join(" ")).as_bytes(),
        );
    }
    for i in 0..20 {
        put(root, &format!("img{i}.png"), b"\0");
    }
}

fn b64(s: &str) -> Vec<u8> {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.bytes().filter(|c| *c != b'=') {
        acc = (acc << 6) | A.iter().position(|a| *a == c).expect("base64") as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// Materialises a golden `tree` (a manifest or a named generator) under `root`.
fn build_tree(root: &Path, tree: &Value) {
    if let Some(kind) = tree.get("generate").and_then(Value::as_str) {
        let count = tree["count"].as_u64().expect("count") as usize;
        match kind {
            "chain" => chain(root, count),
            "mixed" => big_mixed(root),
            other => panic!("unknown generator {other}"),
        }
        return;
    }
    for e in tree["files"].as_array().expect("files") {
        let rel = e["path"].as_str().expect("path");
        if let Some(_target) = e.get("link").and_then(Value::as_str) {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(_target, path);
            continue;
        }
        let bytes = if let Some(t) = e.get("text").and_then(Value::as_str) {
            t.as_bytes().to_vec()
        } else if let Some(b) = e.get("b64").and_then(Value::as_str) {
            b64(b)
        } else if let Some(b) = e.get("pad_b64").and_then(Value::as_str) {
            let mut v = b64(b);
            v.resize(v.len() + e["zeros"].as_u64().expect("zeros") as usize, 0);
            v
        } else {
            let ch = e["ch"].as_str().expect("ch").chars().next().expect("char");
            let text = format!(
                "{}{}{}",
                e.get("head").and_then(Value::as_str).unwrap_or(""),
                filler(e["fill"].as_u64().expect("fill") as usize, ch),
                e.get("tail").and_then(Value::as_str).unwrap_or("")
            );
            text.into_bytes()
        };
        put(root, rel, &bytes);
    }
}

fn roots(tree: &Value) -> (Project, PathBuf) {
    let p = Project::new();
    build_tree(p.root(), tree);
    let root = std::fs::canonicalize(p.root()).expect("root");
    (p, root)
}

/// Every `graph-*.json`: builds, local and folder graphs under several caps.
pub fn check_graphs() {
    for name in golden_names("graph-") {
        let g = load(&name);
        let (_keep, root) = roots(&g["tree"]);
        for scenario in g["scenarios"].as_array().expect("scenarios") {
            let runs = scenario["runs"].as_array().expect("runs");
            let req = json!({"mode": "graph", "root": root, "max_nodes": scenario["max_nodes"], "runs": runs});
            let got = run(&req);
            for (spec, answer) in runs.iter().zip(got["runs"].as_array().expect("runs")) {
                let label = spec["name"].as_str().unwrap_or("?");
                if let Some(full) = scenario["full"].get(label) {
                    assert_eq!(
                        canonical(&answer["result"]),
                        canonical(full),
                        "{name} {label}"
                    );
                }
                assert_eq!(
                    digest(&answer["result"]),
                    spec["digest"].as_str().expect("digest"),
                    "{name} cap {} run {label}",
                    scenario["max_nodes"]
                );
            }
        }
    }
}

/// Every `live-*.json`: scripted edits, the graph after each one.
pub fn check_live() {
    for name in golden_names("live-") {
        let g = load(&name);
        let (_keep, root) = roots(&g["tree"]);
        let req = json!({"mode": "live", "root": root, "options": g["options"], "ops": g["ops"]});
        let got = run(&req);
        let digests = g["digests"].as_array().expect("digests");
        let steps = got["steps"].as_array().expect("steps");
        assert_eq!(steps.len(), digests.len(), "{name}");
        for (i, (step, want)) in steps.iter().zip(digests).enumerate() {
            assert_eq!(
                digest(&step["result"]),
                want.as_str().expect("digest"),
                "{name} step {i}"
            );
        }
    }
}

/// `text.json`: link, tag, front matter and import extraction on a corpus.
pub fn check_text() {
    let g = load("text.json");
    let snippets = g["snippets"].as_array().expect("snippets");
    let got = run(&json!({"mode": "text", "snippets": snippets}));
    for ((snippet, mut answer), want) in snippets
        .iter()
        .zip(got["snippets"].as_array().expect("answers").clone())
        .zip(g["digests"].as_array().expect("digests"))
    {
        // the reference collapses whitespace in aliases and reads angle-bracket autolinks
        if let Some(links) = answer["links"].as_array_mut() {
            for l in links {
                if let Some(a) = l["alias"].as_str() {
                    l["alias"] = json!(a.split_whitespace().collect::<Vec<_>>().join(" "));
                }
            }
        }
        assert_eq!(
            digest(&answer),
            want.as_str().expect("digest"),
            "{}",
            snippet["name"]
        );
    }
}

fn same_number(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_number(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_number(v, w)))
        }
        _ => a == b,
    }
}

/// `prefs.json`: hostile preference inputs and the reference's normal form.
pub fn check_prefs() {
    let g = load("prefs.json");
    let got = run(&json!({"mode": "prefs", "values": g["values"]}));
    let (got, want) = (
        got["values"].as_array().unwrap(),
        g["expected"].as_array().unwrap(),
    );
    for (i, (a, b)) in got.iter().zip(want).enumerate() {
        assert!(
            same_number(a, b),
            "prefs case {i}: input {}\n got {a}\nwant {b}",
            g["values"][i]
        );
    }
    assert_eq!(got.len(), want.len());
}
