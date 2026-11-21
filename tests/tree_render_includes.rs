//! A supplied tree with expanded includes renders their children, with the
//! definitions of each included file (C2-N-053, C2-N-075, carve-rs#4331).

use carve::api;

#[test]
fn every_expanded_include_example_renders_from_its_tree() {
    let text = std::fs::read_to_string("spec-conformance/examples.json").unwrap();
    let mut de = serde_json::Deserializer::from_str(&text);
    de.disable_recursion_limit();
    let all: serde_json::Value = serde::Deserialize::deserialize(&mut de).unwrap();
    let mut checked = 0;
    for example in all["examples"].as_array().unwrap() {
        let (Some(tree), Some(html)) = (
            example.pointer("/expected/tree"),
            example.pointer("/expected/html").and_then(|h| h.as_str()),
        ) else {
            continue;
        };
        let json = tree.to_string();
        if !json.contains(r#""type":"include""#)
            || example
                .get("options")
                .is_some_and(|o| o.as_object().is_some_and(|m| !m.is_empty()))
        {
            continue;
        }
        let response = api::json::call(&format!(r#"{{"call":"render","tree":{json}}}"#));
        let got: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(
            got.pointer("/ok/html").and_then(|h| h.as_str()),
            Some(html),
            "{}",
            example["id"]
        );
        checked += 1;
    }
    assert!(checked >= 40, "{checked}");
}
