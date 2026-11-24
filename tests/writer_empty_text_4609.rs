use carve::Options;
use carve::api::{self, Carve, WriteOptions, json::call};
use serde_json::{Value, json};

fn invoke(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

fn paragraph(children: &Value) -> Value {
    json!({"type":"document","astVersion":"2.0","children":[{"type":"paragraph","children":children}]})
}

#[test]
fn empty_text_in_a_nonempty_run_is_invisible() {
    let strong = paragraph(
        &json!([{"type":"strong","children":[{"type":"text","value":""},{"type":"text","value":"*"},{"type":"text","value":"a"}]}]),
    );
    let no_empty = paragraph(
        &json!([{"type":"strong","children":[{"type":"text","value":"*"},{"type":"text","value":"a"}]}]),
    );
    assert_eq!(
        invoke(&json!({"call":"write","tree":no_empty}))["ok"]["output"],
        "{**a*}\n"
    );
    let output = invoke(&json!({"call":"write","tree":strong}));
    assert_eq!(output["ok"]["output"], "{**a*}\n", "{output}");
    assert_eq!(output["ok"]["diagnostics"], json!([]), "{output}");
    let code = paragraph(
        &json!([{"type":"code","value":"c"},{"type":"text","value":""},{"type":"text","value":"`x`"}]),
    );
    let output = invoke(&json!({"call":"write","tree":code}));
    assert_eq!(output["ok"]["output"], "`c`\\`x`\n", "{output}");
    assert_eq!(output["ok"]["diagnostics"], json!([]), "{output}");
}

#[test]
fn loss_paths_follow_the_supplied_tree_after_a_text_run() {
    let tree = paragraph(&json!([
        {"type":"text","value":"a"},
        {"type":"text","value":"b"},
        {"type":"text","value":"c","attrs":{"keyValues":[["x","a\nb"]]}}
    ]));
    let output = invoke(&json!({"call":"writeWithDiagnostics","tree":tree}));
    let paths: Vec<String> = output["ok"]["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["path"].as_str().map(str::to_string))
        .collect();
    assert!(
        paths.contains(&"/children/0/children/2/attrs/keyValues/0/1".to_string()),
        "{output}"
    );
}

#[test]
fn wrap_offset_moves_with_the_second_text_fragment() {
    let source = "| a |\n+ b |\n";
    let parsed = api::parse_with_layout(source, &Options::default()).unwrap();
    let mut tree: Value = serde_json::from_str(&api::encode_tree(&parsed.tree)).unwrap();
    let mut layout = parsed.layout;
    let wrap = layout["nodes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find_map(|entry| entry.get_mut("wraps")?.as_array_mut()?.first_mut())
        .unwrap();
    let path = wrap["path"].as_str().unwrap().to_string();
    let (parent, _) = path.rsplit_once('/').unwrap();
    let children = tree.pointer_mut(parent).unwrap().as_array_mut().unwrap();
    assert_eq!(children[0]["value"], "a b");
    children[0]["value"] = json!("a");
    children.insert(1, json!({"type":"text","value":" b"}));
    wrap["path"] = json!(format!("{parent}/1"));
    wrap["offset"] = json!(1);
    let tree = api::decode_tree(&tree.to_string()).unwrap();
    let written = Carve::new()
        .write_with_diagnostics(
            &tree,
            &WriteOptions {
                layout: Some(layout),
                ..WriteOptions::default()
            },
        )
        .unwrap();
    assert_eq!(written.output, source);
    assert!(written.diagnostics.is_empty(), "{:?}", written.diagnostics);
}

#[test]
fn adjacent_table_text_coalesces_when_the_table_form_stays_pipe() {
    let cell = |children: Value| json!({"type":"table_cell","header":false,"children":[{"type":"cell_paragraph","children":children}]});
    let tree = json!({"type":"document","astVersion":"2.0","children":[{"type":"table",
    "columns":[{"type":"table_column"},{"type":"table_column"}],
    "children":[{"type":"table_body","children":[{"type":"table_row","children":[
        cell(json!([{"type":"text","value":"a"},{"type":"text","value":"b"}])),
        cell(json!([{"type":"text","value":"c"}]))
    ]}]}]}]});
    let output = invoke(&json!({"call":"writeWithDiagnostics","tree":tree}));
    assert_eq!(output["ok"]["output"], "| ab | c |\n", "{output}");
    assert_eq!(output["ok"]["diagnostics"], json!([]), "{output}");
}
