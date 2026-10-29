use serde_json::Value;

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn a_description_keeps_the_container_paragraph_after_a_band_marker() {
    let rows: Value =
        serde_json::from_str(include_str!("fixtures/description_band_wave.json")).unwrap();
    let whitespace = regex::Regex::new(r">\s+<").unwrap();
    for row in rows.as_array().unwrap() {
        let source = row["source"].as_str().unwrap();
        for html in [
            carve::to_html(source),
            carve::render_html(&carve::parse(source)).unwrap(),
        ] {
            assert_eq!(
                whitespace.replace_all(&html, "><").trim(),
                row["html"].as_str().unwrap(),
                "{}: {source}",
                row["id"]
            );
        }
    }
}
