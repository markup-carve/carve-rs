use carve::{from_json, parse, render_carve, to_carve, to_html, BlockNode};
use serde_json::Value;

fn comment_contents(source: &str) -> Vec<String> {
    fn visit(value: &Value, contents: &mut Vec<String>) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| visit(item, contents)),
            Value::Object(fields) => {
                if fields.get("type").and_then(Value::as_str) == Some("comment") {
                    contents.push(fields["content"].as_str().unwrap().to_string());
                }
                fields.values().for_each(|value| visit(value, contents));
            }
            _ => {}
        }
    }
    let tree: Value = serde_json::from_str(&carve::to_json(&parse(source))).unwrap();
    let mut contents = Vec::new();
    visit(&tree, &mut contents);
    contents
}

#[test]
fn a_line_comment_drops_trailing_ascii_whitespace() {
    for source in [":::\n%%. \n", ":::\n%%. \t\n", ":::\n%%.\t\n"] {
        let formatted = to_carve(source);
        assert_eq!(formatted, ":::\n%% .\n:::\n");
        let BlockNode::Div(div) = &parse(source).children[0] else {
            panic!("expected a div");
        };
        let BlockNode::Comment(comment) = &div.children[0] else {
            panic!("expected a comment");
        };
        assert_eq!(comment.content, ".");
        assert_eq!(to_carve(&formatted), formatted);
        assert_eq!(to_html(&formatted), to_html(source));
    }
}

#[test]
fn comment_content_uses_one_separator_and_drops_only_trailing_ascii_whitespace() {
    for (source, expected) in [
        ("%%  x \t\n", " x"),
        ("%% x \r\n", "x"),
        ("x %%  y \t\n", " y"),
        ("::: |\na\n%%  z \t\nb\n:::\n", " z"),
        ("%% \u{a0}\n", "\u{a0}"),
        ("%%\u{000b}x\n", "\u{000b}x"),
        ("%%\u{000b} x\n", "\u{000b} x"),
    ] {
        assert_eq!(comment_contents(source), [expected], "{source:?}");
        let written = to_carve(source);
        assert_eq!(comment_contents(&written), [expected], "{written:?}");
        assert_eq!(to_html(&written), to_html(source), "{source:?}");
    }
}

#[test]
fn an_ingested_inline_comment_writes_no_trailing_layout_whitespace() {
    let json = r#"{"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[{"type":"text","value":"x "},{"type":"comment","block":false,"content":"y "}]}]}"#;
    let document = from_json(json).unwrap();
    assert_eq!(render_carve(&document).unwrap(), "x %% y\n");
}
