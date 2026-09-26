//! markup-carve/carve#2369: Carve source has no boundary between two
//! definition lists, so an attribute-less one joins the list before it.

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
fn adjacent_lists_merge() {
    for (html, carve, paths) in [
        (
            "<dl><dt>a</dt><dd>x</dd></dl><dl><dt>b</dt><dd>y</dd></dl>",
            ":: a\n: x\n:: b\n: y\n",
            vec!["/dl[2]"],
        ),
        (
            "<dl><dt>a</dt><dd>x</dd></dl>\n<dl><dt>b</dt><dd>y</dd></dl>\n<dl><dt>c</dt><dd>z</dd></dl>",
            ":: a\n: x\n:: b\n: y\n:: c\n: z\n",
            vec!["/dl[3]", "/dl[5]"],
        ),
    ] {
        let (value, rows) = import(html);
        assert_eq!(value, carve);
        let expected: Vec<_> = paths
            .iter()
            .map(|p| ("element-unwrapped".to_string(), p.to_string()))
            .collect();
        assert_eq!(rows, expected);
        let tree = html_to_ast(html, &HtmlImportOptions::default()).unwrap();
        assert_eq!(tree.value.children.len(), 1);
    }
}

#[test]
fn an_attributed_list_or_a_paragraph_keeps_them_apart() {
    for (html, carve) in [
        (
            r#"<dl><dt>a</dt><dd>x</dd></dl><dl class="k"><dt>b</dt><dd>y</dd></dl>"#,
            ":: a\n: x\n\n{.k}\n:: b\n: y\n",
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
