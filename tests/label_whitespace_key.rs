use carve::{to_carve, to_html};

#[test]
fn links_and_images_normalize_ascii_label_whitespace() {
    assert!(to_html("[t][ a  b ]\n\n[a b]: /u\n").contains("href=\"/u\""));
    assert!(to_html("![x][ a\tb ]\n\n[a b]: /i\n").contains("src=\"/i\""));
}

#[test]
fn labels_remain_case_sensitive_and_preserve_non_ascii_whitespace() {
    assert!(!to_html("[t][A B]\n\n[a b]: /u\n").contains("href=\"/u\""));
    assert!(!to_html("[t][a\u{a0}b]\n\n[a b]: /u\n").contains("href=\"/u\""));
}

#[test]
fn normalization_does_not_make_multiline_labels_valid() {
    assert!(!to_html("[t][a\nb]\n\n[a b]: /u\n").contains("href=\"/u\""));
    assert!(!to_html("x[^a\nb]\n\n[^a b]: note\n").contains("doc-noteref"));
}

#[test]
fn footnotes_use_the_same_key_and_first_definition_wins() {
    let html = to_html("[^ a\t b ]\n\n[^a b]: first\n\n[^ a  b ]: second\n");
    assert!(html.contains("doc-noteref"));
    assert!(html.contains("first"));
    assert!(!html.contains("second"));
}

#[test]
fn the_winning_link_definition_keeps_its_raw_spelling() {
    let source = "[t][a b]\n\n[a b]: /first\n\n[ a  b ]: /last\n";
    let html = to_html(source);
    assert!(html.contains("href=\"/last\""), "{html}");
    assert!(to_carve(source).contains("[ a  b ]: /last"));
}
