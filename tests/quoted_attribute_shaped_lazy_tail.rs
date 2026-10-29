use carve::to_html;

#[test]
fn an_indented_attribute_shaped_line_keeps_the_quote_paragraph_open() {
    let source = "> > :: t\n> : d\n>   {#x}\ntail";
    let expected = "<blockquote>\n  <blockquote>\n    <dl>\n      <dt>t</dt>\n    </dl>\n  </blockquote>\n  <p>: d</p>\n</blockquote>\n<p>tail</p>";
    assert_eq!(to_html(source), expected);
}
