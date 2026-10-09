use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    name: String,
    source: String,
    html: String,
}

#[test]
fn failed_braced_openers_remain_literal() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/djot-failed-braced-openers.json")).unwrap();
    for row in cases {
        assert_eq!(
            carve::to_html(&carve::djot_to_carve(&row.source)).trim(),
            row.html,
            "{}",
            row.name
        );
    }
}
