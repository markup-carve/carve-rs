//! A quote-prefixed `+` followed by a list item at the same quote level stays
//! text, and `continuation-marker-refused` reports it (carve#3421, corpus 630-4).

use carve::{lint_carve, to_html};

fn refusals(source: &str) -> Vec<(usize, usize, String)> {
    lint_carve(source)
        .into_iter()
        .filter(|warning| warning.rule == "continuation-marker-refused")
        .map(|warning| (warning.line, warning.column, warning.message))
        .collect()
}

const MESSAGE: &str = "This + attaches nothing because the next list item is a sibling. It remains text in the block quote.";

#[test]
fn a_nested_quote_plus_before_a_list_item_is_reported() {
    let source = "> > q\n> > +\n> > - s\n";
    assert_eq!(refusals(source), vec![(2, 5, MESSAGE.to_string())]);
    assert!(to_html(source).contains("<p>q\n+\n- s</p>"));
}

#[test]
fn a_quote_plus_before_a_list_item_is_reported() {
    let source = "> q\n> +\n> - s\n";
    assert_eq!(refusals(source), vec![(2, 3, MESSAGE.to_string())]);
    assert!(to_html(source).contains("<p>q\n+\n- s</p>"));
}

#[test]
fn an_unprefixed_plus_still_attaches_a_list_to_the_quote() {
    let source = "> quoted\n+\n- item\n";
    assert!(refusals(source).is_empty());
    assert!(to_html(source).contains("<ul>"));
}

#[test]
fn a_plus_at_an_inner_quote_marker_column_still_attaches_a_list() {
    let source = "> > q\n> +\n> - a\n";
    assert!(refusals(source).is_empty());
    assert!(to_html(source).contains("<ul>"));
}
