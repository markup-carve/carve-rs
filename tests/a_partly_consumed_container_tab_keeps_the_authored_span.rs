use serde_json::{json, Value};

fn visit<'a>(value: &'a Value, nodes: &mut Vec<&'a Value>) {
    match value {
        Value::Object(object) => {
            if object.contains_key("type") {
                nodes.push(value);
            }
            for child in object.values() {
                visit(child, nodes);
            }
        }
        Value::Array(values) => {
            for child in values {
                visit(child, nodes);
            }
        }
        _ => {}
    }
}

#[test]
fn a_partly_consumed_container_tab_keeps_the_authored_span() {
    let source = "- a\n\n  ::: |\n\t  x\n  :::\n";
    let document = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
    let tree: Value = serde_json::from_str(&carve::ast_json::to_json(&document)).unwrap();
    let mut nodes = Vec::new();
    visit(&tree, &mut nodes);
    let paragraph = nodes
        .iter()
        .find(|node| {
            node["type"] == "paragraph" && node["children"][0]["type"] == "non_breaking_space"
        })
        .unwrap();
    let children = paragraph["children"].as_array().unwrap();
    assert_eq!(children.len(), 5);
    let rows: Vec<Value> = std::iter::once(*paragraph)
        .chain(children.iter())
        .map(|node| {
            json!([
                node["type"],
                node["pos"]["startLine"],
                node["pos"]["startColumn"],
                node["pos"]["startOffset"],
                node["pos"]["endOffset"]
            ])
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            json!(["paragraph", 4, 1, 13, 17]),
            json!(["non_breaking_space", null, null, null, null]),
            json!(["non_breaking_space", 4, 1, 13, 14]),
            json!(["non_breaking_space", 4, 2, 14, 15]),
            json!(["non_breaking_space", 4, 3, 15, 16]),
            json!(["text", 4, 4, 16, 17]),
        ]
    );
    assert!(children[0].get("pos").is_none());
    assert_eq!(
        nodes
            .iter()
            .filter(|node| node["type"] == "non_breaking_space" && node.get("pos").is_none())
            .count(),
        1
    );
    let text = &children[4];
    assert_eq!(text["value"], "x");
    let start = text["pos"]["startOffset"].as_u64().unwrap() as usize;
    let end = text["pos"]["endOffset"].as_u64().unwrap() as usize;
    assert_eq!((start, end), (16, 17));
    assert_eq!(&source[start..end], "x");

    let lines: Vec<&str> = source.split('\n').collect();
    for node in nodes {
        if let Some(pos) = node.get("pos") {
            let line = pos["startLine"].as_u64().unwrap() as usize;
            let column = pos["startColumn"].as_u64().unwrap() as usize;
            assert!(column >= 1, "{node}");
            let prefix: String = lines[line - 1].chars().take(column - 1).collect();
            assert_eq!(prefix.chars().count(), column - 1, "{node}");
            let offset = lines[..line - 1]
                .iter()
                .map(|line| line.chars().count() + 1)
                .sum::<usize>()
                + prefix.chars().count();
            assert_eq!(pos["startOffset"], offset, "{node}");
        }
    }
}
