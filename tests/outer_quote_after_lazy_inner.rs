#[test]
fn outer_marked_quote_closes_an_inner_quote_after_lazy_text() {
    for source in ["- - > a\nx\n  > b", "- - > a\nx\n   > b"] {
        let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <blockquote><p>a\nx</p></blockquote>\n      </li>\n    </ul>\n    <blockquote><p>b</p></blockquote>\n  </li>\n</ul>";
        assert_eq!(carve::to_html(source), expected, "{source}");
    }
}

#[test]
fn a_quote_below_the_ancestor_column_stays_lazy_text() {
    let source = "- - > a\nx\n > b";
    let expected = "<ul>\n  <li>\n    <ul>\n      <li>\n        <blockquote><p>a\nx\n&gt; b</p></blockquote>\n      </li>\n    </ul>\n  </li>\n</ul>";
    assert_eq!(carve::to_html(source), expected);
}
