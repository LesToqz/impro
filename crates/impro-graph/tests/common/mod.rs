#![allow(dead_code)]
//! Shared helpers: a temporary project, a deterministic vault generator and a
//! comparable snapshot of a graph.

pub mod golden;
pub mod oracle_dump;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use impro_graph::filter::GraphOptions;
use impro_graph::index::{CancelToken, ProjectIndex, ScanConfig};
use impro_graph::layout::Rng;
use impro_graph::model::Graph;
use tempfile::TempDir;

pub struct Project {
    pub dir: TempDir,
}

impl Project {
    pub fn new() -> Project {
        Project {
            dir: tempfile::tempdir().expect("temp dir"),
        }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }

    pub fn write(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.path(rel);
        std::fs::create_dir_all(p.parent().expect("has parent")).expect("mkdir");
        std::fs::write(&p, text).expect("write");
        p
    }

    pub fn write_bytes(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = self.path(rel);
        std::fs::create_dir_all(p.parent().expect("has parent")).expect("mkdir");
        std::fs::write(&p, bytes).expect("write");
        p
    }

    pub fn remove(&self, rel: &str) {
        let p = self.path(rel);
        if p.is_dir() {
            std::fs::remove_dir_all(p).expect("rmdir");
        } else {
            std::fs::remove_file(p).expect("rm");
        }
    }

    pub fn scan(&self) -> ProjectIndex {
        ProjectIndex::scan(self.root(), ScanConfig::default(), &CancelToken::new()).expect("scan")
    }
}

pub fn ids(g: &Graph) -> BTreeSet<String> {
    g.nodes.iter().map(|n| n.id.to_string()).collect()
}

pub fn opts_all() -> GraphOptions {
    GraphOptions {
        existing_only: false,
        ..GraphOptions::default()
    }
}

/// Everything observable about a graph, in a form that compares with `==`.
pub fn snapshot(g: &Graph) -> String {
    let mut out = String::new();
    for n in &g.nodes {
        out.push_str(&format!(
            "N {} {:?} {}/{} g={:?}\n",
            n.id, n.kind, n.in_degree, n.out_degree, n.group
        ));
    }
    for e in &g.edges {
        out.push_str(&format!(
            "E {} -> {} {} x{} r={}\n",
            g.nodes[e.src as usize].id,
            g.nodes[e.dst as usize].id,
            e.kind.as_str(),
            e.count,
            e.resolved
        ));
    }
    out.push_str(&format!(
        "S {:?} orphans={} comps={} trunc={} unresolved={:?} amb={}\n",
        g.stats.counts,
        g.stats.orphans,
        g.stats.components,
        g.truncated,
        g.stats.unresolved,
        g.stats
            .ambiguous
            .iter()
            .map(|a| format!(
                "{}>{}:{}={}",
                a.source,
                a.target,
                a.candidates.len(),
                a.resolved
            ))
            .collect::<Vec<_>>()
            .join(",")
    ));
    out
}

/// Description of the generated vault, for assertions.
pub struct Vault {
    pub notes: usize,
    pub hubs: usize,
}

/// A deterministic project: hubs with spokes, a chain, a ring, isolated pairs,
/// orphans, nested tags, attachments, missing targets, ambiguous names and
/// source imports, plus `bulk` extra linked notes.
pub fn generate_vault(p: &Project, bulk: usize) -> Vault {
    let mut rng = Rng::new(2024);
    // hub with spokes
    let spokes = 12;
    let mut hub = String::from("# Hub\n#hub/main\n");
    for i in 0..spokes {
        hub.push_str(&format!("[[spoke-{i}]]\n"));
        p.write(&format!("hub/spoke-{i}.md"), "back to [[Hub]] #hub/spoke\n");
    }
    p.write("hub/Hub.md", &hub);
    // chain a0 -> a1 -> ... -> a7
    for i in 0..8 {
        let next = if i < 7 {
            format!("[[chain-{}]]", i + 1)
        } else {
            String::new()
        };
        p.write(&format!("chain/chain-{i}.md"), &format!("{next}\n"));
    }
    // ring r0 -> r1 -> r2 -> r3 -> r0
    for i in 0..4 {
        p.write(
            &format!("ring/ring-{i}.md"),
            &format!("[[ring-{}]]\n", (i + 1) % 4),
        );
    }
    // isolated pairs
    for i in 0..3 {
        p.write(&format!("pairs/pair-{i}a.md"), &format!("[[pair-{i}b]]\n"));
        p.write(&format!("pairs/pair-{i}b.md"), "end\n");
    }
    // orphans
    for i in 0..5 {
        p.write(&format!("orphans/orphan-{i}.md"), "nothing here\n");
    }
    // nested tags, attachments, missing targets
    p.write(
        "media/Gallery.md",
        "---\naliases: [Pictures]\ntags: [art/photo, art]\n---\n![[sunrise.png]] ![cat](../assets/cat.png) [[Missing Note]] [[ghost.pdf]] #art/photo/raw\n",
    );
    p.write_bytes("media/sunrise.png", &[0x89, b'P', b'N', b'G', 0, 1, 2]);
    p.write_bytes("assets/cat.png", &[0x89, b'P', b'N', b'G', 0, 3]);
    p.write("media/ref-by-alias.md", "see [[Pictures]]\n");
    // ambiguous basenames
    p.write("x/Dup.md", "first\n");
    p.write("y/deep/Dup.md", "second\n");
    p.write("x/uses-dup.md", "[[Dup]]\n");
    p.write("z/uses-dup.md", "[[Dup]]\n");
    // sources
    p.write(
        "src/app/main.py",
        "import util\nfrom . import helper\nimport os\n",
    );
    p.write("src/app/helper.py", "import util\n");
    p.write("src/util.py", "");
    p.write(
        "web/app.ts",
        "import { x } from './lib';\nimport './style.css';\n",
    );
    p.write("web/lib.ts", "export const x = 1;\n");
    p.write("web/style.css", "@import 'base.css';\n");
    p.write("web/base.css", "");
    p.write(
        "crates/demo/src/lib.rs",
        "pub mod engine;\nuse crate::engine::run;\n",
    );
    p.write("crates/demo/src/engine.rs", "");
    p.write("scripts/run.sh", "source ./env.sh\n");
    p.write("scripts/env.sh", "");
    p.write("docs/readme-link.md", "[code](../src/app/main.py)\n");
    // bulk notes in clusters
    for i in 0..bulk {
        let cluster = i % 10;
        let mut text = format!("# n{i}\n[[bulk-{}]]\n", cluster);
        for _ in 0..2 {
            let j = (rng.next_u64() % bulk as u64) as usize;
            text.push_str(&format!("[[bulk-{j}]] "));
        }
        if i % 7 == 0 {
            text.push_str("#bulk/seven\n");
        }
        p.write(&format!("bulk/c{cluster}/bulk-{i}.md"), &text);
    }
    Vault {
        notes: spokes + 1 + 8 + 4 + 6 + 5 + 2 + 2 + 2 + 2 + 1 + bulk,
        hubs: 1,
    }
}
