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

/// CARVE-P12-034: a discarded section attributes field is a `field-unspellable`
/// conversion diagnostic, not a render loss. `CARVE-P2-024`'s code enum is
/// closed at `raw-format-dropped` and `ruby-flattened` (CARVE-P11-046), so a
/// render-loss report cannot name the field this drops.
#[test]
fn text_targets_report_section_attribute_loss_on_the_conversion_channel() {
    let doc = from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"table","rows":[],"rowGroups":{"headRows":0,"footRows":0,"headAttrs":{"id":"h"},"bodies":[]}}]}"#).unwrap();
    for target in [
        carve::RenderTarget::Plain,
        carve::RenderTarget::Markdown,
        carve::RenderTarget::Ansi,
        carve::RenderTarget::Carve,
    ] {
        let result =
            carve::with_render_loss_report(target, carve::CheckedRenderOptions::default(), || {
                match target {
                    carve::RenderTarget::Plain => {
                        carve::render_plain_text(&doc).map_err(|error| error.to_string())
                    }
                    carve::RenderTarget::Markdown => {
                        carve::render_markdown(&doc).map_err(|error| error.to_string())
                    }
                    carve::RenderTarget::Carve => {
                        carve::render_carve(&doc).map_err(|error| error.to_string())
                    }
                    _ => carve::render_ansi(&doc).map_err(|error| error.to_string()),
                }
            })
            .unwrap();
        assert_eq!(result.total_losses, 0, "{target:?}: {:?}", result.losses);
        assert!(result.totals_by_code.is_empty(), "{target:?}");
    }
    let report = carve::conversion_diagnostics(&doc, 100).unwrap();
    assert_eq!(report.total_diagnostics, 1);
    assert_eq!(
        report.diagnostics[0].code,
        carve::ConversionDiagnosticCode::FieldUnspellable
    );
    assert_eq!(report.diagnostics[0].node, "table");
    assert_eq!(
        report.diagnostics[0].field.as_deref(),
        Some("rowGroups.headAttrs")
    );
}

/// Every field path CARVE-P12-034 names reaches the section 1d channel, and an
/// empty attributes object holds nothing to lose and is reported nowhere.
#[test]
fn every_named_section_attribute_field_reaches_the_conversion_channel() {
    let doc = from_json(r#"{"type":"document","srcByteLength":0,"children":[{"type":"table","rows":[],"rowGroups":{"headRows":0,"footRows":0,"headAttrs":{"id":"head"},"footAttrs":{"classes":["foot"]},"bodies":[{"headRows":0,"bodyRows":0,"attrs":{"id":"one"}},{"headRows":0,"bodyRows":0,"attrs":{}}]}}]}"#).unwrap();
    let report = carve::conversion_diagnostics(&doc, 100).unwrap();
    let fields = report
        .diagnostics
        .iter()
        .filter_map(|entry| entry.field.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(
        fields,
        [
            "rowGroups.headAttrs",
            "rowGroups.footAttrs",
            "rowGroups.bodies[0].attrs"
        ]
    );
    assert_eq!(report.total_diagnostics, 3);
    assert_eq!(
        carve::to_conversion_diagnostics_json(&report),
        r#"{"diagnostics":[{"code":"field-unspellable","node":"table","field":"rowGroups.headAttrs","message":"Carve source cannot spell table section attributes"},{"code":"field-unspellable","node":"table","field":"rowGroups.footAttrs","message":"Carve source cannot spell table section attributes"},{"code":"field-unspellable","node":"table","field":"rowGroups.bodies[0].attrs","message":"Carve source cannot spell table section attributes"}],"totalDiagnostics":3,"truncated":false}"#
    );
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
