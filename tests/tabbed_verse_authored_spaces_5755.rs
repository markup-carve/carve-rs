use serde_json::Value;

fn no_break_space_columns(source: &str) -> Vec<Option<(u64, u64)>> {
    let document = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
    let tree: Value = serde_json::from_str(&carve::ast_json::to_json(&document)).unwrap();
    let mut spaces = Vec::new();
    fn visit(value: &Value, spaces: &mut Vec<Option<(u64, u64)>>) {
        match value {
            Value::Object(object) => {
                if object
                    .get("type")
                    .is_some_and(|kind| kind == "non_breaking_space")
                {
                    spaces.push(object.get("pos").map(|pos| {
                        (
                            pos["startLine"].as_u64().unwrap(),
                            pos["startColumn"].as_u64().unwrap(),
                        )
                    }));
                }
                for child in object.values() {
                    visit(child, spaces);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, spaces);
                }
            }
            _ => {}
        }
    }
    visit(&tree, &mut spaces);
    spaces
}

#[test]
fn a_tabbed_stanza_keeps_positions_for_authored_spaces() {
    assert_eq!(
        no_break_space_columns("- a\n\n\t::: |\n  \t  x\n\t:::\n"),
        vec![None, Some((4, 4)), Some((4, 5)), None]
    );
    assert_eq!(
        no_break_space_columns("- a\n  - a\n\n  \t::: |\n  \t  x\n  \t:::\n"),
        vec![Some((5, 5)), None]
    );
    assert_eq!(
        no_break_space_columns("- a\n  - a\n\n  \t::: |\n\t\t  x\n\t:::\n"),
        vec![Some((5, 3)), Some((5, 4)), None, None]
    );
    assert_eq!(
        no_break_space_columns("- a\n  - a\n\n  \t::: |\n   \t    x\n"),
        vec![Some((5, 7)), Some((5, 8)), None, None]
    );
}

#[test]
fn a_nested_tabbed_fence_keeps_literal_code_after_collection() {
    let source = "- a\n  - a\n  \t```\n    \t z\n  \t```\n";
    let formatted = carve::to_carve(source);
    let html = carve::to_html(source);
    assert!(html.contains("<pre><code>\t z\n</code></pre>"), "{html}");
    assert_eq!(html, carve::to_html(&formatted));
}
