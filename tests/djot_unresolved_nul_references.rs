use serde_json::Value;

#[test]
fn unresolved_nul_references_do_not_select_replacement_character_destinations() {
    let controls: Value =
        serde_json::from_str(include_str!("fixtures/djot_unresolved_nul_references.json")).unwrap();
    let pattern = regex::Regex::new(r#"\b(?:href|src)="([^"]*)""#).unwrap();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let native = row["html"].as_str().unwrap();
        if let Some(empty) = row.get("emptyHeading").and_then(Value::as_bool) {
            let report = carve::migrate_djot(source);
            assert_eq!(
                report
                    .report
                    .diagnostics
                    .iter()
                    .any(|item| item.message.contains("empty Djot heading")),
                empty,
                "{source}"
            );
            continue;
        }
        if row.get("noAlias").and_then(Value::as_bool) == Some(true) {
            let html = carve::to_html(&carve::migrate_djot(source).value);
            assert!(!html.contains("carve-djot-reference-"), "{source}: {html}");
            if row.get("linked").is_none() {
                continue;
            }
        }
        if row.get("heading").is_some() || row.get("linked").is_some() {
            let report = carve::migrate_djot(source);
            let html = carve::to_html(&report.value);
            let destinations = |value: &str| {
                pattern
                    .captures_iter(value)
                    .map(|caps| caps[1].to_string())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                destinations(&html),
                destinations(&native.replace('\0', "�")),
                "{source}"
            );
            continue;
        }
        assert!(
            !native.contains("href=") && !native.contains("src="),
            "{source}"
        );
        let report = carve::migrate_djot(source);
        let html = carve::to_html(&report.value);
        assert!(
            !html.contains("href=") && !html.contains("src="),
            "{source}: {html}"
        );
        if let Some(depth) = row.get("quoteOwn").and_then(Value::as_u64) {
            assert_eq!(
                html.matches("<blockquote>").count(),
                native.matches("<blockquote>").count(),
                "{source}"
            );
            if depth > 0 {
                assert!(!html.contains("&gt;"), "{source}: {html}");
            }
        }
        if row.get("own").is_some() || row.get("keyLiteral").is_some() {
            assert!(
                !["<span", "<em", "<strong", "<code"]
                    .iter()
                    .any(|tag| html.contains(tag)),
                "{source}: {html}"
            );
        }
        assert!(
            report
                .report
                .diagnostics
                .iter()
                .any(|item| item.message.contains("unresolved Djot")),
            "{source}: {:?}",
            report.report.diagnostics
        );
    }
}
