//! Row-level table attributes: a `{...}` block glued to a row's closing pipe
//! sets the `<tr>` attributes (the row twin of a cell's opening-pipe block).
//! Matches carve-php / carve-js.

#[test]
fn class_on_a_body_row() {
    assert_eq!(
        carve::to_html("| a | b |{.x}"),
        "<table>\n  <tbody>\n    <tr class=\"x\"><td>a</td><td>b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn id_and_key_value() {
    assert_eq!(
        carve::to_html("| a |{#r1 data-k=v}"),
        "<table>\n  <tbody>\n    <tr id=\"r1\" data-k=\"v\"><td>a</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn header_row_and_gfm_separator() {
    assert_eq!(
        carve::to_html("| H |{.hd}\n|---|\n| c |{.bd}"),
        "<table>\n  <thead><tr class=\"hd\"><th scope=\"col\">H</th></tr></thead>\n  <tbody>\n    <tr class=\"bd\"><td>c</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn composes_with_a_cell_attribute_block() {
    assert_eq!(
        carve::to_html("|{.c} a |{.r}"),
        "<table>\n  <tbody>\n    <tr class=\"r\"><td class=\"c\">a</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn space_before_brace_is_not_a_row_attribute() {
    assert_eq!(carve::to_html("| a | {.x}"), "<p>| a | {.x}</p>");
}

#[test]
fn empty_or_invalid_payload_is_not_a_row_attribute() {
    assert_eq!(carve::to_html("| a |{}"), "<p>| a |{}</p>");
    assert_eq!(carve::to_html("| a |{1bad}"), "<p>| a |{1bad}</p>");
}
