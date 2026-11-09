//! PART 9R R3: authored inline text is eligible at every inline depth.

#[test]
fn abbreviation_expands_in_inline_note_editorial_and_title() {
    let cases = [
        ("^[HTML]\n*[HTML]: t\n", "<abbr title=\"t\">HTML</abbr>"),
        ("{-HTML-}\n*[HTML]: t\n", "<abbr title=\"t\">HTML</abbr>"),
        ("::: e \"HTML\"\nbody\n:::\n*[HTML]: t\n", "<abbr title=\"t\">HTML</abbr>"),
    ];
    for (source, expected) in cases {
        let html = carve::to_html(source);
        assert!(html.contains(expected), "{source:?}: {html}");
    }
}
