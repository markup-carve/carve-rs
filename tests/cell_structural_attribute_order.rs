use carve::to_html;

#[test]
fn spans_precede_authored_cell_attributes_in_head_and_body() {
    for (source, structural) in [
        ("|={x} a|<|\n", "colspan=\"2\""),
        ("|{x} a|<|\n", "colspan=\"2\""),
        ("|={x} a|\n|^|\n", "rowspan=\"2\""),
        ("|{x} a|\n|^|\n", "rowspan=\"2\""),
    ] {
        let html = to_html(source);
        let span = html.find(structural).expect(&html);
        let authored = html.find(" x=\"\"").expect(&html);
        assert!(span < authored, "{source:?}: {html}");
    }
}
