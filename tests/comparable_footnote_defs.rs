//! The minimal/conservative comparison (PART 11 section 4 W3) has to see the
//! WHOLE document, and footnote definitions are not in `children` -- they hang
//! off the document in their own map.
//!
//! Leaving them un-normalized meant any escape inside one made the two renders
//! differ, so W4 escalated the whole document to conservative. The paragraph
//! that got over-escaped did not even contain the escape that caused it
//! (carve#352, corpus 22-footnotes).

#[test]
fn a_footnote_definition_does_not_escalate_the_document() {
    // Identical paragraph, with and without a definition beside it.
    assert_eq!(carve::to_carve("a.\n"), "a.\n");
    assert_eq!(carve::to_carve("a.\n\n[^f]: b.\n"), "a.\n\n[^f]: b.\n");
}

#[test]
fn the_definition_itself_is_minimally_escaped() {
    let src = "Carve has footnotes.[^fn]\n\n[^fn]: Defined anywhere; resolved by label.\n";
    assert_eq!(carve::to_carve(src), src);
}

#[test]
fn an_escape_that_is_needed_still_survives_in_a_definition() {
    // `--` would re-derive as an en dash, so the escape is load-bearing and the
    // writer must keep it -- the fix must not turn escalation off wholesale.
    let src = "a.\n\n[^f]: literal \\-\\- dashes.\n";
    let out = carve::to_carve(src);
    assert!(out.contains("\\-\\-"), "escape was dropped: {out:?}");
    assert_eq!(carve::to_html(&out), carve::to_html(src));
    assert_eq!(carve::to_carve(&out), out, "fmt is not idempotent");
}
