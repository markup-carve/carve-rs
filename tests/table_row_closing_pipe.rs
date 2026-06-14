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
    assert_eq!(
        carve::to_html("# H\n| a"),
        "<section id=\"h-a\">\n  <h1>H\n| a</h1>\n</section>"
    );
}
