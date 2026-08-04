//! A comment on a list marker line is a BLOCK, and a block that renders to
//! nothing contributes no line.
//!
//! `- %% c` used to route to the lead-PARAGRAPH path, where the inline scanner
//! consumed the comment and left the item holding an EMPTY paragraph. Three
//! things followed from that one gap: the comment was absent from the AST where
//! carve-js publishes a `comment` node, the canonical writer saw an item with
//! no content and wrote the CONTINUATION MARKER instead (`- +`, a different
//! construct - one that takes a body), and the empty paragraph rendered a
//! whitespace-only line inside the `<li>` (carve-rs#511 item 7, carve-rs#532).

use carve::{to_carve, to_html};

#[test]
fn a_marker_line_comment_is_a_comment_node() {
    let ast = carve::to_json(&carve::parse("- %% c"));

    assert!(ast.contains("\"comment\""), "{ast}");
    assert!(!ast.contains("\"paragraph\""), "{ast}");
}

#[test]
fn the_writer_keeps_the_comment_rather_than_writing_a_continuation_marker() {
    assert_eq!(to_carve("* %%").trim_end(), "* %%");
    assert_eq!(to_carve("- %% c").trim_end(), "- %% c");
    assert_eq!(to_carve("- a\n\n- %% c").trim_end(), "- a\n\n- %% c");
}

#[test]
fn an_item_holding_only_a_comment_renders_empty() {
    assert_eq!(to_html("- %% c"), "<ul>\n  <li></li>\n</ul>");
}

#[test]
fn a_comment_beside_content_leaves_no_whitespace_line() {
    // The `<li>` used to close on a line of its own after `a`, with the child
    // indentation left behind by the block that rendered to nothing.
    assert_eq!(to_html("- a\n  %% c"), "<ul>\n  <li>a</li>\n</ul>");
}

#[test]
fn a_comment_fence_in_an_item_leaves_no_whitespace_line() {
    // The same rule reached by the BLOCK comment, which is a real block inside
    // the item rather than a marker-line one.
    assert_eq!(
        to_html("- a\n  %%%\n  hidden\n  %%%"),
        "<ul>\n  <li>a</li>\n</ul>"
    );
}
