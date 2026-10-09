#[test]
fn label_text_pairs_with_verbatim_reference_source() {
    let sources: Vec<String> =
        serde_json::from_str(include_str!("fixtures/raw-reference-label-brackets.json")).unwrap();
    for source in sources {
        let formatted = carve::to_carve(&source);
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(&source),
            "{source}"
        );
        assert_eq!(carve::to_carve(&formatted), formatted, "{source}");
    }
}
