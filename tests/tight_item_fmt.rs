//! Regression: `carve fmt` must keep a tight list item tight when it has more
//! than one child (text after a fenced block, corpus 162), while leaving a
//! tight item whose child is a nested list (corpus 142) idempotent. Before the
//! fix, `render_list` joined an item's blocks with a blank line unconditionally,
//! which loosened the multi-child tight item on re-parse.

use carve::{render_carve, to_html};

fn parse(src: &str) -> carve::Document {
    carve::parse(src)
}

fn fmt(src: &str) -> String {
    render_carve(&parse(src)).expect("the tree under test is within the render ceiling")
}

#[test]
fn tight_item_trailing_text_after_a_block_round_trips() {
    // text after a fenced block in a tight item stays bare and tight
    let src = "- item\n  ```\n  c\n  ```\n  tail\n";
    assert_eq!(
        to_html(&fmt(src)),
        to_html(src),
        "fmt loosened the tight item"
    );
    // and the formatted form is idempotent
    assert_eq!(fmt(&fmt(src)), fmt(src), "fmt is not idempotent");
    // the tail is not wrapped in a paragraph
    assert!(
        !fmt(src).contains("\n\n"),
        "tight item picked up a blank line: {:?}",
        fmt(src)
    );
}

#[test]
fn tight_item_after_a_div_round_trips() {
    let src = "- item\n  :::note\n  body\n  :::\n  tail\n";
    assert_eq!(to_html(&fmt(src)), to_html(src));
    assert_eq!(fmt(&fmt(src)), fmt(src));
}

#[test]
fn tight_outer_item_with_a_loose_nested_list_stays_idempotent() {
    // corpus 142: the nested-list child keeps its blank-line separation, so the
    // formatter emits `- a\n\n    - b` (a known pre-existing limitation where fmt
    // does not preserve the tight-outer semantics; the corpus semantic check
    // skips this shape). What must hold is idempotency: fmt(fmt(x)) == fmt(x).
    let src = "- a\n  - b\n\n  - c\n";
    assert_eq!(fmt(&fmt(src)), fmt(src), "142 lost idempotency");
}
