//! A comment fence's body is indented FROM ITS FENCE, not from column 0
//! (PART 11, markup-carve/carve#653, carve-rs#601).
//!
//! #585 made a comment span below an item's content column keep its own
//! columns, which is right for the parse. It also meant the body arrived at the
//! node still carrying the fence's indent, and `render_block_comment` writes the
//! content verbatim between fences the writer indents itself - so the fence
//! column was applied twice and corpus 186 came back one column deeper than
//! carve-js and carve-php write it.
//!
//! The divergence is invisible to everything that was watching: a comment
//! renders nothing, so `to_html(fmt(x)) == to_html(x)` holds either way, and the
//! output is idempotent at either column. markup-carve/carve#653 had already
//! settled this exact document and closed one day before #585 reintroduced it
//! with the engines swapped - carve-js used to be the one writing the extra
//! column, until markup-carve/carve-js#639.
//!
//! Expectations measured against carve-js and carve-php on current main.

use carve::{to_carve, to_html};

#[test]
fn a_below_column_fence_body_keeps_the_fence_s_own_column() {
    // Corpus 186. The fence sits at column 1, below the item's content column,
    // and its body sits at column 1 too - the same column as its fence, so it
    // is written at the fence's column and no deeper.
    assert_eq!(
        to_carve("- a\n %%% n\n x\n %%%\n tail\n"),
        "- a\n  %%%\n  n\n  x\n  %%%\n  tail\n"
    );
}

#[test]
fn the_nested_form_agrees_too() {
    // Corpus 191, the same shape one level deeper.
    assert_eq!(
        to_carve("- - a\n %%% c\n x\n %%%\n b\n"),
        "- - a\n    %%%\n    c\n    x\n    %%%\n    b\n"
    );
}

#[test]
fn a_body_indented_past_its_fence_keeps_the_difference() {
    // The control against over-stripping. Relative means relative: a body two
    // columns inside its fence stays two columns inside it. This is the half
    // carve-php#782 fixed in the other direction, and dropping it would trade
    // one divergence for another.
    assert_eq!(to_carve("%%%\n  x\n%%%\n"), "%%%\n  x\n%%%\n");
}

#[test]
fn a_body_shallower_than_its_fence_is_not_eaten_into() {
    // The strip is capped at the line's own indent, so a body line that starts
    // before its fence's column loses its whitespace and none of its text.
    //
    // carve-php writes the same. carve-js degrades this shape to a `%% %` line
    // comment instead, which is a separate divergence about how a below-column
    // body is recognized at all, not about the column it is written at.
    assert_eq!(
        to_carve("- a\n  %%%\n x\n  %%%\n"),
        "- a\n  %%%\n  x\n  %%%\n"
    );
}

#[test]
fn a_tab_straddling_the_fence_column_is_consumed_whole() {
    // The strip is not residual-aware, so a tab that straddles the fence column
    // goes entirely: a body written `\tx` under a fence at column 1 lands at the
    // fence's column, not three columns inside it.
    //
    // That reads like a rounding bug and is the fix a reviewer proposes, so it
    // is pinned rather than left to be "corrected". carve-js and carve-php both
    // write `  x` here. Re-emitting the unconsumed columns as spaces - what
    // `slice_columns`'s `keep_residual` does for sub-list markers - makes
    // carve-rs write `     x` alone, trading a fixed divergence for a new one.
    // Measured on all three engines, not reasoned about.
    assert_eq!(
        to_carve("- a\n %%% n\n\tx\n %%%\n tail\n"),
        "- a\n  %%%\n  n\n  x\n  %%%\n  tail\n"
    );
}

#[test]
fn the_content_the_body_carries_is_unchanged() {
    // The fence is a comment: whatever the columns, none of it renders. Pinned
    // so a repair of the indentation cannot start dropping the body.
    assert_eq!(
        to_html("- a\n %%% n\n x\n %%%\n tail\n"),
        to_html(&to_carve("- a\n %%% n\n x\n %%%\n tail\n"))
    );
    assert!(!to_html("- a\n %%% n\n x\n %%%\n tail\n").contains('x'));
}

#[test]
fn fmt_still_settles_on_the_first_pass() {
    // Idempotence held before the fix at the wrong column, so it proves nothing
    // on its own - but a fix that broke it would be worse than the bug.
    for src in [
        "- a\n %%% n\n x\n %%%\n tail\n",
        "- - a\n %%% c\n x\n %%%\n b\n",
        "%%%\n  x\n%%%\n",
    ] {
        let once = to_carve(src);
        assert_eq!(to_carve(&once), once, "second pass differs for {src:?}");
    }
}
