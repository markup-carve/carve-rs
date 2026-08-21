//! A colon fence that fails PART 9 §12's opener test opens nothing, so the
//! item's paragraph is still open and PART 1 S4 folds the flush-left line below
//! it into that paragraph.
//!
//! The lead-paragraph collector had an explicit exception for this shape: if the
//! paragraph held an invalid colon fence AND ended in a bare one, a flush-left
//! line broke out and ended the item. That is the answer the corpus used to
//! pin and no longer does (carve#891, carve#895), and it was the only place in
//! this engine that decided the question on the SHAPE of a line rather than on
//! whether a block had been opened.
//!
//! The rest of the machinery was already right, which is why deleting the
//! exception is the whole fix: `suppress_colon_interrupt` a few lines below it
//! already implements §12's absorption for the same paragraph, so `:::note`
//! never interrupts and the bare `:::` under it is absorbed as text.
//!
//! The neighbouring shapes are here rather than in three other files because
//! they are consequences of the one reading, and an implementation can get the
//! first one right for the wrong reason.

fn html(src: &str) -> String {
    carve::to_html(src).trim().to_string()
}

#[test]
fn the_flush_left_line_folds_because_the_fence_opened_nothing() {
    assert_eq!(
        html("- item\n  :::note\n  body\n  :::\ntail\n"),
        "<ul>\n  <li>item\n:::note\nbody\n:::\ntail</li>\n</ul>"
    );
}

#[test]
fn a_valid_opener_closes_the_item_instead() {
    // The contrast that makes the rule legible: one space between the fence and
    // the type word decides which answer the same five lines get. A real
    // admonition opens, its closer completes it, and a closed block leaves no
    // open paragraph - so `tail` ends the item.
    assert_eq!(
        html("- item\n  ::: note\n  body\n  :::\ntail\n"),
        "<ul>\n  <li>item\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>body</p>\n    </aside>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn a_lazy_line_one_column_in_folds_too() {
    assert_eq!(
        html("- item\n  :::note\n  body\n  :::\n tail\n"),
        "<ul>\n  <li>item\n:::note\nbody\n:::\ntail</li>\n</ul>"
    );
}

#[test]
fn the_malformed_fence_may_be_the_paragraphs_first_line() {
    // `- :::note` puts it on the marker line, so the item opens with a paragraph
    // that BEGINS with fence-shaped text. This was wrong here before the fix
    // even though the column-1 shape above was right - the two went through
    // different branches, which is the shape of defect a single corpus document
    // does not catch.
    assert_eq!(
        html("- :::note\n  body\n  :::\ntail\n"),
        "<ul>\n  <li>:::note\nbody\n:::\ntail</li>\n</ul>"
    );
}

#[test]
fn it_folds_inside_a_block_quote() {
    // The quote's prefix matches on the lazy line but the item's indentation
    // does not - the partial match S4 is written for.
    assert_eq!(
        html("> - item\n>   :::note\n>   body\n>   :::\n> tail\n"),
        "<blockquote>\n  <ul>\n    <li>item\n:::note\nbody\n:::\ntail</li>\n  </ul>\n</blockquote>"
    );
}

#[test]
fn a_wider_bare_fence_is_absorbed_too() {
    // §12: "the absorption is not width-tagged". A malformed opener has no
    // length to match against, so after `:::note` a `::::` is absorbed as
    // readily as a `:::`.
    assert_eq!(
        html("- item\n  :::note\n  body\n  ::::\ntail\n"),
        "<ul>\n  <li>item\n:::note\nbody\n::::\ntail</li>\n</ul>"
    );
}

#[test]
fn a_valid_opener_after_the_malformed_one_still_interrupts() {
    // Absorption covers a BARE run only. `::: note` opens its block, the `:::`
    // below is that block's closer, and a closed block leaves no open paragraph.
    assert_eq!(
        html("- item\n  :::note\n  ::: note\n  body\n  :::\ntail\n"),
        "<ul>\n  <li>item\n:::note\n    <aside class=\"admonition note\" aria-label=\"Note\">\n      <p>body</p>\n    </aside>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn a_heading_between_them_ends_the_absorbing_paragraph() {
    // Absorption belongs to ONE paragraph. A heading ends it, so the `:::`
    // below opens a real div and `tail` ends the item - the same answer the
    // three lines get at the top level.
    assert_eq!(
        html("- item\n  :::note\n  # h\n  :::\ntail\n"),
        "<ul>\n  <li>item\n:::note\n    <h1 id=\"h\">h</h1>\n    <div>\n    </div>\n  </li>\n</ul>\n<p>tail</p>"
    );
}

#[test]
fn a_blank_line_ends_the_absorption() {
    // The other boundary: the paragraph that was absorbing ends at the blank,
    // so the `:::` below it IS an opener.
    assert_eq!(
        html("- item\n  :::note\n\n  :::\ntail\n"),
        "<ul>\n  <li>item\n:::note\n    <div>\n    </div>\n  </li>\n</ul>\n<p>tail</p>"
    );
}
