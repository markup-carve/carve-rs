//! Three blank lines end a description list (markup-carve/carve#4759), so two
//! adjacent `<dl>` elements stay two lists. This replaces the carve#2369 join.

use carve::{html_to_ast, html_to_carve, HtmlImportOptions};

fn import(html: &str) -> (String, Vec<(String, String)>) {
    let result = html_to_carve(html, &HtmlImportOptions::default()).unwrap();
    let rows = result
        .report
        .diagnostics
        .iter()
        .map(|d| {
            (
                d.code.as_str().to_string(),
                d.path.clone().unwrap_or_default(),
            )
        })
        .collect();
    (result.value, rows)
}

#[test]
fn adjacent_lists_stay_apart() {
    for (html, carve, lists) in [
        (
            "<dl><dt>a</dt><dd>x</dd></dl><dl><dt>b</dt><dd>y</dd></dl>",
            ":: a\n: x\n\n\n\n:: b\n: y\n",
            2,
        ),
        (
            "<dl><dt>a</dt><dd>x</dd></dl>\n<dl><dt>b</dt><dd>y</dd></dl>\n<dl><dt>c</dt><dd>z</dd></dl>",
            ":: a\n: x\n\n\n\n:: b\n: y\n\n\n\n:: c\n: z\n",
            3,
        ),
    ] {
        let (value, rows) = import(html);
        assert_eq!(value, carve);
        assert!(rows.is_empty());
        let tree = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(tree.value.children.len(), lists);
        assert_eq!(carve::parse(&value).children.len(), lists);
    }
}

#[test]
fn an_attributed_list_or_a_paragraph_separates_them_too() {
    for (html, carve) in [
        (
            r#"<dl><dt>a</dt><dd>x</dd></dl><dl class="k"><dt>b</dt><dd>y</dd></dl>"#,
            ":: a\n: x\n\n\n\n{.k}\n:: b\n: y\n",
        ),
        (
            "<dl><dt>a</dt><dd>x</dd></dl><p>p</p><dl><dt>b</dt><dd>y</dd></dl>",
            ":: a\n: x\n\np\n\n:: b\n: y\n",
        ),
    ] {
        let (value, rows) = import(html);
        assert_eq!(value, carve);
        assert!(rows.is_empty());
    }
}
