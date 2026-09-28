//! A line block opened at the DOCUMENT column is one whatever list is open
//! above it, so the link-definition pre-pass must not look inside its body
//! (markup-carve/carve-rs#2096, the carve-rs#491 family one column out).
//!
//! Expectations are the executable spec's reading, taken from
//! `scripts/spec/layout.mjs` plus `scripts/spec/html.mjs` at markup-carve/carve
//! 38829a97. An INDENTED opener stays refused, which the pre-pass's own note
//! explains and which no case here asserts.

#[test]
fn a_line_block_at_the_document_column_under_a_list() {
    assert_eq!(
        carve::to_html(". r\n::: |\n[f]: t\n"),
        "<ol>\n  <li>r</li>\n</ol>\n<div class=\"line-block\">\n  <p>[f]: t</p>\n</div>"
    );
}

#[test]
fn a_line_block_at_the_document_column_keeps_its_closer() {
    assert_eq!(
        carve::to_html(". r\n::: |\n[f]: t\n:::\n\n[f]\n"),
        "<ol>\n  <li>r</li>\n</ol>\n<div class=\"line-block\">\n  <p>[f]: t</p>\n</div>\n<p>[f]</p>"
    );
}
