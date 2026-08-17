//! A row's closing pipe may be ESCAPED, and the escape is honored there like
//! anywhere else (markup-carve/carve#1293, corpus `333-...-2`).
//!
//! The row closes either way, because the line ends in a pipe. What the escape
//! decides is what the CELL holds: a literal pipe, not an orphaned backslash.
//! Cutting the closer off blindly left a trailing `\` in the cell, which the
//! inline parser then read as a hard break, so `| a b \|` published a `<br>`
//! where the author wrote a pipe.
//!
//! THE ARGUMENT THAT SETTLED IT is the control below: this engine ALREADY
//! honored `\|` mid-cell. Reading the escape at every position except the last
//! is a position exception with nothing behind it, and `\|` is the only way to
//! put a literal pipe in a cell.
//!
//! TWO SITES, ONE RULE. The row's own closer and a `+` continuation row's
//! closer are stripped by different statements, and both had the defect. They
//! are fixed through one helper and pinned together here, because a rule with
//! two spellings is how the first one comes back.
//!
//! carve-js and carve-php both answer these rows correctly and were the oracle.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

fn one_cell(text: &str) -> String {
    format!("<table>\n  <tbody>\n    <tr><td>{text}</td></tr>\n  </tbody>\n</table>")
}

#[test]
fn an_escaped_closing_pipe_is_a_literal_pipe_in_the_cell() {
    // The corpus document.
    assert_eq!(html("| a b \\|\n"), one_cell("a b |"));
}

#[test]
fn a_continuation_rows_closing_pipe_answers_the_same_way() {
    // The second site. The rule is positional, not per-statement.
    assert_eq!(html("| a |\n+ b \\|\n"), one_cell("a b |"));
}

#[test]
fn an_even_backslash_run_leaves_the_closer_plain() {
    // The backslashes escape each other, so the pipe is the ordinary closer and
    // the cell keeps one literal backslash. A naive "is the previous char a
    // backslash" test answers this row wrong.
    assert_eq!(html("| a b \\\\|\n"), one_cell("a b \\"));
}

// ---------------------------------------------------------------------------
// CONTROLS. Each passed before the fix and must go on passing.
// ---------------------------------------------------------------------------

#[test]
fn control_an_escaped_pipe_mid_cell_was_already_honored() {
    // The asymmetry that made the defect a position exception rather than a
    // design choice.
    assert_eq!(
        html("| a \\| b | c |\n"),
        "<table>\n  <tbody>\n    <tr><td>a | b</td><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_a_plain_row_still_closes_and_splits() {
    assert_eq!(
        html("| a | b |\n"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_a_plain_continuation_still_joins() {
    assert_eq!(html("| a |\n+ b |\n"), one_cell("a b"));
}

#[test]
fn control_a_row_attribute_block_still_binds() {
    // The closer is found through `split_row_attrs`, so an escaped pipe must not
    // disturb a glued attribute block.
    assert_eq!(
        html("| a b \\|{.x}\n"),
        "<table>\n  <tbody>\n    <tr class=\"x\"><td>a b |</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn control_a_continuations_open_run_still_spans_the_rows() {
    // markup-carve/carve-rs#1041's row, which shares this scanner. The pipe
    // inside the run is content and the run closes on the continuation.
    assert_eq!(html("| a `b |\n+ c` |\n"), one_cell("a <code>b c</code>"));
}
