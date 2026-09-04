//! A DEGRADED COMMENT FENCE LEAVES A LAZY FOLLOWER WHERE THE LINE FORM DOES
//! (markup-carve/carve#1920, markup-carve/carve-rs#1543).
//!
//! PART 9 §28 degrades an unterminated `%%%` to an ordinary line comment. That
//! degradation is TOTAL, so the two spellings answer the follower alike: a line
//! in the LAZY band - strictly between the document column and the item's
//! content column - folds into the item, exactly as it does under `%%`.
//!
//! WHAT STILL ENDS THE ITEM is a follower at the DOCUMENT column, which corpus
//! 443 pins and `chunk_ends_in_degraded_comment_fence` decides. This file used
//! to claim the whole band below the content column, which is what split the
//! two spellings §28 equates.
//!
//! THIS FILE REVERSES ITS OWN EARLIER RULING. It was written for
//! markup-carve/carve-rs#1512 against the executable spec at carve `86569bd`,
//! where a below-column follower left the item. markup-carve/carve#1914, merged
//! as #1920, ruled the other way and corpus section 446 pins it, where carve-rs
//! stood alone on three rows.
//! Every expectation below was re-derived by running `scripts/spec/layout.mjs`
//! plus `html.mjs` at carve `4296257a` on the document itself.
//!
//! ONLY AT THE OUTERMOST FRAME. A frame one level in still ends: the enclosing
//! collection CARRIED the answer that the line reached an ancestor and not this
//! container, and `carried_reach` is that answer. Deleting the break outright
//! instead ended nothing anywhere and kept a nested item's follower inside it,
//! which the spec answers the other way - `a_nested_frame_still_ends` pins it.
//!
//! MEASURED, NOT ASSUMED. The same 1312-document grid shape the earlier ticket
//! used - l(ist)/q(uote) prefixes to depth three, a comment at the innermost
//! content column in four spellings (`%%`, `%%%`, `%%%%`, `%%% t`), one follower
//! at every column from 0 to that column, in four kinds. Against the executable
//! spec: 76 diverging before, 52 after, 24 fixed, 0 newly broken.
//!
//! THE ORACLE IS THE SPEC, NOT carve-js, on this band: carve-js diverges from
//! the executable spec on 56 of the same 1312 documents, including both
//! spellings of the nested shape above.

use carve::{to_html, to_html_with_options, Options};

/// The #908 guard: the facade and the position-tracking path must agree. This
/// rule lives in BOTH indented-block collectors, and only this assertion says
/// whether the two were changed alike.
fn both_paths(src: &str) -> String {
    let facade = to_html(src);
    let authoritative = to_html_with_options(src, &Options::default().with_positions(true));
    assert_eq!(
        facade, authoritative,
        "the library path and the position-tracking path disagree on {src:?}"
    );
    facade
}

#[test]
fn the_reported_document_keeps_the_follower() {
    assert_eq!(
        both_paths("- a\n  %%% x\n # h\n"),
        "<ul>\n  <li>a\n    # h\n  </li>\n</ul>",
    );
}

#[test]
fn every_following_line_kind_stays_in_the_item() {
    // The follower is at column 1, inside the item, so `# h` and `---` are
    // paragraph text there and `- y` opens the item's own sublist.
    for (src, expected) in [
        ("- x\n  %%% x\n b\n", "<ul>\n  <li>x\n    b\n  </li>\n</ul>"),
        (
            "- x\n  %%% x\n # h\n",
            "<ul>\n  <li>x\n    # h\n  </li>\n</ul>",
        ),
        (
            "- x\n  %%% x\n ---\n",
            "<ul>\n  <li>x\n    \u{2014}\n  </li>\n</ul>",
        ),
        (
            "- x\n  %%% x\n - y\n",
            "<ul>\n  <li>x\n    <ul>\n      <li>y</li>\n    </ul>\n  </li>\n</ul>",
        ),
    ] {
        assert_eq!(both_paths(src), expected, "{src:?}");
    }
}

#[test]
fn a_bare_fence_answers_like_one_carrying_text() {
    // §28 degrades on the absence of a CLOSER. Neither the width nor whether
    // the opener carries text is part of it - and the bare form is corpus 446's
    // own spelling, so this is the row the section pins.
    for src in [
        "- x\n  %%%\n y\n",
        "- x\n  %%%% x\n y\n",
        "- x\n  %%%%\n y\n",
    ] {
        assert_eq!(
            both_paths(src),
            "<ul>\n  <li>x\n    y\n  </li>\n</ul>",
            "{src:?}"
        );
    }
}

#[test]
fn the_ordered_and_wide_marker_rows_answer_the_same_way() {
    // Corpus 446 rows 5 and 6: the content column is 3 and 4 rather than 2, and
    // the follower is one column below it in each.
    assert_eq!(
        both_paths("1. x\n   %%%\n  y\n"),
        "<ol>\n  <li>x\n    y\n  </li>\n</ol>",
    );
    assert_eq!(
        both_paths("-   x\n    %%%\n   y\n"),
        "<ul>\n  <li>x\n    y\n  </li>\n</ul>",
    );
}

#[test]
fn a_quote_host_answers_the_same_way() {
    assert_eq!(
        both_paths("> - x\n>   %%% x\n>  # h\n"),
        "<blockquote>\n  <ul>\n    <li>x\n      # h\n    </li>\n  </ul>\n</blockquote>",
    );
}

#[test]
fn a_document_column_follower_still_ends_the_item() {
    // WHAT DID NOT MOVE, and the reason the fix is the lazy band and not the
    // degradation itself. Corpus 443: at the document column the follower is
    // not the item's, whatever the comment spelling above it.
    assert_eq!(
        both_paths("- x\n  %%%\ny\n"),
        "<ul>\n  <li>x</li>\n</ul>\n<p>y</p>",
    );
    assert_eq!(
        both_paths("- x\n  %% z\ny\n"),
        "<ul>\n  <li>x</li>\n</ul>\n<p>y</p>",
    );
}

#[test]
fn a_blank_line_still_ends_the_item() {
    // A BLANK ENDS THE PARAGRAPH THE FOLLOWER WOULD HAVE FOLDED INTO, so there
    // is no lazy line left to keep. Unchanged, and it is what says the rule is
    // about the fold rather than about the comment.
    assert_eq!(
        both_paths("- x\n  %%% x\n\n b\n"),
        "<ul>\n  <li>x</li>\n</ul>\n<p>b</p>",
    );
}

#[test]
fn an_at_column_line_after_the_fence_is_still_the_items() {
    // A line AT the column is item content, and the line under THAT one folds
    // into the paragraph it opened.
    assert_eq!(
        both_paths("- x\n  %%% x\n  # h\n"),
        "<ul>\n  <li>x\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>",
    );
    assert_eq!(
        both_paths("- x\n  %%% x\n  b\n c\n"),
        "<ul>\n  <li>x\n    b\nc\n  </li>\n</ul>",
    );
}

#[test]
fn a_line_comment_leaves_the_item_open() {
    // THE FIRST CONTROL, and now the shape the fence is made to match rather
    // than the one it is distinguished from.
    assert_eq!(
        both_paths("- x\n  %% x\n b\n"),
        "<ul>\n  <li>x\n    b\n  </li>\n</ul>",
    );
}

#[test]
fn a_terminated_fence_leaves_the_item_open() {
    // THE SECOND CONTROL. With a closer it is a real span, not a degraded one.
    assert_eq!(
        both_paths("- x\n  %%% c\n  %%%\n b\n"),
        "<ul>\n  <li>x\n    b\n  </li>\n</ul>",
    );
}

#[test]
fn a_fence_below_the_content_column_ends_nothing() {
    // THE THIRD CONTROL. Written below the column the fence reached no
    // container, so it is lazy paragraph text and ends nothing.
    assert_eq!(
        both_paths("- x\n %%% x\n b\n"),
        "<ul>\n  <li>x\n    b\n  </li>\n</ul>",
    );
}

/// A FRAME ONE LEVEL IN STILL ENDS, and this is the row that says the rule is
/// scoped to the outermost frame rather than to the fence. The fence sits at the
/// INNER item's content column and the follower reached an ancestor, so the
/// inner item ends and the line is the outer item's text.
///
/// It is the control a first attempt failed: deleting the break outright fixed
/// corpus 446 and moved six documents of this shape the wrong way. carve-js
/// keeps the follower in the inner item here and the executable spec does not.
#[test]
fn a_nested_frame_still_ends() {
    const INNER_ENDS: &str =
        "<ul>\n  <li>\n    <ul>\n      <li>x</li>\n    </ul>\n    # h\n  </li>\n</ul>";
    assert_eq!(both_paths("- - x\n    %%% x\n # h\n"), INNER_ENDS);
    assert_eq!(both_paths("- - x\n    %%%\n # h\n"), INNER_ENDS);
    assert_eq!(
        both_paths("- - x\n    %%%\n ---\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>x</li>\n    </ul>\n    \u{2014}\n  </li>\n</ul>",
    );
}

/// A NESTED FOLLOWER THAT REACHES THE OUTER ITEM is that item's content, so it
/// is a heading there. The other side of the same carried answer.
#[test]
fn a_nested_follower_reaching_the_outer_item_is_a_heading_there() {
    assert_eq!(
        both_paths("- - x\n    %%%\n   # h\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>x</li>\n    </ul>\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>",
    );
}
