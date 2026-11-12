use carve::lint_carve;

fn refusals(source: &str) -> Vec<(usize, usize)> {
    lint_carve(source)
        .iter()
        .filter(|warning| warning.rule == "continuation-marker-refused")
        .map(|warning| (warning.line, warning.column))
        .collect()
}

#[test]
fn a_comment_kept_in_the_item_does_not_hide_the_refusal() {
    for (source, column) in [
        ("- +\n%% c\n\ntext\n", 3),
        ("- +\n%% c\n- b\n", 3),
        ("- +\n%% c\n%% d\n- b\n", 3),
        ("1. +\n%% c\n\ntext\n", 4),
    ] {
        assert_eq!(refusals(source), [(1, column)], "{source:?}");
    }
}

#[test]
fn a_block_attached_after_the_comment_still_counts() {
    assert_eq!(refusals("- +\n%% c\nx\n"), [], "attached first block");
}
