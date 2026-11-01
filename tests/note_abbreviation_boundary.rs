#[test]
fn document_abbreviation_after_a_description_hosted_note_is_registered() {
    let source = ":: t\n:  - a\n     [^n]: note text\n*[A]: b\n\nSee [^n] A.\n";
    let html = carve::to_html(source);
    assert!(html.contains("<abbr title=\"b\">A</abbr>"), "{html}");
    assert!(html.contains("<p>note text<a href=\"#fnref1\""), "{html}");
    assert!(!html.contains("*[A]: b"), "{html}");
}

#[test]
fn abbreviation_spelling_inside_a_note_body_remains_text() {
    let source = "[^n]: note text\n  *[A]: b\n\nSee [^n] A.\n";
    let html = carve::to_html(source);
    assert!(html.contains("<p>note text\n*[A]: b"), "{html}");
    assert!(html.contains(" A.</p>"), "{html}");
    assert!(!html.contains("<abbr"), "{html}");
}

#[test]
fn comment_delimiter_in_code_does_not_hide_a_following_colon_container() {
    let source = "```\n%%%\n```\n::: note\n- para\n  [^f]: n\n*[A]: b\n```\ncode\n::::\n:::\nafter *b*\n%%%\n";
    let expected = "<pre><code>%%%\n</code></pre>\n<aside class=\"admonition note\" aria-label=\"Note\">\n  <ul>\n    <li>para</li>\n  </ul>\n  <pre><code>code\n::::\n:::\nafter *b*\n%%%\n</code></pre>\n</aside>";
    assert_eq!(carve::to_html(source), expected);
}
