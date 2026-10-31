//! Every recognized refusal of a `+` is reported once, at the `+` (carve-rs#3467).

use carve::lint_carve;

fn refusals(source: &str) -> Vec<(usize, usize)> {
    lint_carve(source)
        .into_iter()
        .filter(|warning| warning.rule == "continuation-marker-refused")
        .map(|warning| (warning.line, warning.column))
        .collect()
}

#[test]
fn both_plus_lines_of_a_stacked_quote_are_refused() {
    assert_eq!(refusals("> - > +\n> - > +\n s\n"), [(1, 7), (2, 7)]);
    assert_eq!(refusals("> > * > +\n> > * > +\n>  tail\n"), [(1, 9), (2, 9)]);
    assert_eq!(refusals("> > - > +\n> > - > +\n>  s\n"), [(1, 9), (2, 9)]);
    assert_eq!(refusals("> > > - > +\n> > > - > +\n>  s\n"), [(1, 11), (2, 11)]);
}

#[test]
fn a_description_items_plus_with_an_unowned_follower_is_refused() {
    assert_eq!(refusals(":: t\n: - +\n # H\n"), [(2, 5)]);
    assert_eq!(refusals(":: t\n: - +\n   # H\n"), [(2, 5)]);
}
