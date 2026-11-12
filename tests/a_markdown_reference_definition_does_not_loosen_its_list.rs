//! cmark-gfm removes a definition-only paragraph before it tests list
//! looseness, so a blank line next to a reference definition in an item does
//! not loosen the list (carve#6017, carve-rs#4301).

#[test]
fn a_definition_opening_an_item_before_a_blank_line() {
    assert_eq!(
        carve::markdown_to_carve("- [r]: https://e\n\n  a\n  b\n"),
        "- a\n  b\n\n[r]: https://e\n"
    );
    assert_eq!(
        carve::markdown_to_carve("- [r]: https://e\n\n  a\n"),
        "- a\n\n[r]: https://e\n"
    );
}

#[test]
fn a_definition_closing_an_item_after_a_blank_line() {
    assert_eq!(
        carve::markdown_to_carve("- a\n\n  [r]: https://e\n"),
        "- a\n\n[r]: https://e\n"
    );
    assert_eq!(
        carve::markdown_to_carve("1. a\n\n   [r]: https://e\n"),
        "1. a\n\n[r]: https://e\n"
    );
    assert_eq!(
        carve::markdown_to_carve("- x\n  - a\n\n    [r]: https://e\n"),
        "- x\n  - a\n\n[r]: https://e\n"
    );
    assert_eq!(
        carve::markdown_to_carve("> - a\n>\n>   [r]: https://e\n"),
        "> - a\n\n[r]: https://e\n"
    );
}

#[test]
fn a_blank_line_that_still_separates_blocks_keeps_the_list_loose() {
    assert_eq!(
        carve::markdown_to_carve("- a\n\n  [r]: https://e\n- b\n"),
        "- a\n\n- b\n\n[r]: https://e\n"
    );
    assert_eq!(
        carve::markdown_to_carve("- a\n\n  [r]: https://e\n\n  c\n"),
        "- a\n\n  c\n\n[r]: https://e\n"
    );
}
