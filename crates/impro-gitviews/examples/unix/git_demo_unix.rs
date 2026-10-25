//! Standalone showcase, with no application/window crate dependency.
use gtk::prelude::*;
use impro_gitviews::{CommitView, GitGraphView};
use std::{cell::RefCell, rc::Rc};
#[path = "../../tests/support/mod.rs"]
mod support;
use support::*;
pub(super) fn main() {
    if std::env::args().nth(1).as_deref() == Some("--export") {
        let dir = std::path::PathBuf::from(std::env::args().nth(2).expect("export directory"));
        std::fs::create_dir_all(&dir).unwrap();
        let repo = support::showcase(&dir.join("repository"));
        let graph = impro_gitviews::log::collect_graph(&repo.path, Some(1500));
        let commit = impro_gitviews::diff::commit_detail(
            &repo.path,
            "feature/ui",
            impro_gitviews::diff::Limits::default(),
            None,
        );
        for (name, text) in [
            ("graph.html", impro_gitviews::assets::graph_html()),
            ("commit.html", impro_gitviews::assets::commit_html()),
            ("graph.json", graph.to_string()),
            ("commit.json", commit.to_string()),
        ] {
            std::fs::write(dir.join(name), text).unwrap();
        }
        for (name, id) in [("dark", "impro-dark"), ("light", "impro-light")] {
            let theme = impro_core::themes::get_theme(Some(id));
            std::fs::write(
                dir.join(format!("graph-{name}.json")),
                impro_gitviews::palette::git_colors(&theme.ui, &theme.ui["accent"]).to_string(),
            )
            .unwrap();
            std::fs::write(
                dir.join(format!("commit-{name}.json")),
                impro_gitviews::palette::commit_colors(theme, &theme.ui, &theme.ui["accent"], 13)
                    .to_string(),
            )
            .unwrap();
        }
        return;
    }
    gtk::init().expect("GTK display");
    let temp = Temp::new();
    let mut repo = GitRepo::new(&temp.0);
    repo.commit(
        &[
            ("README.md", Some(b"# impro\n\nA Git page showcase.\n")),
            (
                "main.rs",
                Some(b"fn main() {\n    println!(\"hello\");\n}\n"),
            ),
        ],
        "Start project",
    );
    repo.git(&["checkout", "-b", "feature/colours"]);
    repo.commit(
        &[("theme.css", Some(b"body { color: #aabbcc; }\n"))],
        "Add theme colours",
    );
    repo.git(&["checkout", "master"]);
    repo.commit(
        &[("README.md", Some(b"# impro\n\nExplore commit history.\n"))],
        "Write guide",
    );
    repo.git(&[
        "merge",
        "--no-ff",
        "feature/colours",
        "-m",
        "Merge theme colours",
    ]);
    repo.tag("v1.0", false);
    let head = repo.git(&["rev-parse", "HEAD"]);
    let graph = GitGraphView::new(&repo.path);
    graph.load_settings();
    let theme = impro_core::themes::get_theme(None);
    graph.set_theme(&theme.ui, &theme.ui["accent"]);
    graph.set_reduce_motion(true);
    let commit = CommitView::new(&repo.path, &head, None);
    commit.set_theme(theme, &theme.ui, &theme.ui["accent"], 13);
    let tabs = gtk::Notebook::new();
    tabs.append_page(commit.widget(), Some(&gtk::Label::new(Some(&head[..7]))));
    let views = Rc::new(RefCell::new(vec![commit]));
    let v = views.clone();
    let notebook = tabs.clone();
    let root = repo.path.clone();
    graph.connect_open_commit(move |hash| {
        let view = CommitView::new(&root, hash, None);
        let theme = impro_core::themes::get_theme(None);
        view.set_theme(theme, &theme.ui, &theme.ui["accent"], 13);
        notebook.append_page(
            view.widget(),
            Some(&gtk::Label::new(Some(&hash[..hash.len().min(7)]))),
        );
        view.widget().show_all();
        v.borrow_mut().push(view);
    });
    let paned = gtk::Paned::new(gtk::Orientation::Vertical);
    paned.pack1(graph.widget(), true, false);
    paned.pack2(&tabs, true, false);
    paned.set_position(330);
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("impro — Git pages");
    window.set_default_size(1100, 900);
    window.add(&paned);
    window.connect_delete_event(move |_, _| {
        graph.shutdown();
        for view in views.borrow().iter() {
            view.shutdown();
        }
        gtk::main_quit();
        glib::Propagation::Proceed
    });
    window.show_all();
    gtk::main();
}
