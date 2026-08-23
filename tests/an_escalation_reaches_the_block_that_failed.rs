//! PART 11 section 2b: the scope of an escalation is the smallest unit that
//! fails.
//!
//! Section 4's two-render strategy asks whether the minimal form of the WHOLE
//! document re-parses to the same tree, and until this clause landed the answer
//! decided the whole document: one character that genuinely needed its escape
//! put every other candidate into the conservative class with it. Section 2b
//! bounds the fallback to the smallest unit whose minimal form fails -- the
//! inline run, or the block containing it -- and every other unit is emitted by
//! section 2's own test, which for a character nothing needs means bare.
//!
//! WHY THE ASSERTIONS ARE ON BYTES. Section 1 forgives escaping on purpose: both
//! spellings render the same HTML and re-parse to the same tree, so a round trip
//! cannot see the difference and neither can the corpus HTML. That is exactly
//! why three engines carried the wider scope with every gate green
//! (markup-carve/carve#1516). The bytes are the only witness, so each case pins
//! them -- and then re-renders the written form to show the narrowing did not
//! buy the minimality by changing the document.

/// The written form, plus the proof it still says what the source said.
fn written(source: &str) -> String {
    let out = carve::to_carve(source);
    assert_eq!(
        carve::to_html(&out),
        carve::to_html(source),
        "fmt changed the document: {out:?}"
    );
    assert_eq!(
        carve::to_carve(&out),
        out,
        "the written form is not settled"
    );
    out
}

/// Indented, so the text IS `## H` rather than a heading. At column zero the
/// minimal form would open one, so this block escalates -- in full, by section
/// 2's THE UNIT IS THE OPENER: the run is `##`, not its first character.
#[test]
fn a_block_whose_minimal_form_opens_a_heading_it_does_not_have_escalates() {
    assert_eq!(written("  ## H\n"), "\\#\\# H\n");
}

#[test]
fn a_block_whose_minimal_form_re_parses_as_itself_is_left_alone() {
    assert_eq!(written("plain (b) text\n"), "plain (b) text\n");
}

/// Corpus 396 in markup-carve/carve#1516. Before section 2b the second paragraph
/// came back `plain \(b\) text`, escaped because a DIFFERENT block failed.
#[test]
fn the_escalation_does_not_spread_from_the_block_that_needed_it() {
    assert_eq!(
        written("  ## H\n\nplain (b) text\n"),
        "\\#\\# H\n\nplain (b) text\n"
    );
}

/// `/a/` is written braced, which puts `_b_` after a `}` instead of after a `/`
/// -- so the run that was TEXT on the way in would re-parse as emphasis, and it
/// escalates. The run after the code span is in the SAME paragraph and needs
/// nothing, so a fallback that stopped at the block would escape its parentheses
/// too.
///
/// WITHIN the run the escape is the OPENER's alone (section 2, per opener
/// occurrence): emphasis needs both delimiters, so the opening `_` escaped is
/// already the whole suppression and the closing one opens nothing on its own.
/// The unit-scoped form wrote `\\_b\\_` and the second backslash was idle
/// (markup-carve/carve#1533).
#[test]
fn the_inline_run_is_reached_before_the_block_containing_it() {
    assert_eq!(
        written("/a/_b_ `x` plain (d)\n"),
        "{/a/}\\_b_ `x` plain (d)\n"
    );
}

/// The failing occurrence is a `|` opening a table row, and it is a property of
/// the LINE the run begins rather than of the run: both lines of this one
/// paragraph carry one, so both are written conservatively while the paragraph
/// beside them keeps its bare candidates.
///
/// A ROW OPENS ON ITS LEADING PIPE, so that is the occurrence escaped and the
/// closing pipe stays bare - the block is what escalates, and section 2 still
/// decides each occurrence inside it (markup-carve/carve#1533).
#[test]
fn the_unit_widens_to_the_block_when_escaping_the_run_is_not_enough() {
    assert_eq!(
        written(" | a |\n | b |\n\nsee (c) 50% now\n"),
        "\\| a |\n\\| b |\n\nsee (c) 50% now\n"
    );
}

/// The conservative form is still reachable -- it is just arrived at because
/// each block needed it, rather than because one did.
#[test]
fn every_block_escalates_in_a_document_where_every_block_fails() {
    assert_eq!(written("  ## H\n\n  ### I\n"), "\\#\\# H\n\n\\#\\#\\# I\n");
}
