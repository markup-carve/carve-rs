//! The `listTable` import option (C2-M-112) and the carve#6853 ruling: a row with no cell and no
//! covered slot gives no item, and the list table counts read only the rows that give one.

use carve::api::{self, ImportOptions, json::call};
use serde_json::{Value, json};

fn import(html: &str) -> (String, Vec<String>) {
    let options = ImportOptions {
        list_table: true,
        ..ImportOptions::default()
    };
    let got = api::import_document(html, &options).expect("import");
    let codes = got.diagnostics.iter().map(|d| d.code.clone()).collect();
    (got.source, codes)
}

fn reply(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

#[test]
fn an_empty_foot_row_counts_in_no_footer_rows() {
    let (source, codes) = import(
        "<table><tbody><tr><td><ul><li>a</li></ul></td></tr><tr><td>b</td></tr></tbody><tfoot><tr></tr><tr><td>f</td></tr></tfoot></table>",
    );
    assert_eq!(
        source,
        "{footer-rows=1}\n::: list-table\n- - - a\n- - b\n- - f\n:::\n"
    );
    assert_eq!(codes, ["import-structure-repaired"]);
}

#[test]
fn an_empty_head_row_counts_in_no_header_rows() {
    let (source, codes) = import(
        "<table><thead><tr></tr><tr><th>H</th></tr></thead><tbody><tr><td><ul><li>a</li></ul></td></tr></tbody></table>",
    );
    assert_eq!(
        source,
        "{header-rows=1}\n::: list-table\n- - H\n- - - a\n:::\n"
    );
    assert_eq!(codes, ["import-structure-repaired"]);
}

#[test]
fn an_empty_row_keeps_the_header_columns_of_the_others() {
    let (source, _) = import(
        "<table><tr><th>R</th><td><ul><li>a</li></ul></td></tr><tr></tr><tr><th>S</th><td>b</td></tr></table>",
    );
    assert_eq!(
        source,
        "{header-cols=1}\n::: list-table\n- - R\n  - - a\n- - S\n  - b\n:::\n"
    );
}

#[test]
fn a_row_a_row_span_covers_keeps_its_mark() {
    let (source, codes) = import(
        "<table><tr><td rowspan=\"2\"><ul><li>a</li></ul></td><td>b</td></tr><tr></tr></table>",
    );
    assert_eq!(source, "::: list-table\n- - - a\n  - b\n- - ^\n:::\n");
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn a_table_of_inline_cells_stays_a_table() {
    let (source, _) = import("<table><tr><td>a</td><td>b</td></tr></table>");
    assert_eq!(source, "| a | b |\n");
}

#[test]
fn the_json_boundary_reads_list_table_for_html_only() {
    let html = "<table><tr><td><p>x</p><p>y</p></td></tr></table>";
    let r = reply(&json!({
        "call": "importDocument", "from": "html", "source": html,
        "options": {"listTable": true},
    }));
    assert_eq!(r["ok"]["source"], "::: list-table\n- - x\n\n    y\n:::\n");
    let gfm = reply(&json!({
        "call": "importDocument", "from": "gfm", "source": "a",
        "options": {"listTable": true},
    }));
    assert_eq!(gfm["error"]["name"], "CarveOptionError");
    let typed = ImportOptions::from_json(Some(&json!({"from": "html", "listTable": true})));
    assert!(typed.is_ok_and(|o| o.list_table));
    let wrong = ImportOptions::from_json(Some(&json!({"from": "html", "listTable": 1})));
    assert!(wrong.is_err());
}
