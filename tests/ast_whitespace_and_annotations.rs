use carve::{from_annotation_ranges_json, from_json};
use serde_json::{json, Value};

#[test]
fn annotation_projection_uses_the_shared_contract_fixture() {
    let fixture: Value = serde_json::from_str(include_str!(
        "spec/tests/fixtures/annotation-projection.json"
    ))
    .unwrap();
    let doc = from_json(&fixture["document"].to_string()).unwrap();
    for (key, accepted) in [("valid", true), ("invalid", false)] {
        for range in fixture[key].as_array().unwrap() {
            let mut range = range.clone();
            range["id"] = json!("r");
            range["kind"] = json!("test");
            let sidecar = json!({"version": 1, "ranges": [range]});
            assert_eq!(
                from_annotation_ranges_json(&sidecar.to_string(), &doc).is_ok(),
                accepted,
                "{sidecar}"
            );
        }
    }
}

#[test]
fn attributes_on_an_ingested_space_survive_source_output() {
    let doc = from_json(&json!({"type":"document","srcByteLength":0,"children":[
        {"type":"paragraph","children":[{"type":"non_breaking_space","attrs":{"classes":["gap"]}}]}
    ]}).to_string()).unwrap();
    let source = carve::render_carve(&doc).unwrap();
    assert_eq!(
        carve::to_html(&source),
        "<p><span class=\"gap\">&nbsp;</span></p>"
    );
}
