//! Paragraph interruption (grammar §10): at the document TOP LEVEL a visible
//! block does NOT interrupt a paragraph without a blank line. Only invisible
//! constructs (ref-defs, comments) and nested markers interrupt. A blank line
//! ends the paragraph and the block parses fresh.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

// --- Top level: visible blocks do NOT interrupt (one paragraph) ---

#[test]
fn fence_does_not_interrupt() {
    // The fence run is prose -> inline code span, not a code block.
    assert_eq!(
        html("text\n```\ncode\n```"),
        "<p>text\n<code>\ncode\n</code></p>"
    );
}

#[test]
fn heading_does_not_interrupt() {
    assert_eq!(html("text\n# H"), "<p>text\n# H</p>");
}

#[test]
fn thematic_break_does_not_interrupt() {
    assert_eq!(html("text\n---\nmore"), "<p>text\n—\nmore</p>");
}

#[test]
fn admonition_does_not_interrupt() {
    assert_eq!(
        html("text\n:::note\nb\n:::"),
        "<p>text\n:::note\nb\n:::</p>"
    );
}

#[test]
fn block_image_does_not_interrupt() {
    assert_eq!(
        html("text\n![a](u)"),
        "<p>text\n<img src=\"u\" alt=\"a\"></p>"
    );
}

#[test]
fn blockquote_does_not_interrupt() {
    assert_eq!(html("text\n> q"), "<p>text\n&gt; q</p>");
}

#[test]
fn table_does_not_interrupt() {
    assert_eq!(html("text\n| a | b |"), "<p>text\n| a | b |</p>");
}

#[test]
fn ordered_list_does_not_interrupt() {
    assert_eq!(html("text\n1. x\n2. y"), "<p>text\n1. x\n2. y</p>");
}

#[test]
fn unordered_list_does_not_interrupt() {
    assert_eq!(html("text\n- a\n- b"), "<p>text\n- a\n- b</p>");
}

// --- Top level: invisible constructs DO interrupt ---

#[test]
fn abbreviation_def_interrupts() {
    // The abbreviation def is collected, leaving only the paragraph.
    assert_eq!(html("text\n*[HT]: Hyper"), "<p>text</p>");
}

#[test]
fn comment_interrupts() {
    // A `%%` comment line is consumed, leaving only the paragraph.
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

// --- Nested context: only LIST MARKERS interrupt (grammar §10 SCOPING).
//     Other visible blocks do NOT interrupt nested either, matching djot. ---

#[test]
fn nested_sublist_interrupts() {
    // The one Carve deviation: a list marker nests a sublist with no blank line.
    assert_eq!(
        html("- a\n   - b"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn heading_does_not_interrupt_inside_blockquote() {
    assert_eq!(
        html("> text\n> # H"),
        "<blockquote><p>text\n# H</p></blockquote>"
    );
}

#[test]
fn heading_does_not_interrupt_inside_list_item() {
    assert_eq!(html("- text\n  # H"), "<ul>\n  <li>text\n# H</li>\n</ul>");
}

#[test]
fn fence_does_not_interrupt_inside_admonition() {
    assert_eq!(
        html(":::note\ntext\n```\ncode\n```\n:::"),
        "<aside class=\"admonition note\">\n  <p>text\n<code>\ncode\n</code></p>\n</aside>"
    );
}
