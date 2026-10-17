use carve::to_html;

#[test]
fn a_caption_below_the_quote_column_folds_into_the_quote() {
    for source in [
        "1. > para\n ^ cap\n",
        "1. > para\n  ^ cap\n",
        "- > para\n^ cap\n",
    ] {
        let html = to_html(source);
        assert!(
            html.contains("<blockquote><p>para\n^ cap</p></blockquote>"),
            "{source:?}: {html}"
        );
        assert!(!html.contains("<figcaption>"), "{source:?}: {html}");
    }
}

#[test]
fn a_caption_at_the_quote_column_attaches() {
    let html = to_html("- > para\n  ^ cap\n");
    assert!(html.contains("<figcaption>cap</figcaption>"), "{html}");
}
