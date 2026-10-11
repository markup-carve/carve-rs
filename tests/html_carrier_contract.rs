use carve::{html_to_ast, html_to_carve, parse, to_json, HtmlImportMode, HtmlImportOptions};
use serde_json::Value;

fn nodes<'a>(value: &'a Value, kind: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Object(fields) => {
            if value["type"] == kind {
                out.push(value);
            }
            for child in fields.values() {
                nodes(child, kind, out);
            }
        }
        Value::Array(values) => {
            for value in values {
                nodes(value, kind, out);
            }
        }
        _ => {}
    }
}
fn json(html: &str, mode: HtmlImportMode) -> Value {
    let result = html_to_ast(
        html,
        &HtmlImportOptions {
            mode,
            ..Default::default()
        },
    )
    .unwrap();
    serde_json::from_str(&to_json(&result.value)).unwrap()
}

#[test]
fn reference_carriers_restore_definitions_and_source_in_every_mode() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        for prefix in ["carve", "djot"] {
            for (tag, html, kind, destination) in [
                ("a", "<p><a href='/u' title='title' data-PREFIX-ref='label'>text</a></p>", "link", "/u"),
                ("img", "<p>before <img src='/i' title='title' alt='image' data-PREFIX-ref='label'></p>", "image", "/i"),
            ] {
                let html = html.replace("PREFIX", prefix);
                let value = json(&html, mode);
                let mut definitions = Vec::new(); nodes(&value, "link_reference_definition", &mut definitions);
                assert_eq!(definitions.len(), 1, "{tag} {prefix} {mode:?}");
                assert_eq!(definitions[0]["label"], "label");
                assert_eq!(definitions[0]["href"], destination);
                let source = html_to_carve(&html, &HtmlImportOptions {mode, ..Default::default()}).unwrap().value;
                assert!(source.contains("][label]"), "{source}");
                let reparsed: Value = serde_json::from_str(&to_json(&parse(&source))).unwrap();
                let mut restored = Vec::new(); nodes(&reparsed, kind, &mut restored);
                assert_eq!(restored[0]["ref"], "label");
                assert_eq!(restored[0][if tag == "a" {"href"} else {"src"}], destination);
            }
        }
    }
}

#[test]
fn conflicting_reference_targets_keep_the_later_destination_inline() {
    let html = "<p><a href='/a' data-carve-ref='label'>a</a> <a href='/b' data-carve-ref='label'>b</a></p>";
    let value = json(html, HtmlImportMode::Safe);
    let mut links = Vec::new();
    nodes(&value, "link", &mut links);
    assert_eq!(links[0]["href"], "/a");
    assert_eq!(links[1]["href"], "/b");
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    assert!(result.value.contains("[a][label]"), "{}", result.value);
    assert!(result.value.contains("[b](/b)"), "{}", result.value);
    let value: Value = serde_json::from_str(&to_json(&parse(&result.value))).unwrap();
    let mut links = Vec::new();
    nodes(&value, "link", &mut links);
    assert_eq!(links[0]["href"], "/a");
    assert_eq!(links[1]["href"], "/b");
}

#[test]
fn abbreviation_templates_restore_only_definition_text() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        for prefix in ["carve", "djot"] {
            let html = format!("<p>HTML</p><template data-{prefix}-abbreviations>*[HTML]: Hyper &amp; Text\n*[CSS]: Style</template>");
            let value = json(&html, mode);
            let mut definitions = Vec::new();
            nodes(&value, "abbreviation_def", &mut definitions);
            assert_eq!(definitions.len(), 2, "{value}");
            assert_eq!(definitions[0]["expansion"], "Hyper & Text");
            let source = html_to_carve(
                &html,
                &HtmlImportOptions {
                    mode,
                    ..Default::default()
                },
            )
            .unwrap()
            .value;
            assert!(source.contains("*[HTML]: Hyper & Text"), "{source}");
            for payload in ["# injected", "<b>*[HTML]: unsafe</b>"] {
                let invalid = format!("<template data-{prefix}-abbreviations>{payload}</template>");
                let result = html_to_ast(
                    &invalid,
                    &HtmlImportOptions {
                        mode,
                        ..Default::default()
                    },
                )
                .unwrap();
                assert!(!result.report.diagnostics.is_empty());
                assert!(result.value.children.is_empty());
            }
        }
    }
}

#[test]
fn container_label_measurement_does_not_leave_reference_bindings_behind() {
    let html = "<div class='c'><p class='div-label'><a href='/u' data-carve-ref='x'>t</a></p><p>b <a href='/u' data-carve-ref='x'>later</a></p></div>";
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let result = html_to_carve(
            html,
            &HtmlImportOptions {
                mode,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(result.value.contains("[later][x]"), "{}", result.value);
        let value = json(html, mode);
        let mut definitions = Vec::new();
        nodes(&value, "link_reference_definition", &mut definitions);
        assert_eq!(definitions.len(), 1, "{value}");
        assert_eq!(definitions[0]["href"], "/u");
    }
}

#[test]
fn reference_labels_in_table_cells_keep_the_cell_boundary() {
    let html =
        "<table><tr><td><a href='/u' data-carve-ref='a|b'>t</a></td><td>next</td></tr></table>";
    let imported_ast = json(html, HtmlImportMode::Safe);
    let mut imported_links = Vec::new();
    nodes(&imported_ast, "link", &mut imported_links);
    assert_eq!(imported_links[0]["ref"], "a|b");
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    assert!(result
        .report
        .diagnostics
        .iter()
        .any(|row| row.code.as_str() == "attribute-dropped"));
    let source = result.value;
    let value: Value = serde_json::from_str(&to_json(&parse(&source))).unwrap();
    let mut links = Vec::new();
    nodes(&value, "link", &mut links);
    assert_eq!(links.len(), 1, "{source}\n{value}");
    assert_eq!(links[0]["href"], "/u", "{source}");
    assert!(links[0].get("ref").is_none(), "{source}");
    let mut cells = Vec::new();
    nodes(&value, "table_cell", &mut cells);
    assert_eq!(cells.len(), 2, "{source}\n{value}");
}

#[test]
fn collision_reports_keep_the_carrier_host_kind_and_attribute_name() {
    for (source, kind, attribute) in [
        ("{data-char=_}\n***\n", "block", "data-char"),
        (
            "[t][r]{data-carve-ref=authored}\n\n[r]: /u\n",
            "inline",
            "data-carve-ref",
        ),
        (
            "![a][r]{data-carve-ref=authored}\n\n[r]: /u\n",
            "block",
            "data-carve-ref",
        ),
    ] {
        let result = carve::to_html_with_options_and_report(
            source,
            &carve::Options::default().with_roundtrip(true),
            carve::CheckedRenderOptions::default(),
        )
        .unwrap();
        let loss = result
            .losses
            .iter()
            .find(|loss| loss.code == "carrier-attribute-dropped")
            .unwrap_or_else(|| panic!("{source} {:?}", result.losses));
        assert_eq!(loss.attribute.as_deref(), Some(attribute));
        assert_eq!(loss.node_type.as_str(), kind);
        assert_eq!(loss.target.as_str(), "html");
        assert!(loss.format.is_none());
    }
}

#[test]
fn unspellable_reference_fields_recover_inline_only_on_the_source_exit() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        for html in [
            "<p><a href='/a b' data-carve-ref='x'>t</a></p>",
            "<p><a href='/a\n# injected' data-carve-ref='x'>t</a></p>",
            "<p><a href='/u' title='a\nb' data-carve-ref='x'>t</a></p>",
            "<p><a href='/u' data-carve-ref='a`b'>t</a> and <code>c</code></p>",
            "<table><tr><td><img src='/i' alt='a' data-carve-ref='p|q'></td><td>next</td></tr></table>",
        ] {
            let value = json(html, mode);
            let mut definitions = Vec::new(); nodes(&value, "link_reference_definition", &mut definitions);
            assert_eq!(definitions.len(), 1, "{value}");
            let result = html_to_carve(html, &HtmlImportOptions {mode, ..Default::default()}).unwrap();
            assert!(result.report.diagnostics.iter().any(|row| row.code.as_str() == "attribute-dropped"));
            let reparsed: Value = serde_json::from_str(&to_json(&parse(&result.value))).unwrap();
            let mut definitions = Vec::new(); nodes(&reparsed, "link_reference_definition", &mut definitions);
            assert!(definitions.is_empty(), "{}\n{reparsed}", result.value);
            let mut headings = Vec::new(); nodes(&reparsed, "heading", &mut headings);
            assert!(headings.is_empty(), "{}", result.value);
            let mut links = Vec::new(); nodes(&reparsed, "link", &mut links);
            let mut images = Vec::new(); nodes(&reparsed, "image", &mut images);
            nodes(&reparsed, "block_image", &mut images);
            assert_eq!(links.len() + images.len(), 1, "{}\n{reparsed}", result.value);
        }
    }
}

#[test]
fn abbreviation_carriers_are_hoisted_and_do_not_import_hidden_frontmatter() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let html = "<blockquote><p>HTML</p><template data-carve-abbreviations>*[HTML]: Hyper</template></blockquote>";
        let result = html_to_carve(
            html,
            &HtmlImportOptions {
                mode,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            result.value.contains("\n\n*[HTML]: Hyper"),
            "{}",
            result.value
        );
        assert!(!result.value.contains("> *[HTML]"), "{}", result.value);
        let html = "<template data-carve-abbreviations>---yaml\nsecret: value\n---\n*[HTML]: Hyper</template>";
        let result = html_to_ast(
            html,
            &HtmlImportOptions {
                mode,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(result.value.children.is_empty());
        assert!(!result.report.diagnostics.is_empty());
    }
}

#[test]
fn carried_references_do_not_mask_unspellable_container_titles() {
    for title in ["say &quot;hi&quot;", "line\nbreak"] {
        for mode in [
            HtmlImportMode::Safe,
            HtmlImportMode::Semantic,
            HtmlImportMode::Roundtrip,
        ] {
            let html = format!("<details><summary><a href='/u' data-carve-ref='x'>t</a> {title}</summary><p>b</p></details>");
            let result = html_to_carve(
                &html,
                &HtmlImportOptions {
                    mode,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(result.value.contains("[t][x]"), "{}", result.value);
            assert!(result.value.contains("[x]: /u"), "{}", result.value);
            let reparsed: Value = serde_json::from_str(&to_json(&parse(&result.value))).unwrap();
            let mut links = Vec::new();
            nodes(&reparsed, "link", &mut links);
            assert_eq!(links[0]["href"], "/u");
            assert!(result
                .diagnostics
                .iter()
                .any(|row| row.code == "structure-unspellable"));
        }
    }
}
