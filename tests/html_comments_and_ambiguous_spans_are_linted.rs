//! Default-on hints for an HTML comment that renders as text (markup-carve/carve#4812)
//! and for comment- or glob-shaped emphasis (markup-carve/carve#4815), checked
//! against the pinned spec's rule vectors.

use carve::{lint_carve, markdown_to_carve};
use serde_json::Value;

fn check(data: &str) {
    let data: Value = serde_json::from_str(data).unwrap();
    let rule = data["rule"].as_str().unwrap();
    for vector in data["vectors"].as_array().unwrap() {
        let name = vector["name"].as_str().unwrap();
        let actual: Vec<_> = lint_carve(vector["source"].as_str().unwrap())
            .into_iter()
            .filter(|warning| warning.rule == rule)
            .collect();
        let expected = vector["expected"]["findings"].as_array().unwrap();
        assert_eq!(
            actual.iter().map(|w| (w.line, w.column)).collect::<Vec<_>>(),
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
                assert!(warning.message.contains(included.as_str().unwrap()), "{name}");
            }
        }
    }
}

#[test]
fn the_spec_vectors_hold() {
    check(include_str!("spec/tests/lint-rule-vectors/html-comment-renders-as-text.json"));
    check(include_str!("spec/tests/lint-rule-vectors/comment-shaped-emphasis.json"));
    check(include_str!("spec/tests/lint-rule-vectors/glob-shaped-strong.json"));
}

#[test]
fn the_default_triggers_fire() {
    for (source, rule) in [
        ("<!-- TODO secret -->\n", "html-comment-renders-as-text"),
        ("/* foo */\n", "comment-shaped-emphasis"),
        ("*.js,*.ts\n", "glob-shaped-strong"),
    ] {
        assert!(lint_carve(source).iter().any(|w| w.rule == rule), "{rule}");
    }
}

#[test]
fn a_markdown_html_comment_imports_as_a_carve_comment() {
    for (markdown, carve) in [
        ("<!-- TODO secret -->\n", "%% TODO secret\n"),
        ("<!---->\n", "%%\n"),
        ("<!--\na\nb\n-->\n", "%%%\na\nb\n%%%\n"),
        ("<!--\n%%%\n-->\n", "%%%%\n%%%\n%%%%\n"),
        ("> <!-- q -->\n", "> %% q\n"),
        ("<!-- a --> x\n", "```=html\n<!-- a --> x\n```\n"),
        ("<!-- a --><!-- b -->\n", "```=html\n<!-- a --><!-- b -->\n```\n"),
        ("<!-- carve: x -->\n", "```=html\n<!-- carve: x -->\n```\n"),
    ] {
        assert_eq!(markdown_to_carve(markdown), carve, "{markdown:?}");
    }
}
