fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn footnote_first_block_marker_leaves_a_column_zero_list_outside() {
    let out = html("see[^a]\n\n[^a]: +\n- a list\n");
    assert!(
        out.contains("<li id=\"fn1\">\n      <ul>\n        <li>a list</li>"),
        "{out}"
    );
}

#[test]
fn description_first_block_marker_leaves_a_column_zero_list_outside() {
    assert_eq!(
        html(":: t\n: +\n- a list\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ul>\n      <li>a list</li>\n    </ul>\n  </dd>\n</dl>"
    );
}
