use carve::{HtmlImportMode, HtmlImportOptions, Options};
use serde_json::Value;

fn fixtures() -> Value {
    serde_json::from_str(include_str!("fixtures/roundtrip-carriers-2.json")).unwrap()
}

fn projection(tree: &Value, expected: &Value) {
    fn nodes<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
        match value {
            Value::Object(object) => {
                if object.contains_key("type") {
                    out.push(value);
                }
                for value in object.values() {
                    nodes(value, out);
                }
            }
            Value::Array(array) => {
                for value in array {
                    nodes(value, out);
                }
            }
            _ => {}
        }
    }
    let mut remaining = Vec::new();
    nodes(tree, &mut remaining);
    for expected in expected.as_array().unwrap() {
        let index = remaining
            .iter()
            .position(|node| {
                expected.as_object().unwrap().iter().all(|(path, value)| {
                    let field = path.split('.').fold(*node, |node, key| {
                        if let Ok(index) = key.parse::<usize>() {
                            &node[index]
                        } else {
                            &node[key]
                        }
                    });
                    field == value
                })
            })
            .unwrap_or_else(|| panic!("missing {expected} in {tree}"));
        remaining.remove(index);
    }
}

#[test]
fn shared_new_markdown_carriers_restore_nodes_and_html() {
    for fixture in fixtures()["markdownExportImport"].as_array().unwrap() {
        let source = fixture["source"].as_str().unwrap();
        projection(
            &serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap(),
            &fixture["expectedNodes"],
        );
        let ordinary = carve::to_markdown(source);
        assert_eq!(
            ordinary,
            carve::to_markdown_with_options(source, &Options::default().with_roundtrip(false))
        );
        assert!(!ordinary.contains("<!-- carve:"));
        let exported =
            carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
        for fragment in fixture["carrierFragments"].as_array().unwrap() {
            assert!(
                exported.contains(fragment.as_str().unwrap()),
                "{fixture}: {exported}"
            );
        }
        let imported = carve::migrate_markdown(&exported);
        assert!(
            !imported
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "carrier-markers-damaged"),
            "{exported}: {:?}",
            imported.report
        );
        projection(
            &serde_json::from_str(&carve::to_json(&carve::parse(&imported.value))).unwrap(),
            &fixture["expectedNodes"],
        );
        assert_eq!(
            carve::to_html(&imported.value).trim(),
            carve::to_html(source).trim(),
            "{fixture}: {exported} => {}",
            imported.value
        );
    }
}

#[test]
fn shared_code_label_is_consumed_in_every_html_mode_and_spelling() {
    for fixture in fixtures()["exportImport"].as_array().unwrap() {
        let source = fixture["source"].as_str().unwrap();
        assert!(!carve::to_html(source).contains("data-carve-label"));
        let html = carve::to_html_with_options(source, &Options::default().with_roundtrip(true));
        assert!(html.contains("<pre data-carve-label=\"Example\"><code class=\"language-php\">"));
        assert!(!html.contains("data-carve-src"));
        for mode in [
            HtmlImportMode::Safe,
            HtmlImportMode::Semantic,
            HtmlImportMode::Roundtrip,
        ] {
            for html in [
                html.clone(),
                html.replace("data-carve-label", "data-djot-label"),
            ] {
                let options = HtmlImportOptions {
                    mode,
                    ..Default::default()
                };
                let ast = carve::html_to_ast(&html, &options).unwrap().value;
                projection(
                    &serde_json::from_str(&carve::to_json(&ast)).unwrap(),
                    &fixture["expectedNodes"],
                );
                let imported = carve::html_to_carve(&html, &options).unwrap().value;
                assert!(
                    !imported.contains("data-carve-label") && !imported.contains("data-djot-label")
                );
                assert_eq!(carve::to_html(&imported), carve::to_html(source));
            }
        }
    }
}

#[test]
fn new_marker_sets_match_the_reference_recovery() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/previous_marker_recovery_wave.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let result = carve::migrate_markdown(source);
        let codes: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .map(|row| row.code.as_str())
            .filter(|code| code.starts_with("carrier-"))
            .collect();
        let expected: Vec<_> = row["codes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|code| code.as_str().unwrap())
            .collect();
        assert_eq!(codes, expected, "{source}: {:?}", result.report);
        assert_eq!(
            carve::to_html(&result.value),
            carve::to_html(row["imported"].as_str().unwrap()),
            "{source}"
        );
        assert!(!result.value.contains("CARVECARRIER"));
    }
}

#[test]
fn new_carriers_keep_their_hosts_and_replace_only_the_recorded_paragraph() {
    for source in [
        "> {.note}\n> paragraph\n",
        "- Item.\n\n  {.note}\n  paragraph\n",
        "> :: *Term*\n> : Definition.\n",
        "- Item.\n\n  :: Term\n  : Definition.\n",
        "> > quote\n> ^ attribution\n",
        "- Item.\n\n  > quote\n  ^ attribution\n",
        ":: One\n: First.\n: Second.\n:: Two\n: Third.\n",
        ":: Term\n: First paragraph.\n\n  Second paragraph.\n\nAfter.\n",
        "```rs\nx\n```\n^ Listing *one*\n",
        "![alt](x.png)\n^ Image caption\n",
        "$$`x`\n^ Equation caption\n",
        "::: figure\n![a](a.png)\n:::\n^ Group caption\n",
        "- {.fancy}\n  ::: note\n  body\n  :::\n",
        "::: figure\n:::: panel\n![a](a.png)\n^ Panel caption\n::::\n:::\n^ Group caption\n",
        "::: \\\nline one\nline two\n:::\n",
        "{.note}\n::: |\nline one\nline two\n:::\n",
        "{title=\"café --> text\"}\nparagraph\n",
        "> quote\n^ Caption\n\nCaption\n",
    ] {
        let exported =
            carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
        let imported = carve::migrate_markdown(&exported);
        assert!(
            !imported
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "carrier-markers-damaged"),
            "{source} => {exported}: {:?}",
            imported.report
        );
        assert_eq!(
            carve::to_html(&imported.value),
            carve::to_html(source),
            "{source} => {exported} => {}",
            imported.value
        );
    }
}

#[test]
fn html_label_validation_precedence_and_attribute_placement() {
    for mode in [
        HtmlImportMode::Safe,
        HtmlImportMode::Semantic,
        HtmlImportMode::Roundtrip,
    ] {
        let options = HtmlImportOptions {
            mode,
            ..Default::default()
        };
        let empty = carve::html_to_carve(
            "<pre data-carve-label=\"\"><code>x\n</code></pre>",
            &options,
        )
        .unwrap();
        let carve::BlockNode::CodeBlock(code) = &carve::parse(&empty.value).children[0] else {
            panic!("{}", empty.value);
        };
        assert_eq!(code.label.as_deref(), Some(""));
        assert!(empty.report.diagnostics.is_empty());

        for bad in ["bad]label", "line&#10;break", "line&#13;break"] {
            let html = format!("<pre data-carve-label=\"{bad}\"><code>x\n</code></pre>");
            let imported = carve::html_to_carve(&html, &options).unwrap();
            let carve::BlockNode::CodeBlock(code) = &carve::parse(&imported.value).children[0]
            else {
                panic!("{}", imported.value);
            };
            assert_eq!(code.label, None);
            assert!(!imported.value.contains("data-carve-label"));
            assert!(!imported.report.diagnostics.is_empty());
        }
        let imported = carve::html_to_carve(
            "<pre data-carve-label=\"new\" data-djot-label=\"old\"><code>x</code></pre>",
            &options,
        )
        .unwrap()
        .value;
        assert!(imported.contains("[new]"));
        for html in [
            "<pre data-carve-label=\"unused\">x</pre>",
            "<p data-djot-label=\"unused\">x</p>",
        ] {
            let imported = carve::html_to_carve(html, &options).unwrap().value;
            assert!(
                !imported.contains("data-carve-label") && !imported.contains("data-djot-label")
            );
        }
    }
    let html = carve::to_html_with_options(
        "{.sample}\n```php \"file.php\" [Example]\nx\n```\n",
        &Options::default().with_roundtrip(true),
    );
    assert!(
        html.contains("<pre title=\"file.php\" data-carve-label=\"Example\" class=\"sample\">"),
        "{html}"
    );
}

#[test]
fn new_marker_shapes_in_verbatim_content_are_never_read() {
    for marker in ["{#id}", ":: Term", ":", "", "^ Caption"] {
        let line = if marker.is_empty() {
            "<!-- carve: -->".to_owned()
        } else {
            format!("<!-- carve: {marker} -->")
        };
        for markdown in [
            format!("```text\n{line}\n```\n"),
            format!("    {line}\n"),
            format!("- item\n\n      {line}\n"),
        ] {
            let imported = carve::migrate_markdown(&markdown);
            assert!(!imported
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "carrier-markers-damaged"));
            assert!(imported.value.contains(&line), "{}", imported.value);
        }
    }
}

#[test]
fn dropped_attribute_lines_cover_core_blocks_and_skip_generated_heading_ids() {
    for source in [
        "{#h .title}\n# Heading\n",
        "{.quote}\n> body\n",
        "{.list}\n- one\n- two\n",
        "{.table}\n|= Header |\n| value |\n",
        "{.code}\n```rs\nx\n```\n",
    ] {
        let exported =
            carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
        let imported = carve::migrate_markdown(&exported);
        assert_eq!(
            carve::to_html(&imported.value),
            carve::to_html(source),
            "{source} => {exported} => {}",
            imported.value
        );
    }
    assert!(!carve::to_markdown_with_options(
        "# Heading\n",
        &Options::default().with_roundtrip(true)
    )
    .contains("<!-- carve:"));
}

#[test]
fn definitions_keep_verbatim_lines_and_container_bodies() {
    for source in [
        ":: Term\n: ```text\n  :: not a term\n  :\n  <!-- carve: ^ not a caption -->\n  ```\n",
        ":: Term\n: ::: note\n  body\n  :::\n",
        "> :: Term\n> : ```text\n>   :: literal\n>   :\n>   ```\n",
    ] {
        let exported =
            carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
        let imported = carve::migrate_markdown(&exported);
        assert!(
            !imported
                .report
                .diagnostics
                .iter()
                .any(|row| row.code == "carrier-markers-damaged"),
            "{source} => {exported}: {:?}",
            imported.report
        );
        assert_eq!(
            carve::to_html(&imported.value),
            carve::to_html(source),
            "{source} => {exported} => {}",
            imported.value
        );
    }
}

#[test]
fn fence_titles_take_a_marker_after_any_attribute_marker() {
    let options = Options::default().with_roundtrip(true);
    let plain = carve::to_markdown_with_options("```rs \"file.rs\"\nx\n```\n", &options);
    assert!(
        plain.starts_with("<!-- carve: ```rs \"file.rs\" -->\n"),
        "{plain}"
    );
    let exported =
        carve::to_markdown_with_options("{.code}\n```rs \"file.rs\"\nx\n```\n", &options);
    assert!(exported.contains("<!-- carve: {.code} -->"), "{exported}");
    assert!(
        exported.contains("<!-- carve: ```rs \"file.rs\" -->"),
        "{exported}"
    );
}

#[test]
fn definition_carriers_keep_nested_lists_and_outer_attributes() {
    let options = Options::default().with_roundtrip(true);
    for (source, expected, restored) in [
        (
            ":: Carve\n: A markup language.\n  :: inner\n  : def\n",
            "<!-- carve: :: Carve -->\n**Carve**\n\n<!-- carve: : -->\nA markup language.\n\n<!-- carve:2 :: inner -->\n**inner**\n\n<!-- carve:2 : -->\ndef\n\n<!-- carve:2 -->\n\n<!-- carve: -->\n",
            ":: Carve\n: A markup language.\n  :: inner\n  : def\n",
        ),
        (
            ":: Carve\n: A markup language.\n",
            "<!-- carve: :: Carve -->\n**Carve**\n\n<!-- carve: : -->\nA markup language.\n\n<!-- carve: -->\n",
            ":: Carve\n: A markup language.\n",
        ),
        (
            "{.c}\n:: Carve\n: A markup language.\n",
            "<!-- carve: {.c} -->\n<!-- carve: :: Carve -->\n**Carve**\n\n<!-- carve: : -->\nA markup language.\n\n<!-- carve: -->\n",
            "{.c}\n:: Carve\n: A markup language.\n",
        ),
    ] {
        let exported = carve::to_markdown_with_options(source, &options);
        assert_eq!(exported, expected);
        let imported = carve::migrate_markdown(&exported);
        assert_eq!(imported.value, restored);
        assert_eq!(carve::to_html(&imported.value), carve::to_html(restored));
        assert!(!imported.report.diagnostics.iter().any(|row| row.code == "carrier-markers-damaged"));
    }
}

#[test]
fn marker_placeholders_preserve_breaks_and_list_equation_figures() {
    for (source, restored) in [
        ("{.x}\n---\n", "{.x}\n---\n"),
        ("{.x}\n***\n", "{.x}\n---\n"),
        ("- {.x}\n  $$`x`\n  ^ Cap\n", "- {.x}\n  $$`x`\n  ^ Cap\n"),
        ("- $$`x`\n  ^ Cap\n", "- $$`x`\n  ^ Cap\n"),
    ] {
        let exported =
            carve::to_markdown_with_options(source, &Options::default().with_roundtrip(true));
        let imported = carve::migrate_markdown(&exported);
        assert_eq!(
            imported.value.trim(),
            restored.trim(),
            "{source} => {exported}: {:?}",
            imported.report
        );
        assert_eq!(
            carve::to_html(&imported.value),
            carve::to_html(source),
            "{source} => {exported} => {}",
            imported.value
        );
        assert!(!imported
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "carrier-markers-damaged"));
    }
}
