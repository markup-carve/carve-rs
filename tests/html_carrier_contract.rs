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
