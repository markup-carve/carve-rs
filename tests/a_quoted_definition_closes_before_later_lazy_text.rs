//! A quote containing or following a definition closes before a later item
//! paragraph, which still takes a lazy colon line (PART 0 S4, carve#5365).

use carve::to_html;

#[test]
fn a_quote_after_a_term_leaves_the_later_item_paragraph_open() {
    let source = "- a\n  :: t\n  > q\n\n  w\n: z\n";
    let html = to_html(source);
    assert!(html.contains("<p>w\n: z</p>"), "{html}");
    assert!(!html.contains("</ul>\n<p>: z</p>"), "{html}");
}

#[test]
fn a_quote_containing_a_description_leaves_the_later_item_paragraph_open() {
    let source = "- a\n  > +\n  :: t\n    : d\n\n    q\n: d\n";
    let html = to_html(source);
    assert!(html.contains("<p>q\n: d</p>"), "{html}");
    assert!(!html.contains("</ul>\n<p>: d</p>"), "{html}");
}

#[test]
fn a_quote_before_a_term_leaves_the_later_item_paragraph_open() {
    let source = "- a\n  > q\n  :: t\n\n  w\n: z\n";
    let html = to_html(source);
    assert!(html.contains("<p>w\n: z</p>"), "{html}");
}

#[test]
fn a_quote_marker_in_code_does_not_open_a_quote() {
    let source = "- a\n  ```\n  > literal\n  ```\n  :: m\n\n  para\n: l\n";
    let html = to_html(source);
    assert!(html.contains("<p>para\n: l</p>"), "{html}");
}
