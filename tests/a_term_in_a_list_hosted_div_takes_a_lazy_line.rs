//! A term in an unclosed colon div inside a list item takes a following plain
//! line like any open paragraph, while a `: ` separator stays out of it
//! (carve#4776, PART 0 B-term-keep).

use carve::to_html;

#[test]
fn a_plain_lazy_line_ends_the_item() {
    for source in [
        "- a\n  :::\n   :: b\na\n",
        "- a\n  ::: note\n  :: b\na\n",
        "- a\n  :::\n  # h\n  :: b\na\n",
    ] {
        let html = to_html(source);
        // A term is one line (carve#4861).
        assert!(html.contains("<dt>b</dt>"), "{source}\n{html}");
        assert!(html.ends_with("</ul>\n<p>a</p>"), "{source}\n{html}");
    }
}

#[test]
fn a_separator_after_the_lazy_line_stays_out_of_the_term() {
    let html = to_html("- a\n  :::\n   :: b\na\n: f\n");
    assert!(html.contains("<dt>b</dt>"), "{html}");
    assert!(html.contains("<p>a\n: f</p>"), "{html}");
    assert!(!html.contains("<dd>"), "{html}");
}
