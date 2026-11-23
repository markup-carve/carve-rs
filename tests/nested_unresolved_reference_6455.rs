//! carve#6455, C2-H-081 and C2-H-085: an unresolved reference inside link text writes its ordinary text,
//! brackets and label included; a resolved one writes its content only.

fn html(src: &str) -> String {
    carve::render(src, &carve::Options::default())
}

#[test]
fn an_unresolved_reference_inside_link_text_keeps_its_brackets() {
    assert_eq!(html("[[a][b]](u)\n"), "<p><a href=\"u\">[a][b]</a></p>");
    assert_eq!(
        html("[outer [x][]](/u)\n"),
        "<p><a href=\"/u\">outer [x][]</a></p>"
    );
    assert_eq!(
        html("[o [x][]][d]\n\n[d]: /dd\n"),
        "<p><a href=\"/dd\">o [x][]</a></p>"
    );
}

#[test]
fn a_resolved_reference_inside_link_text_writes_its_content() {
    assert_eq!(
        html("[[a][b]](u)\n\n[b]: /b\n"),
        "<p><a href=\"u\">a</a></p>"
    );
    assert_eq!(html("[[t](v)](u)\n"), "<p><a href=\"u\">t</a></p>");
}
