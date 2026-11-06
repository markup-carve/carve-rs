#[test]
fn a_quoted_description_note_keeps_verse_text_after_its_host() {
    let html = carve::to_html("> :: t\n> : [^n]: ::: |\n>     x\n: b\n\nr[^n]\n");
    assert!(html.contains("<p>x<br>\n: b</p>"), "{html}");
    assert!(!html.contains("<p>: b</p>"), "{html}");
}
