#[test]
fn an_indented_unmarked_term_folds_into_the_open_quoted_term() {
    crate::common::assert_rule_a_pins("wave7_quoted_term::an_indented_unmarked_term_folds_into_the_open_quoted_term");
}

#[test]
#[ignore = "rule A ledger: carve-rs#3525"]
fn quoted_term_rows_match_the_checker() {
    let rows: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/wave7_quoted_term.json")).unwrap();
    for row in rows.as_array().unwrap() {
        assert_eq!(
            carve::to_html(row["source"].as_str().unwrap()).trim(),
            row["html"].as_str().unwrap()
        );
    }
}
