//! carve#3992: the shared vectors plus the edges carve-js pins beside them.
use carve::lint_carve;
use serde_json::Value;

fn found(rule: &str, source: &str) -> Vec<(usize, usize)> {
    lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == rule)
        .map(|w| (w.line, w.column))
        .collect()
}

fn check(data: &str) {
    let data: Value = serde_json::from_str(data).unwrap();
    let rule = data["rule"].as_str().unwrap();
    for vector in data["vectors"].as_array().unwrap() {
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
                    "{rule}/{name}: {:?} lacks {included:?}",
                    warning.message
                );
            }
        }
    }
}

#[test]
fn shared_vectors() {
    check(include_str!(
        "spec/tests/lint-rule-vectors/block-opener-overindented.json"
    ));
    check(include_str!(
        "spec/tests/lint-rule-vectors/emphasis-marker-before-attributes.json"
    ));
}

#[test]
fn overindented_openers_report_once_at_the_opener() {
    let at = |s| found("block-opener-overindented", s);
    assert_eq!(at("> x\n>\n>  | a |\n>  | b |\n"), [(3, 4)]);
    assert_eq!(at("::: note\n  # h\n:::\n"), [(2, 3)]);
    assert_eq!(at("- a\n+\n# h\n"), []);
    let message = lint_carve("> q\n>   # h\n")
        .into_iter()
        .find(|w| w.rule == "block-opener-overindented")
        .unwrap()
        .message;
    assert!(message.contains("2 columns past the content column of its block quote"));
}

#[test]
fn a_marker_before_attributes_reports_utf16_offsets_into_crlf_source() {
    let w = lint_carve("x\r\né*{.x}\r\n")
        .into_iter()
        .find(|w| w.rule == "emphasis-marker-before-attributes")
        .unwrap();
    assert_eq!((w.line, w.column), (2, 2));
    assert_eq!(
        found("emphasis-marker-before-attributes", "*a $`*` b*{.x}\n"),
        []
    );
}
