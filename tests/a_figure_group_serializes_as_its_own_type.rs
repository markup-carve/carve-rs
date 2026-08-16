//! A composite figure serializes as `figure_group`, discriminated by TYPE.
//!
//! PART 12 §16: `children` in source order (no second `panels` key to
//! disagree with them), `caption` present only when the closer hosted one -
//! absent means uncaptioned, never an empty placeholder - and no `target`, no
//! title, no label, no `shortCaption`. Ingest is closed like every node: an
//! unknown field on a `figure_group` is refused (§11).

fn published(source: &str) -> String {
    carve::to_json(&carve::parse(source))
}

#[test]
fn the_wire_shape_is_the_schema_shape() {
    // A stray-content group, so the payload holds no panel figure: any
    // `target` in it could only be the group's, and the group has none.
    let json = published("::: figure\nstray\n:::\n^ Figure #: G\n");
    assert!(json.contains("\"type\":\"figure_group\""), "{json}");
    assert!(json.contains("\"children\":["), "{json}");
    assert!(json.contains("\"caption\":["), "{json}");
    assert!(!json.contains("\"target\""), "{json}");
}

#[test]
fn an_uncaptioned_group_publishes_no_caption_key() {
    let json = published("::: figure\n![one](a.png)\n^ (a) One\n:::\n");
    assert!(json.contains("\"type\":\"figure_group\""), "{json}");
    assert!(!json.contains("\"caption\":[]"), "{json}");
    // The panel's own caption is the only `caption` in the payload.
    let group_at = json.find("figure_group").expect("the group");
    assert!(
        !json[group_at..json.find("\"figure\"").unwrap_or(json.len())].contains("\"caption\""),
        "{json}"
    );
}

#[test]
fn the_round_trip_holds_with_positions() {
    // PART 12 §6: parse(x) serialized and deserialized equals parse(x).
    let source = "{#g .columns-2}\n::: figure\n![one](a.png)\n^ (a) One\n:::\n^ Figure #: G\n";
    let doc = carve::parse_with_options(source, &carve::Options::default().with_positions(true));
    let json = carve::to_json(&doc);
    let decoded = carve::from_json(&json).expect("own output decodes");
    assert_eq!(carve::to_json(&decoded), json);
}

#[test]
fn ingest_refuses_a_field_the_schema_does_not_name() {
    let payload = r#"{"type":"document","children":[{"type":"figure_group","children":[],"panels":[]}],"srcByteLength":0}"#;
    let err = carve::from_json(payload).expect_err("`panels` is not a schema field");
    assert!(err.to_string().contains("panels"), "{err}");
}

#[test]
fn ingest_requires_children() {
    let payload = r#"{"type":"document","children":[{"type":"figure_group"}],"srcByteLength":0}"#;
    assert!(carve::from_json(payload).is_err());
}

#[test]
fn a_hand_built_group_renders_through_the_ingest_path() {
    // A group the parse never made - stray content only, no panels - holds
    // that content directly (§4c).
    let payload = r#"{"type":"document","children":[{"type":"figure_group","children":[{"type":"paragraph","children":[{"type":"text","value":"stray"}]}]}],"srcByteLength":0}"#;
    let doc = carve::from_json(payload).expect("decodes");
    assert_eq!(
        carve::render_html(&doc).expect("renders"),
        "<figure class=\"carve-figure-group\">\n  <p>stray</p>\n</figure>"
    );
}

#[test]
fn a_denied_figure_group_strips_with_its_shell() {
    let doc = carve::parse("::: figure\n![one](a.png)\n^ (a) One\n:::\n^ Figure #: G\n");
    let profile = carve::Profile::full()
        .deny_block(&["figure_group"])
        .on_disallowed(carve::DisallowedAction::Strip);
    let result = carve::apply_profile(doc, &profile, None).expect("strip mode never errors");
    assert!(
        result
            .violations
            .iter()
            .any(|v| v.node_type == "figure_group"),
        "{:?}",
        result.violations
    );
    assert!(result.doc.children.is_empty(), "{:?}", result.doc.children);
}
