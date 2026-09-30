use carve::{to_html, to_html_with_options, Options};

#[test]
fn a_marker_line_opaque_quote_preserves_authored_ownership() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/opaque-marker-quote-ownership.json")).unwrap();
    for case in cases {
        let source = case["source"].as_str().unwrap();
        let expected = case["html"].as_str().unwrap();
        assert_eq!(to_html(source), expected, "{source:?}");
        assert_eq!(
            to_html_with_options(source, &Options::default().with_positions(true)),
            expected,
            "position path: {source:?}"
        );
    }
}
