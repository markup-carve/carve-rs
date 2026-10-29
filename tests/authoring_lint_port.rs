use carve::{lint_carve, lint_carve_with_options, Options};
use serde_json::Value;
use std::path::Path;

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn authoring_scorecard_matches_the_pinned_spec() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/lint-authoring");
    let data: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("expectations.json")).unwrap())
            .unwrap();
    for case in data["cases"].as_array().unwrap() {
        let file = case["file"].as_str().unwrap();
        let source = std::fs::read_to_string(root.join(file)).unwrap();
        let actual: Vec<_> = lint_carve(&source)
            .into_iter()
            .map(|w| (w.rule.to_string(), w.line))
            .collect();
        let expected: Vec<_> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                (
                    pair[0].as_str().unwrap().to_string(),
                    pair[1].as_u64().unwrap() as usize,
                )
            })
            .collect();
        assert_eq!(actual, expected, "{file}");
        let mut options = Options::new();
        options.lint_caption_looking_paragraph = true;
        let hints: Vec<_> = lint_carve_with_options(&source, &options)
            .into_iter()
            .filter(|w| w.rule == "caption-looking-paragraph")
            .map(|w| (w.rule.to_string(), w.line))
            .collect();
        let expected_hints: Vec<_> = case["optIn"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                (
                    pair[0].as_str().unwrap().to_string(),
                    pair[1].as_u64().unwrap() as usize,
                )
            })
            .collect();
        assert_eq!(hints, expected_hints, "opt-in {file}");
    }
}
