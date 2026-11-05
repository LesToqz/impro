//! `launch::prepare` against a real file system and environment. The whole file is one
//! test because it sets process-wide environment variables.
#![cfg(unix)]
use impro::launch::{self, RootSource, State, Targets};
use std::path::{Path, PathBuf};

fn dir(parent: &Path, name: &str) -> PathBuf {
    let path = parent.join(name);
    std::fs::create_dir_all(&path).unwrap();
    path.canonicalize().unwrap()
}

fn write_state(config: &Path, text: &str) {
    let path = config.join("impro/state.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn test_prepare_resolves_ambient_and_explicit_launches() {
    let tmp = tempfile::tempdir().unwrap();
    let base = tmp.path().canonicalize().unwrap();
    let home = dir(&base, "home");
    let config = dir(&base, "config");
    let project = dir(&base, "work/project");
    let other = dir(&base, "work/other");
    std::env::set_var("HOME", &home);
    std::env::set_var("XDG_CONFIG_HOME", &config);
    let last_json = |folder: &Path| {
        format!(
            r#"{{"version":1,"last_folder":"{0}","recent":["{0}","{1}"]}}"#,
            folder.display(),
            other.display()
        )
    };

    // A launcher starts in $HOME with no path: reopen the last folder.
    write_state(&config, &last_json(&project));
    let got = launch::prepare(Targets::default(), &home);
    assert_eq!(
        (got.root.as_path(), got.source),
        (project.as_path(), RootSource::LastFolder)
    );
    assert!(!got.has_paths && got.files.is_empty());
    assert_eq!(got.state.recent[0], project);
    assert_eq!(got.record_target(), Some(project.as_path()));

    // The last folder is gone: fall back to $HOME and do not remember it.
    write_state(&config, &last_json(&base.join("deleted")));
    let got = launch::prepare(Targets::default(), &home);
    assert_eq!(
        (got.root.as_path(), got.source),
        (home.as_path(), RootSource::Home)
    );
    assert_eq!(got.record_target(), None);
    assert!(!got.state.recent.iter().any(|p| p == &home));

    // Never used before (no state file at all): $HOME, nothing recorded.
    std::fs::remove_file(config.join("impro/state.json")).unwrap();
    let got = launch::prepare(Targets::default(), &home);
    assert_eq!(
        (got.root.as_path(), got.source),
        (home.as_path(), RootSource::Home)
    );
    assert_eq!(got.state, State::default());

    // A corrupt file is the same as no file.
    write_state(&config, "{ not json");
    let got = launch::prepare(Targets::default(), &home);
    assert_eq!(got.root, home);

    // A terminal in any other directory keeps it, and remembers it.
    write_state(&config, &last_json(&other));
    let got = launch::prepare(Targets::default(), &project);
    assert_eq!(
        (got.root.as_path(), got.source),
        (project.as_path(), RootSource::WorkingDir)
    );
    assert_eq!(got.state.recent[0], project);

    // $HOME reached through a symlink still counts as home.
    let link = base.join("home-link");
    std::os::unix::fs::symlink(&home, &link).unwrap();
    std::env::set_var("HOME", &link);
    write_state(&config, &last_json(&project));
    assert_eq!(launch::prepare(Targets::default(), &home).root, project);
    std::env::set_var("HOME", &home);

    // Explicit paths ignore the ambient rule.
    let explicit = Targets {
        dir: Some(other.clone()),
        files: vec![project.join("a.txt")],
    };
    let got = launch::prepare(explicit, &home);
    assert_eq!(
        (got.root.as_path(), got.source),
        (other.as_path(), RootSource::Directory)
    );
    assert!(got.has_paths);
    assert_eq!(got.files, vec![project.join("a.txt")]);
    let files_only = Targets {
        dir: None,
        files: vec![project.join("a.txt")],
    };
    let got = launch::prepare(files_only, &home);
    assert_eq!(
        (got.root.as_path(), got.source),
        (project.as_path(), RootSource::FileParent)
    );
    assert_eq!(got.state.recent[0], project);
}
