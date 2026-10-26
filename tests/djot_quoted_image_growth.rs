use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{json, Value};

fn elements(html: &str) -> Vec<Value> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut nodes: Vec<(Handle, usize)> = vec![(dom.document.clone(), 0)];
    let mut values = Vec::new();
    while let Some((node, depth)) = nodes.pop() {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if !matches!(
                name.local.as_ref(),
                "html" | "head" | "body" | "thead" | "tbody"
            ) {
                let mut attributes = attrs
                    .borrow()
                    .iter()
                    .filter(|attr| {
                        !matches!(
                            attr.name.local.as_ref(),
                            "data-delim" | "data-task-state" | "aria-label"
                        ) && !(attr.name.local.as_ref() == "scope" && attr.value.as_ref() == "col")
                            && !(attr.name.local.as_ref() == "role"
                                && attr.value.as_ref() == "math")
                    })
                    .map(|attr| {
                        (
                            attr.name.local.to_string(),
                            if matches!(attr.name.local.as_ref(), "href" | "src") {
                                attr.value.replace(' ', "%20")
                            } else {
                                attr.value.to_string()
                            },
                        )
                    })
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
                .map(|child| (child, depth + usize::from(!matches!(&node.data, NodeData::Element { name, .. } if matches!(name.local.as_ref(), "thead" | "tbody"))))),
        );
    }
    values
}

fn text(html: &str) -> Vec<String> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut nodes = vec![dom.document.clone()];
    let mut values = Vec::new();
    while let Some(node) = nodes.pop() {
        if let NodeData::Text { contents } = &node.data {
            let value = contents
                .borrow()
                .split_ascii_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !value.is_empty() {
                values.push(value.replace("↩\u{fe0e}", "↩"));
            }
        }
        nodes.extend(node.children.borrow().iter().rev().cloned());
    }
    values
}

fn alt(html: &str) -> Option<String> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut pending = vec![dom.document.clone()];
    while let Some(node) = pending.pop() {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if name.local.as_ref() == "img" {
                return attrs
                    .borrow()
                    .iter()
                    .find(|attr| attr.name.local.as_ref() == "alt")
                    .map(|attr| attr.value.to_string());
            }
        }
        pending.extend(node.children.borrow().iter().rev().cloned());
    }
    None
}

#[test]
fn native_output_and_loss_guards() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/djot_quoted_image_growth.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let result = carve::migrate_djot(source);
        let actual = carve::to_html(&result.value);
        let expected = row["html"].as_str().unwrap();
        assert_eq!(
            elements(&actual),
            elements(&super::image_paragraphs_as_images(expected)),
            "{source}"
        );
        assert_eq!(text(&actual), text(expected), "{source}");
        let losses = result
            .report
            .diagnostics
            .iter()
            .filter(|d| d.code == "structure-unspellable")
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>();
        let expected = row["losses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(losses, expected, "{source}");
    }
}

#[test]
fn deep_quoted_unicode_chains_preserve_all_alternative_text() {
    for label in ["x", "日本語,", "𐐀?"] {
        for quote in ["> ", "> > "] {
            for depth in [256, 512] {
                let source = (format!("![{label}\n").repeat(depth) + "z" + &"](u)".repeat(depth))
                    .split('\n')
                    .map(|line| format!("{quote}{line}"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let result = carve::migrate_djot(&source);
                assert_eq!(
                    alt(&carve::to_html(&result.value)),
                    Some(format!("{label}\n").repeat(depth) + "z")
                );
                assert!(result
                    .report
                    .diagnostics
                    .iter()
                    .all(|diagnostic| diagnostic.code != "structure-unspellable"));
            }
        }
    }
}
