//! §17 L2: a blank line before an item's sub-LIST keeps the item tight, and
//! that blank must not survive to loosen a later sibling marker.
//!
//! The blank is consumed by the compact sub-block. Before carve-rs#286 the
//! `pending_blank` flag leaked past the sub-list branch and the next sibling
//! marker read it as a blank BETWEEN items, so the whole list rendered loose --
//! while the same blank before a plain continuation block cleared the flag and
//! rendered tight. carve-js and carve-php were tight in both.
//!
//! The distinction that must survive: a blank BEFORE the sub-list is compact
//! (L2), a blank AFTER it and before the next marker is a genuine
//! between-items blank and still loosens (L1).

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn blank_before_a_sublist_keeps_a_following_sibling_tight() {
    let out = html("- fruit\n\n  - apples\n- vegetables\n");
    assert!(out.contains("<li>fruit"), "outer item should be tight: {out}");
    assert!(
        out.contains("<li>vegetables</li>"),
        "sibling after the sub-list should be tight: {out}"
    );
    assert!(!out.contains("<p>fruit</p>"), "item must not be wrapped: {out}");
}

#[test]
fn the_sublist_indent_does_not_change_the_outcome() {
    // Four-space nesting sits past the content column but is still the item's
    // sub-block, so L2 applies exactly as at the content column.
    let out = html("- fruit\n\n    - apples\n    - oranges\n- vegetables\n");
    assert!(out.contains("<li>fruit"), "outer item should be tight: {out}");
    assert!(!out.contains("<p>vegetables</p>"), "sibling should be tight: {out}");
}

#[test]
fn a_blank_between_the_sublist_and_the_next_marker_still_loosens() {
    // L1: the blank directly precedes a sibling marker, so it is a
    // between-items blank and the list is loose. This is the case the fix must
    // not break.
    let out = html("- fruit\n  - apples\n\n- vegetables\n");
    assert!(out.contains("<p>fruit</p>"), "list should be loose: {out}");
    assert!(
        out.contains("<p>vegetables</p>"),
        "list should be loose: {out}"
    );
}

#[test]
fn a_blank_before_a_second_paragraph_still_loosens() {
    // L1 again, via the plain-continuation branch: a genuine second paragraph
    // loosens whether or not a sibling follows.
    let out = html("- fruit\n\n  more prose\n- vegetables\n");
    assert!(out.contains("<p>fruit</p>"), "list should be loose: {out}");
}

#[test]
fn formatting_the_nested_list_round_trips_to_the_same_html() {
    // The formatter emits the blank-before-sub-list shape, so the bug made
    // carve-rs violate to_html(fmt(x)) == to_html(x) on ordinary nested lists.
    let source = "- fruit\n  - apples\n  - oranges\n- vegetables\n";
    let formatted = carve::to_carve(source);
    assert_eq!(
        html(&formatted),
        html(source),
        "fmt output must re-render identically; formatted:\n{formatted}"
    );
}
