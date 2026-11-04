use serde_json::Value;

#[test]
fn list_code_fences_accept_closers_at_the_content_floor() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/list_code_fence_floor.json")).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 83);
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for case in cases {
        let source = case["source"].as_str().unwrap();
        let expected = case["html"].as_str().unwrap();
        for actual in [
            carve::to_html(source),
            carve::render_html(&carve::parse(source)).unwrap(),
        ] {
            assert_eq!(
                whitespace.replace_all(&actual, "><").trim(),
                expected,
                "{}: {source}",
                case["id"]
            );
        }
    }
}
