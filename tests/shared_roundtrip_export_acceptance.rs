use carve::{
    html_to_ast, html_to_carve, migrate_markdown, parse, to_html, to_html_with_options,
    to_html_with_options_and_report, to_json, to_markdown_with_options, CheckedRenderOptions,
    HtmlImportMode, HtmlImportOptions, Options,
};
use html5ever::{local_name, ns, tendril::TendrilSink, QualName};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};
use serde_json::Value;

fn fixtures() -> Value {
    serde_json::from_str(include_str!(
        "spec/resources/roundtrip-export-acceptance-fixtures.json"
    ))
    .unwrap()
}
fn rows<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn required_rows<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("missing array {key}"))
        .as_slice()
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap().to_owned()
}
fn ast(source: &str) -> Value {
    serde_json::from_str(&to_json(&parse(source))).unwrap()
}
fn field<'a>(node: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(node, |value, key| {
        if let Some(array) = value.as_array() {
            key.parse::<usize>().ok().and_then(|index| array.get(index))
        } else {
            value.get(key)
        }
    })
}
fn nodes<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(map) => {
            if value["type"].is_string() {
                out.push(value);
            }
            for child in map.values() {
                nodes(child, out);
            }
        }
        Value::Array(array) => {
            for child in array {
                nodes(child, out);
            }
        }
        _ => {}
    }
}
fn node_list(value: &Value) -> Vec<&Value> {
    let mut out = Vec::new();
    nodes(value, &mut out);
    out
}
fn projection(tree: &Value, expected: &Value) -> bool {
    let mut remaining = node_list(tree);
    for row in expected.as_array().unwrap() {
        let found = remaining.iter().position(|node| {
            row.as_object().unwrap().iter().all(|(key, value)| {
                if value.is_null() {
                    field(node, key).is_none()
                } else {
                    field(node, key) == Some(value)
                }
            })
        });
        let Some(index) = found else { return false };
        remaining.remove(index);
    }
    true
}
fn import_projection(tree: &Value, fixture: &Value) -> bool {
    if projection(tree, &fixture["expectedNodes"]) {
        false
    } else {
        assert!(
            fixture["fallbackNodes"]
                .as_array()
                .is_some_and(|rows| !rows.is_empty()),
            "{}: {tree}",
            fixture["id"]
        );
        assert!(
            projection(tree, &fixture["fallbackNodes"]),
            "{}: {tree}",
            fixture["id"]
        );
        true
    }
}
fn reserved(name: &str) -> bool {
    matches!(name, "data-marker" | "data-char")
        || [
            "explicit-id",
            "ref",
            "autolink",
            "footnote-label",
            "inline-footnote",
            "raw",
            "escaped",
            "abbreviations",
            "col-widths",
        ]
        .iter()
        .any(|suffix| {
            name == format!("data-carve-{suffix}") || name == format!("data-djot-{suffix}")
        })
}
fn no_reserved(tree: &Value) {
    for node in node_list(tree) {
        if let Some(attrs) = field(node, "attrs.keyValues").and_then(Value::as_object) {
            assert!(attrs.keys().all(|key| !reserved(key)), "{node}");
        }
    }
}
fn mode(name: &str) -> HtmlImportOptions {
    HtmlImportOptions {
        mode: match name {
            "safe" => HtmlImportMode::Safe,
            "semantic" => HtmlImportMode::Semantic,
            "roundtrip" => HtmlImportMode::Roundtrip,
            _ => panic!("unknown import mode {name}"),
        },
        ..Default::default()
    }
}
fn import_ast(html: &str, options: &HtmlImportOptions) -> Value {
    serde_json::from_str(&to_json(&html_to_ast(html, options).unwrap().value)).unwrap()
}
fn dom(html: &str) -> RcDom {
    html5ever::parse_fragment(
        RcDom::default(),
        Default::default(),
        QualName::new(None, ns!(html), local_name!("div")),
        Vec::new(),
        false,
    )
    .one(html)
}
fn visit(handle: &Handle, out: &mut Vec<Handle>) {
    out.push(handle.clone());
    for child in handle.children.borrow().iter() {
        visit(child, out);
    }
    if let NodeData::Element {
        template_contents, ..
    } = &handle.data
    {
        if let Some(content) = template_contents.borrow().as_ref() {
            visit(content, out);
        }
    }
}
fn elements(html: &str) -> (RcDom, Vec<Handle>) {
    let tree = dom(html);
    let mut all = Vec::new();
    visit(&tree.document, &mut all);
    let elements = all
        .into_iter()
        .filter(|handle| matches!(handle.data, NodeData::Element { .. }))
        .collect();
    (tree, elements)
}
fn attrs(html: &str) -> Vec<(String, String, String)> {
    let (_tree, elements) = elements(html);
    elements
        .into_iter()
        .flat_map(|handle| {
            if let NodeData::Element { name, attrs, .. } = &handle.data {
                attrs
                    .borrow()
                    .iter()
                    .map(|attr| {
                        (
                            name.local.to_string(),
                            attr.name.local.to_string(),
                            attr.value.to_string(),
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        })
        .collect()
}
fn carrier_text(handle: &Handle) -> String {
    let content = match &handle.data {
        NodeData::Element {
            template_contents, ..
        } => template_contents
            .borrow()
            .clone()
            .unwrap_or_else(|| handle.clone()),
        _ => handle.clone(),
    };
    let payload = content
        .children
        .borrow()
        .iter()
        .map(|child| match &child.data {
            NodeData::Text { contents } => contents.borrow().to_string(),
            _ => panic!("carrier payload contains child markup"),
        })
        .collect();
    payload
}
fn deprecated_html(html: &str) -> String {
    let tree = dom(html);
    let mut all = Vec::new();
    visit(&tree.document, &mut all);
    for handle in all {
        if let NodeData::Element { attrs, .. } = &handle.data {
            for attr in attrs.borrow_mut().iter_mut() {
                let name = attr.name.local.to_string();
                if name.starts_with("data-carve-") && reserved(&name) {
                    attr.name.local = name.replacen("data-carve-", "data-djot-", 1).into();
                }
            }
        }
    }
    let mut all = Vec::new();
    visit(&tree.document, &mut all);
    for handle in all {
        if let NodeData::Element {
            template_contents, ..
        } = &handle.data
        {
            if let Some(content) = template_contents.borrow().as_ref() {
                *handle.children.borrow_mut() = content.children.borrow().clone();
            }
        }
    }
    let root = tree.document.children.borrow()[0].clone();
    let mut bytes = Vec::new();
    for child in root.children.borrow().iter() {
        html5ever::serialize(
            &mut bytes,
            &SerializableHandle::from(child.clone()),
            html5ever::serialize::SerializeOpts {
                traversal_scope: html5ever::serialize::TraversalScope::IncludeNode,
                ..Default::default()
            },
        )
        .unwrap();
    }
    String::from_utf8(bytes).unwrap()
}

#[test]
fn shared_export_inventory_restores_approved_ast_fields() {
    let fixtures = fixtures();
    let mut failures = Vec::new();
    for fixture in required_rows(&fixtures, "exportImport") {
        let result =
            std::panic::catch_unwind(|| {
                let source = text(fixture, "source");
                assert!(
                    projection(&ast(&source), &fixture["expectedNodes"]),
                    "{}",
                    fixture["id"]
                );
                let ordinary = to_html(&source);
                assert_eq!(
                    to_html_with_options(&source, &Options::default().with_roundtrip(false)),
                    ordinary
                );
                let exported =
                    to_html_with_options(&source, &Options::default().with_roundtrip(true));
                let attributes = attrs(&exported);
                assert!(
                    !attrs(&ordinary).iter().any(|(_, name, _)| reserved(name)),
                    "{}",
                    fixture["id"]
                );
                for (name, value) in fixture["carriers"].as_object().unwrap() {
                    assert!(
                        attributes.iter().any(|(_, key, found)| key == name
                            && (value.is_null() || Some(found.as_str()) == value.as_str())),
                        "{}: {name}",
                        fixture["id"]
                    );
                }
                assert!(
                    attributes
                        .iter()
                        .filter(|(_, name, _)| reserved(name))
                        .all(|(_, name, _)| fixture["carriers"].get(name).is_some()),
                    "{}",
                    fixture["id"]
                );
                for host in required_rows(fixture, "carrierHosts") {
                    assert!(attributes
                        .iter()
                        .any(|(element, name, _)| Some(element.as_str())
                            == host["element"].as_str()
                            && Some(name.as_str()) == host["attribute"].as_str()));
                }
                for forbidden in required_rows(fixture, "forbiddenExportAttributes") {
                    assert!(attributes.iter().all(|(_, name, _)| name
                        != forbidden
                            .as_str()
                            .expect("forbidden attribute must be a string")));
                }
                if let Some(expected) = fixture["ordinaryAttributes"].as_object() {
                    for (name, value) in expected {
                        for html in [&ordinary, &exported] {
                            assert!(attrs(html).iter().any(|(_, key, found)| key == name
                                && Some(found.as_str()) == value.as_str()));
                        }
                    }
                }
                if let Some(expected) = fixture["carrierText"].as_object() {
                    for (name, value) in expected {
                        let (_tree, elements) = elements(&exported);
                        let hosts: Vec<_> = elements
                            .into_iter()
                            .filter(|handle| match &handle.data {
                                NodeData::Element { attrs, .. } => attrs
                                    .borrow()
                                    .iter()
                                    .any(|attr| attr.name.local.as_ref() == name),
                                _ => false,
                            })
                            .collect();
                        assert_eq!(hosts.len(), 1);
                        let payload = Value::String(carrier_text(&hosts[0]));
                        assert!(
                            value == &payload
                                || value
                                    .as_array()
                                    .is_some_and(|allowed| allowed.contains(&payload)),
                            "{}: {name}; expected {value}, got {payload}, HTML {exported}",
                            fixture["id"]
                        );
                    }
                }
                for name in required_rows(fixture, "importModes") {
                    let options = mode(name.as_str().unwrap());
                    let imported = import_ast(&exported, &options);
                    let declined = import_projection(&imported, fixture);
                    if text(fixture, "id").starts_with("raw-") {
                        let expected_decline = text(fixture, "id").starts_with("raw-html-")
                            && name.as_str().unwrap() != "roundtrip";
                        assert_eq!(declined, expected_decline, "{} {name}", fixture["id"]);
                    }
                    no_reserved(&imported);
                    let legacy = import_ast(&deprecated_html(&exported), &options);
                    assert_eq!(import_projection(&legacy, fixture), declined);
                    no_reserved(&legacy);
                    for absent in rows(fixture, "expectedAbsentFields") {
                        assert!(node_list(&imported)
                            .iter()
                            .filter(|node| node["type"] == absent["type"])
                            .all(|node| field(node, absent["field"].as_str().unwrap()).is_none()));
                    }
                    let reconstructed = html_to_carve(&exported, &options).unwrap().value;
                    let expected = if declined {
                        fixture["fallbackHtml"].as_str().unwrap()
                    } else {
                        &ordinary
                    };
                    assert_eq!(
                        to_html(&reconstructed).trim(),
                        expected.trim(),
                        "{} {name}",
                        fixture["id"]
                    );
                }
            });
        if result.is_err() {
            failures.push(text(fixture, "id"));
        }
    }
    assert!(failures.is_empty(), "failed export fixtures: {failures:?}");
}

#[test]
fn shared_invalid_carriers_report_loss_and_do_not_apply_metadata() {
    let fixtures = fixtures();
    for fixture in required_rows(&fixtures, "importNegative") {
        for name in required_rows(fixture, "importModes") {
            let options = mode(name.as_str().unwrap());
            let html = text(fixture, "html");
            let result = html_to_ast(&html, &options).unwrap();
            let tree: Value = serde_json::from_str(&to_json(&result.value)).unwrap();
            assert!(
                projection(&tree, &fixture["expectedNodes"]),
                "{} {name}: {tree}",
                fixture["id"]
            );
            no_reserved(&tree);
            if fixture["requiresDiagnostic"].as_bool().unwrap_or(false) {
                let control = fixture["diagnosticControlHtml"]
                    .as_str()
                    .map(|html| {
                        html_to_ast(html, &options)
                            .unwrap()
                            .report
                            .diagnostics
                            .len()
                    })
                    .unwrap_or(0);
                assert!(
                    result.report.diagnostics.len() > control,
                    "{} {name}",
                    fixture["id"]
                );
            }
            let source = html_to_carve(&html, &options).unwrap().value;
            for (key, value) in [
                ("forbiddenSourceSubstrings", source.clone()),
                ("forbiddenAstSubstrings", tree.to_string()),
                ("forbiddenRenderedSubstrings", to_html(&source)),
            ] {
                for fragment in rows(fixture, key) {
                    assert!(
                        !value.contains(fragment.as_str().unwrap()),
                        "{} {name}: {key}",
                        fixture["id"]
                    );
                }
            }
            for forbidden in required_rows(fixture, "forbiddenTypes") {
                assert!(!node_list(&tree)
                    .iter()
                    .any(|node| node["type"] == *forbidden));
            }
            for absent in rows(fixture, "expectedAbsentFields") {
                assert!(node_list(&tree)
                    .iter()
                    .filter(|node| node["type"] == absent["type"])
                    .all(|node| field(node, absent["field"].as_str().unwrap()).is_none()));
            }
            for forbidden in rows(fixture, "forbiddenFieldValues") {
                assert!(node_list(&tree)
                    .iter()
                    .filter(|node| node["type"] == forbidden["type"])
                    .all(|node| field(node, forbidden["field"].as_str().unwrap())
                        != Some(&forbidden["value"])));
            }
        }
    }
}

#[test]
fn shared_export_collisions_report_the_dropped_authored_attribute() {
    let fixtures = fixtures();
    for fixture in required_rows(&fixtures, "collisionExport") {
        let result = to_html_with_options_and_report(
            &text(fixture, "source"),
            &Options::default().with_roundtrip(true),
            CheckedRenderOptions::default(),
        )
        .unwrap();
        let name = text(fixture, "attribute");
        let values: Vec<_> = attrs(&result.value)
            .into_iter()
            .filter(|(_, key, _)| key == &name)
            .map(|(_, _, value)| value)
            .collect();
        assert_eq!(values, vec![text(fixture, "value")]);
        assert!(result
            .losses
            .iter()
            .any(|row| row.code == "carrier-attribute-dropped"
                && row.attribute.as_deref() == Some(name.as_str())
                && row.format.is_none()
                && row.target.as_str() == "html"));
    }
}

#[test]
fn shared_markdown_container_carriers_restore_ast_fields() {
    let fixtures = fixtures();
    for fixture in required_rows(&fixtures, "markdownExportImport") {
        let source = text(fixture, "source");
        assert!(
            projection(&ast(&source), &fixture["expectedNodes"]),
            "{}",
            fixture["id"]
        );
        let ordinary = to_markdown_with_options(&source, &Options::default());
        assert_eq!(
            to_markdown_with_options(&source, &Options::default().with_roundtrip(false)),
            ordinary
        );
        assert!(!ordinary.contains("<!-- carve:"));
        let exported = to_markdown_with_options(&source, &Options::default().with_roundtrip(true));
        for fragment in required_rows(fixture, "carrierFragments") {
            assert!(exported.contains(fragment.as_str().unwrap()));
        }
        let imported = migrate_markdown(&exported);
        assert!(
            projection(&ast(&imported.value), &fixture["expectedNodes"]),
            "{}",
            fixture["id"]
        );
        assert_eq!(to_html(&imported.value).trim(), to_html(&source).trim());
    }
}

#[test]
fn shared_footnote_recovery_remains_a_complete_separate_resource() {
    let fixtures = fixtures();
    assert_eq!(
        fixtures["footnoteRecoveryResource"],
        "footnote-carrier-recovery-fixtures.json"
    );
    let paired: Value = serde_json::from_str(include_str!(
        "spec/resources/footnote-carrier-recovery-fixtures.json"
    ))
    .unwrap();
    assert_eq!(paired.as_array().unwrap().len(), 24);
    for fixture in paired.as_array().unwrap() {
        let options = mode(fixture["mode"].as_str().unwrap());
        let html = text(fixture, "html");
        assert_eq!(
            html_to_carve(&html, &options).unwrap().value,
            text(fixture, "expectedSource")
        );
        no_reserved(&import_ast(&html, &options));
    }
}

#[test]
fn shared_denied_destination_is_not_enabled_by_export() {
    let result = to_html_with_options_and_report(
        "[x](javascript:one)\n",
        &Options::default().with_roundtrip(true),
        CheckedRenderOptions::default(),
    )
    .unwrap();
    assert!(!result.value.contains("javascript:"));
    assert!(result
        .losses
        .iter()
        .any(|row| row.code == "destination-denied"));
}

#[test]
fn shared_task_hooks_are_ordinary_output() {
    let fixtures = fixtures();
    let fixture = required_rows(&fixtures, "exportImport")
        .iter()
        .find(|row| row["id"] == "task-states")
        .unwrap();
    let html = to_html(&text(fixture, "source"));
    assert!(html.contains("task-list"));
    for state in ["x", "-", "_", ">", "?"] {
        assert!(attrs(&html)
            .iter()
            .any(|(_, name, value)| name == "data-task-state" && value == state));
    }
}

#[test]
fn shared_acceptance_cases_have_distinct_ids_and_assertions() {
    let fixtures = fixtures();
    let mut ids = std::collections::HashSet::new();
    for (key, minimum) in [
        ("exportImport", 24),
        ("importNegative", 8),
        ("collisionExport", 3),
        ("markdownExportImport", 8),
    ] {
        assert!(required_rows(&fixtures, key).len() >= minimum, "{key}");
        for row in required_rows(&fixtures, key) {
            assert!(ids.insert(text(row, "id")));
            assert!(row["source"].is_string() || row["html"].is_string());
            assert!(
                row["expectedNodes"]
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty())
                    || row["attribute"].is_string()
            );
        }
    }
}
