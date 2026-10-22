//! `list-item-block-overindented` measures against the innermost container: a
//! description inside the item places a block at its own content column.

use carve::lint_carve;

fn overindented(source: &str) -> Vec<(usize, usize)> {
    lint_carve(source)
        .into_iter()
        .filter(|w| w.rule == "list-item-block-overindented")
        .map(|w| (w.line, w.column))
        .collect()
}

#[test]
fn a_block_at_the_description_column_is_silent() {
    for source in [
        "- :: t\n  : d\n\n    :: u\n    : e\n",
        "- :: t\n  : d\n    :: u\n    : e\n",
        "- a\n\n  :: t\n  : d\n\n    > q\n",
        "> - :: t\n>   : d\n>\n>     :: u\n>     : e\n",
    ] {
        assert_eq!(overindented(source), Vec::new(), "{source:?}");
    }
}

#[test]
fn a_block_past_the_item_column_outside_a_description_is_reported() {
    assert_eq!(overindented("- a\n\n    > q\n"), vec![(3, 5)]);
}
