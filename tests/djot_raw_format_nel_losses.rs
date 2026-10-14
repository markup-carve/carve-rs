use serde_json::Value;

#[test]
fn raw_formats_containing_nel_report_losses_outside_image_alternatives() {
    let controls: Value =
        serde_json::from_str(include_str!("fixtures/djot_raw_format_nel_losses.json")).unwrap();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let report = carve::migrate_djot(source);
        let paths: Vec<_> = report
            .report
            .diagnostics
            .iter()
            .filter(|item| {
                item.message
                    .contains("raw-format identifier containing U+0085")
            })
            .map(|item| item.path.clone().unwrap())
            .collect();
        let expected: Vec<_> = row["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| format!("line:{}", line.as_u64().unwrap()))
            .collect();
        assert_eq!(paths, expected, "{source}");
    }
}
