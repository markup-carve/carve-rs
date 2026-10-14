#[test]
fn unicode_after_quote_attributes_remains_payload() {
    let controls: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/djot_unicode_quote_attributes.json")).unwrap();
    let mut failures = Vec::new();
    let spacing = regex::Regex::new(r">[ \t\r\n]+<").unwrap();
    for row in controls.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let expected = row["html"].as_str().unwrap();
        let imported = carve::djot_to_carve(source);
        let actual = carve::to_html(&imported).replace("&nbsp;", "\u{00a0}");
        if spacing.replace_all(&actual, "><").trim() != spacing.replace_all(expected, "><").trim() {
            failures.push(format!(
                "{source:?}\nImported: {imported:?}\nActual: {actual:?}\nExpected: {expected:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
