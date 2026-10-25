//! The generated Windows resource script must stay inside the strictest windres grammar.
#[path = "../windows/rcgen.rs"]
mod rcgen;

use std::path::Path;

fn sample() -> String {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let template = std::fs::read_to_string(dir.join("windows/impro.rc.in")).unwrap();
    let icon = dir.join("../../packaging/icons/windows/impro.ico");
    let manifest = dir.join("windows/impro.manifest");
    rcgen::render(
        &template,
        "0.1.0",
        icon.to_str().unwrap(),
        manifest.to_str().unwrap(),
    )
}

/// Splits a line into string literals (contents) and everything outside them.
fn literals(line: &str) -> (Vec<String>, String) {
    let (mut lits, mut rest, mut cur, mut inside) = (vec![], String::new(), String::new(), false);
    for c in line.chars() {
        match (c, inside) {
            ('"', false) => {
                inside = true;
                rest.push('"');
            }
            ('"', true) => {
                lits.push(std::mem::take(&mut cur));
                inside = false;
                rest.push('"');
            }
            (c, true) => cur.push(c),
            (c, false) => rest.push(c),
        }
    }
    assert!(!inside, "unterminated string: {line}");
    (lits, rest)
}

#[test]
fn rc_is_plain_ascii_without_includes() {
    let rc = sample();
    assert!(rc.is_ascii());
    assert!(!rc.contains("#include") && !rc.contains('#'));
    assert!(!rc.contains('@'), "unsubstituted placeholder");
}

#[test]
fn value_lines_are_two_literals() {
    for line in sample()
        .lines()
        .filter(|l| l.trim_start().starts_with("VALUE"))
    {
        if line.contains("\"Translation\"") {
            assert_eq!(line.trim(), "VALUE \"Translation\", 0x0409, 1200");
            continue;
        }
        let (lits, rest) = literals(line);
        assert_eq!(lits.len(), 2, "{line}");
        assert_eq!(rest.trim(), "VALUE \"\", \"\"", "{line}");
        assert!(!lits[1].contains('\\'), "no escapes expected: {line}");
    }
}

#[test]
fn no_adjacent_string_literals() {
    for line in sample().lines() {
        let (_, rest) = literals(line);
        let squeezed: String = rest.split_whitespace().collect();
        assert!(!squeezed.contains("\"\"\"\""), "adjacent literals: {line}");
    }
}

#[test]
fn version_quad_and_referenced_files() {
    let rc = sample();
    assert_eq!(rcgen::version_quad("0.1.0"), "0,1,0,0");
    assert_eq!(rcgen::version_quad("2.10.3-rc.1"), "2,10,3,0");
    assert!(rc.contains("FILEVERSION 0,1,0,0\n"));
    assert!(rc.contains("PRODUCTVERSION 0,1,0,0\n"));
    for line in rc
        .lines()
        .filter(|l| l.starts_with("1 ICON") || l.starts_with("1 24"))
    {
        let (lits, _) = literals(line);
        let path = &lits[0];
        assert!(
            !path.contains('\\') && Path::new(path).is_absolute(),
            "{path}"
        );
        assert!(Path::new(path).exists(), "missing {path}");
    }
}
