use carve::lint_carve;

#[test]
fn lint_uses_the_attached_list_block() {
    for source in ["- +\nx\n", "> - foo\n>   - +\n>   > q\n"] {
        let warnings = lint_carve(source);
        assert!(
            warnings
                .iter()
                .all(|warning| warning.rule != "continuation-marker-refused"),
            "{source:?}: {warnings:?}"
        );
    }
}

#[test]
fn a_first_block_marker_without_an_attached_block_still_warns() {
    let warnings = lint_carve("- +\n- sibling\n");
    assert!(
        warnings
            .iter()
            .any(|warning| warning.rule == "continuation-marker-refused"),
        "{warnings:?}"
    );
}
