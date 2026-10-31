fn html(source: &str) -> String {
    carve::to_html(source).trim().to_string()
}

#[test]
fn a_fence_below_a_note_marker_line_items_source_column_leaves_the_item() {
    assert_eq!(
        html("[^3]: - a\n   * z\n    ~~~\nq\n\n[^3]\n"),
        "<p>q</p>\n<p><a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a></p>\n<section role=\"doc-endnotes\" aria-label=\"Footnotes\">\n  <hr>\n  <ol>\n    <li id=\"fn1\">\n      <ul>\n        <li>a\n* z</li>\n      </ul>\n      <pre><code></code></pre>\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">↩</a></p>\n    </li>\n  </ol>\n</section>"
    );
}

#[test]
fn ownership_walk_keeps_the_fences_authored_column() {
    let ast: serde_json::Value = serde_json::from_str(&carve::to_json_with_options(
        "[^3]: - a\n   * z\n    ~~~\nq\n\n[^3]\n",
        &carve::Options {
            positions: true,
            ..Default::default()
        },
    ))
    .unwrap();
    let note = ast["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["type"] == "footnote")
        .unwrap();
    let code = &note["children"][1];
    assert_eq!(code["type"], "code_block");
    assert_eq!(code["pos"]["startLine"], 3);
    assert_eq!(code["pos"]["startColumn"], 5);
}
