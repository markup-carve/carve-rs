#[test]
fn adjacent_caption_keeps_the_math_id() {
    let source = "$$`E=mc^2`{#eq-1}\n^ cap\n";
    let expected = "<figure>\n  <p><span class=\"math display\" id=\"eq-1\" role=\"math\">\\[E=mc^2\\]</span></p>\n  <figcaption>cap</figcaption>\n</figure>";
    assert_eq!(carve::to_html(source).trim(), expected);
}

#[test]
fn caption_across_one_blank_line_keeps_math_attributes() {
    let source = "$$`x+y`{.boxed data-k=v}\n\n^ Equation #: Sum\n";
    let expected = "<figure>\n  <p><span class=\"math display boxed\" data-k=\"v\" role=\"math\">\\[x+y\\]</span></p>\n  <figcaption>Equation 1: Sum</figcaption>\n</figure>";
    assert_eq!(carve::to_html(source).trim(), expected);
}
