use carve::{to_html, to_html_with_options, Options};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    source: String,
    html: String,
}

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn container_ownership_and_continuation_boundaries() {
    let fixtures: Vec<Fixture> =
        serde_json::from_str(include_str!("fixtures/container-boundaries.json")).unwrap();
    for fixture in fixtures {
        assert_eq!(
            to_html(&fixture.source).trim(),
            fixture.html,
            "{}",
            fixture.name
        );
        assert_eq!(
            to_html_with_options(&fixture.source, &Options::default().with_positions(true)).trim(),
            fixture.html,
            "{} with positions",
            fixture.name,
        );
    }
}
