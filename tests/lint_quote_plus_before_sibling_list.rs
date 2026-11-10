//! A quote-prefixed `+` followed by a list item at the same quote level stays
//! text, so it is not a refused offer under carve#5736 (corpus 630-4).

use carve::{lint_carve, to_html};

fn refusals(source: &str) -> Vec<(usize, usize, String)> {
    lint_carve(source)
        .into_iter()
        .filter(|warning| {
            matches!(
                warning.rule,
                "continuation-marker-refused" | "refused-continuation-marker"
            )
        })
        .map(|warning| (warning.line, warning.column, warning.message))
        .collect()
}

#[test]
fn a_nested_quote_plus_before_a_list_item_is_quiet() {
    let source = "> > q\n> > +\n> > - s\n";
    assert!(refusals(source).is_empty());
    assert!(to_html(source).contains("<p>q\n+\n- s</p>"));
}

#[test]
fn a_quote_plus_before_a_list_item_is_quiet() {
    let source = "> q\n> +\n> - s\n";
    assert!(refusals(source).is_empty());
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
