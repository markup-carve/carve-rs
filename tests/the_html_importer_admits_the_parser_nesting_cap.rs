//! The HTML importer's depth limit is the parser's nesting cap, counted in
//! element levels (ruling A on markup-carve/carve-js#4135).

use carve::{html_to_ast, html_to_carve, to_carve, to_html, HtmlImportError, HtmlImportOptions};

fn spans(n: usize, leaf: &str) -> String {
    format!(
        "<p>{}{leaf}{}</p>",
        "<span class=\"a\">".repeat(n),
        "</span>".repeat(n)
    )
}

#[test]
fn the_html_of_200_bracket_span_openers_imports_back_to_a_fixed_point() {
    let source = format!("{}x{}\n", "[".repeat(200), "]{.a}".repeat(200));
    let html = to_html(&source);
    assert_eq!(html.matches("<span class=\"a\">").count(), 199);
    let imported = html_to_carve(&html, &HtmlImportOptions::default())
        .unwrap()
        .value;
    assert_eq!(to_carve(&imported), imported);
    assert_eq!(to_html(&imported), html);
}

#[test]
fn one_element_level_past_the_cap_is_refused() {
    let options = HtmlImportOptions::default();
    assert!(html_to_ast(&spans(199, "x"), &options).is_ok());
    assert_eq!(
        html_to_ast(&spans(200, "x"), &options).map(|_| ()),
        Err(HtmlImportError::DepthLimit)
    );
}

#[test]
fn a_text_or_comment_leaf_is_no_level() {
    assert!(html_to_ast(&spans(199, "x<!--c-->y"), &HtmlImportOptions::default()).is_ok());
    let at = |max_depth| HtmlImportOptions {
        max_depth,
        ..HtmlImportOptions::default()
    };
    assert!(html_to_ast(&spans(2, "x"), &at(3)).is_ok());
    assert_eq!(
        html_to_ast(&spans(3, "x"), &at(3)).map(|_| ()),
        Err(HtmlImportError::DepthLimit)
    );
}
