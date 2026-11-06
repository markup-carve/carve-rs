use carve::to_html;

#[test]
fn a_band_paragraph_after_a_nested_marker_span_takes_the_next_lazy_line() {
    assert_eq!(
        to_html("- a\n  - %%%\n  h\nc\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li></li>\n    </ul>\n    h\nc\n  </li>\n</ul>"
    );
}

#[test]
fn a_band_paragraph_after_a_description_marker_span_takes_the_next_lazy_line() {
    assert_eq!(
        to_html("- a\n  :: u\n  : %%%\n  h\nc\n"),
        "<ul>\n  <li>a\n    <dl>\n      <dt>u</dt>\n      <dd></dd>\n    </dl>\n    h\nc\n  </li>\n</ul>"
    );
}

#[test]
fn a_closed_descendant_span_without_a_following_paragraph_keeps_no_lazy_claim() {
    assert_eq!(
        to_html("- a\n  - %%%\nc\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n<p>c</p>"
    );
}
