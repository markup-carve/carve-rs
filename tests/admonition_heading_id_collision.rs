#[test]
fn merged_c1_corpus_row() {
    let source = "{#adm-1}\n# H\n\n::: warning \"Careful\"\nbody\n:::\n";
    let expected = "<section id=\"adm-1\">\n  <h1>H</h1>\n  <aside class=\"admonition warning\" aria-labelledby=\"adm-1-2\">\n    <p class=\"admonition-title\" id=\"adm-1-2\">Careful</p>\n    <p>body</p>\n  </aside>\n</section>";
    assert_eq!(carve::to_html(source).trim(), expected.trim());
}

#[test]
fn automatic_heading_after_admonition() {
    let source = "::: warning \"Careful\"\nbody\n:::\n\n# adm-1\n";
    let html = carve::to_html(source);
    assert!(html.contains("<section id=\"adm-1\">"), "{html}");
    assert!(html.contains("aria-labelledby=\"adm-1-2\""), "{html}");
    assert!(
        html.contains("<p class=\"admonition-title\" id=\"adm-1-2\">"),
        "{html}"
    );
}
