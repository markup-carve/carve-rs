//! The 0.2 layout clauses are an external corpus, not examples of this builder.

use carve::api;
use carve::options::Options;
use serde::Deserialize;
use serde_json::Value;

#[test]
fn every_layout_clause_has_its_required_sidecar_facts() {
    let mut de =
        serde_json::Deserializer::from_str(include_str!("../spec-conformance/clause-cases.json"));
    de.disable_recursion_limit();
    let cases = Value::deserialize(&mut de).unwrap();
    let mut checked = 0;
    for case in cases["cases"].as_array().unwrap() {
        if case["kind"] != "layout" {
            continue;
        }
        let id = case["id"].as_str().unwrap();
        let source = case["input"].as_str().unwrap();
        let result = api::parse_with_layout(source, &Options::default()).unwrap();
        let ordinary = api::parse(source, &Options::default()).unwrap();
        assert_eq!(
            api::encode_tree(&result.tree),
            api::encode_tree(&ordinary),
            "{id}"
        );
        let layout = result.layout;
        api::validate_layout(&layout).unwrap_or_else(|e| panic!("{id}: {e}"));
        let expected = &case["expected"]["layout"];
        for field in ["version", "encoding", "source", "lineEndings", "bom"] {
            if let Some(want) = expected.get(field) {
                assert_eq!(&layout[field], want, "{id}: {field}");
            }
        }
        for want in expected["nodes"].as_array().into_iter().flatten() {
            let path = want["path"].as_str().unwrap();
            let actual = layout["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["path"] == path)
                .unwrap_or_else(|| panic!("{id}: missing {path}"));
            for (field, value) in want.as_object().unwrap() {
                assert_eq!(&actual[field], value, "{id}: {path}/{field}");
            }
        }
        for (path, absent) in expected["layoutAbsent"].as_object().into_iter().flatten() {
            if let Some(actual) = layout["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["path"] == *path)
            {
                for field in absent
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    assert!(
                        actual.get(field).is_none(),
                        "{id}: {path}/{field} must be absent"
                    );
                }
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 38);
}

#[test]
fn invalid_utf8_keeps_original_bytes_and_byte_ranges() {
    let result = api::parse_with_layout(&[0x61, 0xff, 0x62][..], &Options::default()).unwrap();
    assert_eq!(result.layout["encoding"], "base64");
    assert_eq!(result.layout["source"], "Yf9i");
    assert_eq!(result.layout["nodes"][0]["startByte"], 0);
    assert_eq!(result.layout["nodes"][0]["endByte"], 3);
}

#[test]
fn reference_spelling_stops_before_attributes_with_brackets() {
    let source = "[a][b]{title=\"]\"}\n\n[b]: u\n";
    let result = api::parse_with_layout(source, &Options::default()).unwrap();
    let reference = result.layout["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node.get("rawRef").is_some())
        .unwrap();
    assert_eq!(reference["rawRef"], "[a][b]");
}
