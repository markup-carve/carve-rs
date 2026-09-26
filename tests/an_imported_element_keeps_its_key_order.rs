//! An imported element's key-value attributes are written in the order the
//! HTML spells them, which `fmt` keeps (carve-rs#2012).

use carve::{html_to_ast, html_to_carve, to_carve, HtmlImportOptions};

fn migrated(html: &str) -> String {
    html_to_carve(html, &HtmlImportOptions::default())
        .expect("import")
        .value
}

#[test]
fn keys_keep_document_order_after_the_id_and_classes() {
    let cases = [
        (
            "<blockquote data-z=\"1\" data-a=\"2\" class=\"c\" id=\"i\"><p>q</p></blockquote>",
            "{#i .c data-z=1 data-a=2}\n> q\n",
        ),
        (
            "<p>a <span data-z=\"1\" class=\"c\" data-a=\"2\">s</span></p>",
            "a [s]{.c data-z=1 data-a=2}\n",
        ),
        (
            "<table><tr><td data-z=\"1\" data-a=\"2\">x</td></tr></table>",
            "|{data-z=1 data-a=2} x |\n",
        ),
    ];
    for (html, expected) in cases {
        let out = migrated(html);
        assert_eq!(out, expected, "{html}");
        assert_eq!(to_carve(&out), out, "fmt fixed point: {html}");
    }
}

#[test]
fn the_published_tree_records_no_order() {
    let html = "<blockquote data-z=\"1\" data-a=\"2\"><p>q</p></blockquote>";
    let doc = html_to_ast(html, &HtmlImportOptions::default())
        .expect("import")
        .value;
    let carve::BlockNode::BlockQuote(quote) = &doc.children[0] else {
        panic!("a quote");
    };
    assert!(quote.attrs.as_ref().expect("attrs").order.is_empty());
}
