//! PART 11 sections 9a and 11a, rendered against the spec's shared fixture.

use serde_json::Value;

#[test]
fn the_markdown_writer_matches_the_shared_fixture() {
    let cases: Value = serde_json::from_str(include_str!(
        "spec/tests/fixtures/markdown-writer-targets.json"
    ))
    .unwrap();
    let cases = cases.as_array().unwrap();
    assert!(cases.len() >= 3, "the fixture is empty");
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let out = match case["carve"].as_str() {
            Some(source) => carve::to_markdown(source),
            None => carve::render_markdown(&carve::from_json(&case["ast"].to_string()).unwrap())
                .unwrap(),
        };
        assert_eq!(out, case["markdown"].as_str().unwrap(), "{name}");
    }
}
