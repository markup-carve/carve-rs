use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};
use serde_json::Value;

fn alternatives(html: &str) -> Vec<String> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut pending = vec![dom.document.clone()];
    let mut values = Vec::new();
    while let Some(node) = pending.pop() {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if name.local.as_ref() == "img" {
                values.push(
                    attrs
                        .borrow()
                        .iter()
                        .find(|a| a.name.local.as_ref() == "alt")
                        .map(|a| a.value.to_string())
                        .unwrap_or_default(),
                );
            }
        }
        pending.extend(node.children.borrow().iter().cloned());
    }
    values
}

#[test]
fn image_alternative_losses_use_rendered_child_text_and_source_lines() {
    let controls: Value =
        serde_json::from_str(include_str!("fixtures/djot_image_alt_losses.json")).unwrap();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let result = carve::migrate_djot(source);
        let losses: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .filter(|d| {
                d.code == "structure-unspellable"
                    && d.message.starts_with("An image alternative text")
            })
            .collect();
        let kind = row["kind"].as_str().unwrap();
        assert_eq!(losses.len(), usize::from(kind != "none"), "{source}");
        let actual = alternatives(&carve::to_html(&result.value));
        let expected = vec![row["alt"].as_str().unwrap().to_owned()];
        if kind == "none" {
            assert_eq!(actual, expected, "{source}");
        } else {
            assert_eq!(
                losses[0].path.as_deref(),
                Some(format!("line:{}", row["line"].as_u64().unwrap()).as_str()),
                "{source}"
            );
            assert!(
                losses[0].message.contains(if kind == "blank" {
                    "blank line"
                } else {
                    "trailing spaces"
                }),
                "{source}"
            );
            assert_ne!(actual, expected, "{source}");
        }
    }
}
