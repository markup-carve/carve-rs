#[test]
fn first_block_paragraph_yields_to_a_heading_at_the_description_column() {
    let source = ":: t\n: +\nh\n  # H\n";
    let expected =
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <p>h</p>\n    <h1 id=\"H\">H</h1>\n  </dd>\n</dl>";
    assert_eq!(carve::to_html(source), expected);
}
