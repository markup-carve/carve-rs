//! A `dt` written across two physical lines drops the trailing whitespace run
//! on its SECOND line, exactly as it already did on its first
//! (markup-carve/carve-rs#1029).
//!
//! Settled by the clause that already existed rather than by a new ruling:
//! `resources/examples/edge-cases.md`, "Trailing whitespace on a content line
//! is dropped", which markup-carve/carve#926 made general. A term's
//! continuation line is a content line and nothing exempts it. carve-js and
//! carve-php already answered this way; the corpus pair landed in
//! markup-carve/carve#1299.
//!
//! THE PAIR MATTERS MORE THAN EITHER CASE. The second test is the exception the
//! fix must not overshoot into: spaces INSIDE a verbatim run are the
//! construct's content and end at its closing delimiter, so an all-space
//! `` `  ` `` term keeps them. A fix applied one layer too late - after the
//! inline run is built, on the rendered text - passes the first and fails the
//! second. Stripping at the SOURCE layer, where the line ends, is what
//! separates them: the all-space run does not sit at the end of its line, the
//! closing backtick does.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn the_continuation_lines_trailing_space_is_dropped() {
    // Line 2 is `b ` - the trailing space is the whole assertion. The verbatim
    // run spans the break and carries a NEWLINE, not the dropped space.
    assert_eq!(
        html(":: `a\nb \n:  d\n"),
        "<dl>\n  <dt><code>a\nb</code></dt>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn control_an_all_space_verbatim_term_keeps_its_spaces() {
    // The overshoot guard. These spaces are the run's content, not a trailing
    // run on a content line, and they end at the closing backtick.
    assert_eq!(
        html(":: `  `\n:  d\n"),
        "<dl>\n  <dt><code>  </code></dt>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn a_consecutive_terms_continuation_line_drops_it_too() {
    // A run of `::` terms shares one entry, and the second term folds its lines
    // through a different statement than the first. Both had the defect, so
    // both are pinned - fixing only the first leaves this row keeping the space.
    assert_eq!(
        html(":: x\n:: `a\nb \n:  d\n"),
        "<dl>\n  <dt>x</dt>\n  <dt><code>a\nb</code></dt>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn control_the_terms_first_line_already_dropped_it() {
    // The rule this fix extends rather than invents: a single-line term's
    // trailing run never reached the output.
    assert_eq!(
        html(":: a \n:  d\n"),
        "<dl>\n  <dt>a</dt>\n  <dd>d</dd>\n</dl>"
    );
}

#[test]
fn control_a_paragraph_continuation_line_already_dropped_it() {
    // The ticket's control: the same two lines outside a term, where carve-rs
    // was already correct. The fix must not change this.
    assert_eq!(html("a `b\nc \n"), "<p>a <code>b\nc</code></p>");
}

#[test]
fn control_interior_whitespace_is_untouched() {
    // Only whitespace sitting at the END of a source line is stripped. Runs in
    // the middle of a line, and the break itself, are content.
    assert_eq!(
        html(":: a  b\nc  d\n:  e\n"),
        "<dl>\n  <dt>a  b\nc  d</dt>\n  <dd>e</dd>\n</dl>"
    );
}

#[test]
fn the_rule_is_the_clause_not_the_verbatim_run() {
    // The corpus pair (markup-carve/carve#1299) is spelled with a verbatim run,
    // and a fix that only ever trimmed inside a code span would pass it. The
    // clause is about a CONTENT LINE, so the plain-text term drops the run too.
    //
    // Measured 2026-08-17: carve-php answers this row the same way. carve-js
    // KEEPS the space here while dropping it in the verbatim row above, so it
    // passes the corpus pair through code-span trimming rather than through the
    // clause. That gap is carve-js's, and it is recorded here rather than
    // copied - this engine follows the clause on both rows.
    assert_eq!(
        html(":: a\nb \n:  d\n"),
        "<dl>\n  <dt>a\nb</dt>\n  <dd>d</dd>\n</dl>"
    );
}
