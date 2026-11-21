//! `api::parse` builds its tree from the parse itself instead of reading
//! its own JSON back through the tree reader (carve-rs#4336). The tree must
//! be the one the reader gives for that JSON.

use carve::api::{self, Options};
use carve::include::{Cause, Includes, Request};
use serde_json::Value;
use std::sync::Arc;

fn same_as_reader(input: &str, opts: &Options) {
    let tree = api::parse(input, opts).unwrap();
    let read = api::decode_tree(&carve::tree_json(input, opts)).unwrap();
    assert_eq!(tree.document(), read.document(), "document of {input:?}");
    assert_eq!(
        api::encode_tree(&tree),
        api::encode_tree(&read),
        "JSON of {input:?}"
    );
}

#[test]
fn every_clause_case_input_gives_the_reader_tree() {
    let mut de =
        serde_json::Deserializer::from_str(include_str!("../spec-conformance/clause-cases.json"));
    de.disable_recursion_limit();
    let cases = <Value as serde::Deserialize>::deserialize(&mut de).unwrap();
    let mut n = 0;
    for case in cases["cases"].as_array().unwrap() {
        if let Some(input) = case["input"].as_str() {
            same_as_reader(input, &Options::default());
            n += 1;
        }
    }
    assert!(n > 900);
}

#[test]
fn nul_bom_and_comment_only_sources_give_the_reader_tree() {
    for input in [
        "a\0b *c\0*",
        "\u{feff}# Title\n\ntext",
        &format!("%% {}\n", "x".repeat(4000)),
        "",
    ] {
        same_as_reader(input, &Options::default());
    }
}

#[test]
fn an_expanded_include_gives_the_reader_tree() {
    let resolver = Arc::new(|r: &Request<'_>| {
        (r.path == "a.crv")
            .then(|| (b"Text *a*.\n\n- item\n".to_vec(), "a.crv".to_string()))
            .ok_or(Cause::NotFound)
    });
    let opts = Options {
        includes: Some(Includes::new(resolver)),
        ..Options::default()
    };
    same_as_reader("Before.\n\n{{ a.crv }}\n\n{{ b.crv }}\n", &opts);
}

#[test]
fn malformed_bytes_give_the_reader_tree() {
    let bytes: &[u8] = b"a\xffb *c*\n\n- \xfe";
    let tree = api::parse(bytes, &Options::default()).unwrap();
    let read = api::decode_tree(&api::encode_tree(&tree)).unwrap();
    assert_eq!(tree.document(), read.document());
}

#[test]
fn the_json_boundary_returns_the_encoded_tree() {
    let source = "# T\n\nA *b* [c](https://example.com).\n";
    let expected: Value =
        serde_json::from_str(&carve::tree_json(source, &Options::default())).unwrap();
    for call in ["parse", "parseWithDiagnostics"] {
        let request = serde_json::json!({ "call": call, "source": source }).to_string();
        let out: Value = serde_json::from_str(&api::json::call(&request)).unwrap();
        assert_eq!(out["ok"]["tree"], expected, "{call}");
        if call == "parseWithDiagnostics" {
            assert!(out["ok"]["diagnostics"].is_array());
            assert_eq!(out["ok"]["suppressedDiagnostics"], 0);
        }
    }
    let request = serde_json::json!({ "call": "parse", "source": source, "positions": false });
    let out: Value = serde_json::from_str(&api::json::call(&request.to_string())).unwrap();
    assert!(out["ok"]["tree"]["children"][0].get("pos").is_none());
    assert_eq!(out["ok"]["tree"]["children"][0]["type"], "heading");
}
