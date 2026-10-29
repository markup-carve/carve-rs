use serde_json::Value;

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn literal_payload_spans_pair_across_columns() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/literal_payload_columns.json")).unwrap();
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for row in rows.as_array().unwrap() {
        let imported = row["source"].as_str().unwrap();
        for html in [
            carve::to_html(imported),
            carve::render_html(&carve::parse(imported)).unwrap(),
        ] {
            assert_eq!(
                whitespace.replace_all(&html, "><").trim(),
                row["html"].as_str().unwrap(),
                "{}: {imported}",
                row["id"]
            );
        }
    }
}
