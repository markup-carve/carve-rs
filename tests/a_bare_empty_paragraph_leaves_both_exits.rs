//! A bare `<p></p>` carries nothing: the writer writes nothing for it, and the
//! tree exit drops it too, without a row (markup-carve/carve#2423).

use carve::{html_to_ast, html_to_carve};

#[test]
fn a_bare_empty_paragraph_leaves_both_exits() {
    let html = "<p>a</p><p></p><p>b</p>";
    let tree = html_to_ast(html, &Default::default()).expect("imports");
    assert_eq!(tree.value.children.len(), 2, "{:#?}", tree.value.children);
    assert!(
        tree.report.diagnostics.is_empty(),
        "{:?}",
        tree.report.diagnostics
    );
    let carve = html_to_carve(html, &Default::default()).expect("imports");
    assert_eq!(carve.value, "a\n\nb\n");
}
