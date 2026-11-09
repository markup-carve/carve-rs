use carve::to_html;

#[test]
fn a_link_text_scan_reads_a_bracket_in_trailing_attributes() {
    assert_eq!(
        to_html("[t[x](/u){title=\"]\"}y](/z)\n").trim(),
        "<p>[t<a href=\"/u\">x</a>{title=“]”}y](/z)</p>"
    );
}
