use carve::{lint_carve, parse_with_options, render_html, BlockNode, Options};
use serde_json::Value;

fn vectors(data: &str) -> Vec<Value> {
    serde_json::from_str::<Value>(data).unwrap()["vectors"]
        .as_array()
        .unwrap()
        .clone()
}

// Since carve#3885 a marker below an item's content column opens a list item
// where it stands instead of folding into the item as text, and
// `ordered-list-split-by-indent` is retired: both vectors' sources render one
// list (docs/validation.md). The shared split vectors still carry the old
// findings.
#[test]
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
                assert!(!doc.children.windows(2).any(|pair| matches!((&pair[0], &pair[1]), (BlockNode::List(a), BlockNode::List(b)) if a.ordered && b.ordered)), "{name}: expected one ordered list; {html}");
            }
            if !expected.is_empty() && rule == "list-marker-below-content-column" {
                let line = expected[0]["line"].as_u64().unwrap() as usize;
                let marker = source.lines().nth(line - 1).unwrap().trim();
                assert!(
                    !html.contains(marker),
                    "{name}: marker folded into the render as text: {html}"
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
fn list_marker_vectors() {
    check(
        include_str!("fixtures/lint-rule-vectors/list-marker-below-content-column.json"),
        "list-marker-below-content-column",
    );
}

#[test]
fn ordered_split_vectors_report_nothing() {
    let data = include_str!("fixtures/lint-rule-vectors/ordered-list-split-by-indent.json");
    for vector in vectors(data) {
        let name = vector["name"].as_str().unwrap();
        assert!(
            !lint_carve(vector["source"].as_str().unwrap())
                .iter()
                .any(|w| w.rule == "ordered-list-split-by-indent"),
            "{name}"
        );
    }
}

#[test]
fn marker_positions_and_payloads() {
    for (source, expected) in [
        ("10. c\r\n   - d\r\n", vec![(2, 4)]),
        ("> 10. c\n>    - d\n", vec![(2, 6)]),
        ("100. c\n\t- d\n", vec![(2, 2)]),
        ("10. c\n   - [x] d\n", vec![(2, 4)]),
        ("10. c\n   a. d\n", vec![(2, 4)]),
        ("10. c\n   \\- d\n", vec![]),
        // The marker opens an item, so the code span cannot reach it (carve#3885).
        ("10. c\n   `a\n   - d`\n", vec![(3, 4)]),
        ("10. c\r\n\r\n   - d\r\n", vec![(3, 4)]),
        ("> 10. c\n>\n>    - d\n", vec![(3, 6)]),
        ("100. c\n\n\t- d\n", vec![(3, 2)]),
        // A lone letter marker after a blank is a paragraph, not a list.
        ("10. c\n\n   a. d\n", vec![]),
        ("10. c\n\n   `a\n   - d`\n", vec![]),
        // A same-kind sibling below the content column is reported (carve#4835).
        ("1. a\n\n 2. b\n", vec![(3, 2)]),
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
fn split_shapes_in_quotes_and_crlf_report_nothing() {
    for source in [
        " 9. nine\r\n10. ten\r\n",
        ">  9. nine\n> 10. ten\n",
        " 9. nine\n\npara\n\n10. ten\n",
        "- x\n\n   9. nine\n  10. ten\n",
        "::: div\n 9. nine\n10. ten\n::: \n",
        "\t9. nine\n10. ten\n",
    ] {
        assert!(
            !lint_carve(source)
                .iter()
                .any(|w| w.rule == "ordered-list-split-by-indent"),
            "{source:?}"
        );
    }
}
