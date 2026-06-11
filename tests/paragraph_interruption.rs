//! Paragraph interruption (grammar §10, post-Markdown default): a VISIBLE block
//! interrupts an open paragraph with no blank line, at the top level and nested.
//! Ordered lists do not interrupt, `+` is the continuation marker not a bullet,
//! and a bare image stays inline. Invisible constructs (comments, abbreviation
//! definitions) interrupt as before.

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
fn unordered_list_interrupts() {
    assert_eq!(
        html("text\n- a\n- b"),
        "<p>text</p>\n<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>"
    );
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
fn tab_indented_bullet_interrupts_paragraph() {
    // Rule B: a bullet opens a list at any indentation, so a tab-indented one
    // interrupts an open paragraph (matches a space-indented bullet).
    assert_eq!(
        html("text\n\t- item"),
        "<p>text</p>\n<ul>\n  <li>item</li>\n</ul>"
    );
}

#[test]
fn indented_ordered_marker_does_not_interrupt_paragraph() {
    // Ordered markers never interrupt a paragraph, at any indentation.
    assert_eq!(html("text\n  1. item"), "<p>text\n1. item</p>");
}
