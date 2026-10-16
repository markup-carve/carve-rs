fn html(source: &str) -> String {
    carve::to_html(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("> <", "><")
}

#[test]
fn a_below_column_caption_stays_text_after_an_image() {
    for prefix in ["", " "] {
        assert_eq!(
            html(&format!(":: t\n: ![a](x.png)\n{prefix}^ cap\n")),
            "<dl><dt>t</dt><dd><img src=\"x.png\" alt=\"a\"> ^ cap</dd></dl>"
        );
    }
}

#[test]
fn a_below_column_caption_folds_into_nested_body_text() {
    assert_eq!(
        html(":: t\n: > para\n^ cap\n"),
        "<dl><dt>t</dt><dd><blockquote><p>para ^ cap</p></blockquote></dd></dl>"
    );
    assert_eq!(
        html(":: t\n: ::: note\n  para\n^ cap\n"),
        "<dl><dt>t</dt><dd><aside class=\"admonition note\" aria-label=\"Note\"><p>para ^ cap</p></aside></dd></dl>"
    );
}
