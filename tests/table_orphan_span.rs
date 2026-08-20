//! A span marker with nothing to merge renders an EMPTY cell, not dropped
//! (spec PART 9 §5): a `^` in the first row, a `<` in the first column.
//! Matches carve-js / carve-php.
//! PART 10 §T9 gives cells in a promoted head-row run `scope="col"`.

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

#[test]
fn blocked_colspan_marker_is_empty_cell() {
    // The `<` in row 3 column 2 has no available origin to its left: column 1 is
    // held by `x`'s rowspan ("^" above it), so the `<` cannot merge and renders
    // as an empty cell rather than being dropped (the row would otherwise shift
    // `d` left). Matches carve-js.
    assert_eq!(
        carve::to_html("| A | B | C |\n|---|---|---|\n| x | y | z |\n| ^ | < | d |"),
        concat!(
            "<table>\n",
            "  <thead>\n    <tr><th scope=\"col\">A</th><th scope=\"col\">B</th><th scope=\"col\">C</th></tr>\n  </thead>\n",
            "  <tbody>\n",
            "    <tr><td rowspan=\"2\">x</td><td>y</td><td>z</td></tr>\n",
            "    <tr><td></td><td>d</td></tr>\n",
            "  </tbody>\n",
            "</table>"
        )
    );
}

#[test]
fn colspan_marker_scans_left_past_consumed_rowspan_cell() {
    assert_eq!(
        carve::to_html(
            "| p | q | r | s |\n|---|---|---|---|\n| a | b | c | d |\n| p | ^ | < | e |"
        ),
        concat!(
            "<table>\n",
            "  <thead>\n    <tr><th scope=\"col\">p</th><th scope=\"col\">q</th><th scope=\"col\">r</th><th scope=\"col\">s</th></tr>\n  </thead>\n",
            "  <tbody>\n",
            "    <tr><td>a</td><td rowspan=\"2\">b</td><td>c</td><td>d</td></tr>\n",
            "    <tr><td colspan=\"2\">p</td><td>e</td></tr>\n",
            "  </tbody>\n",
            "</table>"
        )
    );
}
