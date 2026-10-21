#[test]
fn a_literal_plus_after_a_nested_fence_keeps_its_column_for_cross_readers() {
    for tail in ["tail", "second line", "[link](a)"] {
        let source = format!("- - [ ] ```js\n        x\n\n  +\n{tail}\n");
        let formatted = carve::to_carve(&source);
        assert!(formatted.contains("\n   +\n"), "{formatted:?}");
        assert_eq!(
            carve::to_html(&formatted),
            carve::to_html(&source),
            "{source}"
        );
        assert_eq!(carve::to_carve(&formatted), formatted, "{source}");
    }
}

#[test]
fn a_plus_without_a_preceding_nested_list_keeps_its_existing_spelling() {
    let source = "- plain\n  \\+\n  tail\n";
    let formatted = carve::to_carve(source);
    assert_eq!(carve::to_html(&formatted), carve::to_html(source));
    assert_eq!(carve::to_carve(&formatted), formatted);
}
