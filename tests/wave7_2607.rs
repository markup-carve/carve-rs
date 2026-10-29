#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn agreed_rows_keep_their_html_and_canonical_source() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/wave7_2607.json")).unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        let written = carve::to_carve(source);
        assert_eq!(
            carve::to_html(source).trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
        assert_eq!(written, row["carve"].as_str().unwrap(), "{}", row["id"]);
        assert_eq!(carve::to_html(&written), carve::to_html(source));
        assert_eq!(carve::to_carve(&written), written);
    }
}
