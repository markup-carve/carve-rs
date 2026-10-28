use carve::to_html;

#[test]
fn second_plus_is_text_inside_first_block_host() {
    assert_eq!(to_html("- +\n+\n"), "<ul>\n  <li>+</li>\n</ul>");
    assert_eq!(to_html("> +\n+\n"), "<blockquote><p>+</p></blockquote>");
    assert_eq!(to_html("- +\n+\ntail\n"), "<ul>\n  <li>tail</li>\n</ul>");
    assert_eq!(
        to_html("> +\n+\ntail\n"),
        "<blockquote><p>tail</p></blockquote>"
    );
}
