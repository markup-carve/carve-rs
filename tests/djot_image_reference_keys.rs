use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{NodeData, RcDom};
use serde_json::{json, Value};

fn images(html: &str) -> Vec<Value> {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut pending = vec![dom.document.clone()];
    let mut values = Vec::new();
    while let Some(node) = pending.pop() {
        if let NodeData::Element { name, attrs, .. } = &node.data {
            if name.local.as_ref() == "img" {
                let attrs = attrs.borrow();
                let value = |key: &str| {
                    attrs
                        .iter()
                        .find(|a| a.name.local.as_ref() == key)
                        .map(|a| a.value.to_string())
                        .unwrap_or_default()
                };
                values.push(json!({"alt": value("alt"), "src": value("src")}));
            }
        }
        pending.extend(node.children.borrow().iter().cloned());
    }
    values
}

#[test]
fn native_image_references_resolve() {
    let controls: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/djot_image_reference_keys.json")).unwrap();
    let blank_lines = regex::Regex::new(r"\n(?:[ \t]*\n)+").unwrap();
    for row in controls {
        let source = row["source"].as_str().unwrap();
        let result = carve::migrate_djot(source);
        let mut expected = images(row["html"].as_str().unwrap());
        let blank = expected
            .iter()
            .any(|image| blank_lines.is_match(image["alt"].as_str().unwrap()));
        for image in &mut expected {
            image["alt"] = json!(blank_lines.replace_all(image["alt"].as_str().unwrap(), "\n"));
        }
        assert_eq!(images(&carve::to_html(&result.value)), expected, "{source}");
        assert!(
            !result
                .report
                .diagnostics
                .iter()
                .any(|d| d.message.contains("unresolved Djot image reference")
                    || d.message.contains("with an empty destination")),
            "{source}"
        );
        if blank {
            assert!(
                result.report.diagnostics.iter().any(|d| d
                    .message
                    .contains("image alternative text with a blank line")),
                "{source}"
            );
        }
    }
}

#[test]
fn unresolved_collapsed_images_remain_unresolved() {
    let result = carve::migrate_djot("![a [b](u)\nc][]\n\n[other]: img");
    assert!(images(&carve::to_html(&result.value)).is_empty());
}

#[test]
fn explicit_reference_names_remain_authored() {
    let result = carve::migrate_djot("![a [b](u)\nc][named]\n\n[named]: img");
    assert_eq!(
        images(&carve::to_html(&result.value)),
        vec![json!({"alt":"a b\nc", "src":"img"})]
    );
}

#[test]
fn footnote_labels_stay_outside_image_resolution() {
    let result = carve::migrate_djot("![^a][]\n\n[^a]: note");
    assert!(result.value.contains("[]"));
    assert!(images(&carve::to_html(&result.value)).is_empty());
}

#[test]
fn outer_image_empty_destination_is_reported() {
    let result = carve::migrate_djot("![a ![b]() c]()");
    assert!(result
        .report
        .diagnostics
        .iter()
        .any(|d| d.message.contains("An image with an empty destination")));
}
