#[test]
fn terminal_breaks_survive_in_every_inline_context() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/markdown-terminal-hard-break.json")).unwrap();
    for case in cases {
        let doc = carve::from_json(&case["document"].to_string()).unwrap();
        assert_eq!(
            carve::render_markdown(&doc).unwrap(),
            case["markdown"].as_str().unwrap(),
            "template={}, case={}",
            case["template"],
            case["name"]
        );
    }
}
