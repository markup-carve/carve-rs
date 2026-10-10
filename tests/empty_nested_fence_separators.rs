use carve::{BlockNode, Document};
use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

fn code_payloads(blocks: &[BlockNode], result: &mut Vec<String>, original: bool) {
    for node in blocks {
        match node {
            BlockNode::CodeBlock(code) => {
                if original {
                    assert_eq!(code.pos.as_ref().unwrap().end_line, 1);
                }
                result.push(code.content.clone());
            }
            BlockNode::RawBlock(raw) => {
                if original {
                    assert_eq!(raw.pos.as_ref().unwrap().end_line, 1);
                }
                if original {
                    assert!(raw.content.is_empty());
                }
            }
            BlockNode::List(list) => {
                for item in &list.items {
                    code_payloads(&item.children, result, original);
                }
            }
            BlockNode::BlockQuote(quote) => code_payloads(&quote.children, result, original),
            _ => {}
        }
    }
}

fn payloads(doc: &Document, original: bool) -> Vec<String> {
    let mut result = Vec::new();
    code_payloads(&doc.children, &mut result, original);
    result
}

fn raw_payloads(blocks: &[BlockNode]) -> Vec<serde_json::Value> {
    let mut result = Vec::new();
    for node in blocks {
        match node {
            BlockNode::RawBlock(raw) => {
                result.push(serde_json::json!({"format":raw.format,"value":raw.content}))
            }
            BlockNode::List(list) => {
                for item in &list.items {
                    result.extend(raw_payloads(&item.children));
                }
            }
            BlockNode::BlockQuote(quote) => result.extend(raw_payloads(&quote.children)),
            _ => {}
        }
    }
    result
}

fn html_code_values(html: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut remaining = html;
    while let Some(start) = remaining.find("<code") {
        remaining = &remaining[start..];
        let open = remaining.find('>').unwrap() + 1;
        let close = remaining[open..].find("</code>").unwrap() + open;
        result.push(remaining[open..close].to_owned());
        remaining = &remaining[close + "</code>".len()..];
    }
    result
}

fn html_tree(html: &str) -> serde_json::Value {
    fn visit(node: &Handle, literal: bool) -> Option<serde_json::Value> {
        match &node.data {
            NodeData::Text { contents } => {
                let value = contents.borrow().to_string();
                if !literal && value.trim().is_empty() {
                    None
                } else {
                    Some(serde_json::json!({"text":value}))
                }
            }
            NodeData::Element { name, attrs, .. } => {
                let tag = name.local.to_string();
                let literal = literal || tag == "pre" || tag == "code";
                let attributes: serde_json::Map<String, serde_json::Value> = attrs
                    .borrow()
                    .iter()
                    .filter_map(|attr| {
                        let key = attr.name.local.to_string();
                        if ["ul", "li", "input"].contains(&tag.as_str())
                            && ["class", "data-task-state", "aria-label"].contains(&key.as_str())
                        {
                            return None;
                        }
                        let value = if key == "checked" || key == "disabled" {
                            serde_json::json!(true)
                        } else {
                            serde_json::json!(attr.value.to_string())
                        };
                        Some((key, value))
                    })
                    .collect();
                let children: Vec<_> = node
                    .children
                    .borrow()
                    .iter()
                    .filter_map(|child| visit(child, literal))
                    .collect();
                Some(serde_json::json!({"tag":tag,"attributes":attributes,"children":children}))
            }
            NodeData::Document => Some(serde_json::Value::Array(
                node.children
                    .borrow()
                    .iter()
                    .filter_map(|child| visit(child, literal))
                    .collect(),
            )),
            _ => None,
        }
    }
    let dom = parse_document(RcDom::default(), Default::default()).one(html);
    visit(&dom.document, false).unwrap()
}

#[test]
fn parent_separators_stay_outside_empty_nested_fences() {
    let controls: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/empty-nested-marker-fences.json")).unwrap();
    for control in controls {
        let source = control["source"].as_str().unwrap();
        let expected: Vec<String> = control["codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|code| code["value"].as_str().unwrap().to_owned())
            .collect();
        let doc =
            carve::parse_with_options(source, &carve::Options::default().with_positions(true));
        assert_eq!(
            payloads(&doc, control["empty"].as_bool().unwrap_or(true)),
            expected,
            "{source:?}"
        );
        assert_eq!(
            serde_json::json!(raw_payloads(&doc.children)),
            control["raw"],
            "{source:?}"
        );
        let written = carve::render_carve(&doc).unwrap();
        assert_eq!(
            raw_payloads(&carve::parse(&written).children),
            raw_payloads(&doc.children),
            "{source:?}"
        );
        assert_eq!(
            payloads(&carve::parse(&written), false),
            expected,
            "{source:?}"
        );
        for html in [
            carve::to_html(source),
            carve::render_html(&doc).unwrap(),
            carve::to_html(&written),
        ] {
            assert_eq!(
                html_tree(&html),
                html_tree(control["html"].as_str().unwrap()),
                "{source:?}"
            );
            assert_eq!(html_code_values(&html), expected, "{source:?}");
        }
    }
}
