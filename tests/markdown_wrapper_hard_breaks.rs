#[test]
fn wrappers_preserve_hard_breaks_and_emphasis() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/markdown-wrapper-hard-breaks.json")).unwrap();
    for case in cases {
        assert_eq!(
            carve::to_markdown(case["source"].as_str().unwrap()),
            case["markdown"].as_str().unwrap(),
            "{}",
            case["source"]
        );
    }
}

#[test]
fn an_escaped_literal_backslash_is_not_a_hard_break() {
    assert_eq!(
        carve::to_markdown(":: word\\\\\n: definition\n"),
        "**word\\\\**\n\ndefinition\n"
    );
}
