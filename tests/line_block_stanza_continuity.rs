use carve::to_html;

#[test]
fn description_payload_markers_are_verse_lines() {
    let html = to_html(":: u\n: ::: |\n  ~~~\n  ~~~\nb\n");
    assert!(html.contains("<p>~~~<br>\n~~~<br>\nb</p>"), "{html}");

    let html = to_html(":: u\n: ::: |\n  ```\n  ```\nb\n");
    assert!(html.contains("<p><code>\n</code><br>\nb</p>"), "{html}");
}

#[test]
fn a_quoted_description_span_preserves_the_prior_stanza() {
    let html = to_html("> :: t\n> : ::: |\n>   x\n>   %%%\n>\n>   %%%\nb\n");
    assert!(html.contains("<p>x<br>\nb</p>"), "{html}");

    let no_stanza = to_html("> :: t\n> : ::: |\n>   %%%\n>\n>   %%%\nb\n");
    assert!(
        no_stanza.ends_with("</blockquote>\n<p>b</p>"),
        "{no_stanza}"
    );

    let ended_stanza = to_html("> :: t\n> : ::: |\n>   x\n>\n>   %%%\n>   y\n>   %%%\nb\n");
    assert!(
        ended_stanza.ends_with("</blockquote>\n<p>b</p>"),
        "{ended_stanza}"
    );
}
