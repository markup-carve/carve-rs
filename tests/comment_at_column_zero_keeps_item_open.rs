//! PART 9 section 24 C3 (markup-carve/carve#624): a comment is recognized at
//! any column, and being invisible it closes nothing.
//!
//! The sibling of `comment_below_content_column_keeps_item_open`, one column
//! further left. At column 0 a comment used to end the item AND the list: the
//! line under it became a top-level paragraph, a sibling marker started a
//! second list, and a trailing comment was hoisted out of the item it was
//! written in. Every shape below is measured against carve-js, carve-php and
//! the executable spec (markup-carve/carve-rs#562).

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src)
}

#[test]
fn a_column_zero_comment_keeps_the_item_open() {
    // `b` stays in the item, as a SECOND paragraph: the comment ends the
    // paragraph above it (section 10) without ending the item.
    assert_eq!(
        html("- a\n+\n%% c\n+\nb\n"),
        "<ul>\n  <li>a\n    b\n  </li>\n</ul>"
    );
}

#[test]
fn a_sibling_marker_after_the_comment_resumes_the_same_list() {
    // The list stays open too, so `- b` is a second ITEM rather than a second
    // list. This is the shape that shows the comment doing no structural work.
    assert_eq!(
        html("- a\n+\n%% c\n- b\n"),
        "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>"
    );
}

#[test]
fn text_after_a_second_comment_stays_in_the_item_as_well() {
    // Nothing about the rule is once-only: each comment is passed over and the
    // text under it keeps landing in the same item.
    assert_eq!(
        html("- a\n+\n%% c\n+\nb\n+\n%% d\n+\ne\n"),
        "<ul>\n  <li>a\n    b\n    e\n  </li>\n</ul>"
    );
}

#[test]
fn the_comment_renders_nothing_at_any_column() {
    for src in [
        "- a\n+\n%% c\n+\nb\n",
        "- a\n+\n %% c\n+\nb\n",
        "- a\n+\n  %% c\n+\nb\n",
    ] {
        assert!(!html(src).contains("%%"), "comment leaked for {src:?}");
    }
}

#[test]
fn a_blank_line_before_the_comment_still_ends_the_list() {
    // The first control. Past a blank the item is closed already and a comment
    // does not reopen it - so `b` is top-level, as in all three engines. Without
    // this the fix would read as "a comment keeps a list open forever".
    assert_eq!(
        html("- a\n\n%% c\n\nb\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<p>b</p>"
    );
}

#[test]
fn a_comment_fence_still_ends_the_list() {
    // The second control. `%%%` opens a multi-line block rather than being the
    // single invisible line the rule is about, and all three engines end the
    // list on one.
    assert_eq!(
        html("- a\n\n%%%\nc\n%%%\n\nb\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<p>b</p>"
    );
}

#[test]
fn a_visible_block_after_the_comment_still_ends_the_list() {
    // The third control: the comment does not make the item swallow whatever
    // follows. A heading under it is its own top-level block.
    let out = html("- a\n+\n%% c\n\n# H\n");
    assert!(
        out.contains("<h1>H</h1>"),
        "expected a top-level heading, got {out}"
    );
    assert!(
        !out.contains("<li>a\n"),
        "heading was pulled into the item: {out}"
    );
}
