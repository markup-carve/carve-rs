use serde_json::Value;

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn floor_text_keeps_definition_terms_open() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/definition_term_floor_text.json")).unwrap();
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let expected = row["html"].as_str().unwrap();
        for actual in [
            carve::to_html(source),
            carve::to_html_with_options(source, &carve::Options::default().with_positions(true)),
            carve::render_html(&carve::parse(source)).unwrap(),
        ] {
            assert_eq!(
                whitespace.replace_all(&actual, "><").trim(),
                expected,
                "{}: {source}",
                row["id"]
            );
        }
    }
}
