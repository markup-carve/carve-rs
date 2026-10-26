#[test]
fn unresolved_references_keep_their_source() {
    for source in [
        "# [x][r]",
        "# [x][]",
        "- [ ] [x][r]",
        "- [ ] [^n]",
        "[x][r] and [^n]",
        "a [^n] b",
        r"\[^n]",
    ] {
        let written = carve::try_markdown_to_carve(source).expect(source);
        assert_eq!(written.trim_end(), source, "{source:?}");
    }
}

#[test]
fn an_unresolved_image_is_still_literal() {
    let written = carve::try_markdown_to_carve("![alt][missing]").unwrap();
    assert!(written.contains("[alt][missing]"), "{written:?}");
}
