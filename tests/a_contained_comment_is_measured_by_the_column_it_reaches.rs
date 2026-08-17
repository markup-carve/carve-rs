//! A contained comment fence is measured by the column it REACHES
//! (carve-rs#1054).
//!
//! The definition pre-passes asked one question - "is the fence at or past the
//! innermost live content column?" - where the honest question is which live
//! column the fence actually reaches, and which column the container it sits in
//! actually holds. Two shapes fell out of the gap, and in both the block parser
//! hid the comment while the pre-pass walked into it and registered a
//! definition: invisible in the document and live in the link table at once,
//! which is the outcome `resources/examples/edge-cases.md` rules out under
//! "A definition inside a comment registers nothing".
//!
//! Both are the same defect the corpus fixtures `335`-`341` pin, reached through
//! a column the pre-pass measured wrongly, and neither is pinned by them.

use carve::to_html;

/// `[r][]` unresolved and the item empty: the comment registered nothing.
fn assert_literal(source: &str, expected_item: &str) {
    let out = to_html(source);
    assert!(
        out.contains("<p>[r][]</p>"),
        "the reference resolved: {out}"
    );
    assert!(out.contains(expected_item), "item shape moved: {out}");
}

#[test]
fn an_opener_past_the_content_column_with_a_body_back_at_it() {
    // The fence is at column 4 and its body at 2, which is still inside the
    // item. The span's bound ends the container at the first line that dedents
    // below it; measured from the DELIMITER's column it read that legal body
    // line as the end of the container and declined the fence. Measured from
    // the column the container holds, the body is inside it.
    assert_literal(
        "- item\n    %%%\n  [r]: /u\n    %%%\n\n[r][]\n",
        "<li>item</li>",
    );
}

#[test]
fn a_fence_at_the_outer_column_of_a_line_that_opened_two_items() {
    // `- - inner` leaves BOTH content columns live, 2 and 4 (carve#655). A
    // fence at 2 is the outer item's, but the gate compared it against the
    // INNERMOST column and declined it while the outer item was still open.
    assert_literal(
        "- - inner\n  %%%\n  [r]: /u\n  %%%\n\n[r][]\n",
        "<li>inner</li>",
    );
}

#[test]
fn the_inner_column_of_that_same_line_was_already_right() {
    // The control from the report: this half already matched the oracle before
    // the fix, and is pinned so the change cannot be read as having moved it.
    assert_literal(
        "- - inner\n    %%%\n    [r]: /u\n    %%%\n\n[r][]\n",
        "<li>inner</li>",
    );
}

#[test]
fn a_footnote_definition_is_gated_by_the_same_two_columns() {
    // Both pre-passes share the columns, so both shapes have a footnote twin.
    for source in [
        "- item\n    %%%\n  [^f]: n\n    %%%\n\nx[^f]\n",
        "- - inner\n  %%%\n  [^f]: n\n  %%%\n\nx[^f]\n",
    ] {
        let out = to_html(source);
        assert!(!out.contains("doc-endnotes"), "emitted an endnote: {out}");
    }
}

#[test]
fn a_fence_that_reaches_no_live_column_is_still_not_the_container_s() {
    // The §24 C3 gate that REMAINS. Below the item's content column the fence
    // reached no container, so it is not the item's comment and the definition
    // under it is not inside one - it registers, exactly as before.
    //
    // This is the direction over-suppression would have broken, and the one a
    // "the reference stopped resolving" reading of the fix would have hidden.
    let out = to_html("- item\n %%%\n[r]: /u\n %%%\n\n[r][]\n");
    assert!(
        out.contains("href=\"/u\""),
        "a definition outside the container stopped registering: {out}"
    );
}

#[test]
fn a_deeper_ladder_measures_every_column_it_holds() {
    // `- - - deep` leaves 2, 4 and 6 live. A fence at any of them is that
    // item's, and the definition inside it registers nothing.
    for col in ["  ", "    ", "      "] {
        let source = format!("- - - deep\n{col}%%%\n{col}[r]: /u\n{col}%%%\n\n[r][]\n");
        let out = to_html(&source);
        assert!(
            out.contains("<p>[r][]</p>"),
            "resolved at column {}: {out}",
            col.len()
        );
    }
}
