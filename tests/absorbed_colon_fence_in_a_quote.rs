//! A colon fence a quoted paragraph has ABSORBED leaves that paragraph open, so
//! PART 1 S4 folds the flush-left line below the quote into it.
//!
//! PART 9 §12: a `:::` line that fails the opener test is ordinary paragraph
//! text, "and from that point the paragraph absorbs the next fence-shaped line
//! as text too, INSTEAD of being interrupted by it". PART 1 S4 then folds a
//! partial-match line into the innermost container holding an OPEN paragraph.
//!
//! The block quote collected the absorbed line into its body, where the nested
//! parse folded it into the paragraph correctly -- but the collector's own
//! `para_open` flag was computed from the line's SHAPE, so a bare `:::` set it
//! false and the quote ended one line early. The quote and its own body
//! disagreed about whether a paragraph was open (carve-rs#727).
//!
//! The item form of the same rule was fixed by carve-rs#718 and is pinned by
//! `absorbed_colon_fence_in_an_item.rs`; the quote form went through a different
//! branch and nothing pinned it. A sweep of every corpus `.crv` for an indented
//! unterminated `:::` with a later column-0 line returns only the item document,
//! so without this file the fix is invisible.
//!
//! Every absorbed-fence expectation below was measured byte-for-byte against
//! the executable-spec oracle at spec cf5c03a, carve-js 3d95e94 and carve-php
//! 876e312, which all three agree on. Three shapes here are boundary cases
//! where the oracle and the engines already differ; each says so at its
//! assertion, and this fix moves none of them.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn an_absorbed_bare_fence_leaves_the_quoted_paragraph_open() {
    // The quote analogue of the item fixture: `:::note` is not an opener (no
    // space before the type word), so the `:::` below it is absorbed and `tail`
    // folds into the paragraph the quote still holds open.
    assert_eq!(
        html("> quote\n> :::note\n> body\n> :::\ntail\n"),
        "<blockquote><p>quote\n:::note\nbody\n:::\ntail</p></blockquote>"
    );
}

#[test]
fn a_valid_opener_ends_the_quoted_paragraph_instead() {
    // CONTROL (holds before the fix as well). The contrast that makes the rule
    // legible: one space between the fence and the type word decides it. A real
    // admonition opens, and `tail` folds into ITS paragraph -- S4 folds into the
    // INNERMOST container holding an open paragraph, which is no longer the
    // quote's.
    assert_eq!(
        html("> quote\n> ::: note\n> body\ntail\n"),
        "<blockquote>\n  <p>quote</p>\n  <aside class=\"admonition note\">\n    <p>body\ntail</p>\n  </aside>\n</blockquote>"
    );
}

#[test]
fn the_absorption_is_not_width_tagged() {
    // §12: "THE ABSORPTION IS NOT WIDTH-TAGGED, unlike the closer rule it
    // resembles". A malformed opener has no length to match, so a `::::` under
    // `:::note` is absorbed as readily as a `:::` and the paragraph stays open.
    assert_eq!(
        html("> quote\n> :::note\n> body\n> ::::\ntail\n"),
        "<blockquote><p>quote\n:::note\nbody\n::::\ntail</p></blockquote>"
    );
    assert_eq!(
        html("> quote\n> :::note\n> body\n> ::::\n> :::\ntail\n"),
        "<blockquote><p>quote\n:::note\nbody\n::::\n:::\ntail</p></blockquote>"
    );
}

#[test]
fn the_malformed_fence_may_be_the_quotes_first_line() {
    // No preceding prose: the quote opens with a paragraph that BEGINS with
    // fence-shaped text. The item form of this went through its own branch and
    // was wrong there while the second-line form was right, which is the shape a
    // single fixture does not catch.
    assert_eq!(
        html("> :::note\n> :::\ntail\n"),
        "<blockquote><p>:::note\n:::\ntail</p></blockquote>"
    );
}

#[test]
fn an_attribute_shaped_opener_absorbs_too() {
    // §12's own example of a line that fails the opener test: the opener takes a
    // type word, so `::: {.x}` is prose. A different way to fail the same test,
    // reaching the same absorption.
    assert_eq!(
        html("> quote\n> ::: {.x}\n> :::\ntail\n"),
        "<blockquote><p>quote\n::: {.x}\n:::\ntail</p></blockquote>"
    );
}

#[test]
fn a_glued_label_shaped_opener_absorbs_too() {
    // `:::]` is fence-shaped and not a label (`[` opens one, `]` does not), so
    // it fails the opener test like `:::note` -- the carve-rs#496 shape, reached
    // through the quote.
    assert_eq!(
        html("> quote\n> :::]\n> :::\ntail\n"),
        "<blockquote><p>quote\n:::]\n:::\ntail</p></blockquote>"
    );
}

#[test]
fn quoted_prose_after_the_absorbed_fence_keeps_it_open() {
    // CONTROL (holds before the fix as well): plain quoted prose reopened the
    // paragraph even when the fence had wrongly closed it, so this shape was
    // already right. It is here because it is the shape a reader reaches for to
    // check the fix and would otherwise conclude from -- the absorbed line does
    // not end the paragraph for the lines after it either.
    assert_eq!(
        html("> quote\n> :::note\n> body\n> :::\n> more\ntail\n"),
        "<blockquote><p>quote\n:::note\nbody\n:::\nmore\ntail</p></blockquote>"
    );
}

#[test]
fn a_valid_opener_after_the_malformed_one_still_interrupts() {
    // Absorption covers a BARE run only: `::: note` under `:::note` opens its
    // admonition as usual, and `tail` folds into THAT paragraph rather than the
    // quote's. Matches the oracle.
    assert_eq!(
        html("> quote\n> :::note\n> ::: note\n> body\ntail\n"),
        "<blockquote>\n  <p>quote\n:::note</p>\n  <aside class=\"admonition note\">\n    <p>body\ntail</p>\n  </aside>\n</blockquote>"
    );
}

#[test]
fn a_valid_opener_after_the_malformed_one_leaves_nothing_open() {
    // The shape above with its `body` removed, and the one that decides what
    // `absorbed` may test. With no quoted line after it, the admonition holds
    // no paragraph, so nothing in the stack is open and `tail` is a top-level
    // block. Matches the oracle byte-for-byte.
    //
    // Widening the absorption test to "any colon-fence-shaped line" passes
    // every other case in this file, because a following `body` reopens the
    // paragraph and hides the difference. It fails here, which is why this case
    // is not redundant with the one above. carve-js 3d95e94 and carve-php
    // 876e312 both fold `tail` into the admonition instead; that divergence is
    // theirs and predates this fix, which does not move this shape.
    // markup-carve/carve#920 shape A has since ruled the answer below correct
    // and the two folding engines wrong.
    assert_eq!(
        html("> quote\n> :::note\n> ::: note\ntail\n"),
        "<blockquote>\n  <p>quote\n:::note</p>\n  <aside class=\"admonition note\">\n\n  </aside>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn a_valid_opener_with_no_malformed_one_before_it_is_unchanged() {
    // CONTROL, and the boundary of what this fix touches: with no absorbed
    // fence there is no absorption to carry, so the quoted opener ends the
    // window exactly as it did before. Matches the oracle; carve-js and
    // carve-php fold `tail` in here too, the same divergence as above, and
    // markup-carve/carve#920 shape A ruled against them.
    assert_eq!(
        html("> quote\n> ::: note\ntail\n"),
        "<blockquote>\n  <p>quote</p>\n  <aside class=\"admonition note\">\n\n  </aside>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn a_nested_quote_marker_does_not_take_the_line_out_of_the_outer_one() {
    // The absorbed line may open another quote level. `innermost` looks THROUGH
    // the further `>` before deciding, because S4 folds into the innermost open
    // paragraph "regardless of how many containers the line failed to match" --
    // so the outer quote keeps `tail` instead of ending before it.
    //
    // Before this fix carve-rs put `tail` outside the outer quote, which is the
    // one answer nobody produces. The expectation below is carve-js 3d95e94 and
    // carve-php 876e312 byte-for-byte. The oracle goes one step further and
    // folds `tail` into the div; this fix moves the shape toward that answer
    // without reaching it, and does not decide the remaining step.
    assert_eq!(
        html("> :::note\n> > :::\ntail\n"),
        "<blockquote>\n  <p>:::note</p>\n  <blockquote>\n    <div>\n    </div>\n  </blockquote>\n  <p>tail</p>\n</blockquote>"
    );
}

#[test]
fn a_blank_line_ends_the_absorption() {
    // Holds before the fix too, but it is not spare: it is what fails if the
    // new flag is never reset. The absorption belongs to ONE paragraph. The
    // blank closes it, so the `:::` below is a real opener and `tail` -- with no
    // open paragraph anywhere in the stack -- is a top-level block (S4, "NO OPEN
    // PARAGRAPH, NO LAZY LINE").
    //
    // Since decided: this is markup-carve/carve#920 shape C, and the ruling is
    // that S4 is read as written -- the empty div holds no open paragraph, so
    // the answer below is correct and the oracle's fold is not. All three
    // engines already agreed here; this fix does not move the shape.
    assert_eq!(
        html("> quote\n> :::note\n>\n> :::\ntail\n"),
        "<blockquote>\n  <p>quote\n:::note</p>\n  <div>\n  </div>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn a_heading_between_them_ends_the_absorbing_paragraph() {
    // The other boundary, and the other reset guard: a heading interrupts, so
    // the `:::` under it opens a real div and nothing is left open for `tail`
    // to fold into. Same recorded divergence as the blank-line case above.
    assert_eq!(
        html("> quote\n> :::note\n> # h\n> :::\ntail\n"),
        "<blockquote>\n  <p>quote\n:::note</p>\n  <h1 id=\"h\">h</h1>\n  <div>\n  </div>\n</blockquote>\n<p>tail</p>"
    );
}

#[test]
fn a_flush_left_fence_still_ends_the_quote() {
    // CONTROL. The absorption is about lines INSIDE the quote. A `:::` at column
    // 0 carries no quote marker, so it is a lazy-continuation candidate, and a
    // fence-shaped line interrupts there under the strict column-0 rule -- the
    // quote ends and the fence opens its own div. carve-js and carve-php agree;
    // the oracle absorbs it into the quoted paragraph instead. markup-carve/carve#920
    // shape B ruled the oracle wrong here: §12's absorption is written about a
    // paragraph's OWN lines, and this line is not one of the quote's. Untouched
    // by this fix.
    assert_eq!(
        html("> quote\n> :::note\n> body\n:::\n"),
        "<blockquote><p>quote\n:::note\nbody</p></blockquote>\n<div>\n</div>"
    );
}

#[test]
fn it_holds_at_quote_depth_two() {
    // S4 folds into the innermost open paragraph "regardless of how many
    // containers the line failed to match" (carve#506): `tail` matches neither
    // quote prefix and still folds into the inner one's paragraph.
    assert_eq!(
        html("> > quote\n> > :::note\n> > body\n> > :::\ntail\n"),
        "<blockquote>\n  <blockquote><p>quote\n:::note\nbody\n:::\ntail</p></blockquote>\n</blockquote>"
    );
}

#[test]
fn the_top_level_paragraph_was_already_right() {
    // CONTROL. The same five lines without the quote: this path (parse_paragraph
    // and its own `suppress_colon_interrupt`) already implemented §12, which is
    // why the defect was quote-only and why the fix spells the rule from the
    // same two helpers rather than a fourth time.
    assert_eq!(
        html(":::note\nbody\n:::\ntail\n"),
        "<p>:::note\nbody\n:::\ntail</p>"
    );
}
