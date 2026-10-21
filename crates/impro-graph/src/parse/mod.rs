//! Text parsers: Markdown links/tags/properties and source-file imports.
//!
//! Everything here is a pure function of the text. Offsets reported to callers
//! are character offsets and lines are one-based.

pub mod code;
pub mod frontmatter;
pub mod links;
pub mod mask;
pub mod tags;
pub mod text;

pub use code::{extract_imports, supported_extensions, CodeKind, CodeRef};
pub use frontmatter::{Frontmatter, PropValue};
pub use links::{extract_links, Link, LinkKind, LinkSyntax};
pub use tags::extract_tags;

/// Everything the index needs to know about one note.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteInfo {
    pub links: Vec<Link>,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
}

/// Parses a note once, sharing the frontmatter between links, tags and aliases.
pub fn parse_note(text: &str) -> NoteInfo {
    let fm = frontmatter::parse(text);
    let mut tags = tags::frontmatter_tags(&fm);
    tags::inline_tags(&text[fm.body_start..], &mut tags);
    NoteInfo {
        links: extract_links(text),
        tags,
        aliases: fm.aliases(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_info_combines_parsers() {
        let info = parse_note("---\naliases: [Nick]\ntags: [t/1]\n---\n[[B]] #inline");
        assert_eq!(info.aliases, ["Nick"]);
        assert_eq!(info.tags, ["t/1", "inline"]);
        assert_eq!(info.links.len(), 1);
    }
}
