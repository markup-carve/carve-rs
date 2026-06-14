//! §11 ordered-list dialect / delimiter splitting + ambiguous-letter tie-break.

fn first_line(s: &str) -> String {
    carve::to_html(s).lines().next().unwrap_or("").to_string()
}

#[test]
fn ambiguous_roman_letter_resolves_by_sibling() {
    // consecutive roman numeral -> roman
    assert_eq!(first_line("i. one\nii. two\n"), "<ol type=\"i\">");
    // consecutive letter -> alpha
    assert_eq!(
        first_line("c. one\nd. two\n"),
        "<ol type=\"a\" start=\"3\">"
    );
    // lone `i` defaults to roman
    assert_eq!(first_line("i. only\n"), "<ol type=\"i\">");
}

#[test]
fn delimiter_change_starts_a_new_list() {
    let html = carve::to_html("1. a\n2) b\n");
    assert!(html.contains("</ol>\n<ol start=\"2\">"), "{html}");
}

#[test]
fn dialect_change_starts_a_new_list() {
    let html = carve::to_html("1. a\nb. b\n");
    assert!(
        html.contains("</ol>\n<ol type=\"a\" start=\"2\">"),
        "{html}"
    );
}
