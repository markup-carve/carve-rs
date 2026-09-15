//! carve-rs#1615, the third rule in this writer after the padding family
//! (`markdown_emphasis_pads_outside_the_delimiters`).
//!
//! A delimiter run only CLOSES emphasis while it is right-flanking and only
//! OPENS it while it is left-flanking (CommonMark 6.2). A run whose inner
//! neighbour is punctuation needs an outer neighbour that is whitespace or
//! punctuation; against an alphanumeric it can do neither, and the emphasis
//! reaches the reader as literal text. Unlike the padding family this needs no
//! padding at all - what decides it is the SIBLING across the seam, so it is
//! answered where the siblings are joined and nowhere else.
//!
//! Measured over a sweep of nine inline kinds crossed with twelve content
//! shapes and twelve seam contexts (1296 rows), read back with markdown-it-py
//! 3.0.0 (`commonmark` preset, `strikethrough` enabled) and compared to this
//! engine's own HTML: 233 rows read back wrong before, 83 after, and every row
//! still wrong is the ADJACENCY defect carve-rs#1619 rather than a flanking
//! one.

/// What a CommonMark reader makes of the Markdown this renderer wrote. The
/// importer is pulldown-cmark, so this is the ecosystem's own reading, not a
/// second opinion from the engine that produced the bytes.
fn readback(source: &str) -> String {
    let md = carve::to_markdown(source);
    carve::render_html(&carve::markdown_to_ast(&md)).expect("the reader renders")
}

// ---------------------------------------------------------------------------
// A closing run that cannot close.
// ---------------------------------------------------------------------------

#[test]
fn re_spells_strong_when_the_content_ends_in_punctuation_and_a_letter_follows() {
    assert_eq!(
        carve::to_markdown("a {*x!*}b\n"),
        "a <strong>x!</strong>b\n"
    );
}

#[test]
fn the_strong_that_could_not_close_reads_back_as_strong() {
    let back = readback("a {*x!*}b\n");
    assert!(back.contains("<strong>x!</strong>"), "got: {back}");
}

#[test]
fn re_spells_emphasis_in_the_same_seam() {
    assert_eq!(carve::to_markdown("a {/x!/}b\n"), "a <em>x!</em>b\n");
}

#[test]
fn re_spells_strike_in_the_same_seam() {
    assert_eq!(carve::to_markdown("a {~x!~}b\n"), "a <del>x!</del>b\n");
}

#[test]
fn re_spells_bold_italic_in_the_same_seam() {
    // `/*x!*/` is a BoldItalic NODE, which carve-js and carve-php do not have,
    // so the `***` run is this engine's own row. `{*/x!/*}` would NOT do: that
    // is a Strong holding an Italic, and it would pass on the Strong arm alone.
    assert_eq!(
        carve::to_markdown("a /*x!*/b\n"),
        "a <strong><em>x!</em></strong>b\n"
    );
}

#[test]
fn a_bold_italic_run_that_can_flank_keeps_its_triple() {
    assert_eq!(carve::to_markdown("a /*x!*/ b\n"), "a ***x!*** b\n");
}

#[test]
fn answers_the_same_way_for_a_digit_across_the_seam() {
    assert_eq!(
        carve::to_markdown("a {*x!*}1\n"),
        "a <strong>x!</strong>1\n"
    );
}

#[test]
fn answers_the_same_way_for_a_closing_bracket_as_the_inner_neighbour() {
    assert_eq!(
        carve::to_markdown("a {*x)*}b\n"),
        "a <strong>x)</strong>b\n"
    );
}

// ---------------------------------------------------------------------------
// An opening run that cannot open.
// ---------------------------------------------------------------------------

#[test]
fn re_spells_strong_when_the_content_starts_with_punctuation_after_a_letter() {
    assert_eq!(carve::to_markdown("a{*!x*}b\n"), "a<strong>!x</strong>b\n");
}

#[test]
fn re_spells_emphasis_on_the_opening_side() {
    assert_eq!(carve::to_markdown("a{/!x/}b\n"), "a<em>!x</em>b\n");
}

#[test]
fn re_spells_strike_on_the_opening_side() {
    assert_eq!(carve::to_markdown("a{~!x~}b\n"), "a<del>!x</del>b\n");
}

// ---------------------------------------------------------------------------
// The carriers. `_`, `#` and `[` travel as private-use code points until
// `resolve_narrowed_escapes` runs, and no punctuation property matches those.
// A flanking test that asked about the carrier instead of the character it
// stands for would read these three as ordinary letters and leave the dead run
// in.
// ---------------------------------------------------------------------------

#[test]
fn sees_the_underscore_behind_its_carrier() {
    assert_eq!(
        carve::to_markdown("a {*x_*}b\n"),
        "a <strong>x_</strong>b\n"
    );
}

#[test]
fn sees_the_hash_behind_its_carrier() {
    assert_eq!(
        carve::to_markdown("a {*x#*}b\n"),
        "a <strong>x#</strong>b\n"
    );
}

#[test]
fn sees_the_bracket_behind_its_carrier() {
    assert_eq!(
        carve::to_markdown("a {*x[*}b\n"),
        "a <strong>x[</strong>b\n"
    );
}

// ---------------------------------------------------------------------------
// The punctuation class is CommonMark 0.31's - ASCII punctuation plus Unicode
// P* AND S*. 0.30 left the symbol categories out, so the two versions disagree
// about `©`: a 0.30 reader closes the run, a 0.31 reader does not. Taking the
// wider class is the answer that is right under both, because inline HTML reads
// the same way everywhere.
// ---------------------------------------------------------------------------

#[test]
fn treats_a_symbol_as_punctuation_which_the_p_category_alone_would_not() {
    assert_eq!(
        carve::to_markdown("a {*x\u{a9}*}b\n"),
        "a <strong>x\u{a9}</strong>b\n"
    );
}

// ---------------------------------------------------------------------------
// Runs that CAN flank keep their delimiters. Inline HTML is heavier and less
// portable, so it is taken only where the delimiter form does not survive the
// reader.
// ---------------------------------------------------------------------------

#[test]
fn keeps_the_delimiter_when_whitespace_follows_the_closing_run() {
    assert_eq!(carve::to_markdown("a {*x!*} b\n"), "a **x!** b\n");
}

#[test]
fn keeps_the_delimiter_when_the_content_is_alphanumeric_at_both_edges() {
    assert_eq!(carve::to_markdown("a {*x*}b\n"), "a **x**b\n");
}

#[test]
fn keeps_the_delimiter_for_an_ordinary_run_between_two_spaces() {
    assert_eq!(carve::to_markdown("x {*y*} z\n"), "x **y** z\n");
}

#[test]
fn reads_the_neighbour_off_the_padding_the_run_moved_outside_itself() {
    // `pad_outside` has already moved the trailing space out of the run, so the
    // character across the closing seam is that space and not the sibling `b`
    // behind it. Asking the sibling instead would re-spell a run that flanks.
    assert_eq!(carve::to_markdown("a {*x! *}b\n"), "a **x!** b\n");
}

#[test]
fn reads_the_opening_neighbour_off_the_padding_too() {
    assert_eq!(carve::to_markdown("a{* !x*}b\n"), "a **!x**b\n");
}

#[test]
fn keeps_the_delimiter_when_the_outer_neighbour_is_punctuation() {
    // Punctuation on BOTH sides of the seam flanks, so the run stands.
    assert_eq!(carve::to_markdown("a {*x!*}.\n"), "a **x!**.\n");
}

// ---------------------------------------------------------------------------
// No neighbour at all counts as whitespace. The part is first or last in its
// paragraph, so the character across the seam is a line start or a newline, and
// both of those flank.
// ---------------------------------------------------------------------------

#[test]
fn an_absent_opening_neighbour_flanks() {
    // Nothing precedes the run, so the leading `!` has the line start across
    // from it. Reading an absent neighbour as non-flanking would re-spell a run
    // a CommonMark reader opens without complaint.
    assert_eq!(carve::to_markdown("{*!x*}b\n"), "**!x**b\n");
}

#[test]
fn an_absent_closing_neighbour_flanks() {
    assert_eq!(carve::to_markdown("a{*x!*}\n"), "a**x!**\n");
}

// ---------------------------------------------------------------------------
// The inlines this writer already spells as inline HTML carry no flanking
// question at all. These rows are SCOPE GUARDS: they pin the sweep's finding
// that only the four delimiter-run kinds move. No mutation of the pass can
// redden them, because the pass never reaches a part that is not a delimiter
// run - they would go red only if one of these kinds were given a delimiter
// spelling, which is the change they exist to catch.
// ---------------------------------------------------------------------------

#[test]
fn leaves_underline_alone() {
    assert_eq!(carve::to_markdown("a {_x!_}b\n"), "a <u>x!</u>b\n");
}

#[test]
fn leaves_highlight_alone() {
    assert_eq!(carve::to_markdown("a {=x!=}b\n"), "a <mark>x!</mark>b\n");
}

#[test]
fn leaves_superscript_alone() {
    assert_eq!(carve::to_markdown("a {^x!^}b\n"), "a <sup>x!</sup>b\n");
}

// ---------------------------------------------------------------------------
// A run alone in its paragraph has no sibling on either side, and a line start
// and a newline both flank, so the delimiter form stands.
// ---------------------------------------------------------------------------

#[test]
fn a_lone_run_at_the_paragraph_edges_keeps_its_delimiters() {
    assert_eq!(carve::to_markdown("{*x!*}\n"), "**x!**\n");
}
