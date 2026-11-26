//! C2-W-151 (carve#6319): spaces and tabs that would end a source line in text are dropped without a
//! report; a verbatim value keeps its report, and so does a line the dropping leaves empty.

use carve::api::{self, Carve, WriteOptions};
use serde_json::{Value, json};

fn write(tree: &Value) -> (String, Vec<String>) {
    let tree = api::decode_tree(&tree.to_string()).expect("a valid tree");
    let written = Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .expect("a write");
    let losses = written
        .diagnostics
        .iter()
        .map(|e| format!("{} {}", e.code, e.path.clone().unwrap_or_default()))
        .collect();
    (written.output, losses)
}

fn text(v: &str) -> Value {
    json!({ "type": "text", "value": v })
}

fn soft() -> Value {
    json!({ "type": "soft_break" })
}

fn paragraph(children: &[Value]) -> Value {
    json!({ "type": "document", "astVersion": "2.0", "children": [{ "type": "paragraph", "children": children }] })
}

#[test]
fn line_end_spaces_of_text_are_no_loss() {
    let cases = [
        (paragraph(&[text("x ")]), "x\n"),
        (
            paragraph(&[
                text("a "),
                json!({ "type": "emphasis", "children": [text("b")] }),
                text(" c "),
            ]),
            "a /b/ c\n",
        ),
        (paragraph(&[text("a \t"), soft(), text("b")]), "a\nb\n"),
        (
            paragraph(&[
                text("a "),
                json!({ "type": "code", "value": "x\n|" }),
                text("|a "),
            ]),
            "a `x\n|`|a\n",
        ),
        (paragraph(&[text("a"), text("  ")]), "a\n"),
        (
            json!({ "type": "document", "astVersion": "2.0", "children": [{ "type": "heading", "level": 1, "children": [text("T ")] }] }),
            "= T\n",
        ),
        (
            json!({ "type": "document", "astVersion": "2.0", "children": [{ "type": "line_block", "children": [
                { "type": "line", "children": [text("x ")] }, { "type": "line", "children": [text("y")] }] }] }),
            "::: |\nx\ny\n:::\n",
        ),
    ];
    for (tree, source) in cases {
        assert_eq!(write(&tree), (source.to_string(), vec![]), "{tree}");
    }
}

#[test]
fn a_delimited_cell_keeps_its_spaces_as_data() {
    let tree = json!({ "type": "document", "astVersion": "2.0", "children": [{ "type": "table",
        "columns": [{ "type": "table_column" }],
        "children": [{ "type": "table_body", "children": [{ "type": "table_row", "children": [
            { "type": "table_cell", "header": false, "children": [{ "type": "cell_paragraph", "children": [text("x ")] }] }] }] }] }] });
    assert_eq!(
        write(&tree),
        (
            "{table header-rows=0}\n```csv\nx \n```\n".to_string(),
            vec![]
        )
    );
}

#[test]
fn the_caption_of_a_delimited_table_is_trimmed() {
    let tree = json!({ "type": "document", "astVersion": "2.0", "children": [{ "type": "table",
        "columns": [{ "type": "table_column" }],
        "children": [{ "type": "table_body", "children": [{ "type": "table_row", "children": [
            { "type": "table_cell", "header": false, "children": [{ "type": "cell_paragraph", "children": [text("x ")] }] }] }] }],
        "caption": { "type": "caption", "children": [{ "type": "paragraph", "children": [text("cap ")] }] } }] });
    assert_eq!(
        write(&tree),
        (
            "{table header-rows=0}\n```csv\nx \n```\n^ cap\n".to_string(),
            vec![]
        )
    );
}

#[test]
fn a_table_that_trimming_would_turn_delimited_keeps_its_cells() {
    let cell = |children: Vec<Value>| json!({ "type": "table_cell", "header": false, "children": [{ "type": "cell_paragraph", "children": children }] });
    let tree = json!({ "type": "document", "astVersion": "2.0", "children": [{ "type": "table",
        "columns": [{ "type": "table_column" }, { "type": "table_column" }],
        "children": [{ "type": "table_body", "children": [{ "type": "table_row", "children": [
            cell(vec![text("a"), text(" ")]), cell(vec![text(" b")])] }] }] }] });
    let (source, losses) = write(&tree);
    assert_eq!(source, "| a  |  b |\n");
    assert_eq!(
        losses
            .iter()
            .map(|l| l.split(' ').next().unwrap_or(""))
            .collect::<Vec<_>>(),
        ["structure-unspellable", "field-unspellable"]
    );
}

#[test]
fn an_attributed_text_keeps_its_report() {
    for value in ["x ", "  "] {
        let tree = paragraph(&[
            text("a"),
            json!({ "type": "text", "value": value, "attrs": { "classes": ["c"] } }),
        ]);
        assert_eq!(
            write(&tree).1,
            ["structure-unspellable /children/0"],
            "{value:?}"
        );
    }
}

#[test]
fn other_losses_are_still_reported_at_the_supplied_paths() {
    let code = paragraph(&[text("a"), json!({ "type": "code", "value": "x \ny" })]);
    assert_eq!(
        write(&code).1,
        ["field-unspellable /children/0/children/1/value"]
    );
    let leading = paragraph(&[text("a "), soft(), text(" b")]);
    assert_eq!(
        write(&leading).1,
        ["field-unspellable /children/0/children/2/value"]
    );
    // The dropped text node at index 2 shifts nothing: the report names the second soft break as supplied.
    let blank = paragraph(&[text("a"), soft(), text("  "), soft(), text("b")]);
    assert!(
        write(&blank)
            .1
            .contains(&"structure-unspellable /children/0/children/3".to_string())
    );
}
