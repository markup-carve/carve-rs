//! `encodeTree` writes canonical JSON (C2-A-217) on every path a tree takes
//! (carve-rs#4619).

use carve::api::{self, Options};
use serde_json::Value;
use std::sync::Arc;

fn canonical(text: &str) -> String {
    serde_json::from_str::<Value>(text).unwrap().to_string()
}

fn with_includes() -> Options {
    let resolver = Arc::new(|request: &carve::include::Request<'_>| match request.path {
        "b.crv" => Ok((
            b"---\ntitle: B\n---\n\nin *b* [x](u)\n".to_vec(),
            "b.crv".to_string(),
        )),
        _ => Err(carve::include::Cause::NotFound),
    });
    Options {
        includes: Some(carve::include::Includes::new(resolver)),
        ..Options::default()
    }
}

const INPUTS: &[&str] = &[
    "",
    "# H\n\npara *s* /i/ `c` [l](u \"t\")\n",
    "{#id .c k=\"v\\\"q\"}\n> - a\n>   - b\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
    "---yaml\nk: v\n---\n\ntext[^n]\n\n[^n]: note \u{e9}\u{1f600}\n",
    "{{ b.crv }}\n\nafter\n",
    "{{ missing.crv }}\n",
];

#[test]
fn parsed_trees_encode_canonically_with_and_without_includes() {
    for opts in [Options::default(), with_includes()] {
        for input in INPUTS {
            let tree = api::parse(*input, &opts).unwrap();
            let encoded = api::encode_tree(&tree);
            assert_eq!(encoded, canonical(&encoded), "{input:?}");
            let bare = api::encode_tree_with(&tree, api::EncodeOptions { positions: false });
            assert_eq!(bare, canonical(&bare), "{input:?}");
        }
    }
}

#[test]
fn an_included_file_stamps_positions_and_the_top_file_does_not() {
    let tree = api::parse("top\n\n{{ b.crv }}\n", &with_includes()).unwrap();
    let encoded = api::encode_tree(&tree);
    assert!(encoded.matches(r#""file":"b.crv""#).count() > 1);
    let plain = api::parse("top\n", &with_includes()).unwrap();
    assert!(!api::encode_tree(&plain).contains(r#""file""#));
}

/// The same tree with members in reverse order and spacing.
fn reversed(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let fields: Vec<String> = m
                .iter()
                .rev()
                .map(|(k, v)| format!("{} : {}", Value::from(k.as_str()), reversed(v)))
                .collect();
            format!("{{ {} }}", fields.join(" , "))
        }
        Value::Array(a) => format!(
            "[ {} ]",
            a.iter().map(reversed).collect::<Vec<_>>().join(" , ")
        ),
        other => other.to_string(),
    }
}

#[test]
fn a_decoded_tree_is_reencoded_in_canonical_order() {
    let tree = api::parse("a *b*\n", &Options::default()).unwrap();
    let encoded = api::encode_tree(&tree);
    let value: Value = serde_json::from_str(&encoded).unwrap();
    let decoded = api::decode_tree(&reversed(&value)).unwrap();
    assert_eq!(api::encode_tree(&decoded), encoded);
}
