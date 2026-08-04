//! A blank line ends a compact nested item, and a line below its content
//! column belongs to the document, not to the item.
//!
//! `- - a` opens two items on ONE line, so nothing is ever collected into the
//! outer item's continuation block before the blank arrives. The blank-line
//! gate in the indented-block collectors lived inside `if let Some(bi) =
//! block_indent`, so with nothing collected it was skipped entirely and ANY
//! indented line after the blank joined the item. carve-js, carve-php and the
//! executable spec all end the list there and parse the line at document level
//! (carve-rs#578).
//!
//! The comment shapes in that issue turned out to be a symptom rather than the
//! cause - the same thing happens with no comment anywhere in the document.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src)
}

/// Collapse layout whitespace so the expectations read as one line, without
/// merging whitespace INSIDE text (`a b` has to stay two words).
fn squash(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
        .replace(" </", "</")
}

#[test]
fn a_line_below_the_content_column_after_a_blank_leaves_the_item() {
    // `b` sits at column 1; the outer item's content column is 2.
    assert_eq!(
        squash(&html("- - a\n\n b\n")),
        squash("<ul><li><ul><li>a</li></ul></li></ul><p>b</p>"),
    );
}

#[test]
fn a_line_at_the_content_column_after_a_blank_stays_in_the_item() {
    // The boundary the fix must not move: column 2 IS the content column, so
    // the line is the outer item's second block.
    assert_eq!(
        squash(&html("- - a\n\n  b\n")),
        squash("<ul><li><ul><li>a</li></ul><p>b</p></li></ul>"),
    );
}

#[test]
fn the_single_level_item_is_unchanged() {
    // This shape always worked - it reaches a different collector, which
    // already tested the content column. Pinned so the two stay in agreement.
    assert_eq!(
        squash(&html("- a\n\n b\n")),
        squash("<ul><li>a</li></ul><p>b</p>"),
    );
}

#[test]
fn an_indented_comment_does_not_hold_the_item_open_across_a_blank() {
    // A comment renders nothing, so it does not establish the column the
    // item's continuation is measured against either. Letting it do so kept
    // `b` in the item by way of the comment's own column.
    assert_eq!(
        squash(&html("- - a\n %% c\n\n b\n")),
        squash("<ul><li><ul><li>a</li></ul></li></ul><p>b</p>"),
    );
}

#[test]
fn a_comment_with_no_blank_still_keeps_the_item_open() {
    // The neighbour fixed in carve-rs#572: with no blank, lazy continuation
    // still applies and `b` folds into the item across the comment.
    assert_eq!(
        squash(&html("- - a\n %% c\n b\n")),
        squash("<ul><li><ul><li>a b</li></ul></li></ul>"),
    );
}
