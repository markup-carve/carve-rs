//! C2-M-039 (carve#6738, ruling A): the importer counts a table's columns as
//! a parse places the cells, with the slots row spans from above take.
//! CC-2109 and CC-2110 are copied from the spec until the pin carries them;
//! CC-2107 and CC-2108 are the overlap of carve#6737, which stays reported.
use carve::api::{self, ImportOptions};

fn import(html: &str) -> (String, usize, Vec<(String, String)>) {
    let got = api::import_document(html, &ImportOptions::default()).unwrap();
    let tree: serde_json::Value = serde_json::from_str(&api::encode_tree_with(
        &got.tree,
        api::EncodeOptions { positions: false },
    ))
    .unwrap();
    let columns = tree["children"][0]["columns"]
        .as_array()
        .map_or(0, Vec::len);
    let entries = got
        .diagnostics
        .iter()
        .map(|d| (d.code.clone(), d.path.clone().unwrap_or_default()))
        .collect();
    (got.source, columns, entries)
}

#[test]
fn cc_2109_row_span_slot_widens_a_column_span_row() {
    let html = r#"<table><tr><td rowspan="2">r</td></tr><tr><td colspan="2">w</td></tr></table>"#;
    assert_eq!(import(html), ("| r |\n| ^ | w | < |\n".into(), 3, vec![]));
}

#[test]
fn cc_2110_row_span_slot_moves_the_next_cell() {
    let html = r#"<table><tr><td rowspan="2">r</td></tr><tr><td>w</td></tr></table>"#;
    assert_eq!(import(html), ("| r |\n| ^ | w |\n".into(), 2, vec![]));
}

#[test]
fn cc_2107_and_cc_2108_overlap_keeps_two_columns_and_its_report() {
    let path = "/children/0/children/0/children/0/children/1/rowspan".to_string();
    let body = r#"<table><tr><td>a</td><td rowspan="2">r</td></tr><tr><td colspan="2">w</td></tr></table>"#;
    assert_eq!(
        import(body),
        (
            "| a | r |\n| w | < |\n".into(),
            2,
            vec![("field-unspellable".into(), path.clone())]
        )
    );
    let head = r#"<table><thead><tr><th>a</th><th rowspan="2">r</th></tr><tr><th colspan="2">w</th></tr></thead></table>"#;
    assert_eq!(
        import(head),
        (
            "|= a |= r |\n|= w |= < |\n".into(),
            2,
            vec![("field-unspellable".into(), path)]
        )
    );
}

/// carve-js#5621: a row shorter than a row span's column still uses up a row
/// of that span, so the next row's cells and alignment land in their columns.
#[test]
fn short_row_beside_a_row_span_keeps_the_next_rows_alignment() {
    let html = r#"<table><thead><tr><th>A</th><th>B</th><th align="right">C</th></tr></thead><tbody><tr><td>a</td><td rowspan="2">r</td><td>x</td></tr><tr><td>b</td></tr><tr><td>c</td><td align="right">d</td><td>y</td></tr></tbody></table>"#;
    let (source, columns, entries) = import(html);
    assert_eq!(
        source,
        "|= A |= B |={>} C |\n| a | r | x |\n| b | ^ |\n| c |{>} d | y |\n"
    );
    assert_eq!((columns, entries), (3, vec![]));
}
