//! Mention/tag name boundaries (grammar PART 9 §7; corpus
//! 89-mention-and-tag-name-boundaries).

#[test]
fn mention_keeps_interior_dot() {
    let html = carve::to_html("Ping @john.doe today.");
    assert!(
        html.contains("<span class=\"mention\"><strong>@john.doe</strong></span>"),
        "{html}"
    );
}

#[test]
fn mention_trailing_dot_is_punctuation() {
    let html = carve::to_html("Reach @markus. end");
    assert!(
        html.contains("<span class=\"mention\"><strong>@markus</strong></span>. end"),
        "{html}"
    );
}

#[test]
fn email_is_not_a_mention() {
    let html = carve::to_html("Write me@example.com please.");
    assert!(!html.contains("mention"), "{html}");
}

#[test]
fn apostrophe_after_a_mention_is_a_right_quote() {
    // Flanking substitution (PART 9 §8): the apostrophe is preceded by the
    // mention (non-whitespace), so it is a RIGHT single quote even though it
    // starts its own text node.
    let html = carve::to_html("That is @john's idea.");
    assert!(html.contains("</span>’s idea."), "{html}");
}
