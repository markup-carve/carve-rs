//! carve#4814: the shared vectors plus the edges carve-js pins beside them.
use carve::lint_carve;
use serde_json::Value;

const RULES: [&str; 3] = [
    "setext-heading-underline",
    "heading-closing-hashes",
    "indented-code-block",
];

fn findings(source: &str) -> Vec<(&'static str, usize, usize)> {
    lint_carve(source)
        .into_iter()
        .filter(|w| RULES.contains(&w.rule))
        .map(|w| (w.rule, w.line, w.column))
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
                assert!(
                    warning.message.contains(included.as_str().unwrap()),
                    "{name}"
                );
            }
        }
    }
}

#[test]
fn shared_vectors() {
    check(include_str!(
        "fixtures/lint-rule-vectors/setext-heading-underline.json"
    ));
    check(include_str!(
        "fixtures/lint-rule-vectors/heading-closing-hashes.json"
    ));
    check(include_str!(
        "fixtures/lint-rule-vectors/indented-code-block.json"
    ));
}

#[test]
fn an_indented_line_after_a_paragraph_flush_under_a_list_is_silent() {
    assert_eq!(findings("- :: t\n:  head\n\n     a\n"), []);
    assert_eq!(
        findings("- a\n\npara\n\n    code\n"),
        [("indented-code-block", 5, 5)]
    );
}

#[test]
fn a_spaced_break_under_a_paragraph_is_not_an_underline() {
    assert_eq!(findings("Title\n- - -\n"), []);
}

#[test]
fn the_rules_reach_into_a_list_item() {
    assert_eq!(
        findings("- Title\n  ---\n"),
        [("setext-heading-underline", 2, 3)]
    );
    assert_eq!(findings("- ## h ##\n"), [("heading-closing-hashes", 1, 8)]);
}

#[test]
fn crlf_lines_read_like_lf_lines() {
    assert_eq!(
        findings("Title\r\n---\r\n"),
        [("setext-heading-underline", 2, 1)]
    );
    assert_eq!(findings("## h ##\r\n"), [("heading-closing-hashes", 1, 6)]);
    assert_eq!(
        findings("para\r\n\r\n    code\r\n"),
        [("indented-code-block", 3, 5)]
    );
}

#[test]
fn a_closing_run_after_formatted_heading_text_is_reported() {
    assert_eq!(findings("## *h* ##\n"), [("heading-closing-hashes", 1, 8)]);
    assert_eq!(
        findings("## [h](/x) ##\n"),
        [("heading-closing-hashes", 1, 12)]
    );
    assert_eq!(findings("## `h` \\#\n"), []);
}
