//! C2-W-145: a span count that no mark cell covers is written with the marks a parse
//! reads back as that count (carve#6269).

use carve::api::json::call;
use serde_json::{Value, json};

fn import(html: &str) -> (String, Vec<String>) {
    let r: Value = serde_json::from_str(&call(
        &json!({"call": "importDocument", "from": "html", "source": html}).to_string(),
    ))
    .unwrap();
    let losses = r["ok"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"].as_str().unwrap().ends_with("unspellable"))
        .map(|d| d["path"].as_str().unwrap().to_string())
        .collect();
    (r["ok"]["source"].as_str().unwrap().to_string(), losses)
}

#[test]
fn an_imported_colspan_is_written_with_a_mark() {
    assert_eq!(
        import("<table><tr><th>A</th><th>B</th></tr><tr><td colspan=\"2\">m</td></tr></table>"),
        ("|= A |= B |\n| m | < |\n".into(), vec![])
    );
    assert_eq!(
        import(
            "<table><thead><tr><th colspan=\"2\">H</th></tr></thead><tbody><tr><td>a</td><td>b</td></tr></tbody></table>"
        ),
        ("|= H |= < |\n| a | b |\n".into(), vec![])
    );
}

#[test]
fn an_imported_rowspan_is_written_at_its_column() {
    assert_eq!(
        import("<table><tr><td>x</td><td rowspan=\"2\">a</td></tr><tr><td>c</td></tr></table>").0,
        "| x | a |\n| c | ^ |\n"
    );
    assert_eq!(
        import(
            "<table><tr><td rowspan=\"3\">a</td><td>b</td><td rowspan=\"2\">c</td></tr><tr><td>d</td></tr><tr><td>e</td><td>f</td></tr></table>"
        )
        .0,
        "| a | b | c |\n| ^ | d | ^ |\n| ^ | e | f |\n"
    );
}

#[test]
fn a_cell_with_both_counts_reports_its_rowspan() {
    assert_eq!(
        import(
            "<table><tr><td rowspan=\"2\" colspan=\"2\">a</td><td>b</td></tr><tr><td>c</td></tr><tr><td>1</td><td>2</td><td>3</td></tr></table>"
        ),
        (
            "| a | < | b |\n| c |\n| 1 | 2 | 3 |\n".into(),
            vec!["/children/0/children/0/children/0/children/0/rowspan".into()]
        )
    );
}

fn write(doc: &Value) -> (String, Vec<String>) {
    let r: Value = serde_json::from_str(&call(
        &json!({"call": "writeWithDiagnostics", "tree": doc}).to_string(),
    ))
    .unwrap();
    let losses = r["ok"]["diagnostics"]
        .as_array()
        .map(|d| {
            d.iter()
                .map(|e| e["code"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default();
    (r["ok"]["output"].as_str().unwrap_or("").to_string(), losses)
}

fn cell(value: &str) -> Value {
    json!({"type": "table_cell", "header": false, "children": [
        {"type": "cell_paragraph", "children": [{"type": "text", "value": value}]}]})
}

fn table(columns: usize, rows: Vec<Vec<Value>>) -> Value {
    json!({"type": "document", "astVersion": "2.0", "children": [{"type": "table",
        "columns": vec![json!({"type": "table_column"}); columns],
        "children": [{"type": "table_body", "children": rows.into_iter()
            .map(|cells| json!({"type": "table_row", "children": cells})).collect::<Vec<_>>()}]}]})
}

#[test]
fn a_count_with_some_of_its_marks_gets_only_the_rest() {
    let mark =
        |span: &str| json!({"type": "table_cell", "header": false, "span": span, "children": []});
    let mut a = cell("a");
    a["colspan"] = json!(3);
    assert_eq!(
        write(&table(3, vec![vec![a, mark("colspan")]])).0,
        "| a | < | < |\n"
    );
    let mut a = cell("a");
    a["rowspan"] = json!(3);
    assert_eq!(
        write(&table(
            2,
            vec![
                vec![a, cell("b")],
                vec![mark("rowspan"), cell("c")],
                vec![cell("d")]
            ]
        )),
        ("| a | b |\n| ^ | c |\n| ^ | d |\n".into(), vec![])
    );
}
