//! A link label's closing `]` is found by a scan that skips spans whose content
//! is LITERAL, because a `]` there is content and no escape can spell it
//! otherwise (PART 9 `link_text`).
//!
//! Code spans were already skipped. An editorial comment was not, so a `]`
//! inside one ended the label early - and `{# ... #}` resolves no escapes, so
//! writing `\]` did not help either: it put a real backslash in the comment.
//! The author had no correct spelling available (carve#403).

#[test]
fn a_label_closes_after_a_comment_containing_a_bracket() {
    let out = carve::to_html("[{#a]b#}](u)\n");
    assert!(out.contains(r#"<a href="u""#), "{out}");
    assert!(
        out.contains(r#"<span class="critic-comment">a]b</span>"#),
        "{out}"
    );
}

#[test]
fn a_code_span_is_still_skipped() {
    assert!(carve::to_html("[`a]b`](u)\n").contains(r#"<a href="u""#));
}

#[test]
fn an_unclosed_brace_hash_is_not_a_comment() {
    // No `#}` follows, so there is no span to skip and the scan is unchanged.
    assert!(!carve::to_html("[{#unclosed](u)\n").contains("critic-comment"));
}

#[test]
fn an_ordinary_bare_bracket_still_closes_the_label() {
    assert!(!carve::to_html("[a]b](u)\n").contains("<a "));
}

#[test]
fn a_comment_can_be_the_whole_label() {
    assert!(carve::to_html("[{#note#}](u)\n").contains(r#"<a href="u""#));
}

#[test]
fn nested_labels_with_comments_stay_balanced() {
    // The precomputed bracket table and the scanner must agree, or a nested
    // case resolves differently depending on which one answered.
    let out = carve::to_html("[[{#a]b#}](u)](v)\n");
    assert!(out.contains("critic-comment"), "{out}");
}
