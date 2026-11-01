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

#[test]
fn overlapping_delimited_edges_leave_plain_body_cells() {
    let source = "{table header-rows=2 footer-rows=1}\n```csv\na\nb\n```\n";
    let html = carve::to_html(source);
    assert_eq!(
        html.trim(),
        "<table>\n  <tbody>\n    <tr><td>a</td></tr>\n    <tr><td>b</td></tr>\n  </tbody>\n</table>"
    );

    let oversized = carve::to_html("{table header-rows=5}\n```csv\na\nb\n```\n");
    assert!(
        oversized.contains("<th scope=\"row\">a</th>"),
        "{oversized}"
    );

    let written = carve::render_carve(&carve::parse(source)).unwrap();
    assert_eq!(carve::to_html(&written), html);
}

#[test]
fn invalid_delimited_header_count_uses_implicit_first_header() {
    for format in ["csv", "tsv"] {
        for invalid in ["x", "1x", "-1", "999999999999999999999999999999999"] {
            let source = format!("{{table header-rows={invalid}}}\n```{format}\na\nb\n```\n");
            let html = carve::to_html(&source);
            assert_eq!(
                html.trim(),
                "<table>\n  <thead>\n    <tr><th scope=\"col\">a</th></tr>\n  </thead>\n  <tbody>\n    <tr><td>b</td></tr>\n  </tbody>\n</table>",
                "{source}"
            );
            let written = carve::render_carve(&carve::parse(&source)).unwrap();
            assert_eq!(carve::to_html(&written), html, "{written}");
        }
    }
}
