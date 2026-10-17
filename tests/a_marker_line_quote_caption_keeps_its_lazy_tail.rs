use carve::{to_html, to_html_with_options, Options};

#[test]
fn the_caption_at_the_quote_column_takes_a_lazy_tail() {
    let source = "- > para\n  ^ cap\ntail\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        assert!(
            html.contains("<figcaption>cap\ntail</figcaption>"),
            "{html}"
        );
        assert!(!html.contains("</ul>\n<p>tail</p>"), "{html}");
    }
}
