use carve::{djot_to_carve, html_to_carve, markdown_to_carve, to_html, HtmlImportOptions};

#[test]
fn invisible_content_does_not_prevent_image_promotion() {
    for source in [
        "{%%}![alt](x.png)",
        "%% hidden\n![alt](x.png)",
        "{%%}![alt][r]\n\n[r]: x.png",
    ] {
        assert_eq!(
            to_html(source).trim(),
            "<img src=\"x.png\" alt=\"alt\">",
            "{source:?}"
        );
    }
}

#[test]
fn importers_write_plain_images() {
    assert_eq!(djot_to_carve("![alt](x.png)"), "![alt](x.png)");
    assert_eq!(markdown_to_carve("![alt](x.png)"), "![alt](x.png)\n");
    let result = html_to_carve(
        "<p><img src=\"x.png\" alt=\"alt\"></p>",
        &HtmlImportOptions::default(),
    )
    .unwrap();
    assert_eq!(result.value, "![alt](x.png)\n");
    assert!(result.report.diagnostics.is_empty());
}
