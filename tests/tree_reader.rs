use carve::reader::{TreeError, read_tree_json, read_tree_value, write_tree_json};
use serde::Deserialize as _;
use serde_json::Value;
use std::fmt::Write as _;

fn load(name: &str) -> Value {
    let path = format!("{}/spec-trees/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).expect("fixture");
    let mut parser = serde_json::Deserializer::from_str(&text);
    parser.disable_recursion_limit();
    Value::deserialize(&mut parser).expect("fixture JSON")
}

#[test]
fn c2_a_044_reader_replaces_nul_in_ingested_strings() {
    let json = r#"{"type":"document","astVersion":"2.0","children":[
        {"type":"paragraph","children":[{"type":"text","value":"a\u0000b"}]}
    ]}"#;
    let read = read_tree_json(json).expect("tree");
    let written: Value = serde_json::from_str(&write_tree_json(&read)).expect("JSON");
    assert_eq!(written["children"][0]["children"][0]["value"], "a\u{fffd}b");
}

#[test]
fn c2_a_165_positive_trees_round_trip() {
    for name in [
        "nodes.json",
        "examples-syntax-blocks.json",
        "examples-syntax-inline.json",
        "examples-html.json",
        "examples-writer.json",
        "examples-includes.json",
    ] {
        let file = load(name);
        for entry in file["trees"].as_array().expect("trees") {
            let original = &entry["tree"];
            let tree = read_tree_value(original.clone()).unwrap_or_else(|e| {
                panic!("{name} {}: {e}", entry["id"]);
            });
            let encoded = write_tree_json(&tree);
            let mut parser = serde_json::Deserializer::from_str(&encoded);
            parser.disable_recursion_limit();
            let written = Value::deserialize(&mut parser).expect("written JSON");
            assert_eq!(&written, original, "{name} {}", entry["id"]);
        }
    }
}

#[test]
fn c2_a_170_imported_corpus_trees_pass_the_reader() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/spec-conformance/examples.json"
    );
    let text = std::fs::read_to_string(path).expect("corpus");
    let mut parser = serde_json::Deserializer::from_str(&text);
    parser.disable_recursion_limit();
    let corpus = Value::deserialize(&mut parser).expect("corpus JSON");
    let mut checked = 0;
    for entry in corpus["examples"].as_array().expect("examples") {
        if entry["kind"] != "import" {
            continue;
        }
        let Some(children) = entry["expected"]["children"].as_array() else {
            continue;
        };
        let tree = serde_json::json!({"type":"document","astVersion":"2.0","children":children});
        read_tree_value(tree).unwrap_or_else(|error| panic!("{}: {error}", entry["id"]));
        checked += 1;
    }
    assert_eq!(checked, 65);
}

#[test]
fn c2_a_188_every_refused_tree_is_refused_by_its_gate() {
    let file = load("refused.json");
    for entry in file["trees"].as_array().expect("trees") {
        let result = read_tree_value(entry["tree"].clone());
        let id = entry["id"].as_str().unwrap_or("");
        let gate = entry["refusedBy"].as_str().unwrap_or("");
        match (gate, result) {
            ("schema", Err(TreeError::Schema { .. }))
            | ("reader", Err(TreeError::Reader { .. })) => {}
            (_, got) => panic!("{id} ({gate}): {got:?}"),
        }
    }
}

fn refused(id: &str, gate: &str) {
    let file = load("refused.json");
    let entry = file["trees"]
        .as_array()
        .expect("trees")
        .iter()
        .find(|entry| entry["id"] == id)
        .expect("refused case");
    let result = read_tree_value(entry["tree"].clone());
    assert!(
        matches!(
            (gate, result),
            ("schema", Err(TreeError::Schema { .. })) | ("reader", Err(TreeError::Reader { .. }))
        ),
        "{id}"
    );
}

#[test]
fn c2_a_035_attribute_key_must_be_a_key() {
    refused("attribute-key-not-a-key", "schema");
}

#[test]
fn c2_a_102_math_number_needs_display_prefix() {
    refused("math-number-without-prefix", "schema");
    refused("math-prefix-inline", "schema");
}

#[test]
fn c2_a_086_table_groups_have_order() {
    refused("table-group-order", "reader");
}

#[test]
fn c2_a_058_c2_a_056_default_spellings_are_refused() {
    refused("default-citation-mode", "schema");
    refused("default-autolink-text", "reader");
    refused("default-empty-attrs", "schema");
    refused("ordered-marker-without-style", "schema");
}

#[test]
fn c2_a_098_smart_punctuation_glyph_matches_kind() {
    refused("smart-punctuation-without-glyph", "schema");
    refused("smart-punctuation-other-glyph", "reader");
}

#[test]
fn c2_a_181_include_caption_requires_code() {
    refused("include-caption-without-code", "schema");
}

#[test]
fn c2_a_018_unknown_type_is_refused_without_recursive_schema_search() {
    let mut child = serde_json::json!({"type":"unknown"});
    for _ in 0..20 {
        child = serde_json::json!({"type":"unknown","children":[child]});
    }
    let tree = serde_json::json!({"type":"document","astVersion":"2.0","children":[child]});
    assert!(matches!(
        read_tree_value(tree),
        Err(TreeError::Schema { .. })
    ));
}

#[test]
fn c2_a_181_reversed_column_range_is_refused() {
    let tree = serde_json::json!({"type":"document","astVersion":"2.0","children":[
        {"type":"include","path":"a.crv","options":{"cols":"5-2"}}
    ]});
    assert!(matches!(
        read_tree_value(tree),
        Err(TreeError::Reader { .. })
    ));
}

#[test]
fn c2_a_188_envelope_schema_and_round_trip() {
    let tree = serde_json::json!({"astVersion":"2.0","document":{
        "type":"document","astVersion":"2.0","children":[]
    },"extensions":[]});
    let read = read_tree_value(tree.clone()).expect("envelope");
    assert_eq!(
        serde_json::from_str::<Value>(&write_tree_json(&read)).expect("wire"),
        tree
    );
    let mut malformed = tree;
    malformed["extensions"] = serde_json::json!([{"bogus":1}]);
    assert!(matches!(
        read_tree_value(malformed),
        Err(TreeError::Schema { .. })
    ));
}

#[test]
fn c2_a_189_opaque_data_is_not_a_core_node() {
    let file = load("nodes.json");
    let mut tree = file["trees"]
        .as_array()
        .expect("trees")
        .iter()
        .find(|entry| entry["id"] == "common-fields")
        .expect("case")["tree"]
        .clone();
    tree["children"][0]["data"]["org.example.flag"] =
        serde_json::json!({"nested":[{"resourceId":"r"}]});
    assert!(read_tree_value(tree.clone()).is_ok());
    tree["children"][0]["data"]["org.example.flag"]["nested"][0]["type"] =
        serde_json::json!("text");
    assert!(matches!(
        read_tree_value(tree),
        Err(TreeError::Reader { .. })
    ));
}

#[test]
fn c2_a_171_accepts_parser_tree_at_the_block_limit() {
    let source = format!("{}x", "> ".repeat(200));
    let tree = carve::tree_json(&source, &carve::Options::default());
    assert!(read_tree_json(&tree).is_ok());
}

#[test]
fn c2_a_171_accepts_included_files_at_their_own_block_limits() {
    let mut text = String::from("{\"type\":\"document\",\"astVersion\":\"2.0\",\"children\":[");
    for _ in 0..200 {
        text.push_str("{\"type\":\"block_quote\",\"children\":[");
    }
    for file in 0..16 {
        let _ = write!(
            text,
            "{{\"type\":\"include\",\"path\":\"f{file}\",\"file\":\"f{file}\",\"children\":["
        );
        for _ in 0..200 {
            text.push_str("{\"type\":\"block_quote\",\"children\":[");
        }
    }
    text.push_str("{\"type\":\"paragraph\",\"children\":[{\"type\":\"text\",\"value\":\"x\"}]}");
    for _ in 0..16 {
        for _ in 0..200 {
            text.push_str("]}");
        }
        text.push_str("]}");
    }
    for _ in 0..200 {
        text.push_str("]}");
    }
    text.push_str("]}");
    assert!(read_tree_json(&text).is_ok());
}

#[test]
fn c2_a_018_known_required_extension_is_accepted() {
    let tree = serde_json::json!({"type":"document","astVersion":"2.0","children":[],"extensions":[
        {"name":"carve.tabs","version":"1","required":true,"slots":["tabs"]}
    ]});
    assert!(read_tree_value(tree).is_ok());
}

#[test]
fn c2_a_170_all_of_checks_extension_slot_item_name() {
    let tree = serde_json::json!({"type":"document","astVersion":"2.0","children":[{
        "type":"block_extension","name":"carve.tabs","version":"1","children":[],"fallback":[],
        "slots":{"tabs":[{"type":"block_extension","name":"org.example.other","version":"1",
            "children":[],"fallback":[]}]}
    }]});
    assert!(matches!(
        read_tree_value(tree),
        Err(TreeError::Schema { .. })
    ));
}

#[test]
fn c2_a_188_reader_byte_mutations_do_not_panic() {
    let file = load("nodes.json");
    for entry in file["trees"].as_array().expect("trees").iter().take(24) {
        let source = entry["tree"].to_string().into_bytes();
        for index in (0..source.len()).step_by((source.len() / 64).max(1)) {
            for byte in [0, b'{', b'"', b'0', 255] {
                let mut changed = source.clone();
                changed[index] = byte;
                let text = String::from_utf8_lossy(&changed);
                let result = std::panic::catch_unwind(|| read_tree_json(&text));
                assert!(result.is_ok(), "{} byte {index}", entry["id"]);
            }
        }
    }
    let deep = format!("{}0{}", "[".repeat(32769), "]".repeat(32769));
    assert!(matches!(read_tree_json(&deep), Err(TreeError::Json(_))));
}
