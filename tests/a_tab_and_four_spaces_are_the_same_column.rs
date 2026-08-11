//! A tab and four spaces are the SAME column, so they open the same blocks.
//!
//! `slice_columns` consumed a straddling tab whole, on the stated ground that
//! "Carve has no indent-sensitive block where the leftover column would change
//! meaning". Inside a list item that is false for every block opener: at the
//! item's content column an opener parses, one column past it the line is
//! paragraph text. So a tab landing PAST the content column dedented flush and
//! opened a block that the same column written in spaces does not.
//!
//! PART 7: "A TAB IN THAT RUN advances the column to the next MULTIPLE OF 4
//! (CommonMark tab stops), so a leading tab indents to column 4." An ordered
//! item's content column is 3, so one tab is past it, exactly as four spaces
//! are.

/// Each case is the same document twice: once indented with a tab, once with
/// four spaces. Both are column 4, so both must produce the same tree.
fn tab_and_four_spaces_agree(opener: &str) {
    let tabbed = format!("1. item\n\n\t{opener}\n");
    let spaced = format!("1. item\n\n    {opener}\n");

    assert_eq!(
        carve::to_html(&tabbed),
        carve::to_html(&spaced),
        "a tab and four spaces are both column 4, so `{opener}` must parse the same way",
    );
}

#[test]
fn a_tabbed_fence_matches_four_spaces() {
    tab_and_four_spaces_agree("~~~\nplain\n~~~");
}

#[test]
fn a_tabbed_heading_matches_four_spaces() {
    tab_and_four_spaces_agree("# h");
}

#[test]
fn a_tabbed_block_quote_matches_four_spaces() {
    tab_and_four_spaces_agree("> q");
}

#[test]
fn a_tabbed_thematic_break_matches_four_spaces() {
    tab_and_four_spaces_agree("---");
}

/// The bound, and it is what keeps the fix from being "never open anything":
/// AT the content column an opener still parses. Three spaces is column 3,
/// which is exactly an ordered item's content column.
///
/// This case passes both before and after the change, so it proves nothing on
/// its own - it is here so a fix that stopped opening blocks inside items
/// altogether would fail rather than look correct.
#[test]
fn an_opener_at_the_content_column_still_parses() {
    let html = carve::to_html("1. ordered\n\n   ~~~\n   plain\n   ~~~\n");

    assert!(
        html.contains("<pre><code>"),
        "a fence at the item's content column must still open: {html}",
    );
}
