//! Opens the Graph view on a generated project, or renders one of the QA
//! scenarios to a PNG.
//!
//! ```text
//! cargo run -p impro-graph --example graph_demo                  # interactive
//! xvfb-run -a cargo run -p impro-graph --example graph_demo -- \
//!     --scenario hover --theme impro-dark --size 1000x700 --out shot.png
//! ```
//!
//! Options: `--vault DIR` (default: a fresh temporary project), `--size WxH`,
//! `--theme ID`, `--scenario global|local|panel|hover|menu|replay`, `--out FILE`,
//! `--seed N` (extra notes), `--print-vault` (print the project path).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gtk::prelude::*;
use impro_graph::view::testing as t;
use impro_graph::view::{MemoryStore, ViewOptions};
use impro_graph::GraphView;
use serde_json::json;

struct Args {
    vault: Option<PathBuf>,
    write_vault: Option<PathBuf>,
    size: (i32, i32),
    theme: String,
    scenario: String,
    out: Option<PathBuf>,
    bulk: usize,
}

fn parse() -> Args {
    let mut a = Args {
        vault: None,
        write_vault: None,
        size: (1000, 700),
        theme: "impro-dark".into(),
        scenario: "global".into(),
        out: None,
        bulk: 240,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut next = || it.next().unwrap_or_default();
        match arg.as_str() {
            "--vault" => a.vault = Some(next().into()),
            "--write-vault" => a.write_vault = Some(next().into()),
            "--size" => {
                let v = next();
                if let Some((w, h)) = v.split_once('x') {
                    a.size = (w.parse().unwrap_or(1000), h.parse().unwrap_or(700));
                }
            }
            "--theme" => a.theme = next(),
            "--scenario" => a.scenario = next(),
            "--out" => a.out = Some(next().into()),
            "--seed" => a.bulk = next().parse().unwrap_or(240),
            _ => {}
        }
    }
    a
}

fn write(root: &Path, rel: &str, text: &str) {
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(p, text);
}

/// Hubs with spokes, chains, rings, pairs, orphans, nested tags, attachments,
/// missing targets, ambiguous names, source imports and clustered notes.
pub fn generate_vault(root: &Path, bulk: usize) {
    let mut hub = String::from("# Hub\n#hub/main\n");
    for i in 0..14 {
        hub.push_str(&format!("[[spoke-{i}]]\n"));
        write(
            root,
            &format!("hub/spoke-{i}.md"),
            "back to [[Hub]] #hub/spoke\n",
        );
    }
    write(root, "hub/Hub.md", &hub);
    for i in 0..9 {
        let next = if i < 8 {
            format!("[[chain-{}]]", i + 1)
        } else {
            String::new()
        };
        write(root, &format!("chain/chain-{i}.md"), &format!("{next}\n"));
    }
    for i in 0..5 {
        write(
            root,
            &format!("ring/ring-{i}.md"),
            &format!("[[ring-{}]]\n", (i + 1) % 5),
        );
    }
    for i in 0..4 {
        write(
            root,
            &format!("pairs/pair-{i}a.md"),
            &format!("[[pair-{i}b]]\n"),
        );
        write(root, &format!("pairs/pair-{i}b.md"), "end\n");
    }
    for i in 0..6 {
        write(root, &format!("orphans/orphan-{i}.md"), "nothing here\n");
    }
    write(
        root,
        "media/Gallery.md",
        "---\naliases: [Pictures]\ntags: [art/photo, art]\n---\n![[sunrise.png]] ![cat](../assets/cat.png) [[Missing Note]] [[ghost.pdf]] #art/photo/raw\n",
    );
    let _ = fs::write(
        root.join("media/sunrise.png"),
        [0x89, b'P', b'N', b'G', 0, 1],
    );
    let _ = fs::create_dir_all(root.join("assets"));
    let _ = fs::write(root.join("assets/cat.png"), [0x89, b'P', b'N', b'G', 0, 3]);
    write(
        root,
        "media/ref-by-alias.md",
        "see [[Pictures]] and [[Another Missing]]\n",
    );
    write(root, "x/Dup.md", "first\n");
    write(root, "y/deep/Dup.md", "second\n");
    write(root, "x/uses-dup.md", "[[Dup]]\n");
    write(root, "z/uses-dup.md", "[[Dup]]\n");
    write(
        root,
        "src/app/main.py",
        "import util\nfrom . import helper\nimport os\n",
    );
    write(root, "src/app/helper.py", "import util\n");
    write(root, "src/util.py", "");
    write(
        root,
        "web/app.ts",
        "import { x } from './lib';\nimport './style.css';\n",
    );
    write(root, "web/lib.ts", "export const x = 1;\n");
    write(root, "web/style.css", "@import 'base.css';\n");
    write(root, "web/base.css", "");
    write(
        root,
        "crates/demo/src/lib.rs",
        "pub mod engine;\nuse crate::engine::run;\n",
    );
    write(root, "crates/demo/src/engine.rs", "");
    write(root, "scripts/run.sh", "source ./env.sh\n");
    write(root, "scripts/env.sh", "");
    write(root, "docs/readme-link.md", "[code](../src/app/main.py)\n");
    let mut state = 2024u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as usize
    };
    for i in 0..bulk {
        let cluster = i % 10;
        let mut text = format!("# n{i}\n[[bulk-{}]]\n", (i / 10) * 10 + (i + 1) % 10);
        for _ in 0..2 {
            let j = next() % bulk.max(1);
            text.push_str(&format!("[[bulk-{j}]] "));
        }
        if i % 7 == 0 {
            text.push_str("#bulk/seven\n");
        }
        write(root, &format!("bulk/c{cluster}/bulk-{i}.md"), &text);
    }
}

/// Copy of a rectangle of a realized window (`gdk_pixbuf_get_from_window`).
fn window_pixbuf(window: &gdk::Window, w: i32, h: i32) -> Option<gdk_pixbuf::Pixbuf> {
    use glib::translate::*;
    // SAFETY: the window is alive; the call returns a new reference or NULL.
    unsafe {
        from_glib_full(gdk::ffi::gdk_pixbuf_get_from_window(
            window.to_glib_none().0,
            0,
            0,
            w,
            h,
        ))
    }
}

fn capture(window: &gtk::Window, whole_screen: bool, out: &Path, size: (i32, i32)) {
    t::pump(250);
    let target = if whole_screen {
        gdk::Screen::default().and_then(|s| s.root_window())
    } else {
        window.window()
    };
    let Some(target) = target else {
        eprintln!("no window to capture");
        std::process::exit(1);
    };
    match window_pixbuf(&target, size.0, size.1) {
        Some(pixbuf) => {
            if let Err(e) = pixbuf.savev(out, "png", &[]) {
                eprintln!("cannot write {}: {e}", out.display());
                std::process::exit(1);
            }
            println!("wrote {}", out.display());
        }
        None => {
            eprintln!("capture failed");
            std::process::exit(1);
        }
    }
}

fn main() {
    let args = parse();
    if let Some(dir) = &args.write_vault {
        generate_vault(dir, args.bulk);
        println!("{}", dir.display());
        return;
    }
    std::env::set_var("GDK_BACKEND", "x11");
    gtk::init().expect("display");
    impro_ui::init();
    impro_graph::init();
    let theme = impro_core::themes::get_theme(Some(&args.theme));
    let _ = impro_ui::apply_style(Some(theme));
    let temp;
    let root = match &args.vault {
        Some(p) => p.clone(),
        None => {
            temp = tempfile::tempdir().expect("temp dir");
            generate_vault(temp.path(), args.bulk);
            temp.path().to_path_buf()
        }
    };
    let store = Arc::new(MemoryStore::new(json!({})));
    let view = GraphView::with_options(&root, ViewOptions { store, scan: true });
    view.set_theme(&theme.ui, &theme.ui["accent"]);
    view.connect_open_file(|p, m| println!("open {} {:?}", p.display(), m));
    view.connect_search_tag(|tag| println!("search tag {tag}"));
    view.connect_create_note(|n| println!("create note {n}"));
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro graph demo");
    window.set_default_size(args.size.0, args.size.1);
    window.add(&*view);
    window.connect_destroy(|_| gtk::main_quit());
    window.show_all();
    window.move_(0, 0);

    if let Some(out) = args.out.clone() {
        t::wait_for(10_000, || t::info(&view).nodes > 0);
        t::settle(&view, 15_000);
        let mut whole = false;
        match args.scenario.as_str() {
            "local" => {
                let file = root.join("hub/Hub.md");
                view.set_active_file(Some(&file));
                view.open_local(&file);
                t::wait_for(5_000, || {
                    view.summary().nodes > 0 && view.summary().nodes < 40
                });
                t::settle(&view, 10_000);
            }
            "panel" => {
                if std::env::var("GRAPH_DEBUG").is_ok() {
                    fn dump(w: &gtk::Widget, depth: usize) {
                        let a = w.allocation();
                        let ctx = w.style_context();
                        let pad = ctx.padding(w.state_flags());
                        println!(
                            "{}{} {:?} at ({},{}) {}x{} pad l{} r{} t{} b{}",
                            "  ".repeat(depth),
                            w.type_().name(),
                            ctx.list_classes()
                                .iter()
                                .map(|c| c.to_string())
                                .collect::<Vec<_>>(),
                            a.x(),
                            a.y(),
                            a.width(),
                            a.height(),
                            pad.left,
                            pad.right,
                            pad.top,
                            pad.bottom
                        );
                        if let Some(c) = w.downcast_ref::<gtk::Container>() {
                            for ch in c.children() {
                                dump(&ch, depth + 1);
                            }
                        }
                    }
                    dump(view.panel().root.upcast_ref(), 0);
                }
                for id in ["filters", "groups", "display", "forces"] {
                    if view.panel().collapsed(id) == Some(true) {
                        view.panel().click_section(id);
                    }
                }
                let mut p = view.prefs().to_value();
                p["colorGroups"] = json!([
                    {"query": "path:hub", "color": "#ed9e63"},
                    {"query": "tag:#bulk", "color": "#70d6a0"}
                ]);
                p["showTags"] = json!(true);
                p["showAttachments"] = json!(true);
                view.apply_prefs(&p);
                t::pump(400);
                t::settle(&view, 10_000);
            }
            "hover" | "menu" => {
                view.focus_file(&root.join("hub/Hub.md"));
                t::settle(&view, 8_000);
                if let Some((x, y, _)) = t::node_screen(&view, "hub/Hub.md") {
                    t::motion(&view, x, y);
                    t::settle(&view, 5_000);
                    if args.scenario == "menu" {
                        whole = true;
                        t::open_context_menu(&view, "hub/Hub.md");
                        t::pump(300);
                    }
                }
            }
            "closed" => {
                view.set_panel_open(false);
                t::pump(500);
            }
            "replay" => {
                view.replay_history();
                t::pump(1200);
            }
            _ => {}
        }
        capture(&window, whole, &out, args.size);
        return;
    }
    gtk::main();
}
