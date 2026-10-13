//! A quote, an attribute line or a link definition past a box's column inside
//! a list item opens on its authored base, as a nested list does.

fn compact(source: &str) -> String {
    carve::to_html(source).lines().map(str::trim).collect()
}

#[test]
fn an_attribute_line_past_the_box_column_attaches() {
    for source in [
        "- ::: box\n    {.k}\n    para\n  :::\n",
        "- a\n  ::: box\n    {.k}\n    para\n  :::\n",
    ] {
        assert!(
            compact(source).contains("<p class=\"k\">para</p>"),
            "{source:?}"
        );
    }
}

#[test]
fn a_quote_past_the_box_column_opens() {
    for source in ["- ::: box\n    > q\n  :::\n", "- ::: box\n   > q\n  :::\n"] {
        assert!(
            compact(source).contains("<blockquote><p>q</p></blockquote>"),
            "{source:?}"
        );
    }
}

#[test]
fn a_link_definition_past_the_box_column_registers() {
    for source in [
        "- a\n  ::: note\n   [r]: /url\n  :::\n\n[r][]\n",
        "- a\n  ::: note\n    [r]: /url\n  :::\n\n[r][]\n",
    ] {
        let html = compact(source);
        assert!(
            html.contains("<p><a href=\"/url\">r</a></p>"),
            "{source:?}: {html}"
        );
        assert!(!html.contains("[r]: /url"), "{source:?}: {html}");
    }
}
