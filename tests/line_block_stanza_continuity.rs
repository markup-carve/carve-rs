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

#[test]
fn a_line_block_spelling_inside_quoted_code_opens_no_stanza() {
    let html = to_html("> ~~~\n> ::: |\n> ~~~\n> # heading\nb\n");
    assert!(html.ends_with("</blockquote>\n<p>b</p>"), "{html}");
}

#[test]
fn a_code_spelling_inside_a_matched_comment_hides_no_later_stanza() {
    let html = to_html("> %%%\n> ~~~\n> %%%\n> ::: |\n> x\n> # h\nb\n");
    assert!(html.contains("<p>x<br>\n# h<br>\nb</p>"), "{html}");
}
