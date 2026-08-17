//! A verbatim run is opened by a run of N backticks and closed by a run of
//! EXACTLY N (PART 9 §22). The row splitter has applied that WITHIN a line since
//! `markup-carve/carve#1284`. Across a `+` continuation it did not: the carry
//! was a flag, and the continuation was re-seeded at ONE backtick.
//!
//! So the same scanner answered the width question two ways, one line apart. A
//! run opened with two was closed by a single backtick on the continuation row,
//! the pipe behind it split again, and the segment after it had no column to
//! join:
//!
//! ```text
//! | a ``b |
//! + c ` | d`` |
//! ```
//!
//! published `a <code>b c `</code>` - the `` d`` `` is simply gone. Category
//! 333's own prose names that outcome as the one it rules against: "Splitting it
//! with a fresh scanner cuts inside the run and leaves a segment with no column
//! to join, and a dropped segment is content loss rather than a second answer."
//! The length-insensitive carry reached it by a different route (carve-rs#1051).
//!
//! THE WIDTH SWEEP IS THE TEST, not the ticket's single document, for the reason
//! `a_row_splitter_carries_a_multi_backtick_run.rs` gives about parity: a fix
//! that restores the two-backtick row without carrying the width will still get
//! width three wrong, and the corpus pins only the width-one shape - where a
//! flag and a width are indistinguishable, which is exactly why this survived.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

fn one_cell(text: &str) -> String {
    format!("<table>\n  <tbody>\n    <tr><td>{text}</td></tr>\n  </tbody>\n</table>")
}

#[test]
fn a_narrower_closer_on_the_continuation_row_is_content() {
    // The ticket's document. The single backtick does not close a run of two, so
    // the pipe behind it is still inside the run and nothing is dropped.
    assert_eq!(
        html("| a ``b |\n+ c ` | d`` |\n"),
        one_cell("a <code>b c ` | d</code>")
    );
}

#[test]
fn every_width_carries_across_the_boundary() {
    // The signature, swept. Width one passed unfixed because a flag and a width
    // agree there; every wider run closed a row early. `n` backticks open on the
    // row, a run of `n - 1` sits on the continuation as CONTENT, and the run of
    // `n` after it closes.
    for n in 1..=5usize {
        let open = "`".repeat(n);
        let src = if n == 1 {
            // No narrower run exists to be content, so the row is just the carry.
            format!("| a {open}b |\n+ c | d{open} |\n")
        } else {
            let narrower = "`".repeat(n - 1);
            format!("| a {open}b |\n+ c {narrower} | d{open} |\n")
        };
        let expected = if n == 1 {
            one_cell("a <code>b c | d</code>")
        } else {
            let narrower = "`".repeat(n - 1);
            one_cell(&format!("a <code>b c {narrower} | d</code>"))
        };
        assert_eq!(html(&src), expected, "width {n}");
    }
}

#[test]
fn a_run_closed_at_its_own_width_stops_carrying() {
    // The other half, shown as a PAIR: the only difference between these two
    // documents is the width of the run on the continuation row, and it decides
    // whether the pipe behind it is content or a separator.
    //
    // A run of two closes the carry, so the pipe separates. The row itself is
    // one column wide - its own pipes were swallowed by the open run - so the
    // segment after the separator has no column to join and is dropped, which is
    // the carve#1293 rule for a continuation naming a column the row has not
    // got. carve-js publishes the same bytes.
    assert_eq!(
        html("| a ``b | x |\n+ c`` | d |\n"),
        one_cell("a <code>b | x c</code>")
    );
    // A run of one does not, so `| d` stays inside the verbatim run and reaches
    // the page. This is the row the fix is for.
    assert_eq!(
        html("| a ``b | x |\n+ c` | d |\n"),
        one_cell("a <code>b | x c` | d</code>")
    );
}

#[test]
fn the_width_survives_two_continuation_rows() {
    // The carry is per row, so a run left open by a continuation has to reach
    // the next one at the SAME width. A flag would have re-seeded at one on the
    // second row too.
    assert_eq!(
        html("| a ``b |\n+ c ` |\n+ d ` | e`` |\n"),
        one_cell("a <code>b c ` d ` | e</code>")
    );
}

#[test]
fn control_the_corpus_width_is_unmoved() {
    // `333-...-4`'s own shape, the one document that pinned this path. A single
    // backtick opens, the continuation's pipe is inside the run, and the closer
    // is the same width.
    assert_eq!(
        html("| a `b |\n+ c | d` |\n"),
        one_cell("a <code>b c | d</code>")
    );
}

#[test]
fn control_a_row_that_leaves_nothing_open_still_splits_its_continuation() {
    // No run crosses the boundary, so the continuation is scanned from outside
    // one and its pipe separates. This is the row that stops the fix from
    // reading as "a continuation is always inside a run".
    assert_eq!(
        html("| a | b |\n+ c | d |\n"),
        "<table>\n  <tbody>\n    <tr><td>a c</td><td>b d</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_the_carry_belongs_to_one_column() {
    // THE SEED BELONGS TO ONE COLUMN. The run was open in the row's LAST cell,
    // so the columns before it are scanned normally and their pipes still
    // separate - carrying the width must not widen the seed's reach.
    assert_eq!(
        html("| a | b ``c |\n+ x | y | z`` |\n"),
        "<table>\n  <tbody>\n    <tr><td>a x</td><td>b <code>c y | z</code></td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn the_width_reaches_a_column_the_continuation_walks_into() {
    // THE SEED IS APPLIED IN TWO PLACES and both need the width. The one above
    // seeds the scanner at the START of the line, for a run left open in the
    // FIRST column; this one seeds it mid-line, when the scan walks into the
    // column the row above left open. That second site was the surviving mutant
    // of the first pass: `control_the_carry_belongs_to_one_column` puts the
    // closer at end of line, where a wrongly-narrow seed leaves a run open to
    // the same place a correct one closes it, and the bytes agree by accident.
    //
    // Put a NARROWER run mid-column and the two answers separate. A single
    // backtick does not close the carried run of two, so the pipe behind it is
    // content:
    assert_eq!(
        html("| a | b ``c |\n+ x | y ` | z |\n"),
        "<table>\n  <tbody>\n    <tr><td>a x</td><td>b <code>c y ` | z</code></td></tr>\n  </tbody>\n</table>"
    );
    // and a run of its own width does close it, after which the pipe separates
    // and the segment behind it has no third column to join. carve-js publishes
    // both of these byte for byte.
    assert_eq!(
        html("| a | b ``c |\n+ x | y `` | z |\n"),
        "<table>\n  <tbody>\n    <tr><td>a x</td><td>b <code>c y </code></td></tr>\n  </tbody>\n</table>"
    );
}
