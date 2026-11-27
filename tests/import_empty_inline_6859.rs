//! carve#6859: an inline node that needs content and has none is dropped on
//! import (C2-M-069), reported only when the drop loses an attribute.

use carve::api::{self, ImportOptions};

fn import(html: &str) -> (String, Vec<String>) {
    let got = api::import_document(html, &ImportOptions::default()).expect("import");
    let codes = got.diagnostics.iter().map(|d| d.code.clone()).collect();
    (got.source, codes)
}

const NEEDS_CONTENT: &[&str] = &[
    "code", "em", "i", "strong", "b", "sup", "sub", "ins", "del", "s", "mark", "u", "span",
];

#[test]
fn ticket_shapes() {
    let (src, codes) = import("<p>a<code></code>b</p>");
    assert_eq!(src, "ab\n");
    assert!(codes.is_empty(), "{codes:?}");

    let (src, codes) = import("<p>a<code class=\"x\"></code>b</p>");
    assert_eq!(src, "ab\n");
    assert_eq!(codes, ["import-structure-repaired"]);

    let (src, codes) = import(concat!(
        "<p><span class=\"comment-start\" data-comment-id=\"1\">n</span><code></code>",
        "<span class=\"comment-end\" data-comment-id=\"1\"></span>y</p>",
    ));
    assert_eq!(src, "{#n#}y\n");
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn every_empty_inline_is_dropped_silently() {
    for tag in NEEDS_CONTENT.iter().copied().chain(["a href=\"/u\""]) {
        let name = tag.split(' ').next().unwrap();
        let html = format!("<p>a<{tag}></{name}>b</p>");
        let (src, codes) = import(&html);
        assert_eq!(src, "ab\n", "{html}");
        assert!(codes.is_empty(), "{html}: {codes:?}");
    }
}

#[test]
fn every_empty_inline_with_a_class_is_reported() {
    for tag in NEEDS_CONTENT.iter().filter(|t| **t != "span") {
        let html = format!("<p>a<{tag} class=\"x\"></{tag}>b</p>");
        let (src, codes) = import(&html);
        assert_eq!(src, "ab\n", "{html}");
        assert_eq!(codes, ["import-structure-repaired"], "{html}");
    }
}

#[test]
fn an_empty_span_with_attributes_is_kept() {
    let (src, codes) = import("<p>a<span class=\"x\"></span>b</p>");
    assert_eq!(src, "a[]{.x}b\n");
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn empty_inline_inside_a_range_leaves_it_whole() {
    for tag in NEEDS_CONTENT.iter().filter(|t| **t != "span") {
        let html = format!(
            "<p><span class=\"comment-start\" data-comment-id=\"1\">n</span><{tag}></{tag}>\
             <span class=\"comment-end\" data-comment-id=\"1\"></span>y</p>"
        );
        let (src, codes) = import(&html);
        assert_eq!(src, "{#n#}y\n", "{html}");
        assert!(codes.is_empty(), "{html}: {codes:?}");
    }
}
