use carve::{lint_carve, parse_with_options, render_html, BlockNode, Options};
use serde_json::Value;

fn vectors(data: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(data).unwrap()["vectors"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn vector_parse_and_render_premises() {
    for (rule, data) in [
        (
            "list-marker-below-content-column",
            include_str!("fixtures/lint-rule-vectors/list-marker-below-content-column.json"),
        ),
        (
            "ordered-list-split-by-indent",
            include_str!("fixtures/lint-rule-vectors/ordered-list-split-by-indent.json"),
        ),
    ] {
        for vector in vectors(data) {
            let source = vector["source"].as_str().unwrap();
            let name = vector["name"].as_str().unwrap();
            let doc = parse_with_options(
                source,
                &Options {
                    positions: true,
                    ..Options::default()
                },
            );
            let html = render_html(&doc).unwrap();
            assert!(!html.is_empty(), "{rule} {name}: empty render");
            let expected = vector["expected"]["findings"].as_array().unwrap();
            if !expected.is_empty() && rule == "ordered-list-split-by-indent" {
                assert!(doc.children.windows(2).any(|pair| matches!((&pair[0], &pair[1]), (BlockNode::List(a), BlockNode::List(b)) if a.ordered && b.ordered)), "{name}: expected sibling ordered lists; {html}");
            }
            if !expected.is_empty() && rule == "list-marker-below-content-column" {
                let line = expected[0]["line"].as_u64().unwrap() as usize;
                let marker = source.lines().nth(line - 1).unwrap().trim();
                let after_blank = source
                    .lines()
                    .nth(line - 2)
                    .is_some_and(|l| l.trim().is_empty());
                assert!(
                    after_blank || html.contains(marker),
                    "{name}: folded marker absent from render: {html}"
                );
            }
        }
    }
}

fn check(data: &str, rule: &str) {
    for vector in vectors(data) {
        let name = vector["name"].as_str().unwrap();
        let actual: Vec<_> = lint_carve(vector["source"].as_str().unwrap())
            .into_iter()
            .filter(|w| w.rule == rule)
            .collect();
        let expected = vector["expected"]["findings"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "{name}");
        for (warning, finding) in actual.iter().zip(expected) {
            assert_eq!(
                (warning.line, warning.column),
                (
                    finding["line"].as_u64().unwrap() as usize,
                    finding["column"].as_u64().unwrap() as usize
                ),
                "{name}"
            );
            for phrase in finding["includes"].as_array().unwrap() {
                assert!(
                    warning.message.contains(phrase.as_str().unwrap()),
                    "{name}: {}",
                    warning.message
                );
            }
        }
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn shared_vectors() {
    check(
        include_str!("fixtures/lint-rule-vectors/list-marker-below-content-column.json"),
        "list-marker-below-content-column",
    );
    check(
        include_str!("fixtures/lint-rule-vectors/ordered-list-split-by-indent.json"),
        "ordered-list-split-by-indent",
    );
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn marker_positions_and_payloads() {
    for (source, expected) in [
        ("10. c\r\n   - d\r\n", vec![(2, 4)]),
        ("> 10. c\n>    - d\n", vec![(2, 6)]),
        ("100. c\n\t- d\n", vec![(2, 2)]),
        ("10. c\n   - [x] d\n", vec![(2, 4)]),
        ("10. c\n   a. d\n", vec![(2, 4)]),
        ("10. c\n   \\- d\n", vec![]),
        ("10. c\n   `a\n   - d`\n", vec![]),
        ("10. c\r\n\r\n   - d\r\n", vec![(3, 4)]),
        ("> 10. c\n>\n>    - d\n", vec![(3, 6)]),
        ("100. c\n\n\t- d\n", vec![(3, 2)]),
        ("10. c\n\n   a. d\n", vec![(3, 4)]),
        ("10. c\n\n   `a\n   - d`\n", vec![]),
        ("1. a\n\n 2. b\n", vec![]),
    ] {
        let actual: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|w| w.rule == "list-marker-below-content-column")
            .map(|w| (w.line, w.column))
            .collect();
        assert_eq!(actual, expected, "{source:?}");
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn split_positions_in_quotes_and_crlf() {
    for (source, expected) in [
        (" 9. nine\r\n10. ten\r\n", vec![(2, 1)]),
        (">  9. nine\n> 10. ten\n", vec![(2, 3)]),
        (" 9. nine\n\npara\n\n10. ten\n", vec![]),
        ("- x\n\n   9. nine\n  10. ten\n", vec![(4, 3)]),
        ("::: div\n 9. nine\n10. ten\n::: \n", vec![(3, 1)]),
        ("\t9. nine\n10. ten\n", vec![(2, 1)]),
    ] {
        let actual: Vec<_> = lint_carve(source)
            .into_iter()
            .filter(|w| w.rule == "ordered-list-split-by-indent")
            .map(|w| (w.line, w.column))
            .collect();
        assert_eq!(actual, expected, "{source:?}");
    }
}
