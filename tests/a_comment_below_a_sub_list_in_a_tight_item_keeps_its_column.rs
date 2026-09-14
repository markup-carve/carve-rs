//! A `%%` comment below a sub-list in a TIGHT item gained a level of
//! indentation on every writer pass (markup-carve/carve-rs#1592, ported from
//! markup-carve/carve-js#1676).
//!
//! A tight item joins its children with a single newline, so a comment below a
//! nested list was written at the item's content column - which IS that
//! sub-list's marker column. The re-parse therefore read the comment into the
//! SUB-LIST'S LAST ITEM instead of the hosting item, and the next pass spelled
//! it a level deeper:
//!
//!     - - d          - - d          - - d
//!                ->    %% q     ->      %% q
//!       %% q
//!
//! PART 11 section 1 is stated over the written bytes, and this writer's output
//! does not read back as the document it was written from.
//!
//! **No HTML comparison can see it.** A comment renders nothing, so
//! `to_html(fmt(x)) == to_html(x)` holds on every pass - the property PART 11
//! section 2a calls necessary and not sufficient. Only the written SOURCE of
//! two passes, or the re-parsed tree, shows the drift, so every assertion here
//! is on `to_carve`, not on `to_html`.
//!
//! A sweep of 1008 documents (14 block kinds below 18 predecessor shapes across
//! four hosts) found the line comment to be the only HTML-invisible block kind
//! affected, in all 13 of its sub-list predecessor rows on all four hosts, and
//! measured the drift as ONE re-attachment: no row moved again on a third pass.
//! The 44 rows that remain are a separate, HTML-VISIBLE defect of the same site
//! (a block image, and a quote below a quote) - carve-rs#1595, not this.

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// PART 11 section 1: the writer's own output must be a fixpoint.
fn is_a_fixpoint(src: &str) -> bool {
    let first = fmt(src);
    first == fmt(&first)
}

/// The ticket's repro, spelled out rather than generated.
#[test]
fn the_comment_stays_at_the_hosting_item_s_column() {
    assert_eq!(fmt("- - d\n\n  %% q\n"), "- - d\n\n  %% q\n");
}

/// Every sub-list shape the sweep covered. The gate is the SIBLING'S KIND - a
/// list - and NOT what that list's last item ends in, so a row here that ends
/// in a heading, a fence, a table, a break or a div is as load-bearing as the
/// paragraph one.
#[test]
fn a_comment_below_any_sub_list_shape_is_a_fixpoint() {
    for predecessor in [
        "- p",
        "- > p",
        "- :: t\n  :  p",
        "- ![p](i.png)",
        "- p\n-",
        "- - p",
        "- # p",
        "- ```\n  p\n  ```",
        "- |= p |\n  | q |",
        "- ---",
        "- ::: note\n  p\n  :::",
        "1. p",
        "- - - p",
    ] {
        let src = format!("- {}\n\n  %% q\n", predecessor.replace('\n', "\n  "));
        assert!(is_a_fixpoint(&src), "drifted: {src:?} -> {:?}", fmt(&src));
    }
}

/// The same shape under each of the four hosts the sweep used.
#[test]
fn every_host_keeps_the_column() {
    for src in [
        "- - d\n\n  %% q\n",
        "1. 1. d\n\n   %% q\n",
        "> - - d\n>\n>   %% q\n",
        "::: note\n- - d\n\n  %% q\n:::\n",
    ] {
        assert!(is_a_fixpoint(src), "drifted: {src:?} -> {:?}", fmt(src));
    }
}

/// The blank line closes the sub-list WITHOUT loosening the item: a comment
/// spells no paragraph for the blank to part. If it ever loosens, the item
/// gains a `<p>` and this fails.
#[test]
fn the_separator_does_not_loosen_the_item() {
    let html = carve::to_html("- - d\n\n  %% q\n");
    assert_eq!(
        html, "<ul>\n  <li>\n    <ul>\n      <li>d</li>\n    </ul>\n  </li>\n</ul>",
        "the item was loosened"
    );
    assert_eq!(carve::to_html(&fmt("- - d\n\n  %% q\n")), html);
}

/// CONTROLS THAT MUST STAY GREEN UNDER THE FIX, and must keep NO separator.
///
/// These are here so a later widening of the rule - keying on the column, or on
/// "a comment in a tight item", instead of on the sibling's kind - fails loudly
/// rather than silently inserting blank lines the author never wrote. Each one
/// is a shape the sweep measured as already correct.
#[test]
fn a_comment_gains_no_separator_where_none_is_needed() {
    for (src, expected) in [
        // Top level: no item, no content column, nothing to collide with.
        ("%% q\n", "%% q\n"),
        // A single-level item: the content column hosts no list.
        ("- d\n\n  %% q\n", "- d\n  %% q\n"),
        // A quote above: the comment cannot re-attach to it at this column.
        ("- > d\n\n  %% q\n", "- > d\n  %% q\n"),
        // A definition list above.
        ("- :: t\n  :  d\n\n  %% q\n", "- :: t\n  : d\n  %% q\n"),
        // Another comment above.
        ("- %% d\n\n  %% q\n", "- %% d\n  %% q\n"),
        // The `%%%` FENCE form below a sub-list: its opener already closes the
        // sub-list, so a separator would be redundant markup.
        ("- - d\n\n  %%%\n  q\n  %%%\n", "- - d\n  %%%\n  q\n  %%%\n"),
    ] {
        assert_eq!(fmt(src), expected, "control moved: {src:?}");
        assert!(is_a_fixpoint(src), "control drifted: {src:?}");
    }
}

/// The `{% text %}` delimited form is excluded by measurement, not by
/// assumption: it does not drift, and it keeps whatever separator it had.
#[test]
fn a_delimited_comment_below_a_sub_list_is_untouched() {
    assert!(is_a_fixpoint("- - d\n\n  {% q %}\n"));
    assert_eq!(fmt("- - d\n\n  {% q %}\n"), "- - d\n\n  {% q %}\n");
}
