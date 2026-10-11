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

fn html_tree(html: &str) -> serde_json::Value {
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
                    .filter_map(|attr| {
                        let key = attr.name.local.to_string();
                        let value = if key == "checked" || key == "disabled" {
                            serde_json::json!(true)
                        } else {
                            serde_json::json!(attr.value.to_string())
                        };
                        Some((key, value))
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
fn definitions_keep_their_attached_block_ownership() {
    let controls: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/attached-definition-boundaries.json")).unwrap();
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
fn extension_blocks_keep_their_definition_shaped_payload() {
    use carve::{BlockMatch, BlockNode, CarveExtension, CodeBlock, MatcherContext, Options};

    struct DefinitionBlock;
    impl CarveExtension for DefinitionBlock {
        fn name(&self) -> &'static str {
            "definition-block"
        }

        fn match_block(
            &self,
            lines: &[&str],
            start: usize,
            _ctx: &MatcherContext<'_>,
        ) -> Option<BlockMatch> {
            if *lines.get(start)? != "@@@" {
                return None;
            }
            Some(BlockMatch {
                node: BlockNode::CodeBlock(CodeBlock {
                    attrs: None,
                    lang: None,
                    title: None,
                    label: None,
                    content: format!("{}\n", lines.get(start + 1)?),
                    pos: None,
                }),
                lines_consumed: 2,
            })
        }
    }
    let doc = carve::parse_with_options(
        "- a\n+\n@@@\n [r]: /url\n\nSee [r][].\n",
        &Options::default().with_extension(&DefinitionBlock),
    );
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("expected list")
    };
    let content = list.items[0].children.iter().find_map(|node| match node {
        BlockNode::CodeBlock(code) => Some(code.content.as_str()),
        _ => None,
    });
    assert_eq!(content, Some(" [r]: /url\n"));
}
