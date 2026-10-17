use carve::{to_html, to_html_with_options, Options};

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
fn a_lazy_tail_stays_with_the_below_column_caption_text() {
    let source = "1. > para\n ^ cap\ntail\n";
    for html in [
        to_html(source),
        to_html_with_options(source, &Options::default().with_positions(true)),
    ] {
        let compact = html.lines().map(str::trim).collect::<Vec<_>>().join("\n");
        assert!(
            compact.contains("<blockquote><p>para\n^ cap\ntail</p></blockquote>"),
            "{html}"
        );
    }
}

#[test]
fn a_caption_at_the_quote_column_attaches() {
    let html = to_html("- > para\n  ^ cap\n");
    assert!(html.contains("<figcaption>cap</figcaption>"), "{html}");
}
