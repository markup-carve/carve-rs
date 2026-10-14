use carve::{
    html_to_ast, html_to_carve, parse, to_html, to_json, HtmlImportMode, HtmlImportOptions,
};
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
            "<p><a href='/u' data-carve-ref='a\\'>t</a></p>",
            "<p><a href='/u' data-carve-ref='a{#b'>t</a> #}</p>",
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
        }
    }
}

#[test]
fn quoted_reference_labels_move_unspellable_titles_into_the_body() {
    let html = "<details><summary><a href='/u' data-carve-ref='a&quot;b'>t</a></summary><p>b</p></details>";
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
        assert!(result.value.contains("[t][a\"b]"), "{}", result.value);
        let value: Value = serde_json::from_str(&to_json(&parse(&result.value))).unwrap();
        let mut links = Vec::new();
        nodes(&value, "link", &mut links);
        assert_eq!(links[0]["href"], "/u");
        assert!(result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code.as_str() == "structure-unspellable"));
    }
}

#[test]
fn opening_brackets_in_carried_labels_do_not_break_the_enclosing_span() {
    let html = "<p><span class='c'>x <a href='/u' data-carve-ref='a[b'>t</a> y</span></p>";
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let ast = json(html, mode);
        let mut references = Vec::new();
        nodes(&ast, "link_reference_definition", &mut references);
        assert_eq!(references[0]["label"], "a[b");
        let result = html_to_carve(
            html,
            &HtmlImportOptions {
                mode,
                ..Default::default()
            },
        )
        .unwrap();
        let value: Value = serde_json::from_str(&to_json(&parse(&result.value))).unwrap();
        let mut spans = Vec::new();
        nodes(&value, "span", &mut spans);
        assert_eq!(spans.len(), 1, "{}", result.value);
        assert_eq!(spans[0]["attrs"]["classes"], serde_json::json!(["c"]));
        let mut links = Vec::new();
        nodes(&value, "link", &mut links);
        assert_eq!(links[0]["href"], "/u");
        assert!(result
            .report
            .diagnostics
            .iter()
            .any(|row| row.code.as_str() == "attribute-dropped"));
    }
}

#[test]
fn reference_source_recovery_preserves_destinations_across_carrier_contexts() {
    let labels = [
        "a\\b", "a\\", "a[b", "a`b", "a{#b", "a|b", "a b", " ", "a\"b",
    ];
    let mut checked = 0;
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        for label in labels {
            for image in [false, true] {
                let label_html = label.replace('&', "&amp;").replace('"', "&quot;");
                let child = if image {
                    format!("<img src='/u' alt='t' data-carve-ref='{label_html}'>")
                } else {
                    format!("<a href='/u' data-carve-ref='{label_html}'>t</a>")
                };
                for context in ["paragraph", "span", "table", "title"] {
                    let html = match context {
                        "span" => format!("<p><span class='c'>before {child} after</span></p>"),
                        "table" => format!("<table><tr><td>{child}</td><td>next</td></tr></table>"),
                        "title" => {
                            format!("<details><summary>{child}</summary><p>body</p></details>")
                        }
                        _ => format!("<p>{child}</p>"),
                    };
                    let ast = json(&html, mode);
                    let mut definitions = Vec::new();
                    nodes(&ast, "link_reference_definition", &mut definitions);
                    assert_eq!(
                        definitions.len(),
                        1,
                        "{mode:?} {context} {label:?} {image} {ast}"
                    );
                    assert_eq!(definitions[0]["label"], label);
                    let result = html_to_carve(
                        &html,
                        &HtmlImportOptions {
                            mode,
                            ..Default::default()
                        },
                    )
                    .unwrap_or_else(|error| {
                        panic!("{mode:?} {context} {label:?} {image}: {error:?}")
                    });
                    let value: Value =
                        serde_json::from_str(&to_json(&parse(&result.value))).unwrap();
                    let mut targets = Vec::new();
                    nodes(&value, if image { "image" } else { "link" }, &mut targets);
                    if image {
                        nodes(&value, "block_image", &mut targets);
                    }
                    assert_eq!(
                        targets.len(),
                        1,
                        "{mode:?} {context} {label:?} {image} {} {value}",
                        result.value
                    );
                    assert_eq!(targets[0][if image { "src" } else { "href" }], "/u");
                    if context == "span" {
                        let mut spans = Vec::new();
                        nodes(&value, "span", &mut spans);
                        assert_eq!(spans.len(), 1, "{}", result.value);
                        assert_eq!(spans[0]["attrs"]["classes"], serde_json::json!(["c"]));
                    }
                    if context == "table" {
                        let mut cells = Vec::new();
                        nodes(&value, "table_cell", &mut cells);
                        assert_eq!(cells.len(), 2, "{} {value}", result.value);
                    }
                    checked += 1;
                }
            }
        }
    }
    assert_eq!(checked, 216);
}

#[test]
fn manual_ast_references_refuse_noncanonical_or_missing_definitions() {
    for image in [false, true] {
        let node = if image {
            serde_json::json!({"type":"image","src":"/u","alt":"t","ref":"x"})
        } else {
            serde_json::json!({"type":"link","href":"/u","ref":"x","children":[{"type":"text","value":"t"}]})
        };
        for definition in [None, Some("/other")] {
            let mut children =
                vec![serde_json::json!({"type":"paragraph","children":[node.clone()]})];
            if let Some(href) = definition {
                children.push(
                    serde_json::json!({"type":"link_reference_definition","label":"x","href":href}),
                );
            }
            let document = carve::from_json(
                &serde_json::json!({"type":"document","srcByteLength":0,"children":children})
                    .to_string(),
            )
            .unwrap();
            assert!(matches!(
                carve::render_carve(&document),
                Err(carve::RenderCarveError::SourceUnspellable(_))
            ));
        }
        let document = carve::from_json(&serde_json::json!({"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[node]},{"type":"block_quote","children":[{"type":"link_reference_definition","label":"x","href":"/u"}]}]}).to_string()).unwrap();
        assert!(matches!(
            carve::render_carve(&document),
            Err(carve::RenderCarveError::SourceUnspellable(_))
        ));
    }
}

#[test]
fn manual_ast_references_refuse_invalid_definition_labels_and_flattened_definitions() {
    for image in [false, true] {
        for label in ["^n", "", "a\nb", "x"] {
            let node = if image {
                serde_json::json!({"type":"image","src":"/u","alt":"t","ref":label})
            } else {
                serde_json::json!({"type":"link","href":"/u","ref":label,"children":[{"type":"text","value":"t"}]})
            };
            let definition =
                serde_json::json!({"type":"link_reference_definition","label":label,"href":"/u"});
            let definition = if label == "x" {
                serde_json::json!({"type":"table","rows":[{"type":"table_row","cells":[{"type":"table_cell","header":false,"blocks":[definition]}]}]})
            } else {
                definition
            };
            let document = carve::from_json(&serde_json::json!({"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[node]},definition]}).to_string()).unwrap();
            assert!(
                matches!(
                    carve::render_carve(&document),
                    Err(carve::RenderCarveError::SourceUnspellable(_))
                ),
                "{label:?} {image}"
            );
        }
    }
}

#[test]
fn reference_definition_validation_and_note_precedence_match_the_reader() {
    for image in [false, true] {
        for (label, href, definition_label, with_note, refuses) in [
            ("a b", "/u", "a\nb", false, true),
            ("x", "/outer", "x", true, false),
            ("x", "/note", "x", true, true),
        ] {
            let node = if image {
                serde_json::json!({"type":"image","src":href,"alt":"t","ref":label})
            } else {
                serde_json::json!({"type":"link","href":href,"ref":label,"children":[{"type":"text","value":"t"}]})
            };
            let mut wire = serde_json::json!({"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[node]},{"type":"link_reference_definition","label":definition_label,"href":if with_note { "/outer" } else { "/u" }}]});
            if with_note {
                wire["children"].as_array_mut().unwrap().push(serde_json::json!({"type":"footnote","label":"n","children":[{"type":"link_reference_definition","label":"x","href":"/note"}]}));
            }
            let document = carve::from_json(&wire.to_string()).unwrap();
            let result = carve::render_carve(&document);
            if refuses {
                assert!(matches!(
                    result,
                    Err(carve::RenderCarveError::SourceUnspellable(_))
                ));
            } else {
                let source = result.unwrap();
                let value: Value = serde_json::from_str(&to_json(&parse(&source))).unwrap();
                let mut found = Vec::new();
                nodes(&value, if image { "image" } else { "link" }, &mut found);
                if image {
                    nodes(&value, "block_image", &mut found);
                }
                assert_eq!(found[0][if image { "src" } else { "href" }], href);
            }
        }
    }
}

#[test]
fn references_refuse_literal_definitions_and_unrepresented_inherited_attributes() {
    for image in [false, true] {
        for kind in ["literal", "attrs", "empty"] {
            let href = if kind == "empty" { "" } else { "/u" };
            let node = if image {
                serde_json::json!({"type":"image","src":href,"alt":"t","ref":"x"})
            } else {
                serde_json::json!({"type":"link","href":href,"ref":"x","children":[{"type":"text","value":"t"}]})
            };
            let mut definition =
                serde_json::json!({"type":"link_reference_definition","label":"x","href":href});
            if kind == "attrs" {
                definition["attrs"] = serde_json::json!({"classes":["c"]});
            }
            if kind == "literal" {
                definition = serde_json::json!({"type":"line_block","children":[definition]});
            }
            let document = carve::from_json(&serde_json::json!({"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[node]},definition]}).to_string()).unwrap();
            let result = carve::render_carve(&document);
            if kind == "empty" {
                assert!(result.is_ok(), "{image} {kind}: {result:?}");
            } else {
                assert!(
                    matches!(result, Err(carve::RenderCarveError::SourceUnspellable(_))),
                    "{image} {kind}"
                );
            }
        }
    }
}

#[test]
fn note_marker_line_definitions_refuse_and_inherited_classes_are_written_once() {
    for image in [false, true] {
        for note in [true, false] {
            let mut node = if image {
                serde_json::json!({"type":"image","src":"/u","alt":"t","ref":"x"})
            } else {
                serde_json::json!({"type":"link","href":"/u","ref":"x","children":[{"type":"text","value":"t"}]})
            };
            let mut definition =
                serde_json::json!({"type":"link_reference_definition","label":"x","href":"/u"});
            if note {
                definition =
                    serde_json::json!({"type":"footnote","label":"n","children":[definition]});
            } else {
                node["attrs"] = serde_json::json!({"classes":["c"]});
                definition["attrs"] = serde_json::json!({"classes":["c"]});
            }
            let document = carve::from_json(&serde_json::json!({"type":"document","srcByteLength":0,"children":[{"type":"paragraph","children":[node]},definition]}).to_string()).unwrap();
            let result = carve::render_carve(&document);
            if note {
                assert!(matches!(
                    result,
                    Err(carve::RenderCarveError::SourceUnspellable(_))
                ));
            } else {
                let source = result.unwrap();
                let value: Value = serde_json::from_str(&to_json(&parse(&source))).unwrap();
                let mut found = Vec::new();
                nodes(&value, if image { "image" } else { "link" }, &mut found);
                if image {
                    nodes(&value, "block_image", &mut found);
                }
                assert_eq!(found[0]["attrs"]["classes"], serde_json::json!(["c"]));
            }
        }
    }
}

#[test]
fn parsed_nested_and_note_references_keep_raw_spelling_and_duplicate_precedence() {
    for (source, href) in [
        ("> [x]: /u\n\n[t][x]\n", "/u"),
        ("[^n]: note\n\n  [x]: /u\n\n[t][x]\n\n[^n]\n", "/u"),
        ("::: box\n[x]: /u\n:::\n\n[t][x]\n", "/u"),
        ("[x]: /a\n[x]: /b\n\n[t][x]\n", "/b"),
        ("- [x]: /u\n\n[t][x]\n", "/u"),
    ] {
        let document = parse(source);
        let before: Value = serde_json::from_str(&to_json(&document)).unwrap();
        let mut links = Vec::new();
        nodes(&before, "link", &mut links);
        assert_eq!(links.len(), 1, "{source}");
        assert_eq!(links[0]["href"], href, "{source}");
        assert!(links[0]["rawRef"].is_string(), "{source} {before}");
        let formatted = carve::render_carve(&document).unwrap();
        let restored = parse(&formatted);
        assert_eq!(to_html(source), to_html(&formatted), "{source} {formatted}");
        let after: Value = serde_json::from_str(&to_json(&restored)).unwrap();
        let mut links_after = Vec::new();
        nodes(&after, "link", &mut links_after);
        assert_eq!(links_after[0]["href"], href);
        assert_eq!(links_after[0]["rawRef"], links[0]["rawRef"]);
    }
}
