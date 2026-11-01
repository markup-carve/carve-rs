use carve::to_html;

#[test]
fn a_first_block_quote_with_an_open_div_keeps_the_item_column() {
    let html = to_html("- +\n> :::\n  x\n");
    assert!(html.contains("</blockquote>\n    x\n  </li>"), "{html}");
    assert!(!html.contains("</ul>\n<p>x</p>"), "{html}");
}

#[test]
fn a_first_block_quote_keeps_the_description_column() {
    let html = to_html(":: t\n: +\n> :::\n  x\n");
    assert!(html.contains("</blockquote>\n    <p>x</p>"), "{html}");
}

#[test]
fn an_own_line_attachment_ends_before_the_following_item_column() {
    let html = to_html("- a\n+\n> :::\n  x\n");
    assert!(html.contains("</ul>\n<p>x</p>"), "{html}");
}
