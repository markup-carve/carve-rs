#[test]
fn table_cell_break_reports_are_complete_and_bounded() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "fixtures/table-cell-break-conversion-reports.json"
    ))
    .unwrap();
    for case in cases {
        let doc = carve::from_json(&case["ast"].to_string()).unwrap();
        for maximum in [0, 1, 100] {
            let report = carve::conversion_diagnostics(&doc, maximum).unwrap();
            let expected = case["diagnostics"].as_array().unwrap();
            assert_eq!(report.total_diagnostics, expected.len(), "{}", case["name"]);
            assert_eq!(
                report.truncated,
                expected.len() > maximum,
                "{}",
                case["name"]
            );
            let rows: Vec<_> = report
                .diagnostics
                .iter()
                .map(|d| {
                    let mut row = serde_json::to_value(d).unwrap();
                    row.as_object_mut().unwrap().remove("message");
                    row.as_object_mut().unwrap().remove("pos");
                    row
                })
                .collect();
            assert_eq!(
                rows,
                expected.iter().take(maximum).cloned().collect::<Vec<_>>(),
                "{}",
                case["name"]
            );
            let source = carve::render_carve(&doc).unwrap();
            let before = carve::render_html(&doc).unwrap().matches("<br>").count();
            let after = carve::to_html(&source).matches("<br>").count();
            let lost = expected
                .iter()
                .filter(|d| d["node"] == "hard_break")
                .count();
            assert_eq!(before - after, lost, "{}", case["name"]);
        }
    }
}

#[test]
fn unrendered_short_caption_does_not_report_a_flattened_break() {
    let ast: serde_json::Value = serde_json::json!({"type": "document", "srcByteLength": 0, "children": [{"type": "table", "rows": [{"type": "table_row", "cells": [{"type": "table_cell", "header": false, "blocks": [{"type": "table", "rows": [{"type": "table_row", "cells": [{"type": "table_cell", "header": false, "children": [{"type": "text", "value": "a"}]}]}], "shortCaption": [{"type": "text", "value": "short"}, {"type": "hard_break"}, {"type": "text", "value": "caption"}]}]}]}]}]});
    let doc = carve::from_json(&ast.to_string()).unwrap();
    let report = carve::conversion_diagnostics(&doc, 100).unwrap();
    assert_eq!(report.total_diagnostics, 1);
    assert!(report.diagnostics.iter().all(|d| d.node != "hard_break"));
}
