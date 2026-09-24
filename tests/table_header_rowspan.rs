//! A `^` rowspan marker keeps its extent when a header cell spans body rows.
//! The HTML renderer puts all rows in one tbody so the span stays in one group.
//! Matches carve-js.
//! PART 10 §T9 still gives those head-row cells `scope="col"`.

#[test]
fn native_header_cell_spans_into_body() {
    assert_eq!(
        carve::to_html("|= H |= G |\n| ^ | b |\n| ^ | c |"),
        "<table>\n  <tbody>\n    <tr><th scope=\"col\" rowspan=\"3\">H</th><th scope=\"col\">G</th></tr>\n    <tr><td>b</td></tr>\n    <tr><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn gfm_separator_header_cell_spans_into_body() {
    assert_eq!(
        carve::to_html("| H | G |\n|---|---|\n| ^ | c |"),
        "<table>\n  <tbody>\n    <tr><th scope=\"col\" rowspan=\"2\">H</th><th scope=\"col\">G</th></tr>\n    <tr><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn header_rowspan_and_body_rowspan_coexist() {
    assert_eq!(
        carve::to_html("|= H |= G |\n| ^ | b |\n| x | ^ |"),
        "<table>\n  <tbody>\n    <tr><th scope=\"col\" rowspan=\"2\">H</th><th scope=\"col\">G</th></tr>\n    <tr><td rowspan=\"2\">b</td></tr>\n    <tr><td>x</td></tr>\n  </tbody>\n</table>"
    );
}
