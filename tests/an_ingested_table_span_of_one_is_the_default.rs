use serde_json::{json, Value};

fn cell_json(colspan: i64, rowspan: i64, header: bool, blocks: bool, text: &str) -> Value {
    let mut cell = json!({
        "type": "table_cell",
        "header": header,
        "colspan": colspan,
        "rowspan": rowspan,
    });
    if blocks {
        cell["blocks"] =
            json!([{"type": "paragraph", "children": [{"type": "text", "value": text}]}]);
    } else {
        cell["children"] = json!([{"type": "text", "value": text}]);
    }
    cell
}

fn document(cell: Value, nest: bool, siblings: usize) -> Value {
    let mut cells = vec![cell];
    for _ in 0..siblings {
        cells.push(json!({"type": "table_cell", "header": false, "children": []}));
    }
    let table = json!({"type": "table", "rows": [{"type": "table_row", "cells": cells}]});
    let child = if nest {
        json!({"type": "section", "level": 1, "children": [table]})
    } else {
        table
    };
    json!({"type": "document", "srcByteLength": 0, "children": [child]})
}

fn first_cell(doc: &Value, nest: bool) -> &Value {
    let table = if nest {
        &doc["children"][0]["children"][0]
    } else {
        &doc["children"][0]
    };
    &table["rows"][0]["cells"][0]
}

#[test]
fn an_ingested_one_is_omitted_across_one_thousand_cell_variants() {
    for i in 0..1000 {
        let colspan = if i % 5 == 0 { 2 } else { 1 };
        let rowspan = if i % 7 == 0 { 3 } else { 1 };
        let header = i % 2 == 0;
        let blocks = i % 3 == 0;
        let nest = i % 4 == 0;
        let source = document(
            cell_json(colspan, rowspan, header, blocks, &format!("cell {i}")),
            nest,
            i % 4,
        );
        let decoded = carve::ast_json::from_json(&source.to_string()).expect("valid cell");
        let encoded: Value = serde_json::from_str(&carve::ast_json::to_json(&decoded)).unwrap();
        let cell = first_cell(&encoded, nest);
        if colspan > 1 {
            assert_eq!(cell["colspan"], colspan);
        } else {
            assert!(cell.get("colspan").is_none());
        }
        if rowspan > 1 {
            assert_eq!(cell["rowspan"], rowspan);
        } else {
            assert!(cell.get("rowspan").is_none());
        }
    }
}

#[test]
fn zero_and_negative_span_counts_are_refused() {
    for (colspan, rowspan) in [(0, 1), (-1, 1), (1, 0), (1, -1)] {
        let source = document(cell_json(colspan, rowspan, false, false, "a"), false, 0);
        assert!(
            carve::ast_json::from_json(&source.to_string()).is_err(),
            "accepted {colspan} by {rowspan}"
        );
    }
}

#[test]
fn a_default_span_does_not_change_delimited_table_writing() {
    let mut source = document(cell_json(1, 1, true, false, "a"), false, 0);
    source["children"][0]["sourceFormat"] = json!("csv");
    let with_defaults = carve::ast_json::from_json(&source.to_string()).expect("decode defaults");
    let encoded = carve::ast_json::to_json(&with_defaults);
    let after_round_trip = carve::ast_json::from_json(&encoded).expect("decode normalized");
    let written = carve::render_carve(&with_defaults).expect("write delimited table");

    assert!(written.contains("```csv"), "wrote {written}");
    assert_eq!(
        written,
        carve::render_carve(&after_round_trip).expect("write normalized table")
    );
}
