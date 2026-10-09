#[test]
fn wrappers_preserve_hard_breaks_and_emphasis() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/markdown-wrapper-hard-breaks.json")).unwrap();
    for case in cases {
        let markdown = carve::to_markdown(case["source"].as_str().unwrap());
        let mut html = String::new();
        pulldown_cmark::html::push_html(
            &mut html,
            pulldown_cmark::Parser::new_ext(
                &markdown,
                pulldown_cmark::Options::ENABLE_STRIKETHROUGH,
            ),
        );
        assert_eq!(html, case["html"].as_str().unwrap(), "{}", case["source"]);
        assert_eq!(
            markdown,
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
