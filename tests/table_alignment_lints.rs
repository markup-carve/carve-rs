//! The `deprecated-cell-alignment` lint of carve#5091 against the shared
//! vectors, and the bytes `carve fmt` writes for each finding.

use carve::lint_carve;
use serde_json::Value;

fn check(json: &str) {
    let file: Value = serde_json::from_str(json).unwrap();
    let rule = file["rule"].as_str().unwrap();
    for vector in file["vectors"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let actual: Vec<_> = lint_carve(vector["source"].as_str().unwrap())
            .into_iter()
            .filter(|w| w.rule == rule)
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
            "{rule}/{name}"
        );
        for (warning, finding) in actual.iter().zip(expected) {
            for included in finding["includes"].as_array().unwrap() {
                let included = included.as_str().unwrap();
                assert!(
                    warning.message.contains(included),
                    "{rule}/{name}: {included:?}"
                );
            }
        }
    }
}

#[test]
fn shared_vectors() {
    check(include_str!(
        "spec/tests/lint-rule-vectors/deprecated-cell-alignment.json"
    ));
}

#[test]
fn fmt_fixes_each_deprecated_prefix() {
    let file: Value = serde_json::from_str(include_str!(
        "spec/resources/spec/table-alignment-lint-vectors.json"
    ))
    .unwrap();
    for vector in file["vectors"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let source = vector["source"].as_str().unwrap();
        assert_eq!(
            carve::to_carve(source),
            vector["fixed"].as_str().unwrap(),
            "{name}"
        );
        let findings = lint_carve(source)
            .into_iter()
            .filter(|w| w.rule == "deprecated-cell-alignment")
            .count();
        assert_eq!(findings, usize::from(source != vector["fixed"]), "{name}");
    }
}

#[test]
fn fmt_replaces_a_gfm_delimiter_row() {
    let source = "| Name | Age |\n|:-----|----:|\n| Ann | 9 |\n";
    let fixed = carve::to_carve(source);
    assert_eq!(fixed, "|={<} Name |={>} Age |\n| Ann | 9 |\n");
    assert_eq!(carve::to_html(&fixed), carve::to_html(source));
}

#[test]
fn a_span_placeholder_or_braced_run_is_not_deprecated() {
    for source in [
        "| < | ^ |\n",
        "|= ^ |= < |\n",
        "|{<} < |\n",
        "|={~ .x} a |\n",
        "|=< |\n",
    ] {
        let findings: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|w| w.rule == "deprecated-cell-alignment")
            .map(|w| w.column)
            .collect();
        let expected: &[usize] = if source == "|=< |\n" { &[3] } else { &[] };
        assert_eq!(findings, expected, "{source:?}");
    }
}
