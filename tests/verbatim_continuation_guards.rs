use html5ever::{parse_document, tendril::TendrilSink};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

fn collapse_spaces(value: &str) -> String {
    let mut result = String::new();
    let mut in_space = false;
    for ch in value.chars() {
        if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\x0c') {
            if !in_space {
                result.push(' ');
            }
            in_space = true;
        } else {
            result.push(ch);
            in_space = false;
        }
    }
    result
}

pub(super) fn html_tree(html: &str) -> serde_json::Value {
    fn visit(node: &Handle, literal: bool, discard_space: bool) -> Option<serde_json::Value> {
        match &node.data {
            NodeData::Text { contents } => {
                let value = contents.borrow().to_string();
                if !literal && discard_space && value.trim().is_empty() {
                    None
                } else {
                    Some(
                        serde_json::json!({"text": if literal { value } else { collapse_spaces(&value) }}),
                    )
                }
            }
            NodeData::Element { name, attrs, .. } => {
                let tag = name.local.to_string();
                let literal = literal || tag == "pre" || tag == "code";
                let attributes: serde_json::Map<String, serde_json::Value> = attrs
                    .borrow()
                    .iter()
                    .map(|attr| {
                        let key = attr.name.local.to_string();
                        let value = if key == "checked" || key == "disabled" {
                            serde_json::json!(true)
                        } else {
                            serde_json::json!(attr.value.to_string())
                        };
                        (key, value)
                    })
                    .collect();
                let mut children: Vec<_> = node
                    .children
                    .borrow()
                    .iter()
                    .filter_map(|child| {
                        visit(
                            child,
                            literal,
                            [
                                "html",
                                "head",
                                "body",
                                "blockquote",
                                "ul",
                                "ol",
                                "dl",
                                "table",
                                "tbody",
                                "tr",
                            ]
                            .contains(&tag.as_str()),
                        )
                    })
                    .collect();
                if !literal && ["p", "li", "dt", "dd", "blockquote"].contains(&tag.as_str()) {
                    if let Some(text) = children.first_mut().and_then(|child| child.get_mut("text"))
                    {
                        *text = serde_json::json!(text.as_str().unwrap().trim_start());
                    }
                    if let Some(text) = children.last_mut().and_then(|child| child.get_mut("text"))
                    {
                        *text = serde_json::json!(text.as_str().unwrap().trim_end());
                    }
                }
                Some(serde_json::json!({"tag":tag,"attributes":attributes,"children":children}))
            }
            NodeData::Document => Some(serde_json::Value::Array(
                node.children
                    .borrow()
                    .iter()
                    .filter_map(|child| visit(child, literal, true))
                    .collect(),
            )),
            _ => None,
        }
    }
    let dom = parse_document(RcDom::default(), Default::default()).one(html);
    visit(&dom.document, false, true).unwrap()
}

#[test]
fn raw_literal_and_math_continuations_keep_their_payload_when_formatted() {
    let controls: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/verbatim-continuation-guards.json")).unwrap();
    for control in controls {
        let source = control["source"].as_str().unwrap();
        let doc =
            carve::parse_with_options(source, &carve::Options::default().with_positions(true));
        let written = carve::render_carve(&doc).unwrap();
        let expected = html_tree(control["html"].as_str().unwrap());
        for html in [
            carve::to_html(source),
            carve::render_html(&doc).unwrap(),
            carve::to_html(&written),
        ] {
            assert_eq!(html_tree(&html), expected, "{source:?}");
        }
    }
}

#[test]
fn definition_term_refusals_identify_the_verbatim_node() {
    let content = "x\n[r]: /url".to_owned();
    let nodes = [
        (
            "raw_inline",
            carve::InlineNode::RawInline(carve::RawInline {
                content: content.clone(),
                format: "html".into(),
                injected: false,
                pos: None,
            }),
        ),
        (
            "literal_inline",
            carve::InlineNode::LiteralInline(carve::LiteralInline {
                content: content.clone(),
                attrs: None,
                pos: None,
            }),
        ),
        (
            "math",
            carve::InlineNode::Math(carve::Math {
                content,
                attrs: None,
                display: false,
                label: None,
                number: None,
                pos: None,
            }),
        ),
    ];
    for (kind, node) in nodes {
        let mut doc = carve::parse(":: term\n: body\n");
        let carve::BlockNode::DefinitionList(list) = &mut doc.children[0] else {
            panic!("expected a description list");
        };
        list.items[0].terms[0].children = vec![node];
        match carve::render_carve(&doc).expect_err("the term cannot retain its continuation") {
            carve::RenderCarveError::SourceUnspellable(error) => {
                assert_eq!(error.node_type(), kind);
                assert_eq!(
                    error.reason(),
                    "a block marker on a continuation line ends the definition term"
                );
            }
            other => panic!("unexpected refusal: {other}"),
        }
    }
}
