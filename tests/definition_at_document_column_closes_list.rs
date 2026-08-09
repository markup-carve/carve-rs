fn fmt(source: &str) -> String {
    carve::to_carve(source)
}

#[test]
fn a_column_zero_link_definition_closes_an_ordered_item() {
    let source = "1. x\n[t](u)\n_u_\n[r]: /u\n[t][r]\n\\#\n";
    let expected = "1. x\n   [t](u)\n   _u_\n\n[t][r]\n\\#\n\n[r]: /u\n";

    assert_eq!(fmt(source), expected);
    assert_eq!(carve::to_html(source), carve::to_html(expected));
}

#[test]
fn a_column_zero_footnote_definition_closes_an_unordered_item() {
    let source = "- a\n[^f]: note\nafter[^f]\n";
    let expected = "- a\n\nafter[^f]\n\n[^f]: note\n";

    assert_eq!(fmt(source), expected);
    assert_eq!(carve::to_html(source), carve::to_html(expected));
}

#[test]
fn a_definition_at_the_item_content_column_stays_inside() {
    let source = "1. x\n   [r]: /u\n   [t][r]\n";
    assert_eq!(fmt(source), source);
    assert_eq!(carve::to_carve(&fmt(source)), source);
}

#[test]
fn a_nonzero_definition_below_the_content_column_is_literal_text() {
    let source = "1. x\n [r]: /u\n[t][r]\n";
    let expected = "1. x\n   \\[r\\]: \\/u\n   [t][r]\n";
    let out = fmt(source);

    assert_eq!(out, expected);
    assert_eq!(carve::to_html(source), carve::to_html(&out));
    assert_eq!(carve::to_carve(&out), out);
}
