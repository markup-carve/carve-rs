//! A wrapped block attribute that reaches no block is reported like its one-line
//! twin (corpus 329-6, markup-carve/carve-rs#3094).

use carve::lint_carve;

fn unattached(source: &str) -> Vec<(usize, usize)> {
    lint_carve(source)
        .into_iter()
        .filter(|warning| warning.rule == "unattached-block-attribute")
        .map(|warning| (warning.line, warning.column))
        .collect()
}

#[test]
fn the_wrapped_form_reports_where_the_one_line_form_does() {
    for (wrapped, one_line, at) in [
        (
            ":: t\n:  d\n   {.k\n   #x}\ntail\n",
            ":: t\n:  d\n   {.k #x}\ntail\n",
            (3, 4),
        ),
        ("- a\n  {.c\n}\npara\n", "- a\n  {.c}\npara\n", (2, 3)),
        (
            "- a\n  - b\n    {.c\n}\npara\n",
            "- a\n  - b\n    {.c}\npara\n",
            (3, 5),
        ),
    ] {
        assert_eq!(unattached(wrapped), vec![at], "{wrapped:?}");
        assert_eq!(unattached(one_line), vec![at], "{one_line:?}");
    }
}
