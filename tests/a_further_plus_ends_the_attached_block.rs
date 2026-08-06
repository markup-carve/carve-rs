//! A further `+` ends the block an earlier one attached (carve-rs#704).
//!
//! §17 L3 says the attached block runs "up to the next blank line, sibling
//! marker, or a further `+`" - with no exception for the first line. So in
//! `- a` / `+` / `+` / `b` the first marker attaches nothing (its block ends
//! immediately) and the second attaches `b`, which is what carve-js, carve-php
//! and the executable spec all produce.
//!
//! This engine required the attached block to be non-empty before a `+` could
//! end it, so the second marker was swallowed as CONTENT of the first
//! attachment and rendered as literal text in the item.

use carve::to_html;

#[test]
fn a_second_marker_ends_the_first_attachment_and_attaches_the_block() {
    let html = to_html("- a\n+\n+\nb\n\nx\n");

    assert!(!html.contains('+'), "{html}");
    assert!(html.contains("b"), "{html}");
}

#[test]
fn the_result_matches_the_single_marker_spelling() {
    // The clause's consequence stated directly: an empty attachment adds
    // nothing, so the two spellings describe the same document.
    assert_eq!(to_html("- a\n+\n+\nb\n\nx\n"), to_html("- a\n+\nb\n\nx\n"));
}

#[test]
fn a_single_marker_still_attaches_its_block() {
    // The control: the shape that already worked must keep working.
    let html = to_html("- a\n+\nb\n\nx\n");

    assert!(!html.contains('+'), "{html}");
}

#[test]
fn a_sibling_marker_on_the_first_attached_line_still_ends_the_item() {
    // The neighbouring guard, which is a different rule and must not move: a
    // list marker at the base column right after `+` is a SIBLING item, not an
    // attached block.
    let html = to_html("- a\n+\n- b\n\nx\n");

    assert_eq!(html.matches("<li>").count(), 2, "{html}");
}
