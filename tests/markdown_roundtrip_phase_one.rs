use carve::{migrate_markdown, parse, to_json, to_markdown_with_options, Options};
use serde_json::Value;

fn tree(source: &str) -> Value {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(fields) => {
                fields.remove("pos");
                fields.remove("srcByteLength");
                for child in fields.values_mut() {
                    strip(child);
                }
            }
            Value::Array(children) => children.iter_mut().for_each(strip),
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
