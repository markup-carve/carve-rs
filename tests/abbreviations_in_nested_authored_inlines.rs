//! PART 9R R3: authored inline text is eligible at every inline depth.

#[test]
fn abbreviation_expands_in_inline_note_editorial_and_title() {
    let cases = [
        ("^[HTML]\n*[HTML]: t\n", "<abbr title=\"t\">HTML</abbr>"),
        ("{-HTML-}\n*[HTML]: t\n", "<abbr title=\"t\">HTML</abbr>"),
        (
            "::: e \"HTML\"\nbody\n:::\n*[HTML]: t\n",
            "<abbr title=\"t\">HTML</abbr>",
        ),
    ];
    for (source, expected) in cases {
        let html = carve::to_html(source);
        assert!(html.contains(expected), "{source:?}: {html}");
    }
}

#[test]
fn directive_title_consumes_its_abbreviation_definition_in_plain_text() {
    let source = "::: references \"HTML\"\n:::\n*[HTML]: t\n";
    let plain = carve::to_plain_text(source);
    assert!(plain.contains("HTML (t)"), "{plain}");
    assert!(!plain.contains("*[HTML]: t"), "{plain}");
}
