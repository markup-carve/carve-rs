use carve::lint_carve;
use serde_json::Value;

#[test]
fn consumed_key_near_miss_vectors() {
    let data: Value = serde_json::from_str(include_str!(
        "spec/tests/lint-rule-vectors/consumed-key-near-miss.json"
    ))
    .unwrap();
    let rule = data["rule"].as_str().unwrap();
    let vectors = data["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 37);
    for vector in vectors {
        let name = vector["name"].as_str().unwrap();
        let actual: Vec<_> = lint_carve(vector["source"].as_str().unwrap())
            .into_iter()
            .filter(|w| w.rule == rule)
            .collect();
        let expected = vector["expected"]["findings"].as_array().unwrap();
        let at = |f: &Value| {
            (
                f["line"].as_u64().unwrap() as usize,
                f["column"].as_u64().unwrap() as usize,
            )
        };
        assert_eq!(
            actual
                .iter()
                .map(|w| (w.line, w.column))
                .collect::<Vec<_>>(),
            expected.iter().map(at).collect::<Vec<_>>(),
            "{name}"
        );
        for (warning, finding) in actual.iter().zip(expected) {
            let word = finding["word"].as_str().unwrap();
            let key = format!("{{{}}}", finding["suggests"].as_str().unwrap());
            assert!(
                warning.message.contains(word) && warning.message.contains(&key),
                "{name}: {}",
                warning.message
            );
        }
    }
}
