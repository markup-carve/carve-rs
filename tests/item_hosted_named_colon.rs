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
fn named_children_keep_paragraphs_and_opaque_payloads() {
    crate::common::assert_rule_a_pins(
        "item_hosted_named_colon::named_children_keep_paragraphs_and_opaque_payloads",
    );
}

#[test]
fn a_body_rebase_does_not_take_a_descendant_items_blocks_or_payload() {
    crate::common::assert_rule_a_pins(
        "item_hosted_named_colon::a_body_rebase_does_not_take_a_descendant_items_blocks_or_payload",
    );
}
