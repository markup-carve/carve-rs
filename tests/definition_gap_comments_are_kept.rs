//! A `%%%` span between definition entries stays in the entry before it, as
//! carve-js reads it, and `fmt` writes it back (carve-rs#3524).

fn comments(source: &str) -> Vec<(String, String)> {
    fn walk(node: &serde_json::Value, owner: &str, found: &mut Vec<(String, String)>) {
        match node {
            serde_json::Value::Object(fields) => {
                let kind = fields.get("type").and_then(|v| v.as_str()).unwrap_or(owner);
                if kind == "comment" {
                    found.push((owner.to_owned(), fields["content"].as_str().unwrap().to_owned()));
                }
                for (key, value) in fields {
                    if key != "pos" {
                        walk(value, kind, found);
                    }
                }
            }
            serde_json::Value::Array(values) => values.iter().for_each(|v| walk(v, owner, found)),
            _ => {}
        }
    }
    let doc: serde_json::Value =
        serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    let mut found = Vec::new();
    walk(&doc, "document", &mut found);
    found
}

#[test]
fn a_span_after_a_term_stays_in_the_term() {
    let source = ":: a\n%%%\nx\n%%%\n: d\n";
    assert_eq!(comments(source), [("definition_term".into(), "x".into())]);
    assert_eq!(carve::to_carve(source), source);
}

#[test]
fn a_span_after_a_description_stays_in_the_description() {
    for source in [
        ":: a\n: d\n\n  %%%\n  x\n  %%%\n: e\n",
        ":: a\n: d\n\n  %%%\n  x\n  %%%\n:: b\n: e\n",
    ] {
        assert_eq!(
            comments(source),
            [("definition_description".into(), "x".into())],
            "{source:?}"
        );
        assert_eq!(carve::to_carve(source), source);
    }
}

#[test]
fn a_nested_term_span_keeps_its_item_column() {
    let source = "- a\n  :: term\n  %%%\n    x\n  %%%\n  : def\n";
    assert_eq!(comments(source), [("definition_term".into(), "  x".into())]);
    assert_eq!(carve::to_carve(source), source);
}

#[test]
fn the_owner_span_covers_the_kept_comment() {
    let doc: serde_json::Value = serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        ":: a\n%%%\nx\n%%%\n: d\n",
        &carve::Options::new().with_positions(true),
    )))
    .unwrap();
    let term = &doc["children"][0]["items"][0];
    assert_eq!(term["type"], "definition_term");
    assert_eq!(term["pos"]["endOffset"], 14);
    assert_eq!(term["children"][1]["pos"]["endOffset"], 14);
}
