//! Two HTML-VISIBLE writer defects at `render_item_blocks`
//! (markup-carve/carve-rs#1595), left over from the sweep that settled
//! markup-carve/carve-rs#1592.
//!
//! A tight item joins its children with a single newline, so a child is written
//! at the item's CONTENT column. Two kinds of child do not survive that:
//!
//! 1. A BLOCK IMAGE below anything that leaves an inline run open at that
//!    column - a sub-list, a quote, a definition list - is read as a
//!    continuation of it, and the image re-parses INSIDE the block above:
//!
//!        - - p            - - p
//!                     ->    ![z](i.png)      (inside the sub-list's item)
//!          ![z](i.png)
//!
//! 2. A QUOTE below a quote, or a TABLE below a table, is taken by the
//!    `adjacent_blocks_merge` arm, which tagged the UPPER block of the pair.
//!    `at_marker_column` undoes its tag by POSITION - a line that starts with
//!    it - and the item's first block never starts a line of its own, because
//!    the list marker is there. The sentinel survived mid-line, the item came
//!    back spelling a literal `+`, and both quotes escaped to the top level.
//!
//! Unlike #1592 both are visible to `to_html(fmt(x)) == to_html(x)`, so every
//! assertion here carries the HTML comparison as well as the written form.
//!
//! THE REPAIR IS THE MARKER COLUMN, NOT A BLANK LINE. #1596 could separate a
//! comment with a blank because a comment spells no paragraph for the blank to
//! part. An image spells one, so a blank there is a looseness question rather
//! than a separator. A `+` at the marker column closes the block above without
//! touching looseness at all.
//!
//! Measured over the same 1008-document grid #1592 used - 14 block kinds below
//! 18 predecessor shapes across four hosts - on source fixpoint, re-parsed tree
//! and HTML preservation. All three counts fall from 48 to 0, and the WRITTEN
//! FORM of exactly those 48 documents moves: no row that was already correct is
//! respelled.

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// PART 11 section 1: the writer's own output must be a fixpoint.
fn is_a_fixpoint(src: &str) -> bool {
    let first = fmt(src);
    first == fmt(&first)
}

/// PART 11 section 2a: the written form must render to the same HTML.
fn keeps_its_html(src: &str) -> bool {
    carve::to_html(&fmt(src)) == carve::to_html(src)
}

#[test]
fn a_block_image_below_a_sub_list_stays_in_the_hosting_item() {
    assert_eq!(fmt("- - p\n\n  ![z](i.png)\n"), "- - p\n+\n![z](i.png)\n");
    assert_eq!(
        carve::to_html("- - p\n\n  ![z](i.png)\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>p</li>\n    </ul>\n    \
         <img src=\"i.png\" alt=\"z\">\n  </li>\n</ul>",
    );
    assert!(keeps_its_html("- - p\n\n  ![z](i.png)\n"));
    assert!(is_a_fixpoint("- - p\n\n  ![z](i.png)\n"));
}

/// Every predecessor shape the sweep found folding. The gate is whether the
/// block above leaves an inline run open AT THIS ITEM'S CONTENT COLUMN, which
/// recurses through a sub-list into its last item and through a quote into its
/// last block - so a sub-list whose last item ends in a quote is as
/// load-bearing as the bare-paragraph one.
#[test]
fn a_block_image_below_any_open_paragraph_keeps_its_item() {
    for predecessor in [
        "- p",
        "1. p",
        "- - p",
        "- > q",
        "- :: t\n  : d",
        "- ![p](a.png)",
        "- - q",
        "- p\n-",
        "> p",
        ":: t\n: d",
    ] {
        let src = format!("- {}\n\n  ![z](i.png)\n", predecessor.replace('\n', "\n  "));
        assert!(
            keeps_its_html(&src),
            "html moved: {src:?} -> {:?}",
            fmt(&src)
        );
        assert!(is_a_fixpoint(&src), "drifted: {src:?} -> {:?}", fmt(&src));
    }
}

/// A quote below a quote, and a table below a table - the two
/// `adjacent_blocks_merge` pairs a tight item can hold as its FIRST two blocks.
#[test]
fn a_merging_run_that_opens_an_item_spells_no_literal_marker() {
    assert_eq!(fmt("- > p\n\n  > z\n"), "- > p\n+\n> z\n");
    assert_eq!(fmt("- | a |\n\n  | z |\n"), "- | a |\n+\n| z |\n");
    // THE BLOCKQUOTE HOST IS WHAT PICKS THIS SPELLING over carve-js's, which
    // strips the tag off the item's first line and opens the item with `- +`
    // (carve-js#1681). Measured on this row: `> - +` / `> > p` re-parses as an
    // EMPTY item with the quote hoisted out beside the list. Reading UP puts the
    // marker on a line of its own inside the quote, where it holds.
    assert_eq!(fmt("> - > p\n>\n>   > z\n"), "> - > p\n> +\n> > z\n");
    for src in ["- > p\n\n  > z\n", "- | a |\n\n  | z |\n"] {
        assert!(
            !fmt(src).contains("<li>+"),
            "the item spelled a literal marker: {:?}",
            fmt(src)
        );
        assert!(keeps_its_html(src), "html moved: {src:?} -> {:?}", fmt(src));
        assert!(is_a_fixpoint(src), "drifted: {src:?} -> {:?}", fmt(src));
    }
}

/// The same two shapes under each of the four hosts the sweep used.
#[test]
fn every_host_keeps_the_item() {
    for src in [
        "- - p\n\n  ![z](i.png)\n",
        "1. 1. p\n\n   ![z](i.png)\n",
        "> - - p\n>\n>   ![z](i.png)\n",
        "::: d\n- - p\n\n  ![z](i.png)\n:::\n",
        "- > p\n\n  > z\n",
        "1. > p\n\n   > z\n",
        "> - > p\n>\n>   > z\n",
        "::: d\n- > p\n\n  > z\n:::\n",
    ] {
        assert!(keeps_its_html(src), "html moved: {src:?} -> {:?}", fmt(src));
        assert!(is_a_fixpoint(src), "drifted: {src:?} -> {:?}", fmt(src));
    }
}

/// A run of THREE merging quotes: the marker moves to the lower block of each
/// pair, so the second and third carry it and the first is written plainly.
#[test]
fn a_run_of_three_quotes_carries_the_marker_on_every_lower_block() {
    let src = "- > p\n\n  > q\n\n  > r\n";
    assert_eq!(fmt(src), "- > p\n+\n> q\n+\n> r\n");
    assert!(keeps_its_html(src));
    assert!(is_a_fixpoint(src));
}

/// CONTROLS THAT MUST STAY GREEN, AND MUST GAIN NO MARKER.
///
/// Each is a shape the sweep measured as already correct, and each is here so a
/// later widening - keying on "a block image in a tight item", or on the
/// column, instead of on whether the block above leaves a run open - fails
/// loudly instead of quietly inventing markup the re-parse does not need. A
/// heading, a fence, a table, a break and a div all END at their last line.
#[test]
fn a_child_gains_no_marker_where_none_is_needed() {
    for (src, expected) in [
        // A sub-list whose last item ends in a block that closes.
        ("- - # h\n\n  ![z](i.png)\n", "- - # h\n  ![z](i.png)\n"),
        ("- - | a |\n\n  ![z](i.png)\n", "- - | a |\n  ![z](i.png)\n"),
        ("- - ---\n\n  ![z](i.png)\n", "- - ---\n  ![z](i.png)\n"),
        (
            "- - ```\n    c\n    ```\n\n  ![z](i.png)\n",
            "- - ```\n    c\n    ```\n  ![z](i.png)\n",
        ),
        (
            "- - ::: d\n    x\n    :::\n\n  ![z](i.png)\n",
            "- - ::: d\n    x\n    :::\n  ![z](i.png)\n",
        ),
        // No container above at all.
        ("- # h\n\n  ![z](i.png)\n", "- # h\n  ![z](i.png)\n"),
        ("- | a |\n\n  ![z](i.png)\n", "- | a |\n  ![z](i.png)\n"),
    ] {
        assert_eq!(fmt(src), expected, "control moved: {src:?}");
        assert!(keeps_its_html(src), "control lost its html: {src:?}");
        assert!(is_a_fixpoint(src), "control drifted: {src:?}");
    }
}

/// #1596's separator is a BLANK LINE, and it stays one. A comment below a
/// sub-list must not be swept into the marker-column repair: the two answers
/// are different markup and only one of them is right for a block that renders
/// nothing.
#[test]
fn the_comment_repair_is_untouched() {
    assert_eq!(fmt("- - d\n\n  %% q\n"), "- - d\n\n  %% q\n");
    assert_eq!(fmt("- - d\n\n  {% q %}\n"), "- - d\n\n  {% q %}\n");
}
