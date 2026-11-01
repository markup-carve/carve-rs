#[test]
fn plus_attaches_the_block_led_by_an_attribute_at_its_column() {
    let source = "- a\n+\n{.x}\n> q\n";
    let expected =
        "<ul>\n  <li>a\n    <blockquote class=\"x\"><p>q</p></blockquote>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn plus_refuses_an_attributed_block_right_of_its_column() {
    let source = "- a\n+\n{.x}\n > q\n";
    let expected = "<ul>\n  <li>a\n+</li>\n</ul>\n<blockquote class=\"x\"><p>q</p></blockquote>";
    assert_eq!(carve::to_html(source), expected);
}

#[test]
fn plus_refuses_an_attributed_sibling_item() {
    let source = "- a\n+\n{.x}\n- b\n";
    let expected = "<ul>\n  <li>a\n+</li>\n</ul>\n<ul class=\"x\">\n  <li>b</li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
