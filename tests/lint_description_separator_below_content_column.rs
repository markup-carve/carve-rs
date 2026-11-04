use carve::lint_carve;
use serde_json::Value;

const RULE: &str = "description-separator-below-content-column";
fn findings(source: &str) -> Vec<carve::LintWarning> {
    lint_carve(source)
        .into_iter()
        .filter(|warning| warning.rule == RULE)
        .collect()
}

#[test]
fn shared_vectors() {
    let data: Value = serde_json::from_str(include_str!(
        "spec/tests/lint-rule-vectors/description-separator-below-content-column.json"
    ))
    .unwrap();
    for vector in data["vectors"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let source = vector["source"].as_str().unwrap();
        let warnings = lint_carve(source);
        let actual: Vec<_> = warnings
            .iter()
            .filter(|warning| warning.rule == RULE)
            .collect();
        let expected = vector["expected"]["findings"].as_array().unwrap();
        let matches = actual.len() == expected.len()
            && actual.iter().zip(expected).all(|(warning, finding)| {
                warning.line == finding["line"].as_u64().unwrap() as usize
                    && warning.column == finding["column"].as_u64().unwrap() as usize
                    && finding["includes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .all(|phrase| warning.message.contains(phrase.as_str().unwrap()))
            });
        assert!(matches, "{name}: actual {actual:?}, expected {expected:?}");
        if name == "open-description" {
            let additional: Vec<_> = warnings
                .iter()
                .filter(|warning| warning.rule != RULE)
                .map(|warning| warning.rule)
                .collect();
            assert_eq!(additional, ["markdown-definition-paragraph"]);
        } else if !matches!(
            name,
            // `double-colon` opens a new list, which other rules report.
            "within-description" | "correct-document" | "double-colon"
        ) {
            assert_eq!(
                warnings.len(),
                actual.len(),
                "{name}: unexpected other rule"
            );
        }
    }
}

#[test]
fn marker_ranges() {
    for (name, source, line, column, marker, target) in [
        ("CRLF", "- a\r\n  :: term\r\n : detail\r\n", 3, 2, ":", 2),
        ("marker-line term", "- :: t\n:  head\n", 2, 1, ":", 2),
        (
            "quoted marker-line term",
            "> - :: t\n> :  d\n",
            2,
            3,
            ":",
            4,
        ),
        ("tab", "- a\n\t:: term\n : detail\n", 3, 2, ":", 4),
        ("quote", "> - a\n>   :: term\n>  : detail\n", 3, 4, ":", 4),
        (
            "quoted-blank",
            "> - a\n>   :: term\n>\n>  : detail\n",
            4,
            4,
            ":",
            4,
        ),
    ] {
        let actual = findings(source);
        assert_eq!(actual.len(), 1, "{name}");
        let warning = &actual[0];
        assert_eq!((warning.line, warning.column), (line, column), "{name}");
        assert!(
            warning.message.contains(&format!("column {target}")),
            "{name}"
        );
        assert_eq!(&source[warning.start..warning.end], marker, "{name}");
        assert_eq!(warning.end - warning.start, marker.len(), "{name}");
    }
}

#[test]
fn a_term_marker_past_the_closed_item_opens_a_new_list() {
    // carve#3885: `block-opener-overindented` reports it instead (carve#3992).
    let source = "- a\n  :: term\n :: next\n";
    assert!(findings(source).is_empty());
    assert!(lint_carve(source)
        .iter()
        .any(|w| w.rule == "block-opener-overindented" && (w.line, w.column) == (3, 2)));
}

#[test]
fn ignores_literal_and_payload_lines() {
    for source in [
        "- a\n  :: term\n \\: detail\n",
        "- a\n  :: term\n ::: note\n",
        "- a\n  :: term\n  ```\n  : detail\n  ```\n",
    ] {
        assert!(findings(source).is_empty(), "{source:?}");
    }
}

#[test]
fn a_span_opened_on_a_term_line_does_not_reach_the_next_line() {
    // A term is one line (carve#4861), so the separator below it is a real one.
    for source in [
        "- a\n  :: term `code\n : detail`\n",
        "- a\n  :: term $`math\n : detail`\n",
        "- a\n  :: term `<span\n : detail>`{=html}\n",
        "- a\n  :: term !`literal\n : detail`\n",
        "- a\n  :: term {% hidden\n : detail %}\n",
    ] {
        assert!(!findings(source).is_empty(), "{source:?}");
    }
}
