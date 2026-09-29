use carve::to_html;

#[test]
fn comments_preserve_list_content_column() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/comment-list-content-column.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            to_html(source).trim(),
            case["html"].as_str().unwrap(),
            "{source:?}"
        );
    }
}
