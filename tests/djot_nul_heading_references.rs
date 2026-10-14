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

fn text(html: &str) -> String {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html.to_owned());
    let mut nodes = vec![dom.document.clone()];
    let mut value = String::new();
    while let Some(node) = nodes.pop() {
        if let NodeData::Text { contents } = &node.data {
            value.push_str(&contents.borrow());
        }
        nodes.extend(node.children.borrow().iter().rev().cloned());
    }
    value.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn nul_heading_references_preserve_native_values() {
    let controls: Value =
        serde_json::from_str(include_str!("fixtures/djot_nul_heading_references.json")).unwrap();
    let mut failures = Vec::new();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let imported = carve::djot_to_carve(source);
        let actual = carve::to_html(&imported);
        let expected_owned = row["html"].as_str().unwrap().replace('\0', "�");
        let expected = expected_owned.as_str();
        let values = |html: &str| {
            let mut values = elements(html);
            assert!(!values.is_empty(), "No DOM elements for {html:?}");
            if row["destinationOnly"].as_bool().unwrap_or(false) {
                values.retain(|value| value[0] == "img" || value[0] == "a");
                for value in &mut values {
                    value[0] = json!("");
                    value[1] = json!(0);
                    value[2]
                        .as_array_mut()
                        .unwrap()
                        .retain(|attribute| attribute[0] == "href" || attribute[0] == "src");
                }
            }
            values
        };
        if values(&actual) != values(expected) || text(&actual) != text(expected) {
            failures.push(
                json!({"source":source,"imported":imported,"actual":actual,"expected":expected}),
            );
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        serde_json::to_string_pretty(&failures).unwrap()
    );
}

#[test]
fn references_target_the_allocated_heading_id() {
    for (source, target) in [
        ("# A b\0\n\n# A-b\0\n\n[A-b\0][]", 1),
        ("# a�b\n\n# a\0b\n\n[a\0b][]", 1),
        ("# H\0\n\n[H\0][]\n\n{#H�}", 0),
        ("# H\0\n\n[H\0][]\n\n{#H�}\n{.x}", 0),
    ] {
        let html = carve::to_html(&carve::djot_to_carve(source));
        let values = elements(&html);
        let mut ids = Vec::new();
        let mut links = Vec::new();
        for element in &values {
            for attribute in element[2].as_array().unwrap() {
                if attribute[0] == "id"
                    && matches!(
                        element[0].as_str(),
                        Some("section" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                    )
                {
                    ids.push(attribute[1].as_str().unwrap());
                }
                if attribute[0] == "href" {
                    links.push(attribute[1].as_str().unwrap());
                }
            }
        }
        assert!(!ids.is_empty(), "HTML: {html:?}; elements: {values:?}");
        assert_eq!(links, [format!("#{}", ids[target])], "{source}");
    }
}
