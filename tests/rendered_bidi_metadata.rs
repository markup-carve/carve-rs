use carve::to_html;

#[test]
fn link_and_image_metadata_drop_bidi_controls() {
    let source = "[a](/u \"t\u{202e}\")\n\n![t\u{202e}](/u)\n\n![a](/u \"t\u{202e}\")\n\n[b][r]\n\n[r]: /u \"t\u{2066}\"\n";
    let html = to_html(source);
    assert_eq!(html, "<p><a href=\"/u\" title=\"t\">a</a></p>\n<img src=\"/u\" alt=\"t\">\n<img src=\"/u\" alt=\"a\" title=\"t\">\n<p><a href=\"/u\" title=\"t\">b</a></p>");
}

#[test]
fn every_bidi_control_is_removed_only_from_rendered_metadata() {
    for control in [
        '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}', '\u{2066}', '\u{2067}',
        '\u{2068}', '\u{2069}',
    ] {
        let source = format!("[x](/u \"a{control}&b\") ![a{control}&b](/u \"a{control}&b\") [y](/u){{title=\"authored{control}\"}}");
        let html = to_html(&source);
        assert!(
            html.contains("<a href=\"/u\" title=\"a&amp;b\">x</a>"),
            "{html}"
        );
        assert!(html.contains("alt=\"a&amp;b\" title=\"a&amp;b\""), "{html}");
        assert!(
            html.contains(&format!("title=\"authored{control}\"")),
            "{html}"
        );
    }
}
