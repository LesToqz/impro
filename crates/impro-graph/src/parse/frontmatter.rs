//! The deliberately small YAML subset used for note properties.

/// A property value: a scalar or a list of scalars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropValue {
    Text(String),
    List(Vec<String>),
}

impl PropValue {
    /// Scalars become a one-element list unless they are empty.
    pub fn to_list(&self) -> Vec<String> {
        match self {
            PropValue::Text(t) if t.is_empty() => Vec::new(),
            PropValue::Text(t) => vec![t.clone()],
            PropValue::List(l) => l.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frontmatter {
    /// Properties in first-seen order; keys are lowercased, later duplicates win.
    pub props: Vec<(String, PropValue)>,
    /// Byte offset where the note body starts (0 when there is no block).
    pub body_start: usize,
}

impl Frontmatter {
    pub fn get(&self, key: &str) -> Option<&PropValue> {
        self.props.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn set(&mut self, key: &str, value: PropValue) {
        match self.props.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => self.props.push((key.to_string(), value)),
        }
    }

    /// `aliases`, falling back to `alias`.
    pub fn aliases(&self) -> Vec<String> {
        self.get("aliases")
            .or_else(|| self.get("alias"))
            .map(PropValue::to_list)
            .unwrap_or_default()
    }
}

fn scalar(value: &str) -> String {
    let v = value.trim();
    let b = v.as_bytes();
    if b.len() > 1 && b[0] == b[b.len() - 1] && (b[0] == b'"' || b[0] == b'\'') {
        return v[1..v.len() - 1].to_string();
    }
    v.to_string()
}

/// Entries of an inline `[a, "b, c", 'd']` list.
fn inline_list(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = inner;
    loop {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            break;
        }
        let first = trimmed.as_bytes()[0];
        let (entry, after) = if first == b'"' || first == b'\'' {
            match trimmed[1..].find(first as char) {
                Some(close) => (&trimmed[..close + 2], &trimmed[close + 2..]),
                None => split_comma(trimmed),
            }
        } else {
            split_comma(trimmed)
        };
        if !entry.trim().is_empty() {
            out.push(scalar(entry));
        }
        // skip up to and including the next comma
        rest = match after.find(',') {
            Some(i) => &after[i + 1..],
            None => "",
        };
    }
    out
}

fn split_comma(s: &str) -> (&str, &str) {
    match s.find(',') {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}

/// `---` opener at the very start (optionally after a BOM). Returns the byte
/// offset of the first line after it.
fn opening(text: &str) -> Option<usize> {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let bom = text.len() - t.len();
    let rest = t.strip_prefix("---")?;
    let mut i = 0;
    for c in rest.chars() {
        if c == '\n' {
            return Some(bom + 3 + i + 1);
        }
        if !c.is_whitespace() {
            return None;
        }
        i += c.len_utf8();
    }
    None
}

fn is_closing(line: &str) -> bool {
    let rest = match line.strip_prefix("---").or_else(|| line.strip_prefix("...")) {
        Some(r) => r,
        None => return false,
    };
    let rest = rest.strip_suffix('\r').unwrap_or(rest);
    rest.chars().all(|c| c.is_whitespace() && c != '\r')
}

fn is_key_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-'
}

pub fn parse(text: &str) -> Frontmatter {
    let mut fm = Frontmatter::default();
    let start = match opening(text) {
        Some(s) => s,
        None => return fm,
    };
    // locate the closing line
    let mut pos = start;
    let mut close = None;
    while pos <= text.len() {
        let end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);
        if is_closing(&text[pos..end]) {
            close = Some((pos, end));
            break;
        }
        if end >= text.len() {
            break;
        }
        pos = end + 1;
    }
    let (block_end, line_end) = match close {
        Some(c) => c,
        None => return fm,
    };
    // the closing match ends before a trailing '\r' is NOT consumed by "$"
    // semantics: the '\r' belongs to the closing line, then one '\n' follows.
    let mut body = line_end;
    if text.as_bytes().get(body) == Some(&b'\n') {
        body += 1;
    }
    fm.body_start = body;

    let mut key: Option<String> = None;
    for raw in text[start..block_end].split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('-') {
            if rest.starts_with(|c: char| c.is_whitespace()) {
                if let Some(k) = &key {
                    let item = scalar(rest);
                    let mut list = match fm.get(k) {
                        Some(PropValue::List(l)) => l.clone(),
                        _ => Vec::new(),
                    };
                    list.push(item);
                    fm.set(k, PropValue::List(list));
                    continue;
                }
            }
        }
        let name_len: usize = line
            .chars()
            .take_while(|c| is_key_char(*c))
            .map(char::len_utf8)
            .sum();
        if name_len > 0 && line[name_len..].starts_with(':') {
            let k = line[..name_len].to_lowercase();
            let value = line[name_len + 1..].trim();
            if value.starts_with('[') && value.ends_with(']') && value.len() >= 2 {
                fm.set(&k, PropValue::List(inline_list(&value[1..value.len() - 1])));
            } else {
                fm.set(&k, PropValue::Text(scalar(value)));
            }
            key = Some(k);
        }
    }
    fm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scalars_lists_and_body_offset() {
        let text = "---\ntags: [one, \"#two\"]\naliases:\n  - \"First name\"\n  - Second\nrelation: \"[[Other]]\"\n---\nBody";
        let fm = parse(text);
        assert_eq!(
            fm.get("tags"),
            Some(&PropValue::List(vec!["one".into(), "#two".into()]))
        );
        assert_eq!(fm.aliases(), vec!["First name", "Second"]);
        assert_eq!(
            fm.get("relation"),
            Some(&PropValue::Text("[[Other]]".into()))
        );
        assert_eq!(&text[fm.body_start..], "Body");
    }

    #[test]
    fn crlf_and_missing_blocks() {
        let crlf = "---\r\ntags: one\r\n---\r\n\r\nBody";
        assert_eq!(&crlf[parse(crlf).body_start..], "\r\nBody");
        assert_eq!(parse("no YAML"), Frontmatter::default());
        assert_eq!(parse("---\ntags: x"), Frontmatter::default());
        assert_eq!(parse("--- x\ntags: y\n---\n"), Frontmatter::default());
        let q = parse("---\nalias: 'Name'\n---\n");
        assert_eq!(q.aliases(), vec!["Name"]);
        let bom = parse("\u{feff}---\nalias: A\n...\nbody");
        assert_eq!(bom.aliases(), vec!["A"]);
    }

    #[test]
    fn inline_lists_respect_quotes() {
        let fm = parse("---\naliases: [ \"a, b\", 'c', d ,, ]\n---\n");
        assert_eq!(fm.aliases(), vec!["a, b", "c", "d"]);
        let empty = parse("---\naliases:\n---\n");
        assert!(empty.aliases().is_empty());
    }

    #[test]
    fn list_items_replace_scalars_and_later_keys_win() {
        let fm = parse("---\ntags: x\n- y\ntags: z\n---\n");
        assert_eq!(fm.get("tags"), Some(&PropValue::Text("z".into())));
        let fm = parse("---\ntags: x\n  - y\n  - w\n---\n");
        assert_eq!(
            fm.get("tags"),
            Some(&PropValue::List(vec!["y".into(), "w".into()]))
        );
    }
}
