//! Security regressions for abbreviation definition parsing.

#[test]
fn empty_abbreviation_definition_is_literal_text() {
    let doc = carve::parse("*[]: x\n\nA");
    assert!(!doc
        .children
        .iter()
        .any(|node| matches!(node, carve::BlockNode::AbbreviationDef(def) if def.abbr.is_empty())));
    assert_eq!(carve::to_html("*[]: x\n\nA"), "<p>*[]: x</p>\n<p>A</p>");
}

#[test]
fn non_alphanumeric_abbreviation_definition_is_literal_text() {
    assert_eq!(
        carve::to_html("*[C++]: C Plus Plus\n\nC++"),
        "<p>*[C++]: C Plus Plus</p>\n<p>C++</p>"
    );
}
