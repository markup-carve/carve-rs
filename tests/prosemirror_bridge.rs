//! The ProseMirror bridge (carve-rs#4830): the wire shape of the carve-grammars
//! Tiptap kit, parity with the carve-php bridge, and the round trip over the
//! conformance inputs.

use std::sync::Arc;

use carve::api::prosemirror::{
    Mapping, MappingEntry, Position, ProseMirrorError, ProseMirrorOptions, carve_type_for,
    from_prosemirror, from_prosemirror_value, from_prosemirror_with_report, prosemirror_names,
    prosemirror_to_carve, prosemirror_to_carve_with, prosemirror_tree_value, to_prosemirror,
    to_prosemirror_with,
};
use carve::api::{self, EncodeOptions, Options, UrlPolicy, WriteOptions};
use serde_json::{Value, json};

fn deep(text: &str) -> Value {
    let mut de = serde_json::Deserializer::from_str(text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).expect("JSON")
}

fn fixture(name: &str) -> Value {
    let path = format!(
        "{}/tests/fixtures/prosemirror/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    deep(&std::fs::read_to_string(path).expect("fixture"))
}

/// The tree without positions; `segments` and `srcByteLength` are source
/// positions too, which the editor does not keep.
fn without_positions(tree: &api::Tree) -> Value {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.remove("segments");
                map.remove("srcByteLength");
                map.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    let mut value = deep(&api::encode_tree_with(
        tree,
        EncodeOptions { positions: false },
    ));
    strip(&mut value);
    value
}

fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for k in x.keys().chain(y.keys()) {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => {
                        if let Some(d) = first_diff(p, q, &format!("{path}/{k}")) {
                            return Some(d);
                        }
                    }
                    (p, q) => return Some(format!("{path}/{k}: {p:?} vs {q:?}")),
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (p, q))| first_diff(p, q, &format!("{path}/{i}"))),
        _ if a == b => None,
        _ => Some(format!("{path}: {a} vs {b}")),
    }
}

fn tree_of(value: &Value) -> api::Tree {
    api::decode_tree(&value.to_string()).expect("tree")
}

fn parse(source: &str) -> api::Tree {
    api::parse(source, &Options::default()).expect("parse")
}

fn convert(doc: &Value) -> String {
    prosemirror_to_carve(&doc.to_string(), &WriteOptions::default()).expect("convert")
}

fn write(tree: &api::Tree) -> String {
    api::Carve::new()
        .write(tree, &WriteOptions::default())
        .expect("write")
}

/// Rendered and saved back, the tree is the same and the source is what the
/// writer gives for the parsed tree.
fn assert_round_trip(id: &str, source: &str) {
    let tree = parse(source);
    let doc = to_prosemirror(&tree).unwrap_or_else(|e| panic!("{id}: render: {e}"));
    let back = from_prosemirror(&doc.json).unwrap_or_else(|e| panic!("{id}: save: {e}"));
    assert_eq!(
        without_positions(&back),
        without_positions(&tree),
        "{id}: tree\n{source}"
    );
    assert_eq!(
        prosemirror_to_carve(&doc.json, &WriteOptions::default()).ok(),
        api::Carve::new()
            .write(&tree, &WriteOptions::default())
            .ok(),
        "{id}: source\n{source}"
    );
}

fn conformance_inputs() -> Vec<(String, String)> {
    let dir = format!("{}/spec-conformance", env!("CARGO_MANIFEST_DIR"));
    let mut out = Vec::new();
    for (file, list) in [
        ("examples.json", "examples"),
        ("clause-cases.json", "cases"),
    ] {
        let data = deep(&std::fs::read_to_string(format!("{dir}/{file}")).expect("conformance"));
        for case in data[list].as_array().expect("cases") {
            let no_options = match case.get("options") {
                None | Some(Value::Null) => true,
                Some(Value::Object(o)) => o.is_empty(),
                Some(Value::Array(a)) => a.is_empty(),
                Some(_) => false,
            };
            if let (Some(input), true) = (case["input"].as_str(), no_options) {
                out.push((
                    case["id"].as_str().unwrap_or("").to_string(),
                    input.to_string(),
                ));
            }
        }
    }
    out
}

#[test]
fn every_conformance_input_round_trips() {
    let inputs = conformance_inputs();
    assert!(inputs.len() > 1500, "{}", inputs.len());
    for (id, input) in &inputs {
        assert_round_trip(id, input);
    }
}

#[test]
fn generated_documents_round_trip_nested_in_quotes_and_items() {
    let pool: Vec<String> = conformance_inputs()
        .into_iter()
        .map(|(_, input)| input.trim_end_matches('\n').to_string())
        .collect();
    let mut seed: u64 = 5414;
    let mut next = |n: usize| {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        usize::try_from(seed >> 33).unwrap_or(0) % n
    };
    let prefix = |text: &str, first: &str, rest: &str| {
        text.split('\n')
            .enumerate()
            .map(|(k, line)| {
                if line.is_empty() {
                    rest.trim_end().to_string()
                } else {
                    format!("{}{line}", if k == 0 { first } else { rest })
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    for i in 0..300 {
        let parts: Vec<String> = (0..=next(4))
            .map(|_| {
                let part = pool[next(pool.len())].clone();
                match next(6) {
                    0 => prefix(&part, "> ", "> "),
                    1 => prefix(&part, "- ", "  "),
                    2 => prefix(&part, "1. ", "   "),
                    _ => part,
                }
            })
            .collect();
        assert_round_trip(&format!("generated {i}"), &(parts.join("\n\n") + "\n"));
    }
}

/// The carve-php bridge on the Carve inputs of the 0.1 bridge tests: the same
/// ProseMirror document and the same saved source.
#[test]
fn the_bridge_agrees_with_carve_php() {
    let data = fixture("carve-php-bridge.json");
    let rows = data["rows"].as_array().expect("rows");
    assert!(rows.len() > 150);
    for row in rows {
        let source = row["carve"].as_str().expect("carve");
        let doc = to_prosemirror(&parse(source)).expect("render");
        assert_eq!(doc.doc, row["pm"], "{source}");
        assert_eq!(json!(doc.blanked), row["blanked"], "{source}");
        assert_eq!(
            Some(convert(&doc.doc).as_str()),
            row["back"].as_str(),
            "{source}"
        );
    }
}

#[test]
fn a_tree_loads_as_the_kit_loads_it() {
    for case in fixture("kit-wire.json")["loaded"]
        .as_array()
        .expect("loaded")
    {
        let doc = to_prosemirror(&tree_of(&case["tree"])).expect("render");
        assert_eq!(doc.doc, case["pm"], "{}", case["id"]);
    }
}

#[test]
fn a_loaded_document_saves_as_the_kit_saves_it() {
    for case in fixture("kit-wire.json")["loaded"]
        .as_array()
        .expect("loaded")
    {
        let tree =
            prosemirror_tree_value(&case["pm"], &ProseMirrorOptions::default()).expect("save");
        assert_eq!(tree, case["back"], "{}", case["id"]);
        assert_eq!(
            without_positions(&tree_of(&tree)),
            without_positions(&tree_of(&case["tree"]))
        );
    }
}

#[test]
fn an_editor_document_saves_as_the_kit_saves_it() {
    for case in fixture("kit-wire.json")["edited"]
        .as_array()
        .expect("edited")
    {
        let tree =
            prosemirror_tree_value(&case["pm"], &ProseMirrorOptions::default()).expect("save");
        assert_eq!(tree, case["tree"], "{}", case["name"]);
    }
}

#[test]
fn a_mapped_document_saves_and_loads_as_the_kit_does() {
    for case in fixture("kit-wire.json")["mapped"]
        .as_array()
        .expect("mapped")
    {
        let options = ProseMirrorOptions {
            mapping: Mapping::from_json(&case["mapping"]).expect("mapping"),
            ..ProseMirrorOptions::default()
        };
        let saved = from_prosemirror_value(&case["doc"], &options).expect("save");
        assert_eq!(
            prosemirror_tree_value(&case["doc"], &options).expect("save"),
            case["saved"],
            "{}",
            case["name"]
        );
        assert_eq!(json!(saved.pointers), case["pointers"], "{}", case["name"]);
        if let Some(tree) = case.get("tree") {
            let doc = to_prosemirror_with(&tree_of(tree), &options).expect("render");
            assert_eq!(doc.doc, case["pm"], "{}", case["name"]);
        }
    }
}

#[test]
fn an_editor_document_gets_the_carve_defaults() {
    let doc = json!({"type": "doc", "content": [
        {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Title"}]},
        {"type": "paragraph", "content": [{"type": "text", "text": "bold", "marks": [{"type": "bold"}]}]},
        {"type": "bulletList", "content": [{"type": "listItem", "content": [
            {"type": "paragraph", "content": [{"type": "text", "text": "a"}]}]}]},
        {"type": "orderedList", "attrs": {"start": 3}, "content": [{"type": "listItem", "content": [
            {"type": "paragraph", "content": [{"type": "text", "text": "b"}]}]}]},
        {"type": "paragraph"},
    ]});
    assert_eq!(
        convert(&doc),
        "== Title\n\n*bold*\n\n- a\n\n{start=3}\n. b\n"
    );
}

#[test]
fn a_node_with_no_carve_node_is_an_error() {
    let doc = json!({"type": "doc", "content": [{"type": "mention", "attrs": {"id": "7"}}]});
    let error = from_prosemirror(&doc.to_string()).expect_err("unmapped");
    assert_eq!(
        error,
        ProseMirrorError::Invalid(
            "ProseMirrorToCarve: no Carve node for the ProseMirror node \"mention\"".into()
        )
    );
    let mark = json!({"type": "doc", "content": [{"type": "paragraph", "content": [
        {"type": "text", "text": "x", "marks": [{"type": "textStyle"}]}]}]});
    assert!(matches!(
        from_prosemirror(&mark.to_string()),
        Err(ProseMirrorError::Invalid(_))
    ));
    assert!(matches!(
        from_prosemirror("{\"type\":\"paragraph\"}"),
        Err(ProseMirrorError::Invalid(_))
    ));
}

#[test]
fn a_denied_destination_is_blanked_in_the_editor() {
    let options = ProseMirrorOptions {
        url_policy: UrlPolicy::Allow(vec!["https".into()]),
        ..ProseMirrorOptions::default()
    };
    let tree = parse("[ok](https://a.example) [no](javascript:alert(1)) ![i](data:x)");
    let doc = to_prosemirror_with(&tree, &options).expect("render");
    assert_eq!(doc.blanked, ["javascript:alert(1)", "data:x"]);
    let content = &doc.doc["content"][0]["content"];
    let no = content
        .as_array()
        .expect("inlines")
        .iter()
        .find(|n| n["text"] == "no")
        .expect("no");
    assert_eq!(no["marks"][0]["attrs"]["href"], Value::Null);
    assert_eq!(
        no["marks"][0]["attrs"]["carve"]["href"],
        "javascript:alert(1)"
    );
    let image = content
        .as_array()
        .expect("inlines")
        .iter()
        .find(|n| n["type"] == "image")
        .expect("image");
    assert_eq!(image["attrs"]["src"], Value::Null);
    assert_eq!(image["attrs"]["carve"]["src"], "data:x");
    let back = from_prosemirror(&doc.json).expect("save");
    assert_eq!(without_positions(&back), without_positions(&tree));
}

#[test]
fn a_mapping_entry_declares_a_host_node_and_mark() {
    let mapping = Mapping::from_json(&json!({
        "nodes": {"placeholderToken": {"as": "span", "class": "placeholder", "attributes": ["name"]}},
        "marks": {"textStyle": {"as": "span", "class": "text-style", "attributes": ["color"]}},
    }))
    .expect("mapping");
    let options = ProseMirrorOptions {
        mapping,
        ..ProseMirrorOptions::default()
    };
    let content = json!([
        {"type": "text", "text": "Dear "},
        {"type": "placeholderToken", "attrs": {"name": "first"}},
        {"type": "text", "text": ", "},
        {"type": "text", "text": "red", "marks": [{"type": "textStyle", "attrs": {"color": "#f00"}}]},
    ]);
    let doc = json!({"type": "doc", "content": [{"type": "paragraph", "content": content}]});
    let source = prosemirror_to_carve_with(&doc.to_string(), &options, &WriteOptions::default())
        .expect("convert");
    assert_eq!(
        source,
        "Dear []{.placeholder name=first}, [red]{.text-style color=#f00}\n"
    );
    let loaded = to_prosemirror_with(&parse(&source), &options).expect("render");
    assert_eq!(loaded.doc["content"][0]["content"], content);
}

#[test]
fn a_malformed_mapping_entry_is_refused() {
    for mapping in [
        json!({"nodes": {"x": {"as": "p", "class": "x"}}}),
        json!({"marks": {"x": {"as": "div", "class": "x"}}}),
        json!({"nodes": {"x": {"as": "span", "class": ""}}}),
        json!({"nodes": {"x": {"as": "span", "class": "x", "attributes": [1]}}}),
        json!({"nodes": {"x": {"as": "span", "class": "x", "carve": "yes"}}}),
        json!(["x"]),
    ] {
        assert!(Mapping::from_json(&mapping).is_err(), "{mapping}");
    }
}

#[test]
fn callback_entries_convert_both_ways() {
    let to_tree: carve::api::prosemirror::ToTree = Arc::new(|node, ctx| {
        assert_eq!(ctx.position(), Position::Block);
        let children = ctx.blocks(&node["content"])?;
        Ok(json!({"type": "div", "attrs": {"classes": ["poll"]}, "children": children}))
    });
    let from_tree: carve::api::prosemirror::FromTree = Arc::new(|node, ctx| {
        if node["attrs"]["classes"] != json!(["poll"]) {
            return Ok(None);
        }
        let content = ctx.blocks(&node["children"])?;
        Ok(Some(json!({"type": "poll", "content": content})))
    });
    let options = ProseMirrorOptions {
        mapping: Mapping {
            nodes: vec![(
                "poll".into(),
                MappingEntry::Callback {
                    to_tree: Some(to_tree),
                    from_tree: Some(from_tree),
                    carve: false,
                },
            )],
            marks: Vec::new(),
        },
        ..ProseMirrorOptions::default()
    };
    let doc = json!({"type": "doc", "content": [{"type": "poll", "content": [
        {"type": "paragraph", "content": [{"type": "text", "text": "Lunch?"}]}]}]});
    let source = prosemirror_to_carve_with(&doc.to_string(), &options, &WriteOptions::default())
        .expect("convert");
    assert_eq!(source, "::: poll\nLunch?\n:::\n");
    let loaded = to_prosemirror_with(&parse(&source), &options).expect("render");
    assert_eq!(loaded.doc["content"][0]["type"], "poll");
    assert_eq!(
        loaded.doc["content"][0]["content"][0]["content"][0]["text"],
        "Lunch?"
    );
}

fn cell(ty: &str, text: &str, attrs: &Value) -> Value {
    json!({"type": ty, "attrs": attrs, "content": [{"type": "paragraph", "content": [{"type": "text", "text": text}]}]})
}

fn table(rows: &[Vec<Value>]) -> Value {
    let rows: Vec<Value> = rows
        .iter()
        .map(|cells| json!({"type": "tableRow", "content": cells}))
        .collect();
    json!({"type": "doc", "content": [{"type": "table", "content": rows}]})
}

/// The saved tree is the tree its own source parses to.
fn saves_as_its_source_parses(doc: &Value) -> String {
    let tree = from_prosemirror(&doc.to_string()).expect("save");
    let source = write(&tree);
    let (mut p, t) = (without_positions(&parse(&source)), without_positions(&tree));
    // `stages` records the resolver's run on the parsed tree; an editor document has none.
    if let Value::Object(map) = &mut p {
        map.remove("stages");
    }
    assert!(p == t, "{source}\n{:?}", first_diff(&p, &t, ""));
    source
}

#[test]
fn stock_tiptap_cell_spans_save_as_counts_and_marks() {
    let none = json!({});
    let doc = table(&[
        vec![
            cell("tableHeader", "a", &none),
            cell("tableHeader", "b", &none),
        ],
        vec![cell(
            "tableCell",
            "merged",
            &json!({"colspan": 2, "rowspan": 1}),
        )],
    ]);
    assert_eq!(
        saves_as_its_source_parses(&doc),
        "|= a |= b |\n| merged | < |\n"
    );

    let doc = table(&[
        vec![
            cell("tableHeader", "h", &json!({"rowspan": 3})),
            cell("tableHeader", "i", &none),
        ],
        vec![cell("tableCell", "a", &none), cell("tableCell", "b", &none)],
    ]);
    assert_eq!(saves_as_its_source_parses(&doc), "|= h |= i |\n| a | b |\n");

    let doc = table(&[
        vec![cell("tableCell", "a", &json!({"colspan": 2, "rowspan": 2}))],
        vec![cell("tableCell", "b", &none), cell("tableCell", "c", &none)],
    ]);
    let report = from_prosemirror_with_report(&doc.to_string(), &ProseMirrorOptions::default())
        .expect("save");
    assert_eq!(write(&report.tree), "| a | < |\n| b | c |\n");
    assert_eq!(report.ignored.len(), 1);
    assert_eq!(report.ignored[0].path, "/content/0/content/0/content/0");
    assert_eq!(report.ignored[0].attribute, "rowspan");
    assert_eq!(report.ignored[0].value, json!(2));
}

/// Every consistent span grid of up to three rows and three columns, as an
/// editor makes them.
#[test]
fn every_small_span_grid_saves_as_the_source_that_parses_to_it() {
    let mut state: u64 = 5440;
    let mut rand = |lo: usize, hi: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        lo + usize::try_from(state >> 33).unwrap_or(0) % (hi - lo + 1)
    };
    let mut grids = 0;
    for seed in 0..400 {
        let (height, width) = (rand(1, 3), rand(1, 3));
        let mut taken = vec![vec![false; width]; height];
        let mut rows = Vec::new();
        for r in 0..height {
            let mut cells = Vec::new();
            for c in 0..width {
                if taken[r][c] {
                    continue;
                }
                let mut colspan = 1;
                while c + colspan < width && !taken[r][c + colspan] && rand(0, 2) == 0 {
                    colspan += 1;
                }
                let rowspan = if rand(0, 2) == 0 {
                    rand(1, height - r)
                } else {
                    1
                };
                for row in taken.iter_mut().skip(r).take(rowspan) {
                    for x in row.iter_mut().skip(c).take(colspan) {
                        *x = true;
                    }
                }
                let ty = if r == 0 && seed % 2 == 0 {
                    "tableHeader"
                } else {
                    "tableCell"
                };
                cells.push(cell(
                    ty,
                    &format!("c{r}{c}"),
                    &json!({"colspan": colspan, "rowspan": rowspan}),
                ));
            }
            rows.push(cells);
        }
        if rows.iter().any(Vec::is_empty) {
            continue;
        }
        saves_as_its_source_parses(&table(&rows));
        grids += 1;
    }
    assert!(grids > 300, "{grids}");
}

#[test]
fn a_leading_header_row_is_the_head() {
    let none = json!({});
    let doc = table(&[
        vec![cell("tableHeader", "a", &none)],
        vec![cell("tableCell", "b", &none)],
    ]);
    let tree = prosemirror_tree_value(&doc, &ProseMirrorOptions::default()).expect("save");
    let sections: Vec<&Value> = tree["children"][0]["children"]
        .as_array()
        .expect("sections")
        .iter()
        .map(|s| &s["type"])
        .collect();
    assert_eq!(sections, [&json!("table_head"), &json!("table_body")]);
    saves_as_its_source_parses(&doc);
}

#[test]
fn a_block_image_is_an_image_paragraph() {
    let image = json!({"type": "image", "attrs": {"src": "a.png", "alt": "A"}});
    let tree = prosemirror_tree_value(
        &json!({"type": "doc", "content": [image]}),
        &ProseMirrorOptions::default(),
    )
    .expect("save");
    assert_eq!(
        tree["children"][0],
        json!({"type": "paragraph", "role": "image", "children": [{"type": "image", "src": "a.png", "alt": "A"}]})
    );
    assert_eq!(
        convert(&json!({"type": "doc", "content": [{"type": "blockquote", "content": [image]}]})),
        "> ![A](a.png)\n"
    );
    assert_eq!(
        convert(&table(&[vec![
            json!({"type": "tableCell", "content": [image]})
        ]])),
        "| ![A](a.png) |\n"
    );
}

#[test]
fn a_paragraph_with_an_image_saves_with_its_role() {
    let image = json!({"type": "image", "attrs": {"src": "a.png", "alt": "a", "title": null}});
    for (content, source) in [
        (json!([image]), "![a](a.png)\n"),
        (json!([image, {"type": "hardBreak"}]), "![a](a.png)\\\n"),
        (
            json!([image, {"type": "text", "text": " x"}]),
            "![a](a.png) x\n",
        ),
    ] {
        let doc = json!({"type": "doc", "content": [{"type": "paragraph", "content": content}]});
        assert_eq!(convert(&doc), source);
    }
}

#[test]
fn attributes_the_save_does_not_read_are_reported() {
    let doc = json!({"type": "doc", "content": [{
        "type": "paragraph",
        "attrs": {"textAlign": "center"},
        "content": [
            {"type": "text", "text": "x", "marks": [
                {"type": "link", "attrs": {"href": "https://a", "target": "_self", "rel": "noopener noreferrer nofollow", "class": null}},
                {"type": "highlight", "attrs": {"color": "#ff0"}},
            ]},
            {"type": "image", "attrs": {"src": "i.png", "alt": null, "title": null, "width": 120}},
        ],
    }]});
    let report = from_prosemirror_with_report(&doc.to_string(), &ProseMirrorOptions::default())
        .expect("save");
    assert_eq!(write(&report.tree), "[{=x=}](https://a)![](i.png)\n");
    let ignored: Vec<(String, String, String, Value)> = report
        .ignored
        .iter()
        .map(|i| {
            (
                i.path.clone(),
                i.type_.clone(),
                i.attribute.clone(),
                i.value.clone(),
            )
        })
        .collect();
    assert_eq!(
        ignored,
        [
            (
                "/content/0".into(),
                "paragraph".into(),
                "textAlign".into(),
                json!("center")
            ),
            (
                "/content/0/content/0/marks/0".into(),
                "link".into(),
                "target".into(),
                json!("_self")
            ),
            (
                "/content/0/content/0/marks/1".into(),
                "highlight".into(),
                "color".into(),
                json!("#ff0")
            ),
            (
                "/content/0/content/1".into(),
                "image".into(),
                "width".into(),
                json!(120)
            ),
        ]
    );
    let loaded =
        to_prosemirror(&parse("|= a | b |\n| [x](https://a) | ![i](i.png) |\n")).expect("render");
    let report =
        from_prosemirror_with_report(&loaded.json, &ProseMirrorOptions::default()).expect("save");
    assert!(report.ignored.is_empty(), "{:?}", report.ignored);
}

#[test]
fn findings_point_at_their_editor_nodes() {
    let doc = json!({"type": "doc", "content": [
        {"type": "heading", "attrs": {"level": 1}, "content": [{"type": "text", "text": "Title"}]},
        {"type": "paragraph", "content": [{"type": "text", "text": "a "}, {"type": "text", "text": "b", "marks": [{"type": "italic"}]}]},
        {"type": "heading", "attrs": {"level": 3}, "content": [{"type": "text", "text": "Too deep"}]},
    ]});
    let report = from_prosemirror_with_report(&doc.to_string(), &ProseMirrorOptions::default())
        .expect("save");
    assert_eq!(report.pointer_for("/children/2"), Some("/content/2"));
    assert_eq!(
        report.pointer_for("/children/2/children/0"),
        Some("/content/2/content/0")
    );
    assert_eq!(
        report.pointer_for("/children/1/children/1"),
        Some("/content/1/content/1/marks/0")
    );
    assert_eq!(
        report.pointer_for("/children/1/children/1/children/0"),
        Some("/content/1/content/1")
    );
    assert_eq!(report.pointer_for(""), Some(""));
    assert_eq!(report.pointer_for("/children/9/children/0"), Some(""));
    assert_eq!(report.origins, [None, None, None]);
    let loaded = to_prosemirror(&parse("# a\n\nb\n")).expect("render");
    let saved =
        from_prosemirror_with_report(&loaded.json, &ProseMirrorOptions::default()).expect("save");
    assert_eq!(saved.origins, [Some(0), Some(1)]);
}

#[test]
fn code_blocks_keep_their_trailing_newlines() {
    for (text, expected) in [("x", "x\n"), ("x\n", "x\n"), ("x\n\n", "x\n\n"), ("", "\n")] {
        let content = if text.is_empty() {
            json!([])
        } else {
            json!([{"type": "text", "text": text}])
        };
        let doc = json!({"type": "doc", "content": [{"type": "codeBlock", "content": content}]});
        let tree = prosemirror_tree_value(&doc, &ProseMirrorOptions::default()).expect("save");
        assert_eq!(tree["children"][0]["value"], expected, "{text:?}");
    }
    for length in 1..=64usize {
        for newlines in 0..=3 {
            let value = "a".repeat(length) + &"\n".repeat(newlines);
            let tree = json!({"type": "document", "astVersion": "2.0", "children": [{"type": "code_block", "value": value}]});
            let doc = to_prosemirror(&tree_of(&tree)).expect("render");
            let back =
                prosemirror_tree_value(&doc.doc, &ProseMirrorOptions::default()).expect("save");
            assert_eq!(
                back["children"][0]["value"],
                json!(value),
                "{length} {newlines}"
            );
        }
    }
}

#[test]
fn the_kit_names_are_public() {
    assert_eq!(carve_type_for("tableHeader"), Some("table_cell"));
    assert_eq!(carve_type_for("carveDiv"), Some("div"));
    assert_eq!(carve_type_for("bold"), Some("strong"));
    assert_eq!(carve_type_for("carveBlock"), None);
    assert_eq!(carve_type_for("mention"), None);
    assert!(prosemirror_names().len() > 40);
}
