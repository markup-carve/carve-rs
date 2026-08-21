//! A malformed colon fence inside an OPEN container does not absorb that
//! container's closer.
//!
//! PART 9 §12 says a `:::` line that fails the opener test is ordinary paragraph
//! text, "and from that point the paragraph absorbs the next fence-shaped line
//! as text too, INSTEAD of being interrupted by it".
//!
//! The rule is about a paragraph absorbing a would-be OPENER. A CLOSER is not a
//! would-be opener: it belongs to the block that opened it, and that block was
//! opened BEFORE the malformed line was ever read, so no absorption reaches it.
//! Being "interrupted by" a line and being "closed by" one are different
//! relations, and §12 speaks only about the first.
//!
//! carve-rs was alone in reading it the other way. The container's closer
//! disappeared into the body, nothing closed the block afterwards, and the rest
//! of the document went inside it - a four-line input swallowed its own tail,
//! and a longer one swallowed everything (carve-rs#719).
//!
//! The spec oracle, carve-js and carve-php all keep the closer, so this needed
//! no ruling. Measured against the oracle at spec cf5c03a.
//!
//! Nothing pinned this shape: the full corpus renders byte-identically across
//! all six targets before and after the fix, which is the class catalogued in
//! markup-carve/carve#755.
//!
//! Two walks implement the container's extent - `collect_colon_container_body`,
//! which collects the body, and `find_colon_container_end`, which measures it
//! for the caller. Both carried the same state and both had to move; a fix to
//! one alone makes the two disagree about where the container ends.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn a_malformed_fence_does_not_absorb_the_containers_closer() {
    // The ticket's document. `:::oops` has no space before its type word, so it
    // fails the opener test and is paragraph text. The `:::` below it is the
    // admonition's own closer and stays that way, so `tail` is outside.
    assert_eq!(
        html("::: note\n:::oops\n:::\ntail\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>:::oops</p>\n</aside>\n<p>tail</p>"
    );
}

#[test]
fn the_rule_is_not_width_tagged() {
    // §12's absorption is explicitly NOT width-tagged, so the rule that limits
    // it must not become width-tagged either. A four-colon malformed opener
    // under a three-colon container behaves exactly like a three-colon one: it
    // is prose, and it still does not take the closer.
    assert_eq!(
        html("::: note\n::::oops\n:::\ntail\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>::::oops</p>\n</aside>\n<p>tail</p>"
    );
}

#[test]
fn it_holds_one_level_in_inside_a_list_item() {
    // The ticket's second acceptance shape. The container is collected through
    // the item's body rather than at top level, which is a different entry into
    // the same walk.
    assert_eq!(
        html("- ::: note\n  :::oops\n  :::\n  tail\n"),
        "<ul>\n  <li>\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>:::oops</p>\n    </aside>\n    tail\n  </li>\n</ul>"
    );
}

#[test]
fn it_holds_inside_a_block_quote() {
    // The third container flavor, and the one whose own absorption rule
    // (`absorbed_colon_fence_in_a_quote.rs`) is adjacent to this one. The quote
    // does not change what the admonition's closer is.
    assert_eq!(
        html("> ::: note\n> :::oops\n> :::\n> tail\n"),
        "<blockquote>\n  <aside class=\"admonition note\" aria-label=\"Note\">\n    <p>:::oops</p>\n  </aside>\n  <p>tail</p>\n</blockquote>"
    );
}

#[test]
fn an_inner_containers_closer_is_not_absorbable_either() {
    // The stack, not just its bottom. `::: note` is nested inside `:::: outer`,
    // and the malformed line sits in the INNER one - so the closer that must
    // survive is the inner container's, with the outer one still open behind it.
    // A fix that only protected the outermost closer passes every test above.
    assert_eq!(
        html(":::: outer\n::: note\n:::oops\n:::\n::::\ntail\n"),
        "<div class=\"outer\">\n  <aside class=\"admonition note\" aria-label=\"Note\">\n    <p>:::oops</p>\n  </aside>\n</div>\n<p>tail</p>"
    );
}

#[test]
fn the_malformed_line_may_be_the_containers_first_line() {
    // No prose before it: the container opens with a paragraph that BEGINS with
    // fence-shaped text. This is the shape that goes wrong on its own in the
    // sibling absorption fixtures, so it is not redundant with the base case.
    assert_eq!(
        html("::: note\n:::oops\n:::\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>:::oops</p>\n</aside>"
    );
}

#[test]
fn the_malformed_line_may_follow_real_body_text() {
    // The other order: a real paragraph first, then the malformed fence. The
    // paragraph is already open when the malformed line arrives, which is the
    // arrangement §12 is actually written about.
    assert_eq!(
        html("::: note\nbody\n:::oops\n:::\ntail\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>body\n:::oops</p>\n</aside>\n<p>tail</p>"
    );
}

#[test]
fn two_malformed_fences_still_do_not_take_the_closer() {
    // Absorption is not a counter. Two failing openers in a row leave the
    // closer exactly as reachable as one does.
    assert_eq!(
        html("::: note\n:::oops\n:::bad\n:::\ntail\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>:::oops\n:::bad</p>\n</aside>\n<p>tail</p>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS - shapes this fix must NOT move
// ---------------------------------------------------------------------------

#[test]
fn control_a_container_with_no_malformed_fence_is_unchanged() {
    // CONTROL. The same four lines with ordinary body text: the closer closed
    // before this fix too, and must still.
    assert_eq!(
        html("::: note\nbody\n:::\ntail\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>body</p>\n</aside>\n<p>tail</p>"
    );
}

#[test]
fn control_the_top_level_paragraph_still_absorbs() {
    // CONTROL, and the boundary of this fix. With NO container open there is no
    // closer to protect, so §12's absorption applies in full and the paragraph
    // swallows the fence-shaped line and the one after it. This is the shape the
    // fix must leave alone - it is where absorption is correct.
    assert_eq!(
        html(":::note\nbody\n:::\ntail\n"),
        "<p>:::note\nbody\n:::\ntail</p>"
    );
}

#[test]
fn control_a_blank_line_ends_the_paragraph_as_before() {
    // CONTROL. The blank closes the absorbing paragraph, so the `:::` below is
    // reached as the container's closer by the ordinary route. Unchanged, and
    // here because it is the shape a reader checks the fix against.
    assert_eq!(
        html("::: note\n:::oops\n\n:::\ntail\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>:::oops</p>\n</aside>\n<p>tail</p>"
    );
}

#[test]
fn control_absorption_still_works_inside_the_container() {
    // CONTROL, and the assertion that keeps this fix from reading as "closers
    // always win, absorption deleted". A bare `::::` under a three-colon
    // container is NOT that container's closer, so nothing protects it: the
    // paragraph absorbs it under §12, and having absorbed it, absorbs the `:::`
    // after it as well. Both lines are prose and the admonition runs to end of
    // input.
    //
    // So absorption is intact INSIDE the container - what the fix removed is
    // only its reach over the one line that closes the container. Matches the
    // oracle byte-for-byte.
    assert_eq!(
        html("::: note\n:::oops\n::::\n:::\n"),
        "<aside class=\"admonition note\" aria-label=\"Note\">\n  <p>:::oops\n::::\n:::</p>\n</aside>"
    );
}
