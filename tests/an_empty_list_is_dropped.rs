//! markup-carve/carve#2367: a list with no item has no spelling, so it is
//! dropped with one row that covers its attributes too.

use carve::{html_to_ast, html_to_carve, HtmlImportOptions};

#[test]
fn an_empty_list_is_dropped_with_one_row() {
    for (html, carve, path, lists) in [
        (
            r#"<div class="m"><ul class="vector-menu-content-list"></ul></div><p>after</p>"#,
            "::: m\n\n:::\n\nafter\n",
            "/div[1]/ul[1]",
            0,
        ),
        ("<ul></ul><p>a</p>", "a\n", "/ul[1]", 0),
        (r#"<ol id="o" start="3"></ol><p>a</p>"#, "a\n", "/ol[1]", 0),
        (
            r#"<ul><li>a<ul class="n"></ul></li></ul>"#,
            "- a\n",
            "/ul[1]/li[1]/ul[2]",
            1,
        ),
    ] {
        let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(result.value, carve, "{html}");
        let rows: Vec<_> = result
            .report
            .diagnostics
            .iter()
            .map(|d| (d.code.as_str(), d.path.clone().unwrap_or_default()))
            .collect();
        assert_eq!(rows, [("element-dropped", path.to_string())], "{html}");
        let tree = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
        let json = carve::ast_json::to_json(&tree.value);
        assert_eq!(json.matches(r#""type":"list""#).count(), lists, "{html}");
    }
}
