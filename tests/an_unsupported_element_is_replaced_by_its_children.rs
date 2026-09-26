//! markup-carve/carve#2341. Importing `<x>C</x>` for an unsupported element `x`
//! gives the document importing `C` gives in the same position, plus one
//! `element-unwrapped` row for `x`. Block children stay blocks.

use carve::{
    html_to_ast, html_to_carve, HtmlImportDiagnosticCode, HtmlImportOptions, HtmlImportSeverity,
};

const SHAPES: [(&str, &str, &[&str]); 6] = [
    (
        "<react-app><div><h1>Title</h1><p>Body</p><ul><li>one</li></ul></div></react-app>",
        "<div><h1>Title</h1><p>Body</p><ul><li>one</li></ul></div>",
        &["/react-app[1]"],
    ),
    (
        "<p>a <tool-tip>b <em>c</em></tool-tip> d</p>",
        "<p>a b <em>c</em> d</p>",
        &["/p[1]/tool-tip[2]"],
    ),
    (
        "<x-a>loose text<p>para</p>more</x-a>",
        "loose text<p>para</p>more",
        &["/x-a[1]"],
    ),
    (
        "<foo><h2>H</h2><pre><code>x = 1</code></pre></foo>",
        "<h2>H</h2><pre><code>x = 1</code></pre>",
        &["/foo[1]"],
    ),
    (
        "<p>before</p><x-a><x-b><blockquote><p>q</p></blockquote></x-b></x-a><p>after</p>",
        "<p>before</p><blockquote><p>q</p></blockquote><p>after</p>",
        &["/x-a[2]", "/x-a[2]/x-b[1]"],
    ),
    (
        "<ul><li><x-a><p>one</p><p>two</p></x-a></li></ul>",
        "<ul><li><p>one</p><p>two</p></li></ul>",
        &["/ul[1]/li[1]/x-a[1]"],
    ),
];

#[test]
fn the_wrapped_document_equals_the_bare_one() {
    let opts = HtmlImportOptions::default();
    for (wrapped, bare, _) in SHAPES {
        let w = html_to_carve(wrapped, &opts).expect("import");
        let b = html_to_carve(bare, &opts).expect("import");
        assert_eq!(w.value, b.value, "Carve source for {wrapped}");
        let w = html_to_ast(wrapped, &opts).expect("import");
        let b = html_to_ast(bare, &opts).expect("import");
        assert_eq!(w.value, b.value, "AST for {wrapped}");
    }
}

#[test]
fn only_the_wrapper_reports_an_unwrap() {
    let opts = HtmlImportOptions::default();
    for (wrapped, bare, paths) in SHAPES {
        assert!(
            html_to_carve(bare, &opts)
                .expect("import")
                .report
                .diagnostics
                .is_empty(),
            "bare {bare}"
        );
        let rows = html_to_carve(wrapped, &opts)
            .expect("import")
            .report
            .diagnostics;
        assert!(
            rows.iter()
                .all(|d| d.code == HtmlImportDiagnosticCode::ElementUnwrapped
                    && d.severity == HtmlImportSeverity::Info),
            "{wrapped}: {rows:?}"
        );
        let got: Vec<&str> = rows.iter().filter_map(|d| d.path.as_deref()).collect();
        assert_eq!(got, paths, "{wrapped}");
    }
}

#[test]
fn an_empty_unsupported_element_is_still_dropped() {
    let rows = html_to_carve("<p>a</p><x-a></x-a>", &HtmlImportOptions::default())
        .expect("import")
        .report
        .diagnostics;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].code, HtmlImportDiagnosticCode::ElementDropped);
}
