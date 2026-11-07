//! A child block after the blank that ends an attached fence leaves no
//! paragraph for a flush-left lazy line (PART 0 S4, carve#5358).

use carve::to_html;

#[test]
fn a_heading_after_the_fence_sends_the_tail_to_the_document() {
    let source = "- a\n  - +\n  ```\n\n    # h\ntail\n";
    let html = to_html(source);
    assert!(html.contains("<h1 id=\"h\">h</h1>"), "{html}");
    assert!(html.ends_with("</ul>\n<p>tail</p>"), "{html}");
}

#[test]
fn a_paragraph_after_the_fence_keeps_its_lazy_tail() {
    let source = "- a\n  - +\n  ```\n\n    p\ntail\n";
    let html = to_html(source);
    assert!(html.contains("<p>p\ntail</p>"), "{html}");
    assert!(!html.ends_with("<p>tail</p>"), "{html}");
}

#[test]
fn a_quote_paragraph_after_the_fence_keeps_its_lazy_tail() {
    let source = "- a\n  - +\n  ```\n\n    > p\ntail\n";
    let html = to_html(source);
    assert!(
        html.contains("<blockquote><p>p\ntail</p></blockquote>"),
        "{html}"
    );
    assert!(!html.ends_with("<p>tail</p>"), "{html}");
}
