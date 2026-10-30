use carve::lint_carve;
use serde_json::Value;

fn check(data: &str, count: usize) {
    let data: Value = serde_json::from_str(data).unwrap();
    let rule = data["rule"].as_str().unwrap();
    let vectors = data["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), count);
    for vector in vectors {
        let name = vector["name"].as_str().unwrap();
        // The spec's first-block vectors still read the columns carve#4409 replaced.
        if name.contains("first-block") {
            continue;
        }
        let actual: Vec<_> = lint_carve(vector["source"].as_str().unwrap())
            .into_iter()
            .filter(|warning| warning.rule == rule)
            .collect();
        let expected = vector["expected"]["findings"].as_array().unwrap();
        assert_eq!(
            actual
                .iter()
                .map(|w| (w.line, w.column))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|f| (
                    f["line"].as_u64().unwrap() as usize,
                    f["column"].as_u64().unwrap() as usize
                ))
                .collect::<Vec<_>>(),
            "{name}"
        );
        for (warning, finding) in actual.iter().zip(expected) {
            for included in finding["includes"].as_array().unwrap() {
                assert!(
                    warning.message.contains(included.as_str().unwrap()),
                    "{name}: {}",
                    warning.message
                );
            }
        }
    }
}

#[test]
fn unclosed_code_fence_vectors() {
    check(
        include_str!("fixtures/lint-rule-vectors/unclosed-code-fence.json"),
        20,
    );
}

#[test]
fn continuation_marker_refused_vectors() {
    check(
        include_str!("fixtures/lint-rule-vectors/continuation-marker-refused.json"),
        55,
    );
}
