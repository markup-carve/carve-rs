//! The two gaps from the implementation audit (carve#130): a `=>` arrow no
//! longer opens a `=` highlight span, and `::: |` line blocks are implemented.

#[test]
fn arrow_does_not_open_a_highlight() {
    assert_eq!(carve::to_html("a => b; x != y\n"), "<p>a ⇒ b; x ≠ y</p>");
    // a real highlight still works
    assert_eq!(carve::to_html("a =hi= b\n"), "<p>a <mark>hi</mark> b</p>");
}

#[test]
fn line_block_renders_as_a_verse_div() {
    let html = carve::to_html("::: |\nRoses are red,\n  Violets are blue.\n\nStanza two.\n:::\n");
    assert_eq!(
        html,
        "<div class=\"line-block\">\n  <p>Roses are red,<br>\n&nbsp;&nbsp;Violets are blue.</p>\n  <p>Stanza two.</p>\n</div>"
    );
}
