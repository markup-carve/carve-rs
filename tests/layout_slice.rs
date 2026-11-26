//! The first opt-in source-layout slice for plain one-line ATX headings.

use carve::api;
use carve::options::Options;
use serde_json::{Value, json};

#[test]
fn a_heading_returns_the_same_tree_and_a_version_two_sidecar() {
    let opts = Options::default();
    let parsed = api::parse_with_layout("# Title", &opts).unwrap();
    let ordinary = api::parse("# Title", &opts).unwrap();
    assert_eq!(api::encode_tree(&parsed.tree), api::encode_tree(&ordinary));
    assert_eq!(parsed.diagnostics, Vec::<api::Entry>::new());
    assert_eq!(parsed.suppressed_diagnostics, 0);
    assert_eq!(
        parsed.layout,
        json!({
            "version": 2,
            "encoding": "utf-8",
            "source": "# Title",
            "lineEndings": "none",
            "bom": false,
            "nodes": [{
                "path": "/children/0", "startByte": 0, "endByte": 7,
                "blankLinesBefore": 0, "blankLinesAfter": 0,
                "markerRaw": "#", "markerColumn": 0
            }]
        })
    );
}

#[test]
fn the_json_call_returns_the_layout_beside_the_tree() {
    let request = json!({"call": "parseWithLayout", "source": "## A\n"});
    let response: Value = serde_json::from_str(&api::json::call(&request.to_string())).unwrap();
    let ordinary: Value = serde_json::from_str(&api::json::call(
        &json!({"call": "parse", "source": "## A\n"}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["ok"]["tree"], ordinary["ok"]["tree"]);
    assert_eq!(response["ok"]["layout"]["lineEndings"], "lf");
    assert_eq!(
        response["ok"]["layout"]["nodes"][0],
        json!({
            "path": "/children/0", "startByte": 0, "endByte": 4,
            "blankLinesBefore": 0, "blankLinesAfter": 0,
            "markerRaw": "##", "markerColumn": 0
        })
    );
}
