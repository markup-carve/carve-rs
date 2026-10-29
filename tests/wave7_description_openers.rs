#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn visible_description_openers_close_an_unmatched_inline_code_run() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/wave7_description_openers.json")).unwrap();
    for row in rows.as_array().unwrap() {
        for positions in [false, true] {
            let doc = carve::parse_with_options(
                row["source"].as_str().unwrap(),
                &carve::Options::default().with_positions(positions),
            );
            assert_eq!(
                carve::render_html(&doc).unwrap().trim(),
                row["html"].as_str().unwrap(),
                "{} positions={positions}",
                row["id"]
            );
        }
    }
}
