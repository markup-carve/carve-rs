use serde_json::Value;

#[test]
fn root_code_container_openers_report_losses() {
    let controls: Value = serde_json::from_str(include_str!(
        "fixtures/djot_root_code_container_losses.json"
    ))
    .unwrap();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let report = carve::migrate_djot(source);
        assert_eq!(report.value, carve::djot_to_carve(source), "{source}");
        let paths: Vec<_> = report
            .report
            .diagnostics
            .iter()
            .filter(|item| {
                item.message
                    .contains("multiline code span contains a Carve container opener")
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
