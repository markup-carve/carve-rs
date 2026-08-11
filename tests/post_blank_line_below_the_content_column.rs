//! After a blank line, a line must reach the item's content column to still
//! belong to it (PART 9 §24 C3, #578, corpus
//! `190-a-blank-after-a-comment-still-ends-the-item`).
//!
//! The collector applied that rule against the first COLLECTED block's indent,
//! and skipped it entirely when nothing had been collected yet. An item whose
//! content is all on the marker line - `- - a`, `- # H` - is exactly that case,
//! so a post-blank line below the content column was taken as part of the item
//! where every other engine ends the list.
//!
//! A comment made it worse rather than causing it: being invisible it may sit
//! below the content column, and taking ITS indent as the block indent lowered
//! the threshold under the content column even once the no-block case was
//! fixed.

use carve::to_html;

fn html(src: &str) -> String {
    to_html(src)
}

const LIST_THEN_B: &str =
    "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n  </li>\n</ul>\n<p>b</p>";

#[test]
fn a_post_blank_line_below_the_content_column_ends_the_list() {
    // Nothing is collected before the blank - the item's content is the
    // marker-line sub-list - so there was no block indent to compare against.
    assert_eq!(html("- - a\n\nb\n"), LIST_THEN_B);
}

#[test]
fn a_comment_before_the_blank_does_not_lower_the_threshold() {
    // The comment sits below the content column, which it may: it renders
    // nothing. Taking its indent as the block's would put the threshold at 1
    // and let `b` back in.
    assert_eq!(html("- - a\n  +\n  %% c\n\nb\n"), LIST_THEN_B);
}

#[test]
fn at_the_content_column_it_still_belongs_to_the_item() {
    // The control for over-reach. Two columns in, `b` reaches the outer item's
    // content column and is its second block - which is where the rule stops.
    assert_eq!(
        html("- - a\n\n  b\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>a</li>\n    </ul>\n    <p>b</p>\n  </li>\n</ul>"
    );
}

#[test]
fn the_single_level_form_was_always_right() {
    // The shape one level shallower, which never had the bug - kept so a repair
    // of the collector cannot quietly change it.
    assert_eq!(html("- a\n\nb\n"), "<ul>\n  <li>a</li>\n</ul>\n<p>b</p>");
}

#[test]
fn a_deeper_block_after_a_blank_still_belongs() {
    // The other direction: the threshold is capped at the content column, so a
    // block indented PAST it is still the item's (carve-rs#301).
    let out = html("- a\n\n  deep\n");
    assert!(
        out.contains("<li>"),
        "expected the block inside the item: {out}"
    );
    assert!(!out.contains("</ul>\n<p>"), "block escaped the list: {out}");
}

#[test]
fn a_comment_fence_body_does_not_lower_it_either() {
    // The body is as invisible as the delimiters around it, so skipping only
    // the `%%%` lines left a `hidden` line one column in to set the indent the
    // opener above it was skipped for.
    assert_eq!(
        html("- - a\n  +\n  %%%\n  hidden\n  %%%\n\nb\n"),
        LIST_THEN_B
    );
}
