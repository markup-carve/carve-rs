#[test]
fn terms_past_an_unclosed_div_opener_end_before_a_flush_left_description() {
    let expected = "<ul>\n  <li>a\n    <div class=\"box\">\n      <dl>\n        <dt>sub</dt>\n      </dl>\n    </div>\n  </li>\n</ul>\n<p>: def</p>";
    for source in [
        "- a\n  ::: box\n   :: sub\n: def\n",
        "- a\n  ::: box\n    :: sub\n: def\n",
        "- a\n   ::: box\n     :: sub\n: def\n",
    ] {
        assert_eq!(carve::to_html(source).trim(), expected, "{source:?}");
    }
}

#[test]
fn a_description_at_the_term_column_stays_in_the_div() {
    let html = carve::to_html("- a\n  ::: box\n   :: sub\n   : def\n");
    assert!(html.contains("<dd>def</dd>"), "{html}");
}

#[test]
fn an_indented_term_without_a_list_host_is_paragraph_text() {
    let html = carve::to_html("::: box\n :: sub\n: def\n");
    assert!(html.contains("<p>:: sub\n: def</p>"), "{html}");
}
