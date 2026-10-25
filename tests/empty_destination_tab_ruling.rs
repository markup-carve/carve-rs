#[test]
fn empty_destinations_accept_spaces_but_keep_tabs_as_text() {
    assert_eq!(carve::to_html("[t](  )"), "<p><a href=\"\">t</a></p>");
    assert_eq!(carve::to_html("![a](  )"), "<img src=\"\" alt=\"a\">");
    for source in ["[t](\t)", "![a](\t)"] {
        assert_eq!(carve::to_html(source), format!("<p>{source}</p>"));
        assert_eq!(carve::to_carve(source).trim(), source);
    }
}
