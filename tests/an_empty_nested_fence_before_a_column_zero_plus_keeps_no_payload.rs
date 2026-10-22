// markup-carve/carve#3420: the plus manufactures no payload line in an empty fence.
#[test]
fn an_empty_nested_fence_before_a_column_zero_plus_keeps_no_payload() {
    for source in [
        "- - ```\n+\n| a |\n| b |\ntail\n",
        "- - ```\n+\ntail\n",
        "- - - ~~~\n+\ntail\n",
    ] {
        let html = carve::to_html(source);
        assert!(
            html.contains("<pre><code></code></pre>"),
            "{source:?}\n{html}"
        );
    }
}

#[test]
fn an_authored_blank_stays_the_payload() {
    let html = carve::to_html("- - ```\n\n+\ntail\n");
    assert!(html.contains("<pre><code>\n</code></pre>"), "{html}");
}
