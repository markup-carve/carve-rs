#[test]
fn an_empty_nested_item_leaves_following_markers_to_their_list() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/nested-empty-list-tail.json")).unwrap();
    let source_line_attrs = regex::Regex::new(r#" data-source-line="\d+""#).unwrap();
    for case in cases {
        let source = case["source"].as_str().unwrap();
        let html = case["html"].as_str().unwrap();
        assert_eq!(carve::to_html(source).trim(), html.trim(), "{source}");
        for positions in [false, true] {
            for source_lines in [false, true] {
                let options = carve::Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines);
                let doc = carve::parse_with_options(source, &options);
                assert_eq!(
                    source_line_attrs
                        .replace_all(&carve::render_html(&doc).unwrap(), "")
                        .trim(),
                    html.trim(),
                    "positions={positions}, source_lines={source_lines}: {source}"
                );
            }
        }
        let formatted = carve::to_carve(source);
        assert_eq!(
            carve::to_html(&formatted).trim(),
            html.trim(),
            "formatted {source}"
        );
        assert_eq!(
            carve::to_carve(&formatted),
            formatted,
            "idempotence {source}"
        );
    }
}
