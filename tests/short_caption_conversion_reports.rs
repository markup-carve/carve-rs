#[test]
fn short_caption_reports_cover_nested_fields_with_stable_bounds() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "fixtures/short-caption-conversion-reports.json"
    ))
    .unwrap();
    for case in cases {
        let doc = carve::from_json(&case["ast"].to_string()).unwrap();
        let source = carve::render_carve(&doc).unwrap();
        let expected = case["diagnostics"].as_array().unwrap();
        for maximum in [0, 1, 2, 100] {
            let report = carve::conversion_diagnostics(&doc, maximum).unwrap();
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
            assert_eq!(
                carve::render_carve(&doc).unwrap(),
                source,
                "{}",
                case["name"]
            );
        }
    }
}
