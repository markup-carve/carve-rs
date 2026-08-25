//! A TASK ITEM'S `[x] ` IS CONTENT, NOT MARKER.
//!
//! `- [x] a` is the bullet `- `, whose width IS the item's content column, and
//! then `[x] `, which the reader consumes as the item's task state. So the
//! item's content column is 2, exactly as it is for `- a` - the checkbox does
//! not move it, and every block the item holds after its first sits at 2.
//!
//! The READER already knew this. `parse.rs` says so in as many words where it
//! computes an item's content column: "for a TASK the checkbox is content, not
//! marker, so the column is the bullet width". The WRITER did not. It indented
//! every block after the item's first to the full width of what it had put on
//! the marker line - six columns for `- [x] `, ten for `-{#k} [x] ` - which is
//! four past the content column. An ordinary paragraph survives being written
//! there, and that is why this went unseen; a BLOCK OPENER does not, and an
//! indented one opens nothing.
//!
//! carve-js fixed the same site in carve-js#1455. This is the carve-rs port
//! (carve-rs#1362), under the umbrella markup-carve/carve#1690.

/// The written source is a fixed point AND renders to what the input did -
/// PART 11 §1: a writer that moves the column changes the document, not only
/// its spelling.
fn holds(source: &str) {
    assert_eq!(carve::to_carve(source), source, "not a fixed point");
    assert_eq!(
        carve::to_html(&carve::to_carve(source)),
        carve::to_html(source),
        "the render moved"
    );
}

/// A source that is not already canonical: assert what the writer makes of it,
/// that the render is held across the rewrite, and that a second pass changes
/// nothing.
fn writes(expected: &str, source: &str) {
    let written = carve::to_carve(source);
    assert_eq!(written, expected);
    assert_eq!(
        carve::to_html(&written),
        carve::to_html(source),
        "the render moved"
    );
    assert_eq!(carve::to_carve(&written), written, "not idempotent");
}

#[test]
fn writes_a_heading_after_a_floating_attribute_at_the_content_column() {
    holds("- [x] {#h}\n  # h\n");
}

#[test]
fn writes_a_heading_after_a_first_paragraph_at_the_content_column() {
    holds("- [x] a\n  # h\n");
}

#[test]
fn writes_a_quote_after_a_floating_attribute_at_the_content_column() {
    holds("- [ ] {#h}\n  > q\n");
}

#[test]
fn writes_a_fence_after_a_first_paragraph_at_the_content_column() {
    holds("- [x] a\n  ```php\n  1;\n  ```\n");
}

/// Item attributes are metadata and the checkbox is content, so neither moves
/// the bare bullet's column 2 (markup-carve/carve#1701). The writer must use
/// that same column or turn the heading into paragraph text on re-read.
#[test]
fn counts_neither_item_attributes_nor_the_checkbox_into_the_column() {
    holds("-{#k} [x] {#h}\n  # h\n");
}

/// The control: a plain item and an ordered item never had the defect, because
/// their content column and their post-marker column are the same. A fix that
/// subtracted unconditionally would move these.
#[test]
fn leaves_a_plain_item_and_an_ordered_item_alone() {
    holds("- {#h}\n  # h\n");
    holds("1. {#h}\n   # h\n");
}

/// THE THREE CORPUS DOCUMENTS THAT REPORTED IT (markup-carve/carve#1690). Same
/// shape, different nesting, so each is asserted rather than one standing in for
/// the others. Every expectation is what carve-js writes at its `main`, measured
/// rather than assumed.
#[test]
fn writes_the_corpus_nested_list_at_the_content_column() {
    // 05-lists-12
    writes("- [ ] outer\n  - inner\n", "- [ ] outer\n  - inner\n");
}

#[test]
fn writes_the_corpus_wide_marker_heading_at_the_content_column() {
    // The authored column of the recognized heading is accepted, then the
    // writer canonicalizes it to the task item's content column.
    writes("- [ ] item\n  # H\n", "-   [ ] item\n    # H\n");
}

#[test]
fn writes_the_corpus_nested_quote_at_the_content_column() {
    // 144-nested-item-looseness-does-not-propagate-to-the-outer-item-3
    writes("- [ ] a\n  - b\n    > q\n", "- [ ] a\n  - b\n\n    > q\n");
}
