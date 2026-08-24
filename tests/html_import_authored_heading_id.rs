use carve::{html_to_ast, html_to_carve, HtmlImportOptions};

#[test]
fn an_html_heading_id_equal_to_its_slug_stays_authored() {
    let html = r##"<h1 id="Target">Target</h1><p>See <a href="#Target">Target</a>.</p>"##;
    let options = HtmlImportOptions::default();
    let source = html_to_carve(html, &options).unwrap().value;
    let imported = html_to_ast(html, &options).unwrap().value;

    assert_eq!(source, "{#Target}\n# Target\n\nSee [Target](#Target).\n");
    let parsed = carve::parse(&source);
    assert_eq!(parsed.children, imported.children);
    assert_eq!(parsed.footnote_defs, imported.footnote_defs);
}
