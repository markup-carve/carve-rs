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
    // Corpus 514-12 at spec 4577df8f reads the marker as a list (markup-carve/carve#4860, not ported yet).
    let expected = "<blockquote>\n  <dl>\n    <dt>t</dt>\n    <dd>d\n- <code>\nx\nflush</code></dd>\n  </dl>\n</blockquote>";
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
