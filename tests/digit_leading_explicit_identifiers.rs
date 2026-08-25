use carve::{html_to_carve, parse, render_carve, render_html, HtmlImportOptions};

#[test]
fn explicit_ids_and_classes_accept_an_ascii_digit_first() {
    let html = render_html(&parse("[x]{.123}\n[y]{#7-x}\n")).unwrap();
    assert!(html.contains("class=\"123\""), "{html}");
    assert!(html.contains("id=\"7-x\""), "{html}");
}

#[test]
fn attribute_names_and_extensions_are_not_widened() {
    for source in ["[x]{12=v}\n", "[x]{12}\n", ":1[x]\n"] {
        let html = render_html(&parse(source)).unwrap();
        assert!(html.contains(source.trim()), "{source:?}: {html}");
    }
}

#[test]
fn a_digit_leading_bare_type_is_a_round_tripping_generic_div() {
    let source = "::: 123\nbody\n:::\n";
    let doc = parse(source);
    assert!(render_html(&doc).unwrap().contains("<div class=\"123\">"));
    assert_eq!(render_carve(&doc).unwrap(), source);
}

#[test]
fn html_import_preserves_digit_leading_ids_and_classes() {
    let source = html_to_carve(
        "<p id=\"123\" class=\"7-x\">x</p>",
        &HtmlImportOptions::default(),
    )
    .unwrap()
    .value;
    assert!(source.contains("{#123 .7-x}"), "{source}");
    assert!(render_html(&parse(&source))
        .unwrap()
        .contains("id=\"123\" class=\"7-x\""));
}
