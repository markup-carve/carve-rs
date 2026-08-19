const PAYLOAD: &str = r#"{"type":"document","children":[{"type":"figure","target":{"type":"image","src":"/i.png","alt":"alt"},"caption":[{"type":"text","value":"Full caption"}],"shortCaption":[{"type":"text","value":"Navigation label"}]},{"type":"table","rows":[],"shortCaption":[{"type":"text","value":"Navigation label"}]}],"srcByteLength":0}"#;

#[test]
fn structural_short_captions_round_trip_and_stay_out_of_html() {
    let document = carve::ast_json::from_json(PAYLOAD).expect("short captions decode");
    let republished = carve::ast_json::to_json(&document);
    assert!(republished.contains("\"shortCaption\""));
    assert_eq!(republished.matches("Navigation label").count(), 2);

    let html = carve::render_html(&document).expect("renders");
    assert!(html.contains("<figcaption>Full caption</figcaption>"));
    assert!(!html.contains("Navigation label"));
}
