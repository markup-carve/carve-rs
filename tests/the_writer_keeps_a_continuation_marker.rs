//! The writer keeps a `+` continuation marker before an attached paragraph
//! (carve#861).
//!
//! §17 L3 attaches the following block to the item, so `- a` / `+` / `b` is an
//! item holding TWO blocks. The writer dropped the marker and indented `b`,
//! which re-parses as a LAZY CONTINUATION of the paragraph above it (§10 I2) -
//! one block where the author wrote two, so PART 11 §1's
//! `to_html(fmt(x)) == to_html(x)` failed.
//!
//! A PARAGRAPH is the only attached kind this reaches. A fence, quote, heading,
//! table, div or thematic break cannot fold into an open paragraph, so
//! indenting them into the item is a different spelling of the same document.
//! The corpus pinned exactly those harmless kinds, which is why nothing caught
//! it and why all three engines share the defect rather than diverging.

use carve::{to_carve, to_html};

fn round_trips(source: &str) -> bool {
    to_html(&to_carve(source)) == to_html(source)
}

#[test]
fn a_paragraph_attached_at_the_top_level_survives_fmt() {
    assert!(
        round_trips("- a\n+\nb\n\nx\n"),
        "{}",
        to_carve("- a\n+\nb\n\nx\n")
    );
}

#[test]
fn a_paragraph_attached_inside_a_nested_item_survives_fmt() {
    // The marker sits at the ITEM's marker column, which is not column 0 here.
    let source = "- o\n  - a\n  +\n  b\n\nx\n";

    assert!(round_trips(source), "{}", to_carve(source));
}

#[test]
fn the_marker_is_written_back_rather_than_the_text_indented() {
    // The bytes, because the assertions above pass for any spelling whose HTML
    // happens to match - and the point is that the marker survives.
    let written = to_carve("- a\n+\nb\n\nx\n");

    assert!(written.contains("\n+\n"), "{written}");
}

#[test]
fn the_written_form_is_a_fixed_point() {
    let once = to_carve("- a\n+\nb\n\nx\n");

    assert_eq!(to_carve(&once), once);
}

#[test]
fn the_attached_kinds_that_never_folded_are_left_alone() {
    // The control. These already round-tripped by indenting the block into the
    // item, and a fix that emitted `+` everywhere would change all of them.
    for block in ["```\nb\n```", "> b", "# b", "::: note\nb\n:::", "---"] {
        let source = format!("- a\n+\n{block}\n\nx\n");
        assert!(round_trips(&source), "{source}");
        assert!(!to_carve(&source).contains("\n+\n"), "{source}");
    }
}

#[test]
fn a_loose_two_paragraph_item_is_left_alone() {
    // The boundary: a LOOSE item separates its blocks with a blank line and
    // needs no marker. Emitting one would change the item's looseness.
    let source = "- a\n\n  b\n\nx\n";

    assert!(round_trips(source), "{}", to_carve(source));
    assert!(!to_carve(source).contains("\n+\n"));
}
