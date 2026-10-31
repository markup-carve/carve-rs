//! CARVE-P12-069: a `+` continuation join inside a code span, a raw inline or
//! inline math has no child list to hold a `cell_wrap`, so the node carries a
//! `cellWraps` entry instead. Rows from the spec's
//! `resources/ast-cell-wraps-acceptance.json` and carve-js
//! `test/verbatim-cell-wraps.test.ts`.

use std::path::Path;

fn tree(source: &str) -> serde_json::Value {
    let mut doc: serde_json::Value =
        serde_json::from_str(&carve::to_json(&carve::parse(source))).unwrap();
    strip_pos(&mut doc);
    doc
}

fn strip_pos(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("pos");
            map.values_mut().for_each(strip_pos);
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_pos),
        _ => {}
    }
}

fn cell(source: &str) -> serde_json::Value {
    tree(source)
        .pointer("/children/0/rows/0/cells/0/children")
        .unwrap()
        .clone()
}

#[test]
fn the_acceptance_rows_match_and_write_back() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/spec/resources/ast-cell-wraps-acceptance.json");
    let fixture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let rows = fixture["rows"].as_array().unwrap();
    assert!(!rows.is_empty());
    for row in rows {
        let source = row["source"].as_str().unwrap();
        let doc = tree(source);
        assert_eq!(
            doc["children"][0]["rows"][0]["continuationLines"],
            row.get("continuationLines").cloned().unwrap_or(1.into()),
            "{}",
            row["id"]
        );
        assert_eq!(
            doc.pointer(row["path"].as_str().unwrap()),
            Some(&row["expected"]),
            "{}",
            row["id"]
        );
        let written = carve::to_carve(source);
        assert_eq!(
            tree(&written)["children"],
            doc["children"],
            "{}: {written:?}",
            row["id"]
        );
    }
}

#[test]
fn each_remainder_goes_on_the_plus_line_its_entry_names() {
    for (source, written) in [
        ("| `a |\n+ |\n+ b` |\n", "| `a |\n+ |\n+ b` |\n"),
        ("| $`a | z |\n+ b` | w |\n", "| $`a | z |\n+ b` | w |\n"),
        (
            "| `<b | z |\n+ c>`{=html} | w |\n",
            "| `<b | z |\n+ c>`{=html} | w |\n",
        ),
    ] {
        assert_eq!(carve::to_carve(source), written, "{source:?}");
    }
}

#[test]
fn cell_wraps_survive_the_wire() {
    let doc = carve::parse("| `a | z |\n+ b` | w |\n");
    let back = carve::from_json(&carve::to_json(&doc)).unwrap();
    assert_eq!(back.children, doc.children);
}

#[test]
fn the_offset_counts_codepoints_after_the_padding_strip() {
    assert_eq!(
        cell("| `a😀 |\n+ b` |\n"),
        serde_json::json!([{"type": "code", "value": "a😀 b", "cellWraps": [{"offset": 2, "line": 1}]}])
    );
    assert_eq!(
        cell("| ` a |\n+ b |\n+ c ` |\n"),
        serde_json::json!([{"type": "code", "value": "a b c", "cellWraps": [{"offset": 1, "line": 1}, {"offset": 3, "line": 2}]}])
    );
    assert_eq!(carve::to_carve("| `a😀 |\n+ b` |\n"), "| `a😀 |\n+ b` |\n");
}

#[test]
fn an_unclosed_code_span_carries_its_join() {
    assert_eq!(
        cell("| `a |\n+ b |\n"),
        serde_json::json!([{"type": "code", "value": "a b", "cellWraps": [{"offset": 1, "line": 1}]}])
    );
}

#[test]
fn a_span_no_join_crosses_has_no_field_and_html_is_unchanged() {
    assert_eq!(
        cell("| `a b` |\n+ c |\n"),
        serde_json::json!([{"type": "code", "value": "a b"}, {"type": "cell_wrap", "line": 1}, {"type": "text", "value": "c"}])
    );
    assert_eq!(
        carve::to_html("| `a |\n+ b` |\n"),
        carve::to_html("| `a b` |\n")
    );
}

#[test]
fn an_offset_that_names_no_space_is_refused() {
    let mut doc = carve::parse("| `a |\n+ b` |\n");
    let carve::BlockNode::Table(table) = &mut doc.children[0] else {
        panic!("a table");
    };
    let carve::InlineNode::Code(code) = &mut table.rows[0].cells[0].children[0] else {
        panic!("a code span");
    };
    // `b` is no space, and offset 2 is neither padding boundary of `a b`.
    code.cell_wraps.as_mut().unwrap()[0].offset = 2;
    assert!(carve::render_carve(&doc).is_err());
}

#[test]
fn a_join_stripped_as_padding_sits_at_the_boundary_and_writes_back() {
    for (source, value, offset) in [("| ` |\n+ b ` |\n", "b", 0), ("| ` a |\n+ ` |\n", "a", 1)] {
        assert_eq!(
            cell(source),
            serde_json::json!([{"type": "code", "value": value, "cellWraps": [{"offset": offset, "line": 1}]}]),
            "{source:?}"
        );
        assert_eq!(carve::to_carve(source), source);
    }
}
