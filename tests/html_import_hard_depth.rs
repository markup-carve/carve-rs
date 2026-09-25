use carve::{html_to_ast, HtmlImportError, HtmlImportOptions};

#[test]
fn raising_html_import_depth_cannot_build_an_unbounded_tree() {
    let depth = 250;
    let html = format!(
        "{}x{}",
        "<blockquote>".repeat(depth),
        "</blockquote>".repeat(depth)
    );
    let options = HtmlImportOptions {
        max_depth: usize::MAX,
        ..HtmlImportOptions::default()
    };
    assert_eq!(
        html_to_ast(&html, &options).unwrap_err(),
        HtmlImportError::DepthLimit
    );
}
