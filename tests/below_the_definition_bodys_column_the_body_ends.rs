//! BELOW THE BODY'S COLUMN THE BODY ENDS (`definition_indent`,
//! markup-carve/carve#932). The three bands, in column order:
//!
//!   - BELOW the body's content column, the body ENDS and the line is
//!     classified in the surviving context.
//!   - AT the column, the line is the body's own block content, so a block
//!     opener opens inside the `dd`.
//!   - PAST the column, a recognized opener establishes an authored local base
//!     (markup-carve/carve#1729), while ordinary text remains lazy continuation.
//!
//! `definition_indent` states the floor as column arithmetic; this states what
//! happens on the other side of it. Every case below is a `:: t` / `:  body`
//! entry with one more line at a named column, so the file reads as the ladder
//! the clause describes and each boundary is pinned on both sides.
//!
//! WHAT THIS FIXES. The fold never looked at indentation - carve-rs#734 recorded
//! exactly that when it labelled the no-blank shape a control - so BELOW and PAST
//! produced the same bytes and the floor of three columns was unobservable on
//! this side of it. Three bands were two.
//!
//! WHY THE SURVIVING CONTEXT ANSWERS IT THIS WAY. `lazy_continuation_line` is
//! spelled as "a FLUSH-LEFT line with no blank before it", so an indented line
//! is not one; and at document level PART 2's COLUMN-EXACT DELIMITERS makes an
//! indented block opener plain text ("an indented heading, thematic break, or
//! block quote is likewise plain text"). This engine's FOOTNOTE body, which the
//! clause names as the precedent for what "below the content column" means,
//! already answers exactly this way, and it is asserted beside the definition
//! rather than cited.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

fn entry(indent: usize, line: &str) -> String {
    format!(":: t\n:  body\n{}{}\n", " ".repeat(indent), line)
}

const TIGHT: &str = "<dl>\n  <dt>t</dt>\n  <dd>body</dd>\n</dl>";

// ---------------------------------------------------------------------------
// BELOW: the body ends, and the surviving context is the document.
// ---------------------------------------------------------------------------

/// Column 0 is not a special case of this, it is the ordinary case: the body
/// ends here too, and the different output comes from the opener test rather
/// than from a branch on the column. At 0 a quote marker IS an opener.
#[test]
fn at_column_zero_the_body_ends_and_the_quote_opens_at_document_level() {
    assert_eq!(
        html(&entry(0, "> q")),
        format!("{TIGHT}\n<blockquote><p>q</p></blockquote>")
    );
}

/// One and two columns: the body ends the same way, and the residue is an
/// INDENTED block opener at document level, which is plain text.
#[test]
fn below_the_column_the_body_ends_and_the_line_is_a_document_level_paragraph() {
    for indent in [1, 2] {
        assert_eq!(
            html(&entry(indent, "> q")),
            format!("{TIGHT}\n<p>&gt; q</p>"),
            "indent {indent}"
        );
    }
}

/// Plain prose below the column takes the same route: the body ends and the line
/// is a paragraph of its own, rather than lazy text inside the `dd`.
#[test]
fn plain_text_below_the_column_ends_the_body_too() {
    for indent in [1, 2] {
        assert_eq!(
            html(&entry(indent, "x")),
            format!("{TIGHT}\n<p>x</p>"),
            "indent {indent}"
        );
    }
}

/// The precedent, asserted. A footnote body answers the same way at the same two
/// columns - the body ends and the line is classified at document level - which
/// is what the clause means by "the same thing here as it does for a list item
/// and for a footnote body".
#[test]
fn the_footnote_body_answers_the_same_way() {
    // A footnote body's own column is 2, so BELOW it is column 1 - the band, not
    // the number, is what carries over.
    let src = "[^f]: body\n > q\n\nt[^f]\n";
    assert!(
        carve::to_html(src).starts_with("<p>&gt; q</p>"),
        "{}",
        carve::to_html(src)
    );
    // AT its column the quote opens inside the note, the same shape the
    // definition gives at 3.
    assert!(
        carve::to_html("[^f]: body\n  > q\n\nt[^f]\n")
            .contains("<blockquote><p>q</p></blockquote>"),
        "at the footnote body's own column the quote opens inside it"
    );
    assert!(
        carve::to_html("[^f]: body\n> q\n\nt[^f]\n")
            .starts_with("<blockquote><p>q</p></blockquote>"),
        "column 0 opens the quote for the footnote body too"
    );
}

// ---------------------------------------------------------------------------
// AT and PAST: the two bands above the floor, unchanged, and the boundaries.
// ---------------------------------------------------------------------------

/// AT the column the line is the body's own block content, so the quote opens
/// INSIDE the `dd`. This is one side of the BELOW/AT boundary; the two-column
/// case above is the other.
#[test]
fn at_the_column_the_block_opener_opens_inside_the_description() {
    assert_eq!(
        html(&entry(3, "> q")),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>body</p>\n    <blockquote><p>q</p></blockquote>\n  </dd>\n</dl>"
    );
}

/// PAST the minimum column a recognized opener establishes an authored block
/// base (markup-carve/carve#1729). This is the other boundary, and it is what
/// BELOW must not collapse into.
#[test]
fn past_the_column_the_line_opens_at_its_authored_base() {
    assert_eq!(
        html(&entry(4, "> q")),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>body</p>\n    <blockquote><p>q</p></blockquote>\n  </dd>\n</dl>"
    );
}

/// The two bands must not produce the same bytes. Stated as its own assertion
/// because collapsing them is precisely the state this clause found: the fold
/// never looked at indentation, so one band answered for both.
#[test]
fn below_and_past_are_not_the_same_band() {
    assert_ne!(html(&entry(2, "> q")), html(&entry(4, "> q")));
}

// ---------------------------------------------------------------------------
// Controls.
// ---------------------------------------------------------------------------

/// A FLUSH-LEFT line still folds. `lazy_continuation_line` is spelled as a
/// flush-left line, every reader agrees at zero, and a fix that read "indented at
/// all" as "below the column" would take this with it.
#[test]
fn control_a_flush_left_plain_line_still_folds() {
    assert_eq!(
        html(":: t\n:  body\nx\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>body\nx</dd>\n</dl>"
    );
}

/// A blank line then an indented block is FORM A and reaches the body at the
/// column, which is a different branch entirely and must not move.
#[test]
fn control_form_a_still_folds_a_post_blank_indented_block() {
    assert_eq!(
        html(":: t\n:  body\n\n   second\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>body</p>\n    <p>second</p>\n  </dd>\n</dl>"
    );
}

/// A lone `+` attaches the FOLLOWING flush-left block with no indentation at all
/// (FORM B), so the column test must not reach it.
#[test]
fn control_the_pull_left_form_still_attaches_its_block() {
    assert_eq!(
        html(":: t\n:  body\n+\nsecond\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>body</p>\n    <p>second</p>\n  </dd>\n</dl>"
    );
}

/// A `:  ` marker below the column is a new DESCRIPTION on the same term, not a
/// line the column test disposes of - the outer loop picks it up.
#[test]
fn control_a_marker_below_the_column_still_opens_the_next_description() {
    assert_eq!(
        html(":: t\n:  body\n:  second\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>body</dd>\n  <dd>second</dd>\n</dl>"
    );
}
