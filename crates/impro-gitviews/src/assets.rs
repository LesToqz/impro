//! Embedded pages; no files or network are consulted by WebKit.

/// Build the standalone graph document from embedded assets.
pub fn graph_html() -> String {
    include_str!("../web/gitgraph/gitgraph.html")
        .replace(
            "<link rel=\"stylesheet\" href=\"gitgraph.css\">",
            &format!(
                "<style>{}</style>",
                include_str!("../web/gitgraph/gitgraph.css")
            ),
        )
        .replace(
            "<script src=\"gitgraph_core.js\"></script>",
            &script(include_str!("../web/gitgraph/gitgraph_core.js")),
        )
        .replace(
            "<script src=\"gitgraph.js\"></script>",
            &script(include_str!("../web/gitgraph/gitgraph.js")),
        )
}

/// Build the standalone commit document with a nonce for embedded scripts.
pub fn commit_html() -> String {
    include_str!("../web/commit/commit.html")
        .replace("script-src 'self'", "script-src 'nonce-impro'")
        .replace(
            "<link rel=\"stylesheet\" href=\"commit.css\">",
            &format!(
                "<style>{}</style>",
                include_str!("../web/commit/commit.css")
            ),
        )
        .replace(
            "<script src=\"commit_core.js\"></script>",
            &script(include_str!("../web/commit/commit_core.js")),
        )
        .replace(
            "<script src=\"commit.js\"></script>",
            &script(include_str!("../web/commit/commit.js")),
        )
}
fn script(source: &str) -> String {
    format!("<script nonce=\"impro\">{source}</script>")
}
