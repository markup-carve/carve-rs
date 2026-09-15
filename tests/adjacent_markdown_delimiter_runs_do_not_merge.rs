//! carve-rs#1619, the fourth rule in this writer, after the padding family and
//! the flanking pass (carve-rs#1615).
//!
//! Two adjacent inline runs written with the same character do not reach the
//! reader as two runs. They reach it as ONE, of the summed length. No flanking
//! test can see this - each piece IS well-flanked, against the other piece's
//! delimiter - so it is answered at the seam, about the run the reader will
//! actually see, using CommonMark's rule of three over the summed length.
//!
//! The severe face is a strike whose content begins with a tilde. `~~` plus
//! `~x` writes three tildes, and three tildes at the start of a line is a
//! FENCED CODE BLOCK opener: `~~~x~~` followed by the rest of the document
//! swallows all of it, in markdown-it-py AND in pulldown-cmark alike. That one
//! is decided where the run is built rather than at the seam, because the
//! content is inside the run, not beside it.
//!
//! Measured over nine inline kinds crossed with twelve content shapes and
//! fifteen seam contexts (1620 rows), each rendered through the Markdown target
//! and read back with BOTH markdown-it-py 3.0.0 (`commonmark` preset,
//! `strikethrough` enabled) and pulldown-cmark through this crate's own
//! importer: 278 rows wrong on `main`, 128 after carve-rs#1615, 0 after this.

/// What a CommonMark reader makes of the Markdown this renderer wrote.
fn readback(source: &str) -> String {
    let md = carve::to_markdown(source);
    carve::render_html(&carve::markdown_to_ast(&md)).expect("the reader renders")
}

// ---------------------------------------------------------------------------
// The severe face: the content's own tilde lengthens the run into a fence.
// ---------------------------------------------------------------------------

#[test]
fn a_strike_whose_content_opens_with_a_tilde_writes_no_fence() {
    assert_eq!(carve::to_markdown("{~~x~}\n"), "<del>~x</del>\n");
}

#[test]
fn the_rest_of_the_document_survives_that_strike() {
    // `~~~x~~` at the start of a line opened a fenced code block whose info
    // string was `x~~`, and everything after it became the block's content.
    let back = readback("{~~x~}\n\nA whole second paragraph.\n");
    assert!(!back.contains("<pre"), "the document was swallowed: {back}");
    assert!(back.contains("A whole second paragraph."), "got: {back}");
}

#[test]
fn a_strike_whose_content_closes_with_a_tilde_does_the_same() {
    assert_eq!(carve::to_markdown("{~x~~}\n"), "<del>x~</del>\n");
}

#[test]
fn the_tilde_stays_inside_the_strike_mid_paragraph_too() {
    let back = readback("a {~~x~}b\n");
    assert!(back.contains("<s>~x</s>"), "got: {back}");
}

/// CommonMark emphasis has no cap on run length, so a strong holding an italic
/// keeps its `***`. Extending the tilde rule to the asterisk would rewrite a
/// shape every reader already reads back.
#[test]
fn an_asterisk_run_is_allowed_to_grow_past_two() {
    assert_eq!(carve::to_markdown("{*/x/*}\n"), "***x***\n");
}

// ---------------------------------------------------------------------------
// The seam face: two runs of the same character meet.
// ---------------------------------------------------------------------------

#[test]
fn two_adjacent_emphases_do_not_collapse() {
    assert_eq!(carve::to_markdown("a {/x/}{/y/}\n"), "a *x*<em>y</em>\n");
}

#[test]
fn the_second_emphasis_reads_back_as_its_own_element() {
    let back = readback("a {/x/}{/y/}\n");
    assert!(back.contains("<em>x</em><em>y</em>"), "got: {back}");
}

#[test]
fn two_adjacent_strongs_do_not_collapse() {
    assert_eq!(
        carve::to_markdown("a {*x*}{*y*}\n"),
        "a **x**<strong>y</strong>\n"
    );
}

#[test]
fn two_adjacent_strikes_do_not_collapse() {
    // `~~x~~~~y~~` is where the two readers part company: markdown-it splits
    // the four tildes into two pairs, pulldown-cmark reads the lot as text.
    // Inline HTML is the answer both of them agree on.
    assert_eq!(
        carve::to_markdown("a {~x~}{~y~}\n"),
        "a ~~x~~<del>y</del>\n"
    );
}

#[test]
fn a_run_beside_a_literal_tilde_re_spells_the_run_instead() {
    // The part on the right is a text node, so there is no inline-HTML form for
    // it. The strike takes the fallback instead.
    assert_eq!(carve::to_markdown("a {~x~}~y\n"), "a <del>x</del>~y\n");
}

#[test]
fn only_the_merging_pair_moves_in_a_run_of_three() {
    // The middle emphasis is re-spelled, which ends the merge; the third run
    // now meets a `>` and keeps its delimiters.
    assert_eq!(
        carve::to_markdown("a {/x/}{/y/}{/z/}\n"),
        "a *x*<em>y</em>*z*\n"
    );
}

// ---------------------------------------------------------------------------
// The controls. A blanket "convert when two runs are adjacent" rule would
// rewrite these, and every one of them already reads back correctly in both
// readers. The rule of three is what tells them apart from the rows above.
// ---------------------------------------------------------------------------

#[test]
fn a_strong_beside_an_emphasis_keeps_both_delimiters() {
    // Runs of 2 and 1 sum to 3. Closing asks 2 against 3 and opening asks 3
    // against 1; neither sum is a multiple of three, so both matches stand.
    assert_eq!(carve::to_markdown("a {*x*}{/y/}\n"), "a **x***y*\n");
}

#[test]
fn an_emphasis_beside_a_strong_keeps_both_delimiters() {
    assert_eq!(carve::to_markdown("a {/x/}{*y*}\n"), "a *x***y**\n");
}

#[test]
fn a_bold_italic_beside_an_emphasis_keeps_both_delimiters() {
    // 3 and 1. The rule of three would block 3 against 6, but both are
    // multiples of three, which is the clause that exempts them.
    assert_eq!(carve::to_markdown("a /*x*/{/y/}\n"), "a ***x****y*\n");
}

#[test]
fn two_adjacent_bold_italics_keep_their_triples() {
    assert_eq!(carve::to_markdown("a /*x*//*y*/\n"), "a ***x******y***\n");
}

#[test]
fn a_strike_beside_an_emphasis_is_not_a_seam_at_all() {
    // Different characters, so nothing merges and nothing moves.
    assert_eq!(carve::to_markdown("a {~x~}{/y/}\n"), "a ~~x~~*y*\n");
}

/// A SCOPE GUARD rather than a row about this pass: no mutation of the seam
/// logic can redden it, because neither part ends or begins with a run
/// character. It pins the sweep's finding that the inline-HTML kinds do not
/// move, and would go red only if one of them were given a delimiter spelling.
#[test]
fn two_adjacent_deletes_are_inline_html_already() {
    assert_eq!(
        carve::to_markdown("a {-x-}{-y-}\n"),
        "a <del>x</del><del>y</del>\n"
    );
}

/// The second SCOPE GUARD: there is no seam here at all, so the pass cannot
/// reach it. It stands against a future rule that converted on a weaker test
/// than adjacency.
#[test]
fn a_lone_emphasis_keeps_its_delimiters() {
    assert_eq!(carve::to_markdown("a {/x/} b\n"), "a *x* b\n");
}

// ---------------------------------------------------------------------------
// Both controls and both defects read back the way the source does, through
// pulldown-cmark, which is the reader this crate ships.
// ---------------------------------------------------------------------------

#[test]
fn the_strong_emphasis_control_reads_back_as_two_elements() {
    let back = readback("a {*x*}{/y/}\n");
    assert!(back.contains("<strong>x</strong><em>y</em>"), "got: {back}");
}

#[test]
fn the_adjacent_strikes_read_back_as_two_elements() {
    // The importer reads the emitted `<del>` back as a strike node, so both
    // come out as `<s>`. Unfixed, pulldown-cmark read the whole seam as text
    // and the paragraph was `a <s>x~~~~y</s>`.
    let back = readback("a {~x~}{~y~}\n");
    assert!(back.contains("<s>x</s><s>y</s>"), "got: {back}");
}
