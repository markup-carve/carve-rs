#[test]
fn unfinished_nested_fences_release_below_column_lines() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/unfinished_nested_fences.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}

#[test]
fn a_marker_in_an_open_description_paragraph_is_inline_text() {
    let source = "> :: t\n> :  d\n>    - ```\n>      x\nflush\n";
    // Corpus 514-12: the marker opens a list in the description body (markup-carve/carve#4860).
    let expected = "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>\n      <p>d</p>\n      <ul>\n        <li>\n          <pre><code>x\n</code></pre>\n        </li>\n      </ul>\n    </dd>\n  </dl>\n</blockquote>\n<p>flush</p>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn a_released_fence_at_the_ancestor_column_opens_its_own_block() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/nested_review_controls.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap().trim(),
            "{}",
            row["id"]
        );
    }
}
