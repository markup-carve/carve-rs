//! C2-A-219 (carve#6099): a tree number is a finite binary64 value that
//! keeps its value through encodeTree.

#[path = "support/holds.rs"]
mod holds;

use carve::api::{self, json::call};
use holds::{Gate, Holds};
use serde_json::{Value, json};

fn corpus() -> Value {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap()
}

fn metadata(number: &str) -> String {
    format!(
        r#"{{"type":"document","astVersion":"2.0","children":[],"metadata":{{"fields":{{"n":{{"kind":"number","value":{number}}}}}}}}}"#
    )
}

/// Fingerprint cases with a `json` input hash the tree of that text as the
/// engine reads it, its numbers spelled as written.
/// `(case id, cause)`.
const HELD: Holds = &[];

#[test]
fn c2_a_219_fingerprint_cases_of_json_text() {
    let mut gate = Gate::new("fingerprint cases", HELD);
    let mut seen = 0;
    for c in corpus()["cases"].as_array().unwrap() {
        let Some(text) = c["json"].as_str().filter(|_| c["kind"] == "fingerprint") else {
            continue;
        };
        seen += 1;
        let id = c["id"].as_str().unwrap_or("");
        gate.case(id, || {
            let out: Value = serde_json::from_str(&call(&format!(
                r#"{{"call":"treeFingerprint","tree":{text}}}"#
            )))
            .unwrap();
            assert_eq!(
                out["ok"]["fingerprint"], c["expected"]["fingerprint"],
                "{}: {out}",
                c["id"]
            );
            let tree = api::decode_tree(text).unwrap();
            assert_eq!(
                Some(api::tree_fingerprint(&tree).as_str()),
                c["expected"]["fingerprint"].as_str(),
                "{}",
                c["id"]
            );
        });
    }
    gate.finish();
    assert!(seen >= 3, "{seen} fingerprint cases with json");
}

#[test]
fn refused_numbers_name_their_path_in_every_reader() {
    for number in [
        "9007199254740993",
        "-9007199254740993",
        "12345678901234567890",
        "123456789012345678901234567890",
        "0.10000000000000000001",
        "2e-324",
        "1e400",
        "-1e400",
        "1152921504606846976",
    ] {
        let text = metadata(number);
        let path = "/metadata/fields/n/value";
        let err = api::decode_tree(&text).unwrap_err();
        assert_eq!(err.path().as_deref(), Some(path), "{number}: {err:?}");
        let out: Value =
            serde_json::from_str(&call(&format!(r#"{{"call":"encodeTree","tree":{text}}}"#)))
                .unwrap();
        assert_eq!(out["error"]["name"], "CarveTreeError", "{number}: {out}");
        assert_eq!(out["error"]["path"], path, "{number}: {out}");
    }
}

#[test]
fn a_supplied_tree_value_is_checked_too() {
    let mut tree: Value = serde_json::from_str(&metadata("0")).unwrap();
    tree["metadata"]["fields"]["n"]["value"] = json!(9_007_199_254_740_993_u64);
    let err = carve::reader::read_tree_value(tree.clone()).unwrap_err();
    assert!(
        matches!(&err, carve::reader::TreeError::Reader { path, .. } if path == "/metadata/fields/n/value"),
        "{err:?}"
    );
    tree["metadata"]["fields"]["n"]["value"] = json!(9_007_199_254_740_992_u64);
    assert!(carve::reader::read_tree_value(tree).is_ok());
}

#[test]
fn accepted_numbers_are_written_as_rfc_8785_spells_them() {
    for (number, written) in [
        ("1.0", "1"),
        ("-0", "0"),
        ("-0.0", "0"),
        ("1E2", "100"),
        ("1e21", "1e+21"),
        ("1e20", "100000000000000000000"),
        ("0.000001", "0.000001"),
        ("1e-7", "1e-7"),
        ("-1.5E-7", "-1.5e-7"),
        ("5e-324", "5e-324"),
        ("1152921504606847000", "1152921504606847000"),
    ] {
        let tree = api::decode_tree(&metadata(number)).unwrap();
        let encoded = api::encode_tree(&tree);
        assert!(
            encoded.contains(&format!(r#""value":{written}}}"#)),
            "{number}: {encoded}"
        );
        let reread = api::decode_tree(&encoded).unwrap();
        assert_eq!(api::encode_tree(&reread), encoded, "{number}");
    }
}

#[test]
fn a_key_with_escapes_is_unescaped_in_the_path() {
    let text = r#"{"type":"document","astVersion":"2.0","children":[],"metadata":{"fields":{"a/b~\"c":{"kind":"number","value":1e400}}}}"#;
    let err = api::decode_tree(text).unwrap_err();
    assert_eq!(
        err.path().as_deref(),
        Some("/metadata/fields/a~1b~0\"c/value")
    );
}

#[test]
fn a_call_is_refused_only_for_a_tree_it_reads() {
    let lost = r#"{"x":9007199254740993}"#;
    let out: Value = serde_json::from_str(&call(&format!(
        r#"{{"call":"parse","source":"a","tree":{lost},"a":{lost}}}"#
    )))
    .unwrap();
    assert!(out.get("ok").is_some(), "{out}");
    let out: Value = serde_json::from_str(&call(&format!(
        r#"{{"call":"diffTree","a":{},"b":{},"tree":{lost}}}"#,
        metadata("1"),
        metadata("9007199254740993"),
    )))
    .unwrap();
    assert_eq!(out["error"]["path"], "/metadata/fields/n/value", "{out}");
}

#[test]
fn a_patch_value_that_binary64_does_not_keep_is_refused() {
    let out: Value = serde_json::from_str(&call(&format!(
        r#"{{"call":"applyPatch","tree":{},"patch":[{{"op":"replace","path":"/metadata/fields/n/value","value":0.10000000000000000001}}]}}"#,
        metadata("1"),
    )))
    .unwrap();
    assert_eq!(out["error"]["name"], "CarvePatchError", "{out}");
    assert_eq!(out["error"]["path"], "/0/value", "{out}");
    let out: Value = serde_json::from_str(&call(&format!(
        r#"{{"call":"sourceFingerprint","source":"a","tree":{}}}"#,
        metadata("0.10000000000000000001"),
    )))
    .unwrap();
    assert!(out.get("ok").is_some(), "{out}");
}
