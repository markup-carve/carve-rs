use serde_json::Value;

fn visit(value: &Value, source: &str, texts: &mut usize) {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("text") {
                *texts += 1;
                let text = object["value"].as_str().unwrap();
                let pos = object
                    .get("pos")
                    .unwrap_or_else(|| panic!("unplaced {text:?}"));
                let start = pos["startOffset"].as_u64().unwrap() as usize;
                let end = pos["endOffset"].as_u64().unwrap() as usize;
                assert_eq!(
                    source
                        .chars()
                        .skip(start)
                        .take(end - start)
                        .collect::<String>(),
                    text
                );
            }
            if object.get("type").and_then(Value::as_str) == Some("non_breaking_space") {
                assert!(object.get("pos").is_none());
            }
            for (key, child) in object {
                if key != "pos" {
                    visit(child, source, texts);
                }
            }
        }
        Value::Array(values) => {
            for child in values {
                visit(child, source, texts);
            }
        }
        _ => {}
    }
}

#[test]
fn unchanged_runs_beside_tabs_select_exact_source_slices() {
    for source in [
        "::: |\na\tb\n:::\n",
        "::: |\nwide\t\tgap\n\tlead\n:::\n",
        "> ::: |\n> \t😀 *bold* and /italic/\n> :::\n",
        "- item\n\n  ::: |\n  \ttext\n  :::\n",
        "::: |\n*a\tb*\n%% comment\nlast\n:::\n",
        "::: |\n\t[@x]\n:::\n\n[@x]: source\n",
    ] {
        let document =
            carve::parse_with_options(source, &carve::Options::new().with_positions(true));
        let tree: Value = serde_json::from_str(&carve::ast_json::to_json(&document)).unwrap();
        let mut texts = 0;
        visit(&tree, source, &mut texts);
        assert!(texts > 0);
    }
}

#[test]
fn a_leading_tab_stays_inside_the_stanza_span() {
    let source = "> ::: |\n> \t😀 *bold*\n> :::\n";
    let document = carve::parse_with_options(source, &carve::Options::new().with_positions(true));
    let tree: Value = serde_json::from_str(&carve::ast_json::to_json(&document)).unwrap();
    let paragraph = &tree["children"][0]["children"][0]["children"][0];
    assert_eq!(paragraph["type"], "paragraph");
    assert_eq!(paragraph["pos"]["startOffset"], 10);
    assert_eq!(paragraph["pos"]["startColumn"], 3);
}
