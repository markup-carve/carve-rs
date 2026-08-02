//! A standard table row opens AND closes with `|` (grammar standard_row). A
//! stray leading `|` with no closing `|` (`| a`) is ordinary paragraph text,
//! not a table. Matches carve-js, carve-php, and canonical djot.

#[test]
fn incomplete_row_is_paragraph() {
    assert_eq!(carve::to_html("| a"), "<p>| a</p>");
    assert_eq!(carve::to_html("| a | b"), "<p>| a | b</p>");
}

#[test]
fn complete_row_is_a_table() {
    assert_eq!(
        carve::to_html("| a |"),
        "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn incomplete_row_does_not_interrupt_paragraph_or_heading() {
    assert_eq!(carve::to_html("para\n| a"), "<p>para\n| a</p>");
    // The incomplete row opens no table; after a heading it is a paragraph,
    // since a heading ends at its newline and folds nothing in.
    assert_eq!(
        carve::to_html("# H\n| a"),
        "<section id=\"H\">\n  <h1>H</h1>\n  <p>| a</p>\n</section>"
    );
}

// The same rule governs a `+` continuation row: `continuation_row` also ends
// in `'|'`, so a `+` line with content dangling after its last pipe is prose
// and ends the table. carve-rs used to accept any `+`-leading line, so it
// merged cells no other engine merged.

const TABLE_AB: &str =
    "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n  </tbody>\n</table>";

#[test]
fn continuation_row_without_a_closing_pipe_is_paragraph() {
    assert_eq!(
        carve::to_html("| a | b |\n+ c | d"),
        format!("{TABLE_AB}\n<p>+ c | d</p>")
    );
}

#[test]
fn continuation_row_with_a_closing_pipe_still_joins() {
    assert_eq!(
        carve::to_html("| a | b |\n+ c | d |"),
        "<table>\n  <tbody>\n    <tr><td>a c</td><td>b d</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn lone_plus_after_a_row_ends_the_table() {
    assert_eq!(
        carve::to_html("| a | b |\n+"),
        format!("{TABLE_AB}\n<p>+</p>")
    );
}

#[test]
fn continuation_row_takes_no_row_attribute_block() {
    // `continuation_row` has no `row_attributes` slot, so `|{.x}` does not
    // stand in for the closing pipe the way it does on a standard row.
    assert_eq!(
        carve::to_html("| a | b |\n+ c |{.x}"),
        format!("{TABLE_AB}\n<p>+ c |{{.x}}</p>")
    );
}

#[test]
fn an_unclosed_continuation_row_ends_the_table_for_good() {
    assert_eq!(
        carve::to_html("| a | b |\n+ c | d\n+ e | f |"),
        format!("{TABLE_AB}\n<p>+ c | d\n+ e | f |</p>")
    );
}
