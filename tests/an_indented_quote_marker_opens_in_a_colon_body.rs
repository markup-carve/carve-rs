use carve::to_html;

#[test]
fn a_quote_marker_interrupts_a_colon_body_paragraph() {
    for (source, container) in [
        (
            "- ::: note\n  more\n   > q\n",
            "<aside class=\"admonition note\"",
        ),
        (
            "1. ::: note\n   para\n    > q\n",
            "<aside class=\"admonition note\"",
        ),
        (":: t\n: ::: note\n  more\n   > q\n", "<dd>"),
        (
            "[^c]: ::: note\n      more\n   > q\nsee [^c]\n",
            "<section role=\"doc-endnotes\"",
        ),
    ] {
        let html = to_html(source);
        assert!(html.contains(container), "{source:?}: {html}");
        assert!(
            html.contains("<blockquote><p>q</p></blockquote>"),
            "{source:?}: {html}"
        );
        assert!(
            !html.contains("more &gt; q") && !html.contains("para &gt; q"),
            "{source:?}: {html}"
        );
    }
}
