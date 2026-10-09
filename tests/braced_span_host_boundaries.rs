use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    source: String,
    html: String,
    round_trip: bool,
}

#[test]
fn braced_span_host_boundaries() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("fixtures/braced-span-host-boundaries.json")).unwrap();
    for row in cases {
        assert_eq!(carve::to_html(&row.source).trim(), row.html, "{}", row.name);
        if row.round_trip {
            let formatted = carve::to_carve(&row.source);
            assert_eq!(
                carve::to_html(&formatted).trim(),
                row.html,
                "round trip: {}",
                row.name
            );
        }
    }
}
