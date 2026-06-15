//! A `^` rowspan marker extends the cell above it even across the thead/tbody
//! boundary: a header cell can carry a rowspan that spans into body rows.
//! Matches carve-js.

#[test]
fn native_header_cell_spans_into_body() {
    assert_eq!(
        carve::to_html("|= H |= G |\n| ^ | b |\n| ^ | c |"),
        "<table>\n  <thead><tr><th rowspan=\"3\">H</th><th>G</th></tr></thead>\n  <tbody>\n    <tr><td>b</td></tr>\n    <tr><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn gfm_separator_header_cell_spans_into_body() {
    assert_eq!(
        carve::to_html("| H | G |\n|---|---|\n| ^ | c |"),
        "<table>\n  <thead><tr><th rowspan=\"2\">H</th><th>G</th></tr></thead>\n  <tbody>\n    <tr><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn header_rowspan_and_body_rowspan_coexist() {
    assert_eq!(
        carve::to_html("|= H |= G |\n| ^ | b |\n| x | ^ |"),
        "<table>\n  <thead><tr><th rowspan=\"2\">H</th><th>G</th></tr></thead>\n  <tbody>\n    <tr><td rowspan=\"2\">b</td></tr>\n    <tr><td>x</td></tr>\n  </tbody>\n</table>"
    );
}
