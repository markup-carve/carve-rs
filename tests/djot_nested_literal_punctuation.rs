use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{json, Value};

fn elements(html: &str) -> Vec<Value> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut nodes: Vec<(Handle, usize)> = vec![(dom.document, 0)];
    let mut values = Vec::new();
    while let Some((node, depth)) = nodes.pop() {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if !matches!(name.local.as_ref(), "html" | "head" | "body") {
                let mut attributes = attrs
                    .borrow()
                    .iter()
                    .filter(|attr| {
                        !(attr.name.local.as_ref() == "role" && attr.value.as_ref() == "math")
                    })
                    .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                    .collect::<Vec<_>>();
                attributes.sort();
                values.push(json!([name.local.to_string(), depth, attributes]));
            }
        }
        nodes.extend(
            node.children
                .borrow()
                .iter()
                .rev()
                .cloned()
                .map(|child| (child, depth + 1)),
        );
    }
    values
}

fn text(html: &str) -> String {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut nodes = vec![dom.document];
    let mut value = String::new();
    while let Some(node) = nodes.pop() {
        if let NodeData::Text { contents } = &node.data {
            value.push_str(&contents.borrow());
        }
        nodes.extend(node.children.borrow().iter().rev().cloned());
    }
    value.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn nested_labels_preserve_literal_punctuation() {
    let controls: Value = serde_json::from_str(include_str!(
        "fixtures/djot_nested_literal_punctuation.json"
    ))
    .unwrap();
    let mut failures = Vec::new();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let imported = carve::djot_to_carve(source);
        let actual = carve::to_html(&imported);
        let expected = row["html"].as_str().unwrap();
        let values = |html: &str| {
            let mut values = elements(html);
            if row["imageOnly"].as_bool().unwrap_or(false) {
                values.retain(|value| value[0] == "img");
                for value in &mut values {
                    value[1] = json!(0);
                }
            }
            values
        };
        if values(&actual) != values(expected) || text(&actual) != text(expected) {
            failures.push(
                json!({"source":source,"imported":imported,"actual":actual,"expected":expected}),
            );
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        serde_json::to_string_pretty(&failures).unwrap()
    );
}
