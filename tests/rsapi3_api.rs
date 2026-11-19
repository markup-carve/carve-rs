//! The calls of the parity ruling of 19 November (carve-rs#4345,
//! carve-rs#4346): positionOf, diffTree, applyPatch, the fingerprints,
//! headingId and the package facts.

use carve::api::{self, HeadingIds, Options, json::call};
use serde_json::{Value, json};

fn reply(request: &Value) -> Value {
    serde_json::from_str(&call(&request.to_string())).unwrap()
}

#[test]
fn c2_t_041_position_clause_cases() {
    let path = format!(
        "{}/spec-conformance/clause-cases.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    let corpus: Value = serde::Deserialize::deserialize(&mut de).unwrap();
    let mut seen = 0;
    for c in corpus["cases"].as_array().unwrap() {
        if c["kind"] != "position" {
            continue;
        }
        seen += 1;
        let mut request = json!({"call": "positionOf", "offset": c["offset"]});
        if let Some(text) = c["input"].as_str() {
            request["source"] = text.into();
        } else {
            let bytes: Vec<u8> = c["inputBytes"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .map(|h| u8::from_str_radix(h, 16).unwrap())
                .collect();
            request["sourceBytes"] = json!(bytes);
        }
        let got = reply(&request);
        let want = &c["expected"];
        if let Some(refusal) = want.get("refusal") {
            assert_eq!(got["error"]["name"], refusal["error"], "{}", c["id"]);
            assert_eq!(refusal["path"], "/offset");
            assert!(
                got["error"]["message"]
                    .as_str()
                    .unwrap()
                    .starts_with("option offset")
            );
        } else {
            assert_eq!(got["ok"]["line"], want["line"], "{}", c["id"]);
            assert_eq!(got["ok"]["column"], want["column"], "{}", c["id"]);
        }
    }
    assert_eq!(seen, 15);
}

#[test]
fn position_of_in_rust() {
    let p = api::position_of(&[0xC3, 0xA4, 0x0A, 0x62][..], 3).unwrap();
    assert_eq!((p.line, p.column), (2, 1));
    assert!(matches!(
        api::position_of(&[0xC3, 0xA4, 0x0A, 0x62][..], 1),
        Err(api::Error::Option { option, .. }) if option == "offset"
    ));
}

#[test]
fn fingerprints() {
    assert_eq!(
        api::source_fingerprint(b"abc"),
        "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let tree = api::parse("a b", &Options::default()).unwrap();
    let encoded = api::encode_tree(&tree);
    assert_eq!(
        api::tree_fingerprint(&tree),
        api::source_fingerprint(encoded.as_bytes())
    );
    let decoded = api::decode_tree(&encoded).unwrap();
    assert_eq!(
        api::tree_fingerprint(&decoded),
        api::tree_fingerprint(&tree)
    );
}

fn bare(source: &str) -> Value {
    reply(&json!({"call": "parse", "source": source, "positions": false}))["ok"]["tree"].clone()
}

#[test]
fn diff_tree_and_apply_patch() {
    let (a, b) = (bare("a b"), bare("a c"));
    let patch = reply(&json!({"call": "diffTree", "a": a, "b": b}))["ok"]["patch"].clone();
    assert_eq!(
        patch,
        json!([{ "op": "replace", "path": "/children/0/children/0/value", "value": "a c" }])
    );
    let fingerprint =
        reply(&json!({"call": "treeFingerprint", "tree": a}))["ok"]["fingerprint"].clone();
    let applied = reply(
        &json!({"call": "applyPatch", "tree": a, "patch": patch, "fingerprint": fingerprint}),
    );
    assert_eq!(applied["ok"]["tree"], b);
    let same = reply(&json!({"call": "diffTree", "a": a, "b": a}));
    assert_eq!(same["ok"]["patch"], json!([]));
}

#[test]
fn diff_then_apply_gives_the_second_tree() {
    let sources = [
        "a b",
        "# Title\n\ntext *x*",
        "- [ ] x\n- y\n\n> q",
        "| a | b |\n| 1 | 2 |",
        "```js\nx\n```\n\n::: note\nn\n:::",
        "",
    ];
    for x in sources {
        for y in sources {
            let (a, b) = (
                api::parse(x, &Options::default()).unwrap(),
                api::parse(y, &Options::default()).unwrap(),
            );
            let patch = api::diff_tree(&a, &b);
            let got = api::apply_patch(&a, &patch, Some(&api::tree_fingerprint(&a))).unwrap();
            let want: Value = serde_json::from_str(&api::encode_tree(&b)).unwrap();
            let got: Value = serde_json::from_str(&api::encode_tree(&got)).unwrap();
            assert_eq!(got, want, "{x:?} -> {y:?}");
        }
    }
}

#[test]
fn apply_patch_refusals() {
    let a = bare("a b");
    let refused = |patch: Value, fingerprint: Option<&str>| {
        let mut request = json!({"call": "applyPatch", "tree": a, "patch": patch});
        if let Some(f) = fingerprint {
            request["fingerprint"] = f.into();
        }
        let error = reply(&request)["error"].clone();
        assert_eq!(error["name"], "CarvePatchError", "{error}");
        error["path"].as_str().unwrap().to_string()
    };
    assert_eq!(refused(json!([]), Some("sha256:00")), "");
    assert_eq!(refused(json!([{"op": "bogus", "path": ""}]), None), "/0/op");
    assert_eq!(
        refused(json!([{"op": "remove", "path": "/nope"}]), None),
        "/0/path"
    );
    assert_eq!(
        refused(json!([{"op": "add", "path": "/children/0"}]), None),
        "/0/value"
    );
    assert_eq!(
        refused(json!([{"op": "test", "path": "/type", "value": "x"}]), None),
        "/0/value"
    );
    assert_eq!(
        refused(
            json!([{"op": "move", "from": "/children", "path": "/children/0"}]),
            None
        ),
        "/0/path"
    );
    assert_eq!(
        refused(json!([{"op": "remove", "path": "/children/0/type"}]), None),
        "/children/0/type"
    );
    assert_eq!(refused(json!({"op": "add"}), None), "");
    for bad in ["/children~2x", "/children~", "children"] {
        assert_eq!(
            refused(json!([{"op": "remove", "path": bad}]), None),
            "/0/path",
            "{bad}"
        );
    }
}

#[test]
fn heading_id() {
    assert_eq!(
        api::heading_id("Héllo Wörld", HeadingIds::AsciiLower).as_deref(),
        Some("hello-world")
    );
    assert_eq!(api::heading_id("Héllo Wörld", HeadingIds::Off), None);
    assert_eq!(
        api::heading_id("1 two", HeadingIds::KeepCase).as_deref(),
        Some("s-1-two")
    );
    let id = reply(
        &json!({"call": "headingId", "text": "Héllo Wörld", "options": {"headingIds": "ascii-lower"}}),
    );
    assert_eq!(id["ok"]["id"], "hello-world");
    let html = api::render(
        "# Héllo Wörld",
        &Options {
            heading_ids: HeadingIds::AsciiLower,
            ..Options::default()
        },
    )
    .unwrap();
    assert!(html.contains("id=\"hello-world\""), "{html}");
    let off = reply(&json!({"call": "headingId", "text": "x", "options": {"headingIds": "off"}}));
    assert_eq!(off["ok"]["id"], Value::Null);
    let other = reply(&json!({"call": "headingId", "text": "x", "options": {"rawHtml": true}}));
    assert_eq!(other["error"]["name"], "CarveOptionError");
}

#[test]
fn package_facts() {
    assert_eq!(api::VERSION, env!("CARGO_PKG_VERSION"));
    assert_eq!(api::SPEC_VERSION.version, "0.2");
    let pinned = include_str!("../spec-data/SOURCE_COMMIT").trim();
    assert_eq!(api::SPEC_VERSION.commit, pinned);
    assert_eq!(api::NODE_TYPES.block.first(), Some(&"frontmatter"));
    assert!(api::NODE_TYPES.block.contains(&"paragraph"));
    assert!(api::NODE_TYPES.inline.contains(&"text"));
    assert!(!api::NODE_TYPES.inline.contains(&"paragraph"));
}
