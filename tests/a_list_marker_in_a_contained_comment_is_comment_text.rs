//! A list marker inside a contained comment is comment text (carve-rs#1053).
//!
//! The item's line collector cuts the item into chunks BEFORE any chunk is
//! parsed, so it has to know which lines are opaque without understanding them.
//! It knew about code fences (§24) and colon containers, and not about comment
//! fences (§28) - so a marker inside a `%%%` body severed the chunk. The opener
//! then re-parsed alone as an unterminated fence, which §28 degrades to a `%%`
//! line comment; the marker opened a sub-list; and the closer degraded the same
//! way in the next chunk. Both delimiters vanished and everything between them
//! rendered, which is the one outcome a comment may never have (the same
//! invariant carve-rs#573 states for the top-level shape).
//!
//! A heading or a block quote in that position was already hidden, but by
//! omission rather than by handling: neither is a list marker, so the gate that
//! severed the chunk never fired for them. That asymmetry is what made this
//! specific to the list partition.

use carve::to_html;

const HIDDEN: &str = "<ul>\n  <li>item</li>\n</ul>\n<p>tail</p>";

#[test]
fn a_marker_at_the_item_s_content_column_is_hidden() {
    assert_eq!(
        to_html("- item\n  %%%\n  - x\n  y\n  %%%\n\ntail\n"),
        HIDDEN
    );
}

#[test]
fn a_marker_deeper_than_the_fence_is_hidden_the_same_way() {
    // The marker's column never mattered to the answer, only to which wrong
    // answer came out, so both columns are pinned.
    assert_eq!(
        to_html("- item\n  %%%\n    - x\n    y\n  %%%\n\ntail\n"),
        HIDDEN
    );
}

#[test]
fn an_ordered_marker_is_hidden_too() {
    // `detect_list_marker_full` covers ordered markers, so the gate fired for
    // them as well; fixing only the bullet spelling would have left a hole.
    assert_eq!(
        to_html("- item\n  %%%\n  1. x\n  y\n  %%%\n\ntail\n"),
        HIDDEN
    );
}

#[test]
fn a_heading_and_a_block_quote_in_that_position_stay_hidden() {
    // The controls from the report. These were already correct; they are pinned
    // so a later change to the gate cannot fix the marker by breaking them.
    assert_eq!(
        to_html("- item\n  %%%\n  # h\n  y\n  %%%\n\ntail\n"),
        HIDDEN
    );
    assert_eq!(
        to_html("- item\n  %%%\n  > q\n  y\n  %%%\n\ntail\n"),
        HIDDEN
    );
}

#[test]
fn a_definition_under_that_marker_neither_registers_nor_renders() {
    // Both halves of the shape agree for the first time. The pre-pass already
    // declined to register (markup-carve/carve-rs#1052); the body no longer
    // leaks the line it would have registered from.
    let out = to_html("- item\n  %%%\n  - x\n  [r]: /u\n  %%%\n\ntail [r][]\n");
    assert_eq!(out, "<ul>\n  <li>item</li>\n</ul>\n<p>tail [r][]</p>");
}

#[test]
fn a_footnote_definition_under_that_marker_is_hidden_too() {
    let out = to_html("- item\n  %%%\n  - x\n  [^f]: n\n  %%%\n\ntail[^f]\n");
    assert!(!out.contains("doc-endnotes"), "emitted an endnote: {out}");
    assert!(!out.contains(">n<"), "leaked the note body: {out}");
}

#[test]
fn an_unterminated_fence_still_degrades_and_the_marker_still_opens_a_list() {
    // §28: no closer ahead means it was never a fence, so nothing is opaque and
    // the gate must still fire. The guard reads the span state the PRECEDING
    // lines left, and that state is only ever set where a closer really
    // follows - which is what keeps this case working.
    let out = to_html("- item\n  %%%\n  - x\n  y\n\ntail\n");
    assert!(out.contains('x') && out.contains('y'), "{out}");
    assert!(out.contains("<ul>"), "{out}");
}

#[test]
fn a_comment_fence_written_inside_a_code_fence_opens_no_span() {
    // Caught by review on the first cut of this fix, which regressed it.
    //
    // The `%%%` here is CODE TEXT, so the real delimiters are the two that
    // follow and `z` is what they hide. The collector's comment tracker did not
    // exclude code-fence bodies, so it opened a span on that code text; the span
    // was still open when the code fence ended, and the new marker gate then
    // suppressed the boundary for the REAL comment after it, putting `z` on the
    // page. The tracker now refuses to open a span inside a code fence, which is
    // the same reading of a verbatim body the gate itself applies.
    //
    // `x` stays visible and `z` stays hidden - that is the whole assertion.
    let out = to_html("- item\n  ```\n  %%%\n  ```\n  - x\n  %%%\n  - z\n  %%%\n\ntail\n");
    assert!(out.contains(">x<"), "lost the visible item: {out}");
    assert!(!out.contains(">z<"), "leaked the commented item: {out}");
    assert!(out.contains("<code>%%%"), "lost the code text: {out}");
}

#[test]
fn the_body_is_hidden_one_level_deeper_as_well() {
    assert_eq!(
        to_html("- - item\n    %%%\n    - x\n    y\n    %%%\n\ntail\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>item</li>\n    </ul>\n  </li>\n</ul>\n<p>tail</p>"
    );
}
