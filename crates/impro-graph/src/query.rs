//! Search language: implicit AND, `OR` clauses, `-negation`, `"phrases"`,
//! `/regex/` and the `path: file: content: tag:` fields.
//!
//! A [`Query`] is parsed once and evaluated against many documents, so regexes
//! compile once. An invalid regex never panics: it simply does not match.

use std::sync::OnceLock;

use regex::{Regex, RegexBuilder};

use crate::parse::text::casefold;

/// What a query looks at for one node.
#[derive(Debug, Clone, Copy)]
pub struct Doc<'a> {
    pub path: &'a str,
    pub label: &'a str,
    pub content: &'a str,
    pub tags: Tags<'a>,
}

#[derive(Debug, Clone, Copy)]
pub enum Tags<'a> {
    None,
    One(&'a str),
    Many(&'a [Box<str>]),
}

impl<'a> Tags<'a> {
    fn any(&self, mut f: impl FnMut(&str) -> bool) -> bool {
        match self {
            Tags::None => false,
            Tags::One(t) => f(t),
            Tags::Many(list) => list.iter().any(|t| f(t)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Any,
    Path,
    File,
    Content,
    Tag,
}

#[derive(Debug, Clone)]
enum Matcher {
    /// Case-folded needle; `ascii` selects the allocation-free search.
    Text {
        needle: String,
        ascii: bool,
    },
    Regex(Option<Regex>),
    /// Case-folded tag without `#`.
    Tag(String),
}

#[derive(Debug, Clone)]
struct Term {
    negate: bool,
    field: Field,
    matcher: Matcher,
}

#[derive(Debug, Clone, Default)]
pub struct Query {
    clauses: Vec<Vec<Term>>,
    blank: bool,
}

/// Splits a query into terms: an optional `-`, an optional `word:` field, then
/// a `"quoted phrase"`, a `/regex/` (ending at the next `/`) or a run of
/// non-blank characters. An unclosed quote or slash falls back to the plain
/// run, so a lone `-` or `"` is a term of its own.
fn tokenize(query: &str) -> Vec<&str> {
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    let re = TOKEN.get_or_init(|| {
        Regex::new(r#"-?(?:\w+:)?(?:"[^"]*"|/[^/]*(?:\\/[^/]*)*/|[^\s]+)"#)
            .expect("token pattern is valid")
    });
    re.find_iter(query).map(|m| m.as_str()).collect()
}

fn parse_term(token: &str) -> Term {
    let (negate, rest) = match token.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, token),
    };
    let (field, value) = match rest.split_once(':') {
        Some((name, value)) => match name.to_lowercase().as_str() {
            "path" => (Field::Path, value),
            "file" => (Field::File, value),
            "content" => (Field::Content, value),
            "tag" => (Field::Tag, value),
            _ => (Field::Any, rest),
        },
        None => (Field::Any, rest),
    };
    let value = value.trim_matches('"');
    let matcher = if field == Field::Tag {
        Matcher::Tag(casefold(value.trim_start_matches('#')))
    } else if value.starts_with('/') && value.ends_with('/') {
        // A lone `/` is an empty pattern, which matches everything.
        let pattern = value.get(1..value.len().max(1) - 1).unwrap_or("");
        Matcher::Regex(
            RegexBuilder::new(pattern)
                .case_insensitive(true)
                .size_limit(1 << 20)
                .build()
                .ok(),
        )
    } else {
        Matcher::Text {
            needle: casefold(value),
            ascii: value.is_ascii(),
        }
    };
    Term {
        negate,
        field,
        matcher,
    }
}

/// Case-insensitive substring test; `needle` is already lowercase ASCII.
fn contains_ascii_ci(hay: &str, needle: &str) -> bool {
    let n = needle.as_bytes();
    let h = hay.as_bytes();
    if n.is_empty() {
        return true;
    }
    if h.len() < n.len() {
        return false;
    }
    let (lo, up) = (n[0], n[0].to_ascii_uppercase());
    let mut at = 0;
    let last = h.len() - n.len();
    while at <= last {
        let off = if lo == up {
            memchr::memchr(lo, &h[at..=last])
        } else {
            memchr::memchr2(lo, up, &h[at..=last])
        };
        let p = match off {
            Some(o) => at + o,
            None => return false,
        };
        if h[p + 1..p + n.len()].eq_ignore_ascii_case(&n[1..]) {
            return true;
        }
        at = p + 1;
    }
    false
}

fn text_found(hay: &str, needle: &str, ascii: bool) -> bool {
    if needle.is_empty() {
        true
    } else if ascii && hay.is_ascii() {
        contains_ascii_ci(hay, needle)
    } else {
        casefold(hay).contains(needle)
    }
}

impl Term {
    fn found(&self, doc: &Doc<'_>) -> bool {
        let base = doc.path.rsplit('/').next().unwrap_or(doc.path);
        let hit = match &self.matcher {
            Matcher::Tag(wanted) => doc.tags.any(|t| {
                let t = casefold(t);
                t == *wanted
                    || (t.len() > wanted.len()
                        && t.starts_with(wanted.as_str())
                        && t.as_bytes()[wanted.len()] == b'/')
            }),
            Matcher::Regex(None) => false,
            Matcher::Regex(Some(re)) => match self.field {
                Field::Path => re.is_match(doc.path),
                Field::File => re.is_match(base),
                Field::Content => re.is_match(doc.content),
                // anchors and `\n` see the whole "path, label, content" text
                _ => re.is_match(&format!("{}\n{}\n{}", doc.path, doc.label, doc.content)),
            },
            Matcher::Text { needle, ascii } => match self.field {
                Field::Path => text_found(doc.path, needle, *ascii),
                Field::File => text_found(base, needle, *ascii),
                Field::Content => text_found(doc.content, needle, *ascii),
                _ if needle.contains('\n') => text_found(
                    &format!("{}\n{}\n{}", doc.path, doc.label, doc.content),
                    needle,
                    *ascii,
                ),
                _ => {
                    text_found(doc.path, needle, *ascii)
                        || text_found(doc.label, needle, *ascii)
                        || text_found(doc.content, needle, *ascii)
                }
            },
        };
        hit != self.negate
    }
}

impl Query {
    pub fn parse(query: &str) -> Query {
        let mut clauses: Vec<Vec<Term>> = vec![Vec::new()];
        for token in tokenize(query) {
            if token == "OR" {
                clauses.push(Vec::new());
            } else {
                clauses
                    .last_mut()
                    .expect("never empty")
                    .push(parse_term(token));
            }
        }
        // `a OR` / `OR a`: empty clauses are skipped, they never match
        clauses.retain(|c| !c.is_empty());
        Query {
            blank: query.trim().is_empty(),
            clauses,
        }
    }

    /// An empty (or whitespace-only) query matches everything.
    pub fn is_blank(&self) -> bool {
        self.blank
    }

    pub fn matches(&self, doc: &Doc<'_>) -> bool {
        if self.blank {
            return true;
        }
        self.clauses
            .iter()
            .any(|clause| clause.iter().all(|t| t.found(doc)))
    }
}

/// Convenience for one-off checks.
pub fn match_query(doc: &Doc<'_>, query: &str) -> bool {
    Query::parse(query).matches(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc<'a>(tags: &'a [Box<str>]) -> Doc<'a> {
        Doc {
            path: "folder/Alpha.md",
            label: "Alpha",
            content: "bright red planet",
            tags: Tags::Many(tags),
        }
    }

    fn tags() -> Vec<Box<str>> {
        vec!["planet/rocky".into()]
    }

    #[test]
    fn matching_queries() {
        let t = tags();
        let d = doc(&t);
        for q in [
            "PATH:FOLDER file:Alpha",
            "tag:#planet",
            "\"red planet\" -blue",
            "nothing OR content:bright",
            "/RED.*planet/",
            "content:\"red planet\"",
            "tag:planet/rocky",
            "",
            "   ",
            "alpha",
            "-gamma",
        ] {
            assert!(match_query(&d, q), "{q}");
        }
    }

    #[test]
    fn non_matching_queries() {
        let t = tags();
        let d = doc(&t);
        for q in [
            "tag:plan",
            "bright blue",
            "-planet",
            "/[/",
            "nothing OR absent",
            "OR",
            "file:folder",
            "path:/^alpha/",
            "tag:",
            "-/RED/",
            "tag:rocky",
            "bright -",
        ] {
            assert!(!match_query(&d, q), "{q}");
        }
    }

    #[test]
    fn invalid_regex_is_not_a_panic_and_negation_inverts() {
        let t = tags();
        let d = doc(&t);
        assert!(!match_query(&d, "/(/"));
        assert!(match_query(&d, "-/(/"));
        assert!(!match_query(&d, &format!("/{}/", "(a*)*b".repeat(10_000))));
    }

    #[test]
    fn tokenizer_keeps_phrases_and_regexes_whole() {
        assert_eq!(
            tokenize("a \"b c\" -d path:\"x y\" /p q\\/r/ OR é"),
            [
                "a",
                "\"b c\"",
                "-d",
                "path:\"x y\"",
                "/p q\\/",
                "r/",
                "OR",
                "é"
            ]
        );
        assert_eq!(tokenize("\"unclosed phrase"), ["\"unclosed", "phrase"]);
        assert_eq!(tokenize("/unclosed re"), ["/unclosed", "re"]);
        assert!(tokenize("   ").is_empty());
    }

    #[test]
    fn tag_prefix_respects_boundary() {
        let tags: Vec<Box<str>> = vec!["planetary".into(), "a/b".into()];
        let d = Doc {
            path: "x.md",
            label: "x",
            content: "",
            tags: Tags::Many(&tags),
        };
        assert!(!match_query(&d, "tag:planet"));
        assert!(match_query(&d, "tag:a"));
        assert!(match_query(&d, "tag:A/B"));
        assert!(!match_query(&d, "tag:a/b/c"));
        let single = Doc {
            tags: Tags::One("a/b"),
            ..d
        };
        assert!(match_query(&single, "tag:a"));
    }

    #[test]
    fn non_ascii_needles_fold_case() {
        let d = Doc {
            path: "p.md",
            label: "Ünïcode",
            content: "Привет МИР",
            tags: Tags::None,
        };
        assert!(match_query(&d, "ünï"));
        assert!(match_query(&d, "content:привет"));
        assert!(match_query(&d, "/мир/"));
    }

    #[test]
    fn ascii_search_is_exact_on_boundaries() {
        assert!(contains_ascii_ci("xxABc", "abc"));
        assert!(contains_ascii_ci("abc", "abc"));
        assert!(!contains_ascii_ci("ab", "abc"));
        assert!(!contains_ascii_ci("abxbc", "abc"));
        assert!(contains_ascii_ci("anything", ""));
        assert!(contains_ascii_ci("é abc", "abc"));
        assert!(contains_ascii_ci("1-2", "-2"));
    }
}
