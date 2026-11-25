//! C2-W-158: the writer's entries on a supplied tree stand in document order of their paths (carve#6619).

use carve::api::{self, Carve, WriteOptions};
use serde_json::{Value, json};

fn text(v: &str) -> Value {
    json!({ "type": "text", "value": v })
}

fn plain(header: bool) -> Value {
    json!({ "type": "table_cell", "header": header,
        "children": [{ "type": "cell_paragraph", "children": [text("x")] }] })
}

fn blocks() -> Value {
    json!({ "type": "table_cell", "header": false, "children": [
        { "type": "paragraph", "children": [text("a")] },
        { "type": "paragraph", "children": [text("b")] },
    ] })
}

fn row(cells: &[Value]) -> Value {
    json!({ "type": "table_row", "children": cells })
}

fn columns() -> Value {
    json!([{ "type": "table_column" }, { "type": "table_column" }])
}

fn entries(tree: &Value) -> Vec<String> {
    let tree = api::decode_tree(&tree.to_string()).expect("tree");
    Carve::new()
        .write_with_diagnostics(&tree, &WriteOptions::default())
        .expect("written")
        .diagnostics
        .iter()
        .map(|e| format!("{} {}", e.code, e.path.as_deref().unwrap_or("")))
        .collect()
}

#[test]
fn fields_of_a_table_group_come_before_the_nodes_inside_it() {
    let table = json!({ "type": "table", "columns": columns(), "children": [
        { "type": "table_head", "attrs": { "id": "head" }, "children": [row(&[plain(true), plain(true)])] },
        { "type": "table_body", "attrs": { "id": "body" },
          "children": [row(&[plain(false), plain(false)]), row(&[plain(false), blocks()])] },
        { "type": "table_foot", "attrs": { "id": "foot" }, "children": [row(&[plain(false), plain(false)])] },
    ] });
    assert_eq!(
        entries(&json!({ "type": "document", "astVersion": "2.0", "children": [table] })),
        [
            "field-unspellable /children/0/children/0/attrs",
            "field-unspellable /children/0/children/1/attrs",
            "structure-unspellable /children/0/children/1/children/1/children/1",
            "field-unspellable /children/0/children/2/attrs",
        ]
    );
}

#[test]
fn each_index_compares_as_a_number() {
    let rows: Vec<Value> = (0..12)
        .map(|at| {
            row(&[
                plain(false),
                if at == 2 || at == 10 {
                    blocks()
                } else {
                    plain(false)
                },
            ])
        })
        .collect();
    let table = |attrs: Option<Value>| {
        let mut body = json!({ "type": "table_body", "children": rows.clone() });
        if let Some(a) = attrs {
            body["attrs"] = a;
        }
        json!({ "type": "table", "columns": columns(), "children": [body] })
    };
    let mut children = vec![table(None)];
    children.extend((0..9).map(|_| json!({ "type": "paragraph", "children": [text("p")] })));
    children.push(table(Some(json!({ "id": "late" }))));
    assert_eq!(
        entries(&json!({ "type": "document", "astVersion": "2.0", "children": children })),
        [
            "structure-unspellable /children/0/children/0/children/2/children/1",
            "structure-unspellable /children/0/children/0/children/10/children/1",
            "field-unspellable /children/10/children/0/attrs",
            "structure-unspellable /children/10/children/0/children/2/children/1",
            "structure-unspellable /children/10/children/0/children/10/children/1",
        ]
    );
}

#[test]
fn fields_of_the_document_come_before_its_blocks() {
    let table = json!({ "type": "table", "columns": columns(),
        "children": [{ "type": "table_body", "children": [row(&[plain(false), blocks()])] }] });
    let got = entries(
        &json!({ "type": "document", "astVersion": "2.0", "lang": "de",
        "metadata": { "keywords": ["a"] }, "children": [table] }),
    );
    assert_eq!(
        got,
        [
            "field-unspellable /metadata/keywords",
            "field-unspellable /lang",
            "structure-unspellable /children/0/children/0/children/0/children/1",
        ]
    );
}

#[test]
fn an_attribute_line_feed_stands_at_its_node() {
    let table = json!({ "type": "table", "columns": columns(),
        "children": [{ "type": "table_body", "children": [row(&[plain(false), blocks()])] }] });
    let para = json!({ "type": "paragraph", "attrs": { "keyValues": [["title", "a\nb"]] },
        "children": [text("p")] });
    assert_eq!(
        entries(
            &json!({ "type": "document", "astVersion": "2.0", "children": [table.clone(), para, table] })
        ),
        [
            "structure-unspellable /children/0/children/0/children/0/children/1",
            "field-unspellable /children/1/attrs/keyValues/0/1",
            "structure-unspellable /children/2/children/0/children/0/children/1",
        ]
    );
}

#[test]
fn a_title_comes_before_the_children_of_its_node() {
    let lf = || json!({ "keyValues": [["title", "a\nb"]] });
    let div = json!({ "type": "div",
        "title": [{ "type": "span", "attrs": lf(), "children": [text("t")] }],
        "children": [{ "type": "paragraph", "attrs": lf(), "children": [text("p")] }] });
    assert_eq!(
        entries(&json!({ "type": "document", "astVersion": "2.0", "children": [div] })),
        [
            "structure-unspellable /children/0/title/0",
            "field-unspellable /children/0/title/0/attrs/keyValues/0/1",
            "field-unspellable /children/0/children/0/attrs/keyValues/0/1",
        ]
    );
}
