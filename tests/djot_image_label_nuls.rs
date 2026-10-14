use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use serde_json::{json, Value};

fn images(html: &str) -> Vec<Value> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut nodes: Vec<Handle> = vec![dom.document];
    let mut values = Vec::new();
    while let Some(node) = nodes.pop() {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if name.local.as_ref() == "img" {
                let mut attributes = attrs
                    .borrow()
                    .iter()
                    .map(|attr| (attr.name.local.to_string(), attr.value.to_string()))
                    .collect::<Vec<_>>();
                attributes.sort();
                values.push(json!(attributes));
            }
        }
        nodes.extend(node.children.borrow().iter().rev().cloned());
    }
    values
}

#[test]
fn image_labels_restore_source_nuls_before_nested_child_parsing() {
    let controls: Value =
        serde_json::from_str(include_str!("fixtures/djot_image_label_nuls.json")).unwrap();
    let mut failures = Vec::new();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let report = carve::migrate_djot(source);
        assert!(
            report.report.diagnostics.iter().all(|item| item.message
                != "An unresolved Djot image reference loses its image element."),
            "{source:?}: {:?}",
            report.report.diagnostics
        );
        let converted = carve::djot_to_carve(source);
        let actual = images(&carve::to_html(&converted));
        let expected = images(row["html"].as_str().unwrap());
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
