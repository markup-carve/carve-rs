use carve::{html_to_ast, html_to_carve, HtmlImportMode, HtmlImportOptions};
use serde_json::Value;

fn blocks(value: &Value) -> Vec<Value> {
    match value {
        Value::Array(items) => items.iter().flat_map(blocks).collect(),
        Value::Object(node) if node.get("type").and_then(Value::as_str) == Some("code_block") => {
            vec![serde_json::json!({"lang": node.get("lang"), "content": node.get("content")})]
        }
        Value::Object(node) => node.values().flat_map(blocks).collect(),
        _ => vec![],
    }
}

#[test]
fn shared_language_cases() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("spec/tests/html-code-language-cases.json")).unwrap();
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let options = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        for case in &cases {
            let html = case["html"].as_str().unwrap();
            let ast = html_to_ast(html, &options).unwrap();
            let tree: Value = serde_json::from_str(&carve::ast_json::to_json(&ast.value)).unwrap();
            let actual = blocks(&tree);
            assert_eq!(
                actual.iter().map(|b| b["lang"].clone()).collect::<Vec<_>>(),
                case["languages"].as_array().unwrap().clone(),
                "{} {mode:?}",
                case["name"]
            );
            let source = html_to_carve(html, &options).unwrap().value;
            let reparsed: Value =
                serde_json::from_str(&carve::to_json(&carve::parse(&source))).unwrap();
            assert_eq!(blocks(&reparsed), actual, "{} {mode:?}", case["name"]);
        }
    }
}

#[test]
fn representable_fixture_and_reports_are_stable_in_every_mode() {
    let html = include_str!("spec/tests/html-import/code-language-hints/input.html");
    let expected = include_str!("spec/tests/html-import/code-language-hints/expected.crv");
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let options = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        let result = html_to_carve(html, &options).unwrap();
        assert_eq!(result.value, expected);
        assert!(result.report.diagnostics.is_empty());
        assert_eq!(
            carve::render_carve(&carve::parse(&result.value)).unwrap(),
            expected
        );
    }
}

#[test]
fn raw_preservation_does_not_promote_nested_code() {
    let html =
        "<figure><div class=\"panel\"><pre data-lang=\"js\">x</pre></div><figcaption>c</figcaption></figure>";
    let result = html_to_ast(
        html,
        &HtmlImportOptions {
            mode: HtmlImportMode::Roundtrip,
            ..Default::default()
        },
    )
    .unwrap();
    let ast: Value = serde_json::from_str(&carve::to_json(&result.value)).unwrap();
    assert_eq!(ast["children"][0]["type"], "raw_block");
    assert!(blocks(&ast).is_empty());
    assert!(result
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == carve::HtmlImportDiagnosticCode::RawPreserved));
}

#[test]
fn a_shared_wrapper_is_not_rescanned_per_pre() {
    let comments = "<!-- x -->".repeat(40000);
    let pres = "<pre>x</pre>".repeat(40000);
    let measure = |body: String| {
        let start = std::time::Instant::now();
        html_to_ast(
            &format!("<div class=\"highlight highlight-source-js\">{body}</div>"),
            &HtmlImportOptions::default(),
        )
        .unwrap();
        start.elapsed()
    };
    html_to_ast(&"<pre>x</pre>".repeat(1000), &HtmlImportOptions::default()).unwrap();
    let baseline = measure(format!("{pres}{comments}"));
    assert!(
        measure(format!("{comments}{pres}")) < baseline * 4 + std::time::Duration::from_millis(250)
    );
}
