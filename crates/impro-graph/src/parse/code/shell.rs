//! `source file` and `. file` in shell scripts.

use super::{CodeKind, CodeRef};

pub fn imports(text: &str) -> Vec<CodeRef> {
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_start();
        if line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        match words.next() {
            Some("source") | Some(".") => {}
            _ => continue,
        }
        let arg = match words.next() {
            Some(a) => a.trim_matches(|c| c == '"' || c == '\''),
            None => continue,
        };
        if arg.is_empty() || arg.contains(['$', '`', '*', '?', ';', '&', '|']) {
            continue;
        }
        out.push(CodeRef {
            spec: arg.to_string(),
            kind: CodeKind::Shell,
            line: n as u32 + 1,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_forms() {
        let t = "#!/bin/sh\nsource ./lib.sh\n. \"config/env.sh\"\n# source ./no.sh\nsource \"$DIR/x.sh\"\necho source y\n  source util.sh # trailing\n";
        let r: Vec<_> = imports(t).into_iter().map(|r| (r.spec, r.line)).collect();
        assert_eq!(
            r,
            [
                ("./lib.sh".to_string(), 2),
                ("config/env.sh".to_string(), 3),
                ("util.sh".to_string(), 7)
            ]
        );
    }
}
