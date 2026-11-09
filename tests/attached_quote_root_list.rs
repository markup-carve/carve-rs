#[test]
fn a_list_marker_that_misses_a_quote_opens_at_the_document() {
    assert_eq!(
        carve::to_html(":: t\n: +\n1. li\n+\n> q\n- other\n"),
        "<dl>\n  <dt>t</dt>\n  <dd>\n    <ol>\n      <li>li</li>\n    </ol>\n    <blockquote><p>q</p></blockquote>\n  </dd>\n</dl>\n<ul>\n  <li>other</li>\n</ul>"
    );
}
