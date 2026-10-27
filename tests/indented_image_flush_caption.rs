#[test]
fn flush_caption_attaches_to_indented_direct_image() {
    for indent in [" ", "  "] {
        for gap in ["\n", "\n\n"] {
            let source = format!("{indent}![a](u){gap}^ c");
            assert_eq!(
                carve::to_html(&source),
                "<figure>\n  <img src=\"u\" alt=\"a\">\n  <figcaption>c</figcaption>\n</figure>",
                "{source}"
            );
        }
    }
}

#[test]
fn flush_caption_attaches_to_indented_resolved_image() {
    for gap in ["\n", "\n\n"] {
        let source = format!(" ![a][img]{gap}^ c\n\n[img]: u");
        assert_eq!(
            carve::to_html(&source),
            "<figure>\n  <img src=\"u\" alt=\"a\">\n  <figcaption>c</figcaption>\n</figure>",
            "{source}"
        );
    }
}

#[test]
fn two_blanks_leave_caption_detached() {
    let source = " ![a](u)\n\n\n^ c";
    assert_eq!(
        carve::to_html(source),
        "<img src=\"u\" alt=\"a\">\n<p>^ c</p>"
    );
}
