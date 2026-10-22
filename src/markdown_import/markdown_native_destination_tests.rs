use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{json, Value};

fn node_shape(node: &Handle) -> Value {
    if let NodeData::Text { contents } = &node.data {
        let normalized = regex::Regex::new(r"[\t\n\r\x0c ]+")
            .unwrap()
            .replace_all(contents.borrow().as_ref(), " ")
            .into_owned();
        return json!(normalized);
    }
    let (name, mut attrs) = match &node.data {
        NodeData::Document => ("#document-fragment".to_owned(), Vec::new()),
        NodeData::Element { name, attrs, .. } => (
            name.local.to_string(),
            attrs
                .borrow()
                .iter()
                .filter_map(|attr| {
                    let key = attr.name.local.to_string();
                    if key == "scope" && attr.value.as_ref() == "col" {
                        return None;
                    }
                    let mut value = attr.value.to_string();
                    if matches!(key.as_str(), "href" | "src") && value.starts_with('/') {
                        value = regex::Regex::new("(?i)%5b")
                            .unwrap()
                            .replace_all(&value, "[")
                            .into_owned();
                        value = regex::Regex::new("(?i)%5d")
                            .unwrap()
                            .replace_all(&value, "]")
                            .into_owned();
                    }
                    Some((key, value))
                })
                .collect::<Vec<_>>(),
        ),
        _ => panic!("unexpected HTML node"),
    };
    attrs.sort();
    let children = node.children.borrow();
    let values = children.iter().filter(|child| {
        !matches!(&child.data, NodeData::Text { contents } if
            matches!(name.as_str(), "#document-fragment" | "html" | "head" | "body" | "blockquote" | "ul" | "ol" | "table" | "thead" | "tbody" | "tr") &&
            contents.borrow().trim_matches([' ', '\t', '\n', '\r', '\x0c']).is_empty())
    }).map(node_shape).collect::<Vec<_>>();
    if name == "p"
        && values.len() == 1
        && matches!(&children[0].data, NodeData::Element { name, .. } if name.local.as_ref() == "img")
    {
        return values.into_iter().next().unwrap();
    }
    json!([name, attrs, values])
}

fn shape(html: &str) -> Value {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    node_shape(&dom.document)
}

#[test]
fn native_markdown_destinations_match_shared_controls() {
    let rows: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/markdown-native-destinations.json"
    ))
    .unwrap();
    let mut failures = Vec::new();
    for (index, row) in rows.as_array().unwrap().iter().enumerate() {
        let source = row["source"].as_str().unwrap();
        let expected = row["carveHtml"]
            .as_str()
            .or_else(|| row["html"].as_str())
            .unwrap();
        let actual = crate::to_html(&crate::markdown_to_carve(source));
        if shape(&actual) != shape(expected) {
            failures.push((index, source.to_owned(), actual));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
