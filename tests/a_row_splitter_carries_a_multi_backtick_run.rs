//! A table row's cell splitter carries a verbatim run of ANY width, so the
//! pipes inside a `` `` ``-delimited run are content rather than separators
//! (markup-carve/carve#1284, corpus `328-...-4`).
//!
//! The splitter tracked "am I inside a run" with a PARITY TOGGLE flipped once
//! per backtick character. A run of two therefore opened and closed itself on
//! the spot, and the next pipe split the row.
//!
//! THE SIGNATURE IS THE TELL, and it is why the width sweep below is the test
//! rather than the ticket's single document: one backtick worked, two did not,
//! three worked again. Any fix that restores the two-backtick row without
//! restoring that pattern for every even width has not found the cause. A
//! verbatim run is opened by a run of N backticks and closed by a run of
//! EXACTLY N, which is the rule the inline parser already applies.
//!
//! THE FIX HAS TWO HALVES AND EACH IS PINNED SEPARATELY, which a single
//! mutation does not show. Consuming the whole run at once is what carries the
//! pipe; matching the closer's width EXACTLY is what keeps a narrower run
//! inside a wider one from ending it. Reverting to a per-character toggle turns
//! the first three tests red and leaves `a_shorter_run_inside_a_wider_one_...`
//! green; keeping the run but closing on any width does the reverse. Only both
//! mutations together cover the change, and the first mutation attempted here
//! consumed the run while toggling, which reproduced no defect at all.
//!
//! carve-php answers every row below correctly and was the oracle for them.
//! carve-js shares this defect at the revision measured (2026-08-17, `99af44b`).

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

fn one_cell(text: &str) -> String {
    format!("<table>\n  <tbody>\n    <tr><td>{text}</td></tr>\n  </tbody>\n</table>")
}

#[test]
fn a_double_backtick_run_carries_the_pipe() {
    // The corpus document. The run has no closer, so it reaches the end of the
    // cell and the pipe inside it is content.
    assert_eq!(html("| a ``b | c |\n"), one_cell("a <code>b | c</code>"));
}

#[test]
fn every_width_answers_the_same_way() {
    // The parity signature, swept. Widths one and three passed on the unfixed
    // engine; two and four did not. All four must agree now.
    for width in 1..=4 {
        let ticks = "`".repeat(width);
        assert_eq!(
            html(&format!("| a {ticks}b | c |\n")),
            one_cell("a <code>b | c</code>"),
            "a run of {width} backtick(s) did not carry the pipe"
        );
    }
}

#[test]
fn a_run_closed_by_its_own_width_stops_being_a_run() {
    // The other half of the exact-width rule: a matching closer ends the run,
    // and the pipe AFTER it separates again.
    assert_eq!(
        html("| a ``x|`` | b |\n"),
        "<table>\n  <tbody>\n    <tr><td>a <code>x|</code></td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn a_shorter_run_inside_a_wider_one_is_content() {
    // A run of a DIFFERENT width does not close the open one, so the pipe after
    // the inner backtick is still inside the run. A toggle answers this wrong in
    // the opposite direction.
    assert_eq!(
        html("| a ``b ` c | d |\n"),
        one_cell("a <code>b ` c | d</code>")
    );
}

// ---------------------------------------------------------------------------
// CONTROLS. Each passed before the fix and pins a row the change must not move.
// ---------------------------------------------------------------------------

#[test]
fn control_a_single_backtick_run_still_carries_the_pipe() {
    // The row the ticket's category already covered, and the one a parity
    // toggle happened to answer correctly.
    assert_eq!(html("| a `b | c d |\n"), one_cell("a <code>b | c d</code>"));
}

#[test]
fn control_a_closed_single_run_still_splits_after_itself() {
    assert_eq!(
        html("| a `x|` | b |\n"),
        "<table>\n  <tbody>\n    <tr><td>a <code>x|</code></td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_a_plain_row_still_splits() {
    assert_eq!(
        html("| a | b |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_an_escaped_pipe_still_does_not_split() {
    // The escape branch sits beside the backtick branch in the same scanner and
    // must be untouched by the rewrite.
    assert_eq!(
        html("| a \\| b | c |\n"),
        "<table>\n  <tbody>\n    <tr><td>a | b</td><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_a_header_row_is_unaffected() {
    assert_eq!(
        html("|= h |\n| a `b | c |\n"),
        "<table>\n  <thead><tr><th scope=\"col\">h</th></tr></thead>\n  <tbody>\n    <tr><td>a <code>b | c</code></td></tr>\n  </tbody>\n</table>"
    );
}
