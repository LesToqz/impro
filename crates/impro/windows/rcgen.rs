//! Renders the Windows resource script from `impro.rc.in`.
//!
//! Shared by `build.rs` and `tests/test_rc_template.rs` (both include it with `#[path]`).
//! GNU windres still runs the C preprocessor and has a much stricter grammar than
//! Microsoft rc: no macros as values, no adjacent-string concatenation, no system
//! headers. The output therefore contains only plain literals and numeric constants.
//! VALUE strings carry no explicit `\0`: both rc.exe and windres append the NUL
//! terminator themselves, so the plain form is accepted by both.
#![allow(dead_code)]

/// `major,minor,patch,0` from a `major.minor.patch[-pre][+build]` version.
pub fn version_quad(version: &str) -> String {
    let parts: Vec<u16> = version
        .split(['.', '-', '+'])
        .take(3)
        .map(|s| s.parse().expect("numeric version component"))
        .collect();
    assert_eq!(parts.len(), 3, "expected major.minor.patch");
    format!("{},{},{},0", parts[0], parts[1], parts[2])
}

/// Absolute path with forward slashes, valid inside an rc string literal.
fn literal_path(path: &str) -> String {
    assert!(
        path.is_ascii() && !path.contains(['"', '\'', '\n']),
        "unsupported character in resource path: {path}"
    );
    path.replace('\\', "/")
}

pub fn render(template: &str, version: &str, icon: &str, manifest: &str) -> String {
    assert!(version.is_ascii() && !version.contains('"'), "bad version");
    template
        .replace("@VERSION_NUM@", &version_quad(version))
        .replace("@VERSION@", version)
        .replace("@ICON@", &literal_path(icon))
        .replace("@MANIFEST@", &literal_path(manifest))
}
