//! Uniform block position after an open paragraph (grammar §10). Every block
//! marker needs a blank or enclosing structural boundary; without one its bytes
//! remain paragraph content. Tight nesting at a container content column and
//! bounded headings are structural rather than opener-specific exceptions.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

// --- Top-level blocks after an explicit boundary ---

#[test]
fn fence_starts_in_block_position() {
    assert_eq!(
        html("text\n\n```\ncode\n```\n"),
        "<p>text</p>\n<pre><code>code\n</code></pre>"
    );
}

#[test]
fn heading_starts_in_block_position() {
    assert_eq!(
        html("text\n\n# H\n"),
        "<p>text</p>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}

#[test]
fn thematic_break_starts_in_block_position() {
    assert_eq!(
        html("text\n\n---\n\nmore\n"),
        "<p>text</p>\n<hr>\n<p>more</p>"
    );
}

#[test]
fn admonition_starts_in_block_position() {
    assert_eq!(
        html("text\n\n::: note\nb\n:::\n"),
        "<p>text</p>\n<aside class=\"admonition note\">\n  <p>b</p>\n</aside>"
    );
}

#[test]
fn blockquote_starts_in_block_position() {
    assert_eq!(
        html("text\n\n> q\n"),
        "<p>text</p>\n<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn table_starts_in_block_position() {
    assert_eq!(
        html("text\n\n| a | b |\n"),
        "<p>text</p>\n<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn unordered_list_folds_without_blank_line() {
    // Symmetric §10: a bullet does not interrupt a paragraph (needs a blank
    // line), so the bullet lines fold in as lazy continuation.
    assert_eq!(html("text\n- a\n- b\n"), "<p>text\n- a\n- b</p>");
}

// --- Without block position, marker bytes remain paragraph content ---

#[test]
fn ordered_list_marker_stays_in_paragraph() {
    assert_eq!(html("text\n1. x\n2. y\n"), "<p>text\n1. x\n2. y</p>");
}

#[test]
fn image_stays_inline() {
    assert_eq!(
        html("text\n![a](u)\n"),
        "<p>text\n<img src=\"u\" alt=\"a\"></p>"
    );
}

// --- Non-rendering constructs in block position ---

#[test]
fn abbreviation_definition_starts_after_blank() {
    assert_eq!(html("text\n\n*[HT]: Hyper\n"), "<p>text</p>");
}

#[test]
fn comment_starts_after_blank() {
    assert_eq!(html("para\n\n%% c\n"), "<p>para</p>");
}

// --- Blank line ends the paragraph; the block parses fresh ---

#[test]
fn blank_line_starts_heading() {
    assert_eq!(
        html("text\n\n# H\n"),
        "<p>text</p>\n<section id=\"H\">\n  <h1>H</h1>\n</section>"
    );
}

#[test]
fn blank_line_starts_fence() {
    assert_eq!(
        html("text\n\n```\nc\n```\n"),
        "<p>text</p>\n<pre><code>c\n</code></pre>"
    );
}

// --- Nested contexts use the same boundaries ---

#[test]
fn nested_sublist_starts_at_content_column() {
    assert_eq!(
        html("- a\n  - b\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn heading_starts_after_quoted_blank() {
    assert_eq!(
        html("> text\n>\n> # H\n"),
        "<blockquote>\n  <p>text</p>\n  <h1 id=\"H\">H</h1>\n</blockquote>"
    );
}

#[test]
fn fence_starts_after_blank_inside_admonition() {
    assert_eq!(
        html("::: note\ntext\n\n```\ncode\n```\n:::\n"),
        "<aside class=\"admonition note\">\n  <p>text</p>\n  <pre><code>code\n</code></pre>\n</aside>"
    );
}

#[test]
fn tab_indented_bullet_folds_into_paragraph() {
    // Symmetric §10: a bullet does not interrupt a paragraph regardless of
    // indentation (tab or spaces); with no blank line it folds in.
    assert_eq!(html("text\n- item\n"), "<p>text\n- item</p>");
}

#[test]
fn ordered_marker_stays_in_paragraph() {
    // Ordered markers never interrupt a paragraph, at any indentation.
    assert_eq!(html("text\n1. item\n"), "<p>text\n1. item</p>");
}

#[test]
fn block_quote_nests_under_an_ordered_item_without_a_blank() {
    // A continuation marker establishes block position in the item. The content
    // column of `1. ` is 3, so the dedent must use the marker width, not a fixed
    // bullet width of 2 (matches carve-php).
    assert_eq!(
        html("1. a\n+\n> q\n"),
        "<ol>\n  <li>a\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ol>"
    );
}

#[test]
fn heading_nests_under_an_item_without_a_blank() {
    assert_eq!(
        html("- a\n+\n# H\n"),
        "<ul>\n  <li>a\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn block_quote_after_a_sub_list_is_an_outer_item_sibling() {
    assert_eq!(
        html("1. a\n   1. b\n\n   > q\n"),
        "<ol>\n  <li>a\n    <ol>\n      <li>b</li>\n    </ol>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ol>"
    );
}

#[test]
fn indented_prose_still_folds_as_lazy_continuation() {
    // Indented prose is lazy continuation, not a new block.
    assert_eq!(html("1. a\n   more\n"), "<ol>\n  <li>a\nmore</li>\n</ol>");
}
