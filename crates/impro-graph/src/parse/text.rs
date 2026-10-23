//! Small text helpers shared by the parsers: position tracking, percent
//! decoding and the "is this an external reference" test.

use std::borrow::Cow;

/// Converts ascending byte offsets into one-based lines and character offsets
/// without rescanning the text for every query.
pub struct PosTracker<'a> {
    text: &'a str,
    byte: usize,
    chars: usize,
    line: u32,
}

impl<'a> PosTracker<'a> {
    pub fn new(text: &'a str) -> Self {
        PosTracker {
            text,
            byte: 0,
            chars: 0,
            line: 1,
        }
    }

    /// Position of `byte` as `(character offset, one-based line)`. Offsets
    /// must be requested in non-decreasing order (smaller ones are clamped).
    pub fn at(&mut self, byte: usize) -> (usize, u32) {
        let byte = byte.clamp(self.byte, self.text.len());
        let slice = &self.text.as_bytes()[self.byte..byte];
        self.chars += slice.iter().filter(|b| (**b & 0xC0) != 0x80).count();
        self.line += memchr::memchr_iter(b'\n', slice).count() as u32;
        self.byte = byte;
        (self.chars, self.line)
    }
}

/// One-based line numbers for ascending byte offsets.
pub struct LineTracker<'a> {
    bytes: &'a [u8],
    byte: usize,
    line: u32,
}

impl<'a> LineTracker<'a> {
    pub fn new(text: &'a str) -> Self {
        LineTracker {
            bytes: text.as_bytes(),
            byte: 0,
            line: 1,
        }
    }

    /// Line of `byte`; offsets must be non-decreasing.
    pub fn line_at(&mut self, byte: usize) -> u32 {
        let byte = byte.clamp(self.byte, self.bytes.len());
        self.line += memchr::memchr_iter(b'\n', &self.bytes[self.byte..byte]).count() as u32;
        self.byte = byte;
        self.line
    }
}

/// `%XX` decoding; invalid escapes stay literal and invalid UTF-8 is replaced.
pub fn percent_decode(s: &str) -> Cow<'_, str> {
    if !s.as_bytes().contains(&b'%') {
        return Cow::Borrowed(s);
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Cow::Owned(String::from_utf8_lossy(&out).into_owned())
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// True for targets that cannot be project files: URLs and other schemes,
/// protocol-relative references and in-page anchors. Empty targets count too.
pub fn is_external(target: &str) -> bool {
    let t = target.trim();
    if t.is_empty() || t.starts_with('#') || t.starts_with("//") {
        return true;
    }
    let mut chars = t.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    for c in chars {
        if c == ':' {
            return true;
        }
        if !(is_word(c) || c == '+' || c == '.' || c == '-') {
            return false;
        }
    }
    false
}

/// Characters allowed inside a tag name.
pub fn is_tag_char(c: char) -> bool {
    is_word(c) || c == '/' || c == '-'
}

pub fn is_word_char(c: char) -> bool {
    is_word(c)
}

/// Byte index of the extension dot in a file name: the last dot that is not
/// part of the leading run of dots (`.env` and `..x` have no extension).
fn dot_of(name: &str) -> Option<usize> {
    let i = name.rfind('.')?;
    (!name[..i].bytes().all(|b| b == b'.')).then_some(i)
}

/// Lowercased extension (with the dot) of the last path component, or "".
pub fn extension(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match dot_of(name) {
        None => String::new(),
        Some(i) => name[i..].to_lowercase(),
    }
}

/// Case-insensitive comparison key: the full Unicode case fold, so `ß` and
/// `SS` agree. ASCII text takes the allocation-light path.
pub fn casefold(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match super::fold::FOLD_EXCEPTIONS.binary_search_by(|(k, _)| k.cmp(&c)) {
            Ok(i) => out.push_str(super::fold::FOLD_EXCEPTIONS[i].1),
            Err(_) => out.extend(c.to_lowercase()),
        }
    }
    out
}

/// Last path component.
pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Directory part of a `/`-separated path ("" for top-level files).
pub fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[..i],
        None => "",
    }
}

/// File name without its last extension (a leading dot is not an extension).
pub fn stem(path: &str) -> &str {
    let name = basename(path);
    match dot_of(name) {
        None => name,
        Some(i) => &name[..i],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_escapes() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz%41"), "%zzA");
        assert_eq!(percent_decode("%e2%82%ac"), "€");
        assert_eq!(percent_decode("%ff"), "\u{fffd}");
        assert_eq!(percent_decode("%2"), "%2");
        assert_eq!(percent_decode("%20"), " ");
    }

    #[test]
    fn external_targets() {
        for t in [
            "https://x",
            "mailto:x",
            "data:x",
            "#anchor",
            "//cdn/x",
            "",
            "  ",
            "C:\\x",
        ] {
            assert!(is_external(t), "{t}");
        }
        for t in ["Note", "./a.md", "../b", "dir/c.png", "1abc:x", "a b"] {
            assert!(!is_external(t), "{t}");
        }
    }

    #[test]
    fn tracker_counts_characters_and_lines() {
        let text = "é\nab\ncd";
        let mut t = PosTracker::new(text);
        assert_eq!(t.at(0), (0, 1));
        assert_eq!(t.at(3), (2, 2));
        assert_eq!(t.at(6), (5, 3));
        assert_eq!(t.at(100), (7, 3));
    }

    #[test]
    fn path_helpers() {
        assert_eq!(extension("a/B.MD"), ".md");
        assert_eq!(extension(".hidden"), "");
        assert_eq!(stem("a/b.tar.gz"), "b.tar");
        assert_eq!(stem(".hidden"), ".hidden");
        assert_eq!(dirname("a/b/c.md"), "a/b");
        assert_eq!(dirname("c.md"), "");
    }
}
