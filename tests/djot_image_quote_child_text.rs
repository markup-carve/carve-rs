use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{json, Value};
use std::sync::OnceLock;

fn node_shape(node: &Handle, parent: &str, first: bool, last: bool, code: bool) -> Value {
    let (name, mut attributes) = match &node.data {
        NodeData::Document => ("#document".to_owned(), Vec::new()),
        NodeData::Element { name, attrs, .. } => (
            name.local.to_string(),
            attrs
                .borrow()
                .iter()
                .filter(|attr| attr.name.local.as_ref() != "data-delim")
                .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                .collect::<Vec<_>>(),
        ),
        NodeData::Text { contents } => {
            static SPACES: OnceLock<regex::Regex> = OnceLock::new();
            let contents = contents.borrow();
            let mut value = if code {
                contents.to_string()
            } else {
                SPACES
                    .get_or_init(|| regex::Regex::new(r"[\t\r\n ]+").unwrap())
                    .replace_all(contents.as_ref(), " ")
                    .into_owned()
            };
            if !code && matches!(parent, "p" | "li" | "blockquote" | "td" | "th") {
                if first {
                    value = value.trim_start_matches(' ').to_owned();
                }
                if last {
                    value = value.trim_end_matches(' ').to_owned();
                }
            }
            return json!(value);
        }
        _ => panic!("unexpected HTML node in image control"),
    };
    attributes.sort();
    let children = node.children.borrow();
    let meaningful = children
        .iter()
        .filter(|child| {
            !matches!(&child.data, NodeData::Text { contents } if
                matches!(name.as_str(), "#document" | "html" | "head" | "body" | "blockquote" | "ul" | "ol" | "li") &&
                contents.borrow().trim_matches([' ', '\t', '\r', '\n']).is_empty())
        })
        .collect::<Vec<_>>();
    let values = meaningful
        .iter()
        .enumerate()
        .map(|(index, child)| {
            node_shape(
                child,
                &name,
                index == 0,
                index + 1 == meaningful.len(),
                code || matches!(name.as_str(), "code" | "pre"),
            )
        })
        .collect::<Vec<_>>();
    json!([name, attributes, values])
}

fn shape(html: &str) -> Value {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    node_shape(&dom.document, "", true, true, false)
}

#[test]
fn image_quote_wrappers_match_native_child_text() {
    let controls: Value =
        serde_json::from_str(include_str!("fixtures/djot_image_quote_child_text.json")).unwrap();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let expected = row["html"].as_str().unwrap();
        let converted = carve::djot_to_carve(source);
        assert_eq!(
            shape(&carve::to_html(&converted)),
            shape(&super::image_paragraphs_as_images(expected)),
            "{source:?}: {converted}"
        );
    }
}
