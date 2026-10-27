#[test]
fn combined_closer_inside_unclosed_code_is_content() {
    for source in ["/*`*/", "/*a`*/"] {
        let expected = if source == "/*`*/" {
            "<p>/*<code>*/</code></p>"
        } else {
            "<p>/*a<code>*/</code></p>"
        };
        assert_eq!(carve::to_html(source), expected, "{source}");
    }
}

#[test]
fn combined_closer_after_closed_code_still_closes() {
    assert_eq!(
        carve::to_html("/*`*/`*/"),
        "<p><strong><em><code>*/</code></em></strong></p>"
    );
    assert_eq!(
        carve::to_html("/*a*/"),
        "<p><strong><em>a</em></strong></p>"
    );
}
