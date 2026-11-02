//! carve#4865: a hoisted container definition keeps its authored order, so the
//! last definition of a term wins with and without positions.

fn expansion(source: &str) -> String {
    let html = carve::to_html(source);
    let at = html.find("<abbr title=\"").expect("an abbreviation") + 13;
    html[at..at + html[at..].find('"').unwrap()].to_string()
}

#[test]
fn a_later_document_definition_beats_an_earlier_quoted_one() {
    assert_eq!(expansion("> *[A]: first\n\n*[A]: second\n\nA\n"), "second");
}

#[test]
fn a_later_quoted_definition_beats_an_earlier_document_one() {
    assert_eq!(expansion("*[A]: first\n\n> *[A]: second\n\nA\n"), "second");
}

#[test]
fn a_later_item_definition_beats_an_earlier_note_one() {
    assert_eq!(
        expansion("[^n]: x\n  *[A]: first\n\n- y\n  *[A]: second\n\nA[^n]\n"),
        "second"
    );
}
