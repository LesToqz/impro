use impro_core::fsops::*;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::{fs, path::Path};
#[test]
fn test_validate_name() {
    for n in ["ok", ".hidden", "a b", "λ.py"] {
        validate_name(n).unwrap();
    }
    for n in ["", ".", "..", "a/b", "\0", "a\0b"] {
        assert!(validate_name(n).is_err());
    }
}
#[test]
fn test_create_file_and_folder() {
    let d = tempfile::tempdir().unwrap();
    assert!(create_file(d.path(), "a").unwrap().is_file());
    assert!(create_folder(d.path(), "b").unwrap().is_dir());
    assert_eq!(fs::read_to_string(d.path().join("a")).unwrap(), "");
}
#[test]
fn test_create_errors() {
    let d = tempfile::tempdir().unwrap();
    create_file(d.path(), "existing").unwrap();
    assert!(create_file(d.path(), "existing")
        .unwrap_err()
        .to_string()
        .starts_with("Already exists"));
    assert!(create_folder(d.path(), "existing").is_err());
    assert!(create_file(d.path(), "../bad").is_err());
    assert!(create_file(d.path().join("missing"), "child")
        .unwrap_err()
        .to_string()
        .starts_with("Not found"));
}
#[cfg(unix)]
#[test]
fn test_rename_file_folder_and_symlink() {
    let d = tempfile::tempdir().unwrap();
    for (name, folder) in [("file", false), ("folder", true)] {
        let p = if folder {
            create_folder(d.path(), name)
        } else {
            create_file(d.path(), name)
        }
        .unwrap();
        let new = rename(&p, &format!("{name}2")).unwrap();
        assert!(!p.exists());
        assert!(new.exists());
    }
    symlink("absent", d.path().join("broken")).unwrap();
    assert!(rename(d.path().join("broken"), "renamed-link")
        .unwrap()
        .is_symlink());
}
#[cfg(unix)]
#[test]
fn test_rename_errors_and_no_overwrite() {
    let d = tempfile::tempdir().unwrap();
    let source = create_file(d.path(), "source").unwrap();
    fs::write(&source, "source").unwrap();
    let target = create_file(d.path(), "target").unwrap();
    fs::write(&target, "target").unwrap();
    assert!(rename(&source, "target").is_err());
    symlink("missing", d.path().join("link")).unwrap();
    assert!(rename(&source, "link").is_err());
    assert_eq!(fs::read_to_string(source).unwrap(), "source");
    assert_eq!(fs::read_to_string(target).unwrap(), "target");
}
#[test]
fn test_rename_never_overwrites_a_racing_target() {
    let d = tempfile::tempdir().unwrap();
    let a = create_file(d.path(), "a").unwrap();
    let b = d.path().join("b");
    fs::write(&b, "created concurrently").unwrap();
    assert_eq!(
        rename_no_replace(&a, &b).unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_to_string(b).unwrap(), "created concurrently");
}
#[test]
fn test_delete_files_and_recursive_folders() {
    let d = tempfile::tempdir().unwrap();
    let p = create_file(d.path(), "file").unwrap();
    delete(&p, None).unwrap();
    assert!(delete(&p, None).is_err());
    fs::create_dir_all(d.path().join("folder/nested")).unwrap();
    fs::write(d.path().join("folder/nested/data"), "").unwrap();
    delete(d.path().join("folder"), None).unwrap();
}
#[test]
fn test_delete_protected_roots() {
    let d = tempfile::tempdir().unwrap();
    for p in [Path::new("/"), Path::new("/tmp/.."), d.path()] {
        assert!(delete(p, Some(d.path()))
            .unwrap_err()
            .to_string()
            .contains("root"));
    }
}
#[cfg(unix)]
#[test]
fn test_delete_symlinks_without_following() {
    let d = tempfile::tempdir().unwrap();
    let target = create_folder(d.path(), "target").unwrap();
    fs::write(target.join("keep"), "").unwrap();
    for (name, destination) in [
        ("folder-link", target.as_path()),
        ("root-link", Path::new("/")),
        ("broken", Path::new("missing")),
    ] {
        let link = d.path().join(name);
        symlink(destination, &link).unwrap();
        delete(&link, Some(d.path())).unwrap();
        assert!(fs::symlink_metadata(link).is_err());
    }
    assert!(target.join("keep").is_file());
}
#[cfg(unix)]
#[test]
fn test_trash_files_folders_and_symlinks() {
    let d = tempfile::tempdir().unwrap();
    let destination = d.path().join("Trash");
    fs::write(d.path().join("file"), "keep contents").unwrap();
    fs::create_dir(d.path().join("folder")).unwrap();
    symlink("file", d.path().join("link")).unwrap();
    for name in ["link", "file", "folder"] {
        let source = d.path().join(name);
        trash_into(&source, &destination).unwrap();
        assert!(fs::symlink_metadata(source).is_err());
        let info =
            fs::read_to_string(destination.join("info").join(format!("{name}.trashinfo"))).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/"));
        assert!(info.contains("\nDeletionDate="));
    }
    assert_eq!(
        fs::read_to_string(destination.join("files/file")).unwrap(),
        "keep contents"
    );
    assert!(destination.join("files/link").is_symlink());
}
#[cfg(unix)]
#[test]
fn test_trash_protects_roots() {
    assert!(trash("/", None).is_err());
    let d = tempfile::tempdir().unwrap();
    assert!(trash(d.path(), Some(d.path())).is_err());
}
#[cfg(unix)]
#[test]
fn test_unavailable_trash_never_deletes() {
    let d = tempfile::tempdir().unwrap();
    let path = create_file(d.path(), "keep").unwrap();
    let blocked = create_file(d.path(), "blocked").unwrap();
    assert!(trash_into(&path, &blocked).is_err());
    assert!(path.exists());
}
