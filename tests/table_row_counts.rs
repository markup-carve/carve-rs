#[test]
fn explicit_header_and_footer_rows_partition_pipe_tables() {
    let html = carve::to_html(
        "{header-rows=2 footer-rows=1}\n| A | B |\n| C | D |\n| E | F |\n| G | H |\n",
    );
    assert!(html.contains(concat!(
        "<thead>\n",
        "    <tr><th scope=\"col\">A</th><th scope=\"col\">B</th></tr>\n",
        "    <tr><th scope=\"col\">C</th><th scope=\"col\">D</th></tr>\n",
        "  </thead>",
    )));
    assert!(html.contains("<tbody>\n    <tr><td>E</td><td>F</td></tr>\n  </tbody>"));
    assert!(html.contains("<tfoot>\n    <tr><td>G</td><td>H</td></tr>\n  </tfoot>"));
    assert!(!html.contains("header-rows="));
    assert!(!html.contains("footer-rows="));
}

#[test]
fn native_header_cell_remains_in_explicit_body() {
    let html = carve::to_html("{header-rows=1 footer-rows=1}\n| A | B |\n|= C | D |\n| E | F |\n");
    assert!(html.contains("<th scope=\"row\">C</th>"));
}
