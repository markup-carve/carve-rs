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
        (
            "{body-rows=1 body-header-rows=1}\n| A | B |\n| m | < |\n".into(),
            vec![]
        )
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
