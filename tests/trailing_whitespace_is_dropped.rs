//! PART 2 NO TRAILING WHITESPACE (carve#926):
//!
//! > A `whitespace` run at the END of a CONTENT LINE is DROPPED. It does not
//! > reach the output, and it is not content.
//!
//! The maintainer's reasoning, recorded because it generalizes: *trailing
//! (invisible and bad) whitespace is the one important rule we have: no such
//! thing.*
//!
//! TWO THINGS ARE NEW FOR AN ENGINE. The rule was already written down for a
//! paragraph's FINAL line and this engine implemented that much.
//!
//! 1. It holds on EVERY content line, including one before a SOFT BREAK, so
//!    `abc<newline>def` and `abc<SP><newline>def` are the same document.
//!    PART 12 §7 asserted the OPPOSITE and has been corrected: it claimed `a` +
//!    SPACE + newline + `b` renders `<p>a \nb</p>` and argued from that claim
//!    that stripping breaks `to_html(fmt(x)) == to_html(x)`. carve-rs#359
//!    limited stripping to block-final lines for that reason - a correct
//!    response to a PARSER that kept the run, and the parser is the half that
//!    moves here.
//! 2. Every content line, not just a paragraph's: a heading, a list item, a
//!    block quote line, a definition term and description, a footnote body
//!    line, a table CAPTION and a line-block line. The executable spec itself
//!    was missing the caption and the line-block cases until this was measured.
//!
//! THE RUN IS `whitespace`, AND NOTHING ELSE IS. Only U+0020 and U+0009 - the
//! same two-character terminal `blank_line` and `indent` take (carve#890).
//! Every other character is CONTENT and survives, however invisible. An
//! implementation that strips with a Unicode whitespace property (or a
//! language's legacy `\s`) fails seven of the nine rows below, and a
//! plain-space fixture cannot see it. This is why U+FEFF was a red herring in
//! the shape that raised the ticket: in `<SP>U+FEFF<SP>` the BOM is content and
//! what is dropped is the trailing SPACE.

use carve::to_html;

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ---------------------------------------------------------------------------
// (1) every line, including one before a soft break
// ---------------------------------------------------------------------------

#[test]
fn a_run_before_a_soft_break_is_dropped() {
    assert_eq!(to_html("abc \ndef\n").trim(), "<p>abc\ndef</p>");
    assert_eq!(to_html("abc  \ndef\n").trim(), "<p>abc\ndef</p>");
}

#[test]
fn a_tab_before_a_soft_break_is_dropped() {
    assert_eq!(to_html("abc\t\ndef\t\n").trim(), "<p>abc\ndef</p>");
}

#[test]
fn the_two_documents_are_the_same_document() {
    assert_eq!(to_html("abc \ndef\n"), to_html("abc\ndef\n"));
    assert_eq!(to_html("abc\t\ndef\n"), to_html("abc\ndef\n"));
}

// ---------------------------------------------------------------------------
// (2) every context, not just a paragraph
// ---------------------------------------------------------------------------

#[test]
fn a_heading_a_list_item_and_a_quote_line_drop_it() {
    assert_eq!(
        squash(&to_html("# Title \n\n- item \n\n> quoted \n")),
        squash(
            "<section id=\"Title\">\n  <h1>Title</h1>\n  <ul>\n    <li>item</li>\n  </ul>\n  \
             <blockquote><p>quoted</p></blockquote>\n</section>"
        )
    );
}

#[test]
fn a_definition_entry_drops_it() {
    assert_eq!(
        squash(&to_html(":: term \n:  def \n")),
        "<dl> <dt>term</dt> <dd>def</dd> </dl>"
    );
}

#[test]
fn a_table_caption_drops_it() {
    let html = to_html("| a |\n^ Cap \n");
    assert!(html.contains("<caption>Cap</caption>"), "{html}");
}

#[test]
fn a_table_caption_drops_it_on_a_folded_line_too() {
    // The caption is one of the two contexts that needed its own producer: it
    // stripped only its FINAL line, so a folded caption kept the run before its
    // soft break. Asserted narrowly on that run - how many lines a caption folds
    // is a different question, and one where this engine and the executable spec
    // do not currently agree.
    let html = to_html("| a |\n^ Cap \n  more \n");
    assert!(
        !html.contains("Cap \n"),
        "the folded caption kept the run: {html}"
    );
    assert!(html.contains("Cap"), "{html}");
}

#[test]
fn a_block_quote_and_a_footnote_body_drop_it_before_a_soft_break() {
    assert_eq!(
        squash(&to_html("> q \n> r\n")),
        "<blockquote><p>q r</p></blockquote>"
    );
    let html = to_html("x[^f]\n\n[^f]: body \n  cont \n");
    assert!(html.contains("<p>body\ncont<a href=\"#fnref1\""), "{html}");
}

#[test]
fn a_line_block_drops_a_one_column_trailing_gap() {
    // ORDER MATTERS, and it is the whole of this case. PART 9 §23 converts an
    // inner or trailing run of TWO OR MORE columns into NBSP CONTENT first, and
    // content is not whitespace - so the rule never reaches it. What it does
    // reach is §23's ONE-column case.
    assert_eq!(
        squash(&to_html("::: |\nabc  \ndef \n:::\n")),
        squash("<div class=\"line-block\">\n  <p>abc&nbsp;&nbsp;<br>\ndef</p>\n</div>")
    );
}

// ---------------------------------------------------------------------------
// THE CHARACTER CLASS: two dropped, seven kept
// ---------------------------------------------------------------------------

#[test]
fn only_u0020_and_u0009_are_dropped() {
    // Every other invisible character is CONTENT and survives. A Unicode
    // whitespace property fails seven of these.
    let html = to_html("a\u{a0}\nb\u{200b}\nc\u{feff}\nd\u{2000}\ne\u{c}\n");
    assert!(html.contains("a&nbsp;\n"), "U+00A0 NO-BREAK SPACE: {html}");
    assert!(
        html.contains("b\u{200b}\n"),
        "U+200B ZERO WIDTH SPACE: {html}"
    );
    assert!(
        html.contains("c\u{feff}\n"),
        "U+FEFF BYTE ORDER MARK: {html}"
    );
    assert!(html.contains("d\u{2000}\n"), "U+2000 EN QUAD: {html}");
    assert!(html.contains("e\u{c}"), "U+000C FORM FEED: {html}");

    let html = to_html("a\u{3000}\nb\u{b}\n");
    assert!(
        html.contains("a\u{3000}\n"),
        "U+3000 IDEOGRAPHIC SPACE: {html}"
    );
    assert!(html.contains("b\u{b}"), "U+000B VERTICAL TAB: {html}");
}

#[test]
fn a_byte_order_mark_between_two_spaces_keeps_the_mark() {
    // The shape that raised the ticket. The BOM is content; what is dropped is
    // the trailing SPACE. (The LEADING space is indentation, which a paragraph
    // has always ignored.)
    assert_eq!(to_html(" \u{feff} \n").trim(), "<p>\u{feff}</p>");
}

// ---------------------------------------------------------------------------
// CONTROLS: where the rule does not reach
// ---------------------------------------------------------------------------

#[test]
fn control_a_verbatim_payload_keeps_its_bytes() {
    assert_eq!(
        to_html("```\nabc \n```\n").trim(),
        "<pre><code>abc \n</code></pre>"
    );
    assert_eq!(
        to_html("~~~\nabc \n~~~\n").trim(),
        "<pre><code>abc \n</code></pre>"
    );
}

#[test]
fn control_whitespace_inside_a_construct_is_not_at_a_line_end() {
    // A code span and a literal inline end at their own delimiter rather than
    // at the line's end, so the run in front of that delimiter is interior.
    assert_eq!(
        to_html("`x ` and !`y `\n").trim(),
        "<p><code>x </code> and y </p>"
    );
}

#[test]
fn control_a_hard_break_backslash_keeps_the_run_in_front_of_it() {
    // The line ends in a BACKSLASH, which is content - so it does not end in
    // whitespace at all and there is nothing to drop.
    assert_eq!(to_html("a \\\nb\n").trim(), "<p>a <br>\nb</p>");
}

#[test]
fn control_a_line_block_keeps_a_two_column_trailing_gap() {
    let html = to_html("::: |\nabc  \ndef\n:::\n");
    assert!(html.contains("abc&nbsp;&nbsp;<br>"), "{html}");
}

#[test]
fn control_a_table_cell_pads_with_spaces_that_are_not_at_a_line_end() {
    assert!(
        to_html("| a  |  b |\n").contains("<td>a</td>"),
        "cell padding is the cell's own rule"
    );
}
