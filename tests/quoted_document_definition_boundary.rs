use carve::{parse_with_options, render_html, to_carve_with_options, Options};

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn quoted_document_definitions_end_the_item_claim() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/quoted_document_definition_boundary.json"
    ))
    .unwrap();
    for row in cases.as_array().unwrap() {
        for positions in [false, true] {
            let doc = parse_with_options(
                row["source"].as_str().unwrap(),
                &Options::default().with_positions(positions),
            );
            assert_eq!(
                render_html(&doc).unwrap().trim(),
                row["html"].as_str().unwrap().trim(),
                "{} positions={positions}",
                row["id"]
            );
            let source = to_carve_with_options(
                row["source"].as_str().unwrap(),
                &Options::default().with_positions(positions),
            );
            assert_eq!(
                render_html(&parse_with_options(&source, &Options::default()))
                    .unwrap()
                    .trim(),
                row["html"].as_str().unwrap().trim(),
                "{} formatted: {source}",
                row["id"]
            );
        }
    }
}
