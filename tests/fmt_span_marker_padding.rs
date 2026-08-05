//! A lone table span marker keeps its padding.
//!
//! Glued to the opening pipe, `<` is also the LEFT-ALIGNMENT sigil, and the two
//! readings differ: the executable spec reads `|<|` as alignment on an empty
//! cell where all three engines read a colspan (markup-carve/carve#710). The
//! grammar defines `alignment_marker` as glued and lets `colspan_marker` carry
//! surrounding whitespace, so the padded form means the same thing to every
//! reader - and the writer was turning the unambiguous source into the
//! ambiguous one.

fn fmt(src: &str) -> String {
    carve::render_carve(&carve::parse(src)).expect("a table is far below the render ceiling")
}

fn html(src: &str) -> String {
    carve::render_html(&carve::parse(src)).expect("a table is far below the render ceiling")
}

#[test]
fn a_colspan_marker_is_not_glued_to_the_pipe() {
    let src = "| a | b |\n|---|---|\n| < | d |\n";
    assert!(fmt(src).contains("| < |"), "got: {:?}", fmt(src));
    assert!(!fmt(src).contains("|<|"), "got: {:?}", fmt(src));
}

#[test]
fn a_rowspan_marker_is_not_glued_either() {
    let src = "| a | b |\n|---|---|\n| ^ | d |\n";
    assert!(fmt(src).contains("| ^ |"), "got: {:?}", fmt(src));
    assert!(!fmt(src).contains("|^|"), "got: {:?}", fmt(src));
}

#[test]
fn a_glued_marker_in_the_source_is_written_back_padded() {
    // This engine reads the glued form as a span, so the document is a span
    // table either way; fmt canonicalizes it to the portable spelling.
    let src = "| a | b |\n|---|---|\n|<| d |\n";
    assert!(fmt(src).contains("| < |"), "got: {:?}", fmt(src));
}

#[test]
fn the_table_still_says_the_same_thing_after_formatting() {
    for src in [
        "| < | b |\n|---|---|\n| c | d |\n",
        "| a | b |\n|---|---|\n| ^ | d |\n",
        "| a | b | c |\n|---|---|---|\n| d | < | < |\n",
    ] {
        assert_eq!(html(&fmt(src)), html(src), "for {src:?}");
    }
}
