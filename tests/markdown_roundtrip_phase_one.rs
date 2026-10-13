use carve::{migrate_markdown, parse, to_json, to_markdown_with_options, Options};
use serde_json::Value;

fn tree(source: &str) -> Value {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.remove("pos");
                fields.remove("srcByteLength");
                fields.remove("ref");
                fields.remove("rawRef");
                if let Some(Value::String(href)) = fields.get_mut("href") {
                    *href = href
                        .replace("%28", "(")
                        .replace("%29", ")")
                        .replace("%5C", "\\");
                }
                for child in fields.values_mut() {
                    strip(child);
                }
            }
            Value::Array(children) => {
                children.retain(|child| child["type"] != "link_reference_definition");
                children.iter_mut().for_each(strip);
            }
            _ => {}
        }
    }
    let mut value = serde_json::from_str(&to_json(&parse(source))).unwrap();
    strip(&mut value);
    value
}

fn roundtrip(source: &str) -> String {
    let exported = to_markdown_with_options(source, &Options::default().with_roundtrip(true));
    let imported = migrate_markdown(&exported);
    assert!(
        !imported
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "carrier-markers-damaged"),
        "{source}\n{exported}\n{:?}",
        imported.report
    );
    assert_eq!(
        tree(source),
        tree(&imported.value),
        "{source}\n{exported}\n{}",
        imported.value
    );
    exported
}

#[test]
fn definition_lists_keep_terms_without_definitions_and_consecutive_terms() {
    for source in [
        ":: Carve\n",
        ":: Carve\n:: Djot\n: Markup languages.\n",
        ":: First\n:: Second\n",
        ":: First\n: Definition.\n\n:: Last\n",
        ":: Term\n: [unused]: /u\n",
        ":: Term\n: [^n]: note.\n",
    ] {
        let exported = roundtrip(source);
        assert_eq!(
            exported.matches("<!-- carve: :: ").count(),
            parse(source)
                .children
                .iter()
                .filter_map(|block| match block {
                    carve::BlockNode::DefinitionList(list) => Some(
                        list.items
                            .iter()
                            .map(|item| item.terms.len())
                            .sum::<usize>()
                    ),
                    _ => None,
                })
                .sum::<usize>()
        );
        assert_eq!(exported.matches("<!-- carve: -->").count(), 1);
    }
}

#[test]
fn corpus_repros_keep_their_trees_without_damaged_markers() {
    for slug in [
        "222-a-tab-as-the-first-character-of-a-definition-term",
        "227-a-definition-inside-a-definition-list-dd-is-collected-and-the-entry-keeps-no-trace",
        "227-a-definition-inside-a-definition-list-dd-is-collected-and-the-entry-keeps-no-trace-2",
        "116-fence-opener-with-a-nested-list-body-inside-a-list-item-6",
        "279-a-boundary-line-inside-an-open-fence-does-not-end-the-container-5",
        "279-a-boundary-line-inside-an-open-fence-does-not-end-the-container-10",
        "279-a-boundary-line-inside-an-open-fence-does-not-end-the-container-11",
        "160-indented-colon-fence-blocks-stay-literal-4",
        "170-headings-inside-containers-are-not-wrapped",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/spec/tests/corpus")
            .join(format!("{slug}.crv"));
        let source = std::fs::read_to_string(path).unwrap();
        let exported = roundtrip(&source);
        fn containers(value: &Value) -> usize {
            match value {
                Value::Object(fields) => {
                    usize::from(matches!(
                        fields.get("type").and_then(Value::as_str),
                        Some("div" | "admonition" | "directive" | "line_block" | "figure_group")
                    )) + fields.values().map(containers).sum::<usize>()
                }
                Value::Array(children) => children.iter().map(containers).sum(),
                _ => 0,
            }
        }
        assert_eq!(
            exported.matches("<!-- carve: :::").count(),
            2 * containers(&tree(&source)),
            "{slug}: {exported}"
        );
    }
    for entry in std::fs::read_dir(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spec/tests/corpus"),
    )
    .unwrap()
    {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "crv")
            && path
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .starts_with("109-link-destination-parentheses-balance")
        {
            roundtrip(&std::fs::read_to_string(path).unwrap());
        }
    }
}

#[test]
fn restored_fences_keep_code_backslashes_and_lists_after_headings() {
    for source in [
        "::: note\n```\na\n\\- literal code\n```\n:::\n",
        ":::\n```\na\n\\- literal code\n```\n:::\n",
        "::: note\n# heading\n\n- item\n:::\n",
    ] {
        roundtrip(source);
    }
}

#[test]
fn reference_only_definition_matches_the_reference_export_and_import() {
    let source = ":: Term\n: [unused]: /u\n";
    let markdown = to_markdown_with_options(source, &Options::default().with_roundtrip(true));
    assert_eq!(
        markdown,
        "<!-- carve: :: Term -->\n**Term**\n\n<!-- carve: : -->\n<!-- carve: -->\n\n[unused]: /u\n"
    );
    let imported = migrate_markdown(&markdown);
    assert_eq!(imported.value, ":: Term\n: {empty}\n\n[unused]: /u\n");
    assert!(!imported
        .report
        .diagnostics
        .iter()
        .any(|row| row.code == "carrier-markers-damaged"));
}

#[test]
fn invalid_markers_recover_per_payload_against_the_reference() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/carrier_marker_recovery_wave.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let imported = migrate_markdown(row["markdown"].as_str().unwrap());
        assert_eq!(
            imported.value,
            row["value"].as_str().unwrap(),
            "{}",
            row["markdown"]
        );
        let actual: Vec<_> = imported
            .report
            .diagnostics
            .iter()
            .filter(|entry| entry.code.starts_with("carrier-marker"))
            .map(|entry| entry.code.as_str())
            .collect();
        let expected: Vec<_> = row["losses"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["code"].as_str().unwrap())
            .collect();
        assert_eq!(actual, expected, "{}", row["markdown"]);
    }
}

#[test]
fn comment_handoffs_keep_the_following_paragraph_in_its_parent() {
    for slug in [
        "557-a-line-retained-after-a-comment-opens-a-paragraph-2",
        "570-an-at-column-comment-retains-nothing",
    ] {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/spec/tests/corpus")
                .join(format!("{slug}.crv")),
        )
        .unwrap();
        let exported = to_markdown_with_options(&source, &Options::default().with_roundtrip(true));
        let imported = migrate_markdown(&exported);
        assert_eq!(
            carve::to_html(&source),
            carve::to_html(&imported.value),
            "{slug}: {exported}\n{}",
            imported.value
        );
        assert!(!imported
            .report
            .diagnostics
            .iter()
            .any(|row| row.code == "carrier-markers-damaged"));
    }
}

#[test]
fn collected_footnotes_are_not_copied_into_empty_descriptions() {
    let source =
        ":: A\n: {empty}\n\n:: B\n: {empty}\n\nT[^n] and [^m].\n\n[^n]: one.\n\n[^m]: two.\n";
    let exported = roundtrip(source);
    let imported = migrate_markdown(&exported);
    assert_eq!(imported.value.matches("[^n]:").count(), 1);
    assert_eq!(imported.value.matches("[^m]:").count(), 1);
}

#[test]
fn unused_markdown_reference_fields_do_not_become_visible_text() {
    for (source, expected) in [
        (
            "[x]: <http://e.com/a b> \"T\"\n",
            "[x]: http://e.com/a%20b \"T\"\n",
        ),
        ("[x]: <>\n", ""),
        ("[a\\]b]: /u\n", ""),
        ("[x]: /u \"line1\nline2\"\n", ""),
    ] {
        let imported = migrate_markdown(source);
        assert_eq!(
            carve::to_html(&imported.value),
            carve::to_html(expected),
            "{source}: {}",
            imported.value
        );
        assert_eq!(tree(&imported.value), tree(expected), "{source}");
    }
}

#[test]
fn a_definition_a_link_consumed_retains_reference_source() {
    let imported = migrate_markdown("[ref]: https://example.org \"Title\"\n\nSee [ref].");
    assert_eq!(
        imported.value,
        "See [ref](https://example.org \"Title\").\n"
    );

    let imported = migrate_markdown("[ref]: /u\n[unused]: /v\n\nSee [REF][] and ![x][ref].\n");
    assert_eq!(
        imported.value,
        "See [REF][ref] and ![x](/u).\n\n[ref]: /u\n\n[unused]: /v\n"
    );
}

#[test]
fn imported_references_keep_links_when_labels_differ_in_case() {
    let source = "See [x][FOO] and [foo].\n\n[foo]: /u\n";
    let imported = migrate_markdown(source);
    assert_eq!(
        carve::to_html(&imported.value),
        "<p>See <a href=\"/u\">x</a> and <a href=\"/u\">foo</a>.</p>"
    );
    assert!(imported.value.contains("[x][foo]"), "{}", imported.value);
}

#[test]
fn imported_abbreviations_do_not_nest_explicit_html() {
    let source = "*[HTML]: HyperText\n\n<abbr title=\"Other\">HTML</abbr>\n";
    let imported = migrate_markdown(source);
    assert_eq!(
        carve::to_html(&imported.value),
        "<p><abbr title=\"Other\">HTML</abbr></p>"
    );
    let continuation = migrate_markdown("Text\n*[HTML]: HyperText\n");
    assert!(!continuation.value.contains("\n*[HTML]: HyperText"));
}

#[test]
fn a_dropped_spanned_marker_keeps_its_lines() {
    let source = "<!-- carve: {.a\n#b} -->\n";
    let imported = migrate_markdown(source);
    assert!(!imported.value.contains("CARVEMARKERBREAK"));
    assert!(imported.value.contains("<!-- carve: {.a\n#b} -->"));
}
