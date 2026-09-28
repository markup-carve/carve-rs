//! A table row at the document column ends the list above it, so the content
//! columns that list opened are dead for a later definition
//! (markup-carve/carve-rs#2096).
//!
//! Expectations are the executable spec's reading at markup-carve/carve
//! 38829a97.

#[test]
fn a_table_row_ends_the_list_columns_above_it() {
    assert_eq!(
        carve::to_html("* }\n|b|\n  [f]: t\n"),
        "<ul>\n  <li>}</li>\n</ul>\n<table>\n  <tbody>\n    <tr><td>b</td></tr>\n  </tbody>\n</table>\n<p>[f]: t</p>"
    );
}

#[test]
fn a_definition_at_the_item_column_reads_as_before() {
    assert_eq!(
        carve::to_html("* }\n  [f]: t\n\n[f]\n"),
        "<ul>\n  <li>}</li>\n</ul>\n<p>[f]</p>"
    );
}
