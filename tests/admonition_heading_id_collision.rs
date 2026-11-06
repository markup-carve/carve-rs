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

#[test]
fn duplicate_footnote_heading_ids_follow_definition_order() {
    let source = "[^b]: # H\n\n[^a]: # H\n\n[^a] and [^b]\n";
    let html = carve::to_html(source);
    let a = html
        .find("<h1 id=\"H-2\">H</h1>")
        .expect("first referenced note");
    let b = html
        .find("<h1 id=\"H\">H</h1>")
        .expect("second referenced note");
    assert!(a < b, "{html}");
    let ast = carve::to_json_with_options(source, &carve::Options::default());
    assert!(ast.contains("\"label\":\"a\",\"children\":[{\"type\":\"heading\",\"level\":1,\"children\":[{\"type\":\"text\",\"value\":\"H\"}],\"attrs\":{\"id\":\"H-2\"}}]"), "{ast}");
}

#[test]
fn footnote_heading_ids_hold_across_source_and_reference_orders() {
    for variant in 0..1000 {
        let z = format!("z{variant}");
        let a = format!("a{variant}");
        let body_heading = if variant % 5 == 0 { "# H\n\n" } else { "" };
        let prefix = if variant % 3 == 0 { "> " } else { "" };
        let references = if variant % 2 == 0 {
            format!("[^{a}] and [^{z}]")
        } else {
            format!("[^{z}] and [^{a}]")
        };
        let source =
            format!("{body_heading}[^{z}]: {prefix}# H\n\n[^{a}]: {prefix}# H\n\n{references}\n");
        let first_id = if body_heading.is_empty() { "H" } else { "H-2" };
        let second_id = if body_heading.is_empty() {
            "H-2"
        } else {
            "H-3"
        };
        let html = carve::to_html(&source);
        let first = html
            .find(&format!("<h1 id=\"{first_id}\">H</h1>"))
            .unwrap_or_else(|| panic!("variant {variant}: {html}"));
        let second = html
            .find(&format!("<h1 id=\"{second_id}\">H</h1>"))
            .unwrap_or_else(|| panic!("variant {variant}: {html}"));
        if variant % 2 == 0 {
            assert!(second < first, "variant {variant}: {html}");
        } else {
            assert!(first < second, "variant {variant}: {html}");
        }
    }
}
