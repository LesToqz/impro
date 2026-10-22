//! File names that are not UTF-8 stay the files they are: the tree, selection, signals
//! and search results hand back the exact path on disk.
mod common;
use common::*;
use gtk::prelude::*;
use impro_explorer::explorer::ExplorerPanel;
use impro_explorer::util::{decode_path, encode_path};
use std::cell::RefCell;
use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

fn raw(bytes: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(bytes.to_vec()))
}

#[test]
fn encoding_round_trips_every_kind_of_name() {
    let names: [&[u8]; 8] = [
        b"/plain/ascii.txt",
        "/caf\u{e9}/\u{65e5}\u{672c}.md".as_bytes(),
        b"/bad/\xff\xfe.txt",
        b"/lone/\x80",
        b"/truncated/\xe6\x97",
        // A real private-use character that looks like an escaped byte.
        "/pua/\u{f7ff}\u{f780}".as_bytes(),
        b"/mixed/\xc3\x28/\xf0\x90\x28\xbc",
        b"",
    ];
    for bytes in names {
        let path = raw(bytes);
        let text = encode_path(&path);
        assert_eq!(decode_path(&text), path, "{bytes:?}");
    }
    // Names that are valid are left alone, and distinct names stay distinct.
    assert_eq!(encode_path(Path::new("/a/b\u{e9}")), "/a/b\u{e9}");
    assert_ne!(encode_path(&raw(b"/x/\xff")), encode_path(&raw(b"/x/\xfe")));
    assert_ne!(
        encode_path(&raw(b"/x/\xff")),
        encode_path(&raw(b"/x/\xef\xbf\xbd"))
    );
}

struct Fix {
    temp: tempfile::TempDir,
    panel: ExplorerPanel,
    odd: PathBuf,
    twin: PathBuf,
}

impl Drop for Fix {
    fn drop(&mut self) {
        destroy(&self.panel);
    }
}

fn setup() -> Fix {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().join("caf\u{e9}");
    std::fs::create_dir(&dir).unwrap();
    let odd = temp.path().join(raw(b"bad-\xff-name.txt"));
    // Shows the same as `odd` (both invalid bytes display as one replacement character).
    let twin = temp.path().join(raw(b"bad-\xfe-name.txt"));
    std::fs::write(&odd, "needle in the odd file\n").unwrap();
    std::fs::write(&twin, "needle in the twin file\n").unwrap();
    std::fs::write(dir.join(raw(b"inner-\x80.txt")), "needle inside\n").unwrap();
    let panel = ExplorerPanel::new(temp.path());
    wait_idle(&panel);
    Fix {
        temp,
        panel,
        odd,
        twin,
    }
}

#[test]
fn rows_select_and_activate_the_exact_file() {
    run(|| {
        let f = setup();
        for file in [&f.odd, &f.twin] {
            let row = f.panel.find_row(file).expect("row of the raw name");
            let store = f.panel.store();
            assert_eq!(
                decode_path(&impro_explorer::util::string(&store, &row, 2)),
                *file
            );
            f.panel.select(file);
            assert_eq!(f.panel.selected_path().as_ref(), Some(file));
            assert_eq!(f.panel.target_folder(), f.temp.path());
        }
        let activated = Rc::new(RefCell::new(Vec::new()));
        let sink = activated.clone();
        f.panel
            .connect_file_activated(move |_, path| sink.borrow_mut().push(path.to_path_buf()));
        let tree = f.panel.tree();
        let column = tree.column(0).unwrap();
        for file in [&f.odd, &f.twin] {
            let row = f.panel.find_row(file).unwrap();
            tree.row_activated(
                &impro_explorer::util::path_of(&f.panel.store(), &row),
                &column,
            );
        }
        assert_eq!(*activated.borrow(), [f.odd.clone(), f.twin.clone()]);
    });
}

#[test]
fn folders_below_raw_names_expand_and_survive_a_refresh() {
    run(|| {
        let f = setup();
        let dir = f.temp.path().join("caf\u{e9}");
        let inner = dir.join(raw(b"inner-\x80.txt"));
        expand(&f.panel, &dir);
        f.panel.select(&inner);
        assert_eq!(f.panel.selected_path(), Some(inner.clone()));
        f.panel.refresh();
        wait_idle(&f.panel);
        assert_eq!(f.panel.selected_path(), Some(inner));
        assert!(f.panel.find_row(&f.odd).is_some());
        // Removing the file removes its row and no other.
        std::fs::remove_file(&f.odd).unwrap();
        f.panel.refresh();
        wait_idle(&f.panel);
        assert!(f.panel.find_row(&f.odd).is_none());
        assert!(f.panel.find_row(&f.twin).is_some());
    });
}

#[test]
fn search_results_activate_the_exact_file() {
    run(|| {
        let f = setup();
        let search = f.panel.search();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        search.connect_result_activated(move |_, path, line, _, _| {
            sink.borrow_mut().push((path.to_path_buf(), line));
        });
        search.entry().set_text("needle");
        let panel = search.clone();
        pump_until("search finished", move || {
            !panel.is_searching() && panel.results().total_files == 3 && !panel.timeout_pending()
        });
        let store = search.store();
        let tree = search.tree();
        let column = tree.column(0).unwrap();
        // Activate every match row (they sit below their group headers).
        let mut matches = Vec::new();
        let mut group = store.iter_first();
        while let Some(parent) = group {
            tree.expand_row(&impro_explorer::util::path_of(&store, &parent), false);
            if let Some(child) = store.iter_children(Some(&parent)) {
                matches.push(impro_explorer::util::path_of(&store, &child));
            }
            group = impro_explorer::util::next_iter(&store, &parent);
        }
        assert_eq!(matches.len(), 3);
        for path in &matches {
            tree.row_activated(path, &column);
        }
        let mut got: Vec<PathBuf> = seen.borrow().iter().map(|(p, _)| p.clone()).collect();
        got.sort();
        let mut want = vec![
            f.odd.clone(),
            f.twin.clone(),
            f.temp.path().join("caf\u{e9}").join(raw(b"inner-\x80.txt")),
        ];
        want.sort();
        assert_eq!(got, want);
        for path in got {
            assert!(path.exists(), "{path:?}");
        }
    });
}
