//! GFM-style header separator rows, in addition to Carve's native `|=` header
//! cells. A delimiter row directly after the first row turns it into a <thead>
//! header and sets per-column alignment. Matches carve-php / carve-js.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn separator_makes_first_row_a_header() {
    assert_eq!(
        html("| x | y |\n|---|---|"),
        "<table>\n  <thead><tr><th scope=\"col\">x</th><th scope=\"col\">y</th></tr></thead>\n</table>"
    );
}

#[test]
fn separator_sets_column_alignment_on_header_and_body() {
    assert_eq!(
        html("| x | y |\n|:--|--:|\n| a | b |"),
        "<table>\n  <thead><tr><th scope=\"col\" style=\"text-align: left;\">x</th>\
<th scope=\"col\" style=\"text-align: right;\">y</th></tr></thead>\n  <tbody>\n    \
<tr><td style=\"text-align: left;\">a</td><td style=\"text-align: right;\">b</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn center_alignment_from_colons_both_sides() {
    assert_eq!(
        html("| x |\n|:-:|\n| a |"),
        "<table>\n  <thead><tr><th scope=\"col\" style=\"text-align: center;\">x</th></tr></thead>\n  <tbody>\n    \
<tr><td style=\"text-align: center;\">a</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn no_separator_keeps_all_rows_as_data() {
    assert_eq!(
        html("| a | b |\n| c | d |"),
        "<table>\n  <tbody>\n    <tr><td>a</td><td>b</td></tr>\n    \
<tr><td>c</td><td>d</td></tr>\n  </tbody>\n</table>"
    );
}

#[test]
fn continuation_after_header_only_separator_table_starts_new_block() {
    assert_eq!(
        html("| a | b |\n| - | - |\n+ cont |"),
        "<table>\n  <thead><tr><th scope=\"col\">a</th><th scope=\"col\">b</th></tr></thead>\n</table>\n<p>+ cont |</p>"
    );
}
