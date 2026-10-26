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

#[test]
fn defined_empty_destinations_still_inline() {
    let written = carve::try_markdown_to_carve("[t][r]\n\n[r]: <> \"T\"\n").unwrap();
    assert_eq!(written.trim_end(), "[t]( \"T\")");
}

#[test]
fn escaped_pipe_is_not_an_unresolved_footnote() {
    let written = carve::try_markdown_to_carve("| [^a\\|b] |\n|---|\n\n[^a\\|b]: Note.\n").unwrap();
    let html = carve::to_html(&written);
    assert!(!html.contains("role=\"doc-noteref\""), "{html}");
}
