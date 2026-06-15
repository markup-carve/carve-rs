//! Paragraph interruption (grammar §10): a VISIBLE block interrupts an open
//! paragraph with no blank line, at the top level and nested. Symmetric rule: a
//! LIST marker (bullet, task, or ordered) does NOT interrupt -- a list needs a
//! blank line and otherwise folds in; `+` is the continuation marker not a
//! bullet, and a bare image stays inline. Invisible constructs (comments,
//! abbreviation definitions) interrupt as before.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

// --- Top level: visible blocks interrupt ---

#[test]
fn fence_interrupts() {
    assert_eq!(
        html("text\n```\ncode\n```"),
        "<p>text</p>\n<pre><code>code\n</code></pre>"
    );
}

#[test]
fn heading_interrupts() {
    assert_eq!(
        html("text\n# H"),
        "<p>text</p>\n<section id=\"h\">\n  <h1>H</h1>\n</section>"
    );
}

#[test]
fn thematic_break_interrupts() {
    assert_eq!(html("text\n---\nmore"), "<p>text</p>\n<hr>\n<p>more</p>");
}

#[test]
fn admonition_interrupts() {
    assert_eq!(
        html("text\n:::note\nb\n:::"),
        "<p>text</p>\n<aside class=\"admonition note\">\n  <p>b</p>\n</aside>"
    );
}

#[test]
fn blockquote_interrupts() {
    assert_eq!(
        html("text\n> q"),
        "<p>text</p>\n<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn table_interrupts() {
    assert_eq!(
        html("text\n| a | b |"),
        "<p>text</p>\n<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn unordered_list_folds_without_blank_line() {
    // Symmetric §10: a bullet does not interrupt a paragraph (needs a blank
    // line), so the bullet lines fold in as lazy continuation.
    assert_eq!(html("text\n- a\n- b"), "<p>text\n- a\n- b</p>");
}

// --- Top level: these do NOT interrupt ---

#[test]
fn ordered_list_does_not_interrupt() {
    assert_eq!(html("text\n1. x\n2. y"), "<p>text\n1. x\n2. y</p>");
}

#[test]
fn block_image_does_not_interrupt() {
    assert_eq!(
        html("text\n![a](u)"),
        "<p>text\n<img src=\"u\" alt=\"a\"></p>"
    );
}

// --- Invisible constructs interrupt ---

#[test]
fn abbreviation_def_interrupts() {
    assert_eq!(html("text\n*[HT]: Hyper"), "<p>text</p>");
}

#[test]
fn comment_interrupts() {
    assert_eq!(html("para\n%% c"), "<p>para</p>");
}

// --- Blank line ends the paragraph; the block parses fresh ---

#[test]
fn blank_line_starts_heading() {
    assert_eq!(
        html("text\n\n# H"),
        "<p>text</p>\n<section id=\"h\">\n  <h1>H</h1>\n</section>"
    );
}

#[test]
fn blank_line_starts_fence() {
    assert_eq!(
        html("text\n\n```\nc\n```"),
        "<p>text</p>\n<pre><code>c\n</code></pre>"
    );
}

// --- Nested context: interruption applies inside containers too ---

#[test]
fn nested_sublist_interrupts() {
    assert_eq!(
        html("- a\n   - b"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn heading_interrupts_inside_blockquote() {
    assert_eq!(
        html("> text\n> # H"),
        "<blockquote>\n  <p>text</p>\n  <h1>H</h1>\n</blockquote>"
    );
}

#[test]
fn fence_interrupts_inside_admonition() {
    assert_eq!(
        html(":::note\ntext\n```\ncode\n```\n:::"),
        "<aside class=\"admonition note\">\n  <p>text</p>\n  <pre><code>code\n</code></pre>\n</aside>"
    );
}

#[test]
fn tab_indented_bullet_folds_into_paragraph() {
    // Symmetric §10: a bullet does not interrupt a paragraph regardless of
    // indentation (tab or spaces); with no blank line it folds in.
    assert_eq!(html("text\n\t- item"), "<p>text\n- item</p>");
}

#[test]
fn indented_ordered_marker_does_not_interrupt_paragraph() {
    // Ordered markers never interrupt a paragraph, at any indentation.
    assert_eq!(html("text\n  1. item"), "<p>text\n1. item</p>");
}

#[test]
fn block_quote_nests_under_an_ordered_item_without_a_blank() {
    // A block opener indented to the item's content column interrupts the item's
    // lead paragraph and nests, rather than folding in as lazy text. The content
    // column of `1. ` is 3, so the dedent must use the marker width, not a fixed
    // bullet width of 2 (matches carve-php).
    assert_eq!(
        html("1. a\n   > q"),
        "<ol>\n  <li>a\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ol>"
    );
}

#[test]
fn heading_nests_under_an_item_without_a_blank() {
    assert_eq!(
        html("- a\n  # H"),
        "<ul>\n  <li>a\n    <h1>H</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn block_quote_after_a_sub_list_is_an_outer_item_sibling() {
    assert_eq!(
        html("1. a\n   1. b\n   > q"),
        "<ol>\n  <li>a\n    <ol>\n      <li>b</li>\n    </ol>\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ol>"
    );
}

#[test]
fn indented_prose_still_folds_as_lazy_continuation() {
    // A non-block-opening indented line is lazy continuation, not a new block.
    assert_eq!(html("1. a\n   more"), "<ol>\n  <li>a\nmore</li>\n</ol>");
}
