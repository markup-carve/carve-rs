//! PART 9 §17 L1: a blank before an item's unclosed `%%%` span still stands
//! between the item and its sibling (carve#5266).

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_blank_before_an_unclosed_span_loosens_the_list() {
    let html = html("- ::: |\n  x\n  \n  %%%\n  hidden\n- b\n");
    assert!(html.contains("<li><p>b</p></li>"), "{html}");
}

#[test]
fn a_blank_before_an_unclosed_span_loosens_a_nested_list() {
    let html = html("- a\n  - ::: note\n    x\n    :::\n    \n    %%%\n    hidden\n  - b\n");
    assert!(html.contains("<li><p>b</p></li>"), "{html}");
}

#[test]
fn a_marker_line_span_blank_still_keeps_its_item_tight() {
    let html = html("- %%%\n  hidden\n\n  more\n  %%%\n- b\n");
    assert!(html.contains("<li>b</li>"), "{html}");
}

#[test]
fn a_nested_marker_line_span_closer_is_not_an_opener() {
    let html = html("- a\n  - %%%\n\n    %%%\n- c\n");
    assert!(html.contains("<li>c</li>"), "{html}");
}

#[test]
fn a_thousand_variants_follow_the_model() {
    for n in 0..1000 {
        let marker = ["- ", "* ", "1. "][n % 3];
        let sibling = ["- ", "* ", "2. "][n % 3];
        let pad = " ".repeat(marker.len());
        let source =
            format!("{marker}::: |\n{pad}x{n}\n{pad}\n{pad}%%%\n{pad}h{n}\n{sibling}b{n}\n");
        let html = html(&source);
        assert!(html.contains(&format!("<li><p>b{n}</p></li>")), "{html}");
    }
}
