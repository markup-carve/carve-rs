// A term nested in a description after a blank line leaves the outer list open
// in a list item, as it does at the root (CARVE-P0-024, corpus 813).

fn outer_sibling(source: &str) {
    let html = carve::to_html(source);
    assert!(
        html.contains("<dd>e</dd>") && !html.contains(": e"),
        "{source:?} folded the outer description:\n{html}"
    );
}

#[test]
fn a_description_at_the_outer_column_is_an_outer_sibling() {
    outer_sibling("- :: t\n  : d\n\n    :: u\n    : v\n  : e\n");
    outer_sibling("- x\n\n  :: t\n  : d\n\n    :: u\n    : v\n  : e\n");
    outer_sibling("1. :: t\n   : d\n\n     :: u\n     : v\n   : e\n");
    outer_sibling("> - :: t\n>   : d\n>\n>     :: u\n>     : v\n>   : e\n");
}

#[test]
fn the_canonical_form_of_corpus_813_3_reads_back() {
    let source = "- x\n\n  :: t\n  :  d\n     :: u\n     : v\n    : e\n";
    let written = carve::to_carve(source);
    assert_eq!(carve::to_html(&written), carve::to_html(source), "{written}");
}
