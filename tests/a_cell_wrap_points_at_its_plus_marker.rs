//! CARVE-P12-069: a `cell_wrap` is placed on the `+` of the continuation line
//! it joins, as carve-js places it (markup-carve/carve-js#3002).

fn wraps(source: &str) -> Vec<serde_json::Value> {
    let doc: serde_json::Value = serde_json::from_str(&carve::to_json(&carve::parse_with_options(
        source,
        &carve::Options::default().with_positions(true),
    )))
    .unwrap();
    let mut found = Vec::new();
    collect(&doc, &mut found);
    found
}

fn collect(value: &serde_json::Value, found: &mut Vec<serde_json::Value>) {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("type").and_then(|t| t.as_str()) == Some("cell_wrap") {
                found.push(value.clone());
            }
            map.values().for_each(|v| collect(v, found));
        }
        serde_json::Value::Array(items) => items.iter().for_each(|v| collect(v, found)),
        _ => {}
    }
}

fn place(line: u64, column: u64, offset: u64) -> serde_json::Value {
    serde_json::json!({
        "startLine": line, "endLine": line,
        "startColumn": column, "endColumn": column + 1,
        "startOffset": offset, "endOffset": offset + 1,
    })
}

#[test]
fn each_join_points_at_its_plus() {
    let found = wraps("| a | b |\n+ x\t| y\t|\n");
    assert_eq!(found.len(), 2);
    for wrap in &found {
        assert_eq!(wrap["pos"], place(2, 1, 10), "{wrap}");
    }
}

#[test]
fn an_indented_plus_keeps_its_column() {
    let found = wraps("| a |\n  + x |\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0]["pos"], place(2, 3, 8));
}

#[test]
fn a_join_into_a_rowspan_points_at_the_lower_rows_plus() {
    let found = wraps("| a | b |\n| ^ | c |\n+ x | y |\n");
    assert!(!found.is_empty());
    for wrap in &found {
        assert_eq!(wrap["pos"], place(3, 1, 20), "{wrap}");
    }
}

#[test]
fn a_join_without_a_tab_points_at_its_plus() {
    let found = wraps("| a | b |\n+ x | y |\n");
    assert_eq!(found.len(), 2);
    for wrap in &found {
        assert_eq!(wrap["pos"], place(2, 1, 10), "{wrap}");
    }
}
