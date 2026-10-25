#[test]
fn merged_c1_corpus_row() {
    let source = include_str!("spec/tests/corpus/663-a-titled-admonition-skips-an-authored-heading-id.crv");
    let expected = include_str!("spec/tests/corpus/663-a-titled-admonition-skips-an-authored-heading-id.html");
    assert_eq!(carve::to_html(source).trim(), expected.trim());
}

#[test]
fn automatic_heading_after_admonition() {
    let source = "::: warning \"Careful\"\nbody\n:::\n\n# adm-1\n";
    let html = carve::to_html(source);
    assert!(html.contains("<section id=\"adm-1\">"), "{html}");
    assert!(html.contains("aria-labelledby=\"adm-1-2\""), "{html}");
    assert!(html.contains("<p class=\"admonition-title\" id=\"adm-1-2\">"), "{html}");
}
