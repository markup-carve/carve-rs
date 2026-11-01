//! PART 9R R3: an abbreviation boundary needs a neighbor that is not a Unicode
//! letter or number (general category L or N), not merely a non-ASCII one
//! (carve#4729).

fn render(body: &str) -> String {
    carve::to_html(&format!("*[HTML]: Hypertext\n\n{body}\n"))
}

#[test]
fn a_letter_or_number_of_any_script_glues_the_term_into_a_word() {
    for body in [
        "αHTMLβ", "HTMLé", "éHTML", "HTML٣", "中HTML", "ⅷHTML", "HTML²",
    ] {
        assert!(!render(body).contains("<abbr"), "{body}: {}", render(body));
    }
}

#[test]
fn any_other_neighbor_is_a_boundary() {
    for body in [
        "(HTML)",
        "HTML-x",
        "_HTML_",
        "HTML\u{301}",
        "😀HTML",
        "HTML\u{a0}x",
    ] {
        assert!(
            render(body).contains("<abbr title=\"Hypertext\">HTML</abbr>"),
            "{body}: {}",
            render(body)
        );
    }
}
