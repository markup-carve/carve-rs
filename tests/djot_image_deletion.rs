use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};
use serde_json::Value;

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
fn deletion_spans_contribute_child_text_to_image_alternatives() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/djot_image_deletion.json")).unwrap();
    let trailing_spaces = regex::Regex::new(r"[ \t]+\n").unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let expected = row["alt"].as_str().unwrap();
        let trailing = row["trailingLoss"].as_bool().unwrap();
        let expected = if trailing {
            trailing_spaces.replace_all(expected, "\n").into_owned()
        } else {
            expected.to_owned()
        };
        let result = carve::migrate_djot(source);
        assert_eq!(
            alt(&carve::to_html(&result.value)),
            Some(expected),
            "{source}: {} => {}",
            result.value,
            carve::to_html(&result.value)
        );
        assert_eq!(
            result
                .report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("trailing spaces before a line break")),
            trailing,
            "{source}"
        );
    }
}
