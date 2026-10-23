mod common;

use std::collections::BTreeSet;
use std::path::Path;

use common::*;
use impro_graph::delta::diff;
use impro_graph::filter::{local, GraphOptions};
use impro_graph::index::{EdgeKind, ProjectIndex};
use impro_graph::layout::{Layout, Rng};
use impro_graph::model::NodeKind;

fn set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn generated_vault_has_the_expected_shape() {
    let p = Project::new();
    let vault = generate_vault(&p, 300);
    let idx = p.scan();
    let all = idx.build_graph(&GraphOptions {
        show_tags: true,
        show_attachments: true,
        existing_only: false,
        ..GraphOptions::default()
    });
    let at = |id: &str| &all.nodes[all.index_of(id).unwrap_or_else(|| panic!("missing {id}"))];
    // hub and spokes
    // twelve spokes each way, plus the #hub/main tag
    assert_eq!(
        (at("hub/Hub.md").in_degree, at("hub/Hub.md").out_degree),
        (12, 13)
    );
    assert!(all.nodes.len() > vault.notes);
    // chain and ring
    assert_eq!(
        (
            at("chain/chain-3.md").in_degree,
            at("chain/chain-3.md").out_degree
        ),
        (1, 1)
    );
    assert_eq!(
        (
            at("chain/chain-0.md").in_degree,
            at("chain/chain-0.md").out_degree
        ),
        (0, 1)
    );
    for i in 0..4 {
        let n = at(&format!("ring/ring-{i}.md"));
        assert_eq!((n.in_degree, n.out_degree), (1, 1));
    }
    // orphans
    for i in 0..5 {
        assert_eq!(at(&format!("orphans/orphan-{i}.md")).degree(), 0);
    }
    // nested tags are separate nodes; aliases resolve; missing targets are synthetic
    for tag in [
        "#art",
        "#art/photo",
        "#art/photo/raw",
        "#hub/main",
        "#hub/spoke",
        "#bulk/seven",
    ] {
        assert_eq!(at(tag).kind, NodeKind::Tag, "{tag}");
    }
    let alias_target = all.index_of("media/Gallery.md").unwrap();
    let alias_source = all.index_of("media/ref-by-alias.md").unwrap();
    assert!(all
        .pairs
        .contains(&(alias_source as u32, alias_target as u32)));
    assert_eq!(at("?Missing Note").kind, NodeKind::Unresolved);
    assert_eq!(at("?ghost.pdf").kind, NodeKind::Unresolved);
    assert_eq!(at("media/sunrise.png").kind, NodeKind::Attachment);
    assert_eq!(at("assets/cat.png").kind, NodeKind::Attachment);
    // ambiguity: same folder first, then the shortest path
    let amb: Vec<_> = all
        .stats
        .ambiguous
        .iter()
        .filter(|a| a.target.as_ref() == "Dup")
        .map(|a| (a.source.to_string(), a.resolved.to_string()))
        .collect();
    assert_eq!(
        amb,
        [
            ("x/uses-dup.md".to_string(), "x/Dup.md".to_string()),
            ("z/uses-dup.md".to_string(), "x/Dup.md".to_string())
        ]
    );
    // code imports
    let targets = |src: &str| -> BTreeSet<String> {
        all.edges
            .iter()
            .filter(|e| &*all.nodes[e.src as usize].id == src)
            .map(|e| all.nodes[e.dst as usize].id.to_string())
            .collect()
    };
    assert_eq!(
        targets("src/app/main.py"),
        set(&["src/util.py", "src/app/helper.py"])
    );
    assert_eq!(targets("web/app.ts"), set(&["web/lib.ts", "web/style.css"]));
    assert_eq!(targets("web/style.css"), set(&["web/base.css"]));
    assert_eq!(
        targets("crates/demo/src/lib.rs"),
        set(&["crates/demo/src/engine.rs"])
    );
    let rs = all
        .edges
        .iter()
        .find(|e| &*all.nodes[e.src as usize].id == "crates/demo/src/lib.rs")
        .unwrap();
    assert_eq!((rs.kind, rs.count), (EdgeKind::Import, 2));
    assert_eq!(targets("scripts/run.sh"), set(&["scripts/env.sh"]));
    assert_eq!(targets("docs/readme-link.md"), set(&["src/app/main.py"]));
    // defaults hide tags/attachments; the stats stay consistent
    let defaults = idx.build_graph(&GraphOptions {
        show_tags: false,
        show_attachments: false,
        ..GraphOptions::default()
    });
    assert!(!defaults.nodes.iter().any(|n| n.kind == NodeKind::Tag));
    assert_eq!(
        defaults.stats.orphans,
        defaults.nodes.iter().filter(|n| n.degree() == 0).count()
    );
    assert!(defaults.stats.components >= 1 + 3 + 5);
}

#[test]
fn caps_and_filters_compose() {
    let p = Project::new();
    generate_vault(&p, 200);
    let idx = p.scan();
    let capped = idx.build_graph(&GraphOptions {
        max_nodes: 40,
        ..GraphOptions::default()
    });
    assert_eq!(capped.nodes.len(), 40);
    assert!(capped.truncated);
    assert!(capped.total_nodes > 40);
    // the hub is among the most connected
    assert!(capped.index_of("hub/Hub.md").is_some());
    // every surviving edge connects surviving nodes
    assert!(capped
        .edges
        .iter()
        .all(|e| (e.src as usize) < 40 && (e.dst as usize) < 40));
    let no_orphans = idx.build_graph(&GraphOptions {
        show_orphans: false,
        ..GraphOptions::default()
    });
    assert!(no_orphans.nodes.iter().all(|n| n.degree() > 0));
    assert_eq!(no_orphans.stats.orphans, 0);
    // search by field, tag prefix, regex, negation
    let strict = |q: &str| {
        idx.build_graph(&GraphOptions {
            query: q.into(),
            strict_filters: true,
            show_tags: true,
            ..GraphOptions::default()
        })
    };
    assert_eq!(strict("path:orphans/").nodes.len(), 5);
    let art = strict("tag:art");
    assert!(art.nodes.iter().any(|n| &*n.id == "media/Gallery.md"));
    assert!(art.nodes.iter().any(|n| &*n.id == "#art/photo"));
    assert!(!strict("tag:art")
        .nodes
        .iter()
        .any(|n| &*n.id == "hub/Hub.md"));
    // an unfielded regex is tested against path, label and content separately
    assert_eq!(strict("/^spoke-1[01]$/").nodes.len(), 2);
    assert_eq!(strict("file:/spoke-1[01]/").nodes.len(), 2);
    assert!(strict("/(unclosed").nodes.is_empty());
    let neg = strict("path:hub/ -file:spoke");
    assert_eq!(ids(&neg), set(&["hub/Hub.md", "#hub/main"]));
}

#[test]
fn colour_rules_pick_the_first_match_and_cap_at_twelve() {
    let p = Project::new();
    generate_vault(&p, 20);
    let idx = p.scan();
    let mut groups: Vec<String> = vec![
        "path:nothing-matches".into(),
        "path:hub/spoke".into(),
        "path:hub/".into(),
    ];
    groups.extend((0..12).map(|i| format!("path:zzz{i}")));
    let g = idx.build_graph(&GraphOptions {
        groups,
        strict_filters: true,
        show_tags: true,
        ..GraphOptions::default()
    });
    let group = |id: &str| g.nodes[g.index_of(id).unwrap()].group;
    assert_eq!(group("hub/spoke-1.md"), Some(1));
    assert_eq!(group("hub/Hub.md"), Some(2));
    assert_eq!(group("ring/ring-0.md"), None);
    assert_eq!(group("#hub/main"), None, "tags keep their kind colour");
    assert!(g.nodes.iter().all(|n| n.group.is_none_or(|x| x < 12)));
}

#[test]
fn local_graph_on_the_vault_respects_depth_and_directions() {
    let p = Project::new();
    generate_vault(&p, 30);
    let idx = p.scan();
    let o = GraphOptions::default();
    let focus = Path::new("chain/chain-3.md");
    for depth in 1..=6u32 {
        let both = local(&idx, focus, depth, true, true, &o);
        // chain-3 reaches `depth` neighbours each way
        let expect = {
            let lo = 3usize.saturating_sub(depth as usize);
            let hi = (3 + depth as usize).min(7);
            hi - lo + 1
        };
        assert_eq!(both.graph.nodes.len(), expect, "depth {depth}");
        let out = local(&idx, focus, depth, false, true, &o);
        assert_eq!(
            out.graph.nodes.len(),
            1 + (depth as usize).min(4),
            "outgoing depth {depth}"
        );
        let inc = local(&idx, focus, depth, true, false, &o);
        assert_eq!(
            inc.graph.nodes.len(),
            1 + (depth as usize).min(3),
            "incoming depth {depth}"
        );
    }
    // layers are BFS distances
    let l = local(&idx, focus, 6, true, true, &o);
    for (i, n) in l.graph.nodes.iter().enumerate() {
        let k: i32 =
            n.id.trim_start_matches("chain/chain-")
                .trim_end_matches(".md")
                .parse()
                .unwrap();
        assert_eq!(l.layers[i] as i32, (k - 3).abs(), "{}", n.id);
    }
    // a hub neighbourhood at depth one: the hub, its spokes and its tag
    let hub = local(&idx, Path::new("hub/Hub.md"), 1, true, true, &o);
    assert_eq!(hub.graph.nodes.len(), 14);
}

#[test]
fn deltas_describe_exactly_what_changed() {
    let p = Project::new();
    generate_vault(&p, 20);
    let mut idx = p.scan();
    let o = opts_all();
    let before = idx.build_graph(&o);
    assert!(diff(&before, &before).is_empty());

    p.write("chain/chain-7.md", "[[chain-0]] [[brand-new-ghost]] #fresh");
    p.write("hub/extra.md", "[[Hub]]");
    idx.update_file(&p.path("chain/chain-7.md"), None);
    idx.update_file(&p.path("hub/extra.md"), None);
    let after = idx.build_graph(&GraphOptions {
        show_tags: true,
        ..o.clone()
    });
    // compare against a pre-change graph with the same options
    let p2 = Project::new();
    generate_vault(&p2, 20);
    let old = p2.scan().build_graph(&GraphOptions {
        show_tags: true,
        ..o.clone()
    });
    let d = diff(&old, &after);
    assert!(d.is_structural());
    let added: BTreeSet<String> = d.added_nodes.iter().map(|s| s.to_string()).collect();
    assert_eq!(added, set(&["#fresh", "?brand-new-ghost", "hub/extra.md"]));
    assert!(d.removed_nodes.is_empty());
    // replay the delta on the old id sets
    let mut nodes = ids(&old);
    for r in &d.removed_nodes {
        nodes.remove(&**r);
    }
    for a in &d.added_nodes {
        nodes.insert(a.to_string());
    }
    assert_eq!(nodes, ids(&after));
    let key = |g: &impro_graph::model::Graph| -> BTreeSet<(String, String, EdgeKind)> {
        g.edges
            .iter()
            .map(|e| {
                (
                    g.nodes[e.src as usize].id.to_string(),
                    g.nodes[e.dst as usize].id.to_string(),
                    e.kind,
                )
            })
            .collect()
    };
    let mut edges = key(&old);
    for e in &d.removed_edges {
        edges.remove(&(e.src.to_string(), e.dst.to_string(), e.kind));
    }
    for e in &d.added_edges {
        edges.insert((e.src.to_string(), e.dst.to_string(), e.kind));
    }
    assert_eq!(edges, key(&after));
    // hub degree changed
    assert!(d.changed_nodes.iter().any(|n| &**n == "hub/Hub.md"));
    // the removed direction too
    let back = diff(&after, &old);
    assert_eq!(back.removed_nodes.len(), d.added_nodes.len());
}

#[test]
fn layout_follows_index_updates_without_disturbing_the_rest() {
    let p = Project::new();
    generate_vault(&p, 60);
    let mut idx = p.scan();
    let o = GraphOptions::default();
    let mut layout = Layout::new();
    layout.set_graph(&idx.build_graph(&o));
    layout.settle(1000);
    assert!(layout.settled());
    let before: Vec<_> = layout
        .ids()
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), layout.x()[i], layout.y()[i]))
        .collect();

    p.write("hub/late.md", "[[Hub]]");
    idx.update_file(&p.path("hub/late.md"), None);
    assert!(
        layout.set_graph(&idx.build_graph(&o)),
        "a new node is a structural edit"
    );
    assert!(!layout.settled());
    for (id, x, y) in &before {
        let i = layout.index_of(id).expect("kept");
        assert_eq!(
            (layout.x()[i], layout.y()[i]),
            (*x, *y),
            "{id} must not jump"
        );
    }
    let new = layout.index_of("hub/late.md").unwrap();
    let hub = layout.index_of("hub/Hub.md").unwrap();
    let d = (layout.x()[new] - layout.x()[hub])
        .abs()
        .max((layout.y()[new] - layout.y()[hub]).abs());
    assert!(
        d <= 24.0 + 1e-3,
        "the new note appears beside its neighbour ({d})"
    );
    layout.settle(1000);
    assert!(layout.x().iter().chain(layout.y()).all(|v| v.is_finite()));
}

/// Applies a random edit script to a live index and checks after every step
/// that it equals a fresh scan of the same files and buffers.
#[test]
fn incremental_updates_match_a_fresh_scan() {
    let p = Project::new();
    generate_vault(&p, 25);
    let mut live: ProjectIndex = p.scan();
    let mut rng = Rng::new(99);
    let names = [
        "alpha", "beta", "gamma", "Delta", "x/alpha", "Hub", "chain-1", "Pictures", "util", "lib",
    ];
    let folders = ["", "a", "b", "a/c", "src", "web"];
    let mut buffers: std::collections::BTreeMap<String, String> = Default::default();
    let mut pick = |rng: &mut Rng, n: usize| (rng.next_u64() % n as u64) as usize;
    let mut live_files: Vec<String> = Vec::new();

    let body = |rng: &mut Rng, pick: &mut dyn FnMut(&mut Rng, usize) -> usize| -> String {
        let mut t = String::new();
        if pick(rng, 4) == 0 {
            t.push_str(&format!(
                "---\naliases: [{}]\ntags: [t{}]\n---\n",
                names[pick(rng, names.len())],
                pick(rng, 3)
            ));
        }
        for _ in 0..pick(rng, 5) {
            let n = names[pick(rng, names.len())];
            match pick(rng, 4) {
                0 => t.push_str(&format!("[[{n}]] ")),
                1 => t.push_str(&format!("![[{n}.png]] ")),
                2 => t.push_str(&format!("[l](../{n}.md) ")),
                _ => t.push_str(&format!("#{n} ")),
            }
        }
        t
    };

    for step in 0..220 {
        let folder = folders[pick(&mut rng, folders.len())];
        let name = names[pick(&mut rng, names.len())];
        let rel = if folder.is_empty() {
            format!("{name}.md")
        } else {
            format!("{folder}/{name}.md")
        };
        match pick(&mut rng, 8) {
            0..=2 => {
                let text = body(&mut rng, &mut pick);
                p.write(&rel, &text);
                live.update_file(&p.path(&rel), None);
                buffers.remove(&rel);
                live_files.push(rel);
            }
            3 => {
                if p.path(&rel).exists() {
                    p.remove(&rel);
                    live.update_file(&p.path(&rel), None);
                    buffers.remove(&rel);
                }
            }
            4 => {
                let text = body(&mut rng, &mut pick);
                live.update_file(&p.path(&rel), Some(&text));
                buffers.insert(rel.clone(), text);
            }
            5 => {
                live.update_file(&p.path(&rel), None);
                buffers.remove(&rel);
            }
            6 => {
                if !live_files.is_empty() {
                    let from = live_files[pick(&mut rng, live_files.len())].clone();
                    if p.path(&from).exists()
                        && !buffers.contains_key(&from)
                        && from != rel
                        && !p.path(&rel).exists()
                    {
                        std::fs::create_dir_all(p.path(&rel).parent().unwrap()).unwrap();
                        std::fs::rename(p.path(&from), p.path(&rel)).unwrap();
                        live.rename(&p.path(&from), &p.path(&rel));
                        live_files.push(rel);
                    }
                }
            }
            _ => {
                // alias edit of a fixed note
                let text = format!(
                    "---\naliases: [{}]\n---\n[[{}]]",
                    names[pick(&mut rng, names.len())],
                    name
                );
                p.write("alias-note.md", &text);
                live.update_file(&p.path("alias-note.md"), None);
            }
        }
        assert!(
            live.resolution_is_consistent(),
            "step {step}: stale resolution"
        );
        let mut fresh = p.scan();
        for (path, text) in &buffers {
            fresh.update_file(&p.path(path), Some(text));
        }
        for opts in [
            opts_all(),
            GraphOptions {
                show_tags: true,
                ..opts_all()
            },
        ] {
            assert_eq!(
                snapshot(&live.build_graph(&opts)),
                snapshot(&fresh.build_graph(&opts)),
                "step {step} diverged"
            );
        }
    }
}
