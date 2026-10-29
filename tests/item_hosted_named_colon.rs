#[test]
fn named_child_opens_at_its_authored_column() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/item_hosted_named_colon.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap(),
            "{}",
            row["id"]
        );
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn named_children_keep_paragraphs_and_opaque_payloads() {
    let rows: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/item_hosted_named_colon_controls.json"
    ))
    .unwrap();
    for row in rows.as_array().unwrap() {
        for positions in [false, true] {
            let actual = carve::to_html_with_options(
                row["source"].as_str().unwrap(),
                &carve::Options::default().with_positions(positions),
            );
            assert_eq!(
                actual.trim(),
                row["html"].as_str().unwrap().trim(),
                "{}",
                row["id"]
            );
        }
    }
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn a_body_rebase_does_not_take_a_descendant_items_blocks_or_payload() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/named_review_controls.json")).unwrap();
    for row in rows.as_array().unwrap() {
        for positions in [false, true] {
            let html = carve::to_html_with_options(
                row["source"].as_str().unwrap(),
                &carve::Options::default().with_positions(positions),
            );
            assert_eq!(
                html.trim(),
                row["html"].as_str().unwrap().trim(),
                "{}",
                row["id"]
            );
        }
    }
}
