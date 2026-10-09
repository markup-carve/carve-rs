use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    html: String,
}

#[test]
fn attribute_values_survive_import() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/djot-attribute-wire.json")).unwrap();
    for row in cases {
        assert_eq!(
            carve::to_html(&carve::djot_to_carve(&row.source)).trim(),
            row.html,
            "{}",
            row.name
        );
    }
}
