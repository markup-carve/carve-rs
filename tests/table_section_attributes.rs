use carve::{from_json, render_html, to_json};

#[test]
fn attributed_empty_sections_survive_exchange_and_html() {
    let input = r#"{"type":"document","srcByteLength":0,"children":[{"type":"table","rows":[],"rowGroups":{"headRows":0,"footRows":0,"headAttrs":{"id":"head","keyValues":{"onclick":"evil()"}},"footAttrs":{"classes":["foot"]},"bodies":[{"headRows":0,"bodyRows":0,"attrs":{"id":"body"}}]}}]}"#;
    let doc = from_json(input).unwrap();
    let encoded = to_json(&doc);
    assert!(encoded.contains("\"headAttrs\""));
    assert!(encoded.contains("\"footAttrs\""));
    let html = render_html(&doc).unwrap();
    assert!(html.contains("<thead id=\"head\">"), "{html}");
    assert!(html.contains("<tbody id=\"body\">"), "{html}");
    assert!(html.contains("<tfoot class=\"foot\">"), "{html}");
    assert!(!html.contains("onclick"));
    assert_eq!(to_json(&from_json(&encoded).unwrap()), encoded);
}

#[test]
fn text_targets_report_section_attribute_loss() {
    let doc = from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"table","rows":[],"rowGroups":{"headRows":0,"footRows":0,"headAttrs":{"id":"h"},"bodies":[]}}]}"#).unwrap();
    for target in [
        carve::RenderTarget::Plain,
        carve::RenderTarget::Markdown,
        carve::RenderTarget::Ansi,
    ] {
        let result =
            carve::with_render_loss_report(target, carve::CheckedRenderOptions::default(), || {
                match target {
                    carve::RenderTarget::Plain => carve::render_plain_text(&doc),
                    carve::RenderTarget::Markdown => carve::render_markdown(&doc),
                    _ => carve::render_ansi(&doc),
                }
            })
            .unwrap();
        assert_eq!(result.total_losses, 1);
        assert_eq!(result.losses[0].code, "table-section-attributes-dropped");
        assert!(result.losses[0].message.contains("rowGroups.headAttrs"));
    }
}

#[test]
fn shared_section_rendering_fixtures() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "spec/tests/fixtures/table-section-attributes.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let doc = from_json(&case["ast"].to_string()).unwrap();
        assert_eq!(
            render_html(&doc).unwrap().trim_end(),
            case["html"].as_str().unwrap()
        );
        let encoded: serde_json::Value = serde_json::from_str(&to_json(&doc)).unwrap();
        assert_eq!(encoded, case["ast"]);
    }
}
