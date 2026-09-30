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

#[test]
fn a_closed_quoted_comment_preserves_lazy_payload_indentation() {
    let source = "- > %%%\n  > x\n    > y\n  > %%%\n";
    let document = carve::parse(source);
    let json: serde_json::Value = serde_json::from_str(&carve::to_json(&document)).unwrap();
    assert_eq!(
        json["children"][0]["items"][0]["children"][0]["children"][0]["content"],
        "x\n  > y"
    );
}

#[test]
fn a_nested_closed_quoted_comment_preserves_lazy_payload_indentation() {
    let source = "- > > %%%\n  > > x\n      > y\n  > > %%%\n";
    let json: serde_json::Value =
        serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    assert_eq!(
        json["children"][0]["items"][0]["children"][0]["children"][0]["children"][0]["content"],
        "x\n    > y"
    );
}
#[test]
fn an_attached_block_does_not_reuse_saved_comment_line_indices() {
    let source = "> %%%\n> a\n> b\n  y\n>\n> ```\n+\nz\n> %%%\n";
    let _ = carve::parse(source);
}
