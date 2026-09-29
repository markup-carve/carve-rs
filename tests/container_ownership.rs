use carve::{to_html, to_html_with_options, Options};

#[test]
fn container_ownership() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/container-ownership.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        assert_eq!(
            to_html_with_options(source, &Options::default().with_positions(true)).trim(),
            case["html"].as_str().unwrap(),
            "positioned parse: {source:?}"
        );
        assert_eq!(
            to_html(source).trim(),
            case["html"].as_str().unwrap(),
            "{source:?}"
        );
    }
}

#[test]
fn a_blank_after_retained_text_keeps_the_nested_content_column() {
    let source = "- - intro\n%% c\n - tail\n\n    para\n";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li><p>intro</p>\n        <p>- tail</p>\n        <p>para</p>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(to_html(source).trim(), expected);
    assert_eq!(
        to_html_with_options(source, &Options::default().with_positions(true)).trim(),
        expected
    );
}
