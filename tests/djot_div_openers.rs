use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{json, Value};

fn content(node: &Handle, parent: &str) -> Option<Value> {
    if let NodeData::Text { contents } = &node.data {
        let value = contents.borrow();
        if [
            "#document",
            "html",
            "body",
            "div",
            "blockquote",
            "section",
            "ul",
            "ol",
            "li",
        ]
        .contains(&parent)
            && value.chars().all(char::is_whitespace)
        {
            return None;
        }
        return Some(json!(["text", value.as_ref()]));
    }
    let (name, attributes) = if let NodeData::Element { name, attrs, .. } = &node.data {
        let mut values: Vec<_> = attrs
            .borrow()
            .iter()
            .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
            .collect();
        values.sort();
        (name.local.to_string(), values)
    } else {
        ("#document".to_string(), Vec::new())
    };
    let children: Vec<_> = node
        .children
        .borrow()
        .iter()
        .filter_map(|child| content(child, &name))
        .collect();
    Some(json!([name, attributes, children]))
}

fn html(value: &str) -> Value {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(value.to_owned());
    content(&dom.document, "").unwrap()
}

#[test]
fn compact_and_tab_separated_div_openers_match_native_djot() {
    let controls: Value =
        serde_json::from_str(include_str!("fixtures/djot_div_openers.json")).unwrap();
    let mut failures = Vec::new();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let converted = carve::djot_to_carve(source);
        let actual = html(&carve::to_html(&converted));
        let expected = html(&super::image_paragraphs_as_images(
            row["html"].as_str().unwrap(),
        ));
        if actual != expected {
            failures.push(
                json!({"source":source,"converted":converted,"actual":actual,"expected":expected}),
            );
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        serde_json::to_string_pretty(&failures).unwrap()
    );
}
