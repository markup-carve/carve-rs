//! PART 9 section 24 C3 (markup-carve/carve#624): a comment is recognized at
//! any column, and being invisible it closes nothing.
//!
//! Below a list item's content column every other construct folds as the text
//! it looks like. A comment does not fold - it is the one construct each engine
//! finds after trimming the line, wherever it sits, and rendering it as VISIBLE
//! text is the one outcome a comment may never have. It is consumed without the
//! lazy frame, so the item's own parse sees a comment line: that keeps it
//! invisible AND leaves the item open for the flush-left line under it.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src)
}

#[test]
fn a_comment_below_the_content_column_keeps_the_item_open() {
    // `b` folds into the item's paragraph. Before this, the comment ended the
    // item and `b` became a top-level paragraph.
    assert_eq!(
        html("- a\n %% c\nb\n"),
        "<ul>\n  <li>a\n    b\n  </li>\n</ul>"
    );
}

#[test]
fn the_comment_itself_renders_nothing() {
    // The point of the rule: no `%% c` anywhere in the output, at any column.
    for src in ["- a\n %% c\nb\n", "- a\n  %% c\nb\n", "- a\n   %% c\nb\n"] {
        assert!(!html(src).contains("%%"), "comment leaked for {src:?}");
    }
}

#[test]
fn a_visible_block_in_the_same_place_still_ends_the_item() {
    // The control. Without it the test above would pass just as well if a list
    // item had simply stopped ending at all. An admonition is closed by its
    // `:::` fence, so it holds no open paragraph and the flush-left line after
    // it is its own top-level block.
    let out = html("- a\n  ::: note\n  c\n  :::\nb\n");
    assert!(
        out.contains("<p>b</p>"),
        "expected `b` outside the item, got {out}"
    );
}

#[test]
fn an_abbreviation_definition_in_an_item_is_text_and_does_not_get_the_exemption() {
    // Not a definition inside a container (markup-carve/carve#611), so it is
    // visible text rather than an invisible block - which is why the exemption
    // above is comments only, where the neighboring `renders_nothing` checks in
    // the parser also count an abbreviation definition. Matches carve-js byte
    // for byte.
    let expected = "<ul>\n  <li>a\n*[HTML]: x\nb</li>\n</ul>";
    assert_eq!(html("- a\n  *[HTML]: x\nb\n"), expected);
    assert_eq!(html("- a\n *[HTML]: x\nb\n"), expected);
}
