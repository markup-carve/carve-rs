// Category 509 goldens from markup-carve/carve@40d5922a.
// The extra fixtures cover marker widths, payload, siblings, and tabs.
use carve::Options;

#[test]
fn nested_fence_ownership_matches_spec_509() {
    crate::common::assert_rule_a_pins(
        "nested_fence_ownership::nested_fence_ownership_matches_spec_509",
    );
}

#[test]
fn nested_fence_markers_payload_and_siblings() {
    crate::common::assert_rule_a_pins(
        "nested_fence_ownership::nested_fence_markers_payload_and_siblings",
    );
}

#[test]
fn interrupted_fence_keeps_its_source_span() {
    let source = "- a\n  - b\n    ```\n    p\nx\n    ```\n";
    let options = Options {
        positions: true,
        ..Options::default()
    };
    let doc: serde_json::Value =
        serde_json::from_str(&carve::to_json_with_options(source, &options)).unwrap();
    let code = &doc["children"][0]["items"][0]["children"][1]["items"][0]["children"][1];
    assert_eq!(code["type"], "code_block");
    assert_eq!(code["content"], "p\n");
    assert_eq!(
        code["pos"],
        serde_json::json!({
            "startLine": 3, "endLine": 4, "startColumn": 5, "endColumn": 6,
            "startOffset": 14, "endOffset": 23
        })
    );
    assert_eq!(doc["children"][1]["pos"]["startLine"], 5);
}
