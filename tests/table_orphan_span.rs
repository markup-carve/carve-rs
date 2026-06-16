//! A span marker with nothing to merge renders an EMPTY cell, not dropped
//! (spec PART 9 §5): a `^` in the first row, a `<` in the first column.
//! Matches carve-js / carve-php.

#[test]
fn orphan_rowspan_marker_is_empty_cell() {
    assert_eq!(
        carve::to_html("| ^ | b |"),
        "<table>\n  <tbody>\n    <tr><td></td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn orphan_colspan_marker_is_empty_cell() {
    assert_eq!(
        carve::to_html("| < | b |"),
        "<table>\n  <tbody>\n    <tr><td></td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}
