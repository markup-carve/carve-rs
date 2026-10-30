//! Rows the spec model at carve 3679436f answers for the `+` rulings: carve#4409,
//! #4438, #4467, #4300, and the block a comment or a definition leads.

#[test]
fn every_row_renders_as_the_spec_model() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/plus_rulings_at_3679436f.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        assert_eq!(
            carve::to_html(source).trim(),
            row["html"].as_str().unwrap(),
            "{source:?}"
        );
    }
}
