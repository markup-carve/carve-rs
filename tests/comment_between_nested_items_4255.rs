//! A source-positioned comment between list item markers retains that order
//! when the list is nested under any host (carve-rs#4255).

#[test]
fn comment_between_nested_items_keeps_its_source_order() {
    for source in [
        "- l0\n  - l1\n  %%\n  - c\n",
        "- l0\n  1. l1\n  %% note\n  2. c\n",
        "> - l0\n>   - l1\n>   %% note\n>   - c\n",
        "::: note\n- l0\n  - l1\n  %% note\n  - c\n:::\n",
    ] {
        assert_eq!(carve::to_carve(source), source, "{source:?}");
    }
}

#[test]
fn comments_inside_or_after_the_list_do_not_move_between_items() {
    let inside = "- l0\n  - l1\n    %% note\n  - c\n";
    assert_eq!(carve::to_carve(inside), inside);

    let after = "- l0\n  - l1\n  - c\n  %% note\n";
    assert_eq!(carve::to_carve(after), "- l0\n  - l1\n  - c\n\n  %% note\n");
}
