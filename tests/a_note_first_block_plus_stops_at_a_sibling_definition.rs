#[test]
fn a_duplicate_definition_does_not_take_the_first_notes_plus() {
    let source = "[^a]: +\n[^a]: +\n\nsee[^a]\n";
    let html = carve::to_html(source);
    assert!(html.contains("<p>+<a href=\"#fnref1\""), "{html}");
    assert!(!html.contains("<p>word"), "{html}");
}

#[test]
fn a_distinct_definition_starts_a_new_note_after_the_plus() {
    for source in [
        "[^a]: +\n[^b]: word\n\nsee[^a] [^b]\n",
        "> [^a]: +\n> [^b]: word\n\nsee[^a] [^b]\n",
        "- [^a]: +\n  [^b]: word\n\nsee[^a] [^b]\n",
    ] {
        let html = carve::to_html(source);
        assert!(html.contains("<p>+<a href=\"#fnref1\""), "{source}\n{html}");
        assert!(
            html.contains("<p>word<a href=\"#fnref2\""),
            "{source}\n{html}"
        );
    }
}
