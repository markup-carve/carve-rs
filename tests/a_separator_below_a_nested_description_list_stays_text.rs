const EXPECTED: &str = "<dl>\n  <dt>a</dt>\n  <dd>\n    <p>b</p>\n    <dl>\n      <dt>t</dt>\n      <dd>zq</dd>\n    </dl>\n  </dd>\n</dl>";

#[test]
fn a_separator_below_a_nested_description_list_stays_text() {
    for source in [
        ":: a\n: b\n    :: t\n  : zq\n",
        ":: a\n: b\n  \t:: t\n  : zq\n",
        ":: a\n: b\n\t\t:: t\n  : zq\n",
        ":: a\n: b\n    :: t\n\n  : zq\n",
    ] {
        assert_eq!(carve::to_html(source).trim(), EXPECTED, "{source:?}");
    }
}

#[test]
fn a_term_below_a_nested_description_list_still_joins_it() {
    let html = carve::to_html(":: a\n: b\n    :: t\n  :: u\n");
    assert!(html.contains("<dt>t</dt>\n      <dt>u</dt>"), "{html}");
}

#[test]
fn a_footnote_body_keeps_attaching_the_separator() {
    let html = carve::to_html("[^1]: b\n    :: t\n  : zq\n\nr[^1]\n");
    assert!(html.contains("<dt>t</dt>\n        <dd>zq</dd>"), "{html}");
}
