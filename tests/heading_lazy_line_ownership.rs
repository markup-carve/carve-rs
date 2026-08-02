//! A flush-left line after a nested heading stays INSIDE the item that heading
//! belongs to, no matter how deeply that item is nested. carve-rs previously let
//! it escape to a top-level paragraph when the heading was an indented block, or
//! the item's own block preceded by a blank, or the tail of a nested sub-list
//! (carve#326).
//!
//! Under SINGLE-LINE HEADINGS (PART 2) it no longer folds INTO the heading -- a
//! heading ends at the newline -- so it lands as the item's own content, which
//! renders unwrapped in a tight list. Ownership is what these cases pin; spec
//! corpus 73-list-nesting-and-looseness-4 pins the same shape.

#[test]
fn indented_item_heading_after_blank_keeps_lazy_in_the_item() {
    assert_eq!(
        carve::to_html("- text\n\n  # N\nlazy\n"),
        "<ul>\n  <li>text\n    <h1 id=\"N\">N</h1>\n    lazy\n  </li>\n</ul>"
    );
}

#[test]
fn nested_marker_line_heading_keeps_lazy_in_the_item() {
    assert_eq!(
        carve::to_html("- a\n  - # N\nlazy\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>\n        <h1 id=\"N\">N</h1>\n        lazy\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn deeply_nested_indented_heading_keeps_lazy_in_the_item() {
    // The id is `N`, not `N-lazy`: it comes from the heading line alone, which
    // is the corruption single-line headings removed.
    assert_eq!(
        carve::to_html("- a\n  - b\n    # N\nlazy\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <h1 id=\"N\">N</h1>\n        lazy\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn heading_ending_a_definition_body_keeps_lazy_in_the_body() {
    // The line still belongs to the definition body (the recursive check
    // descends through the definition list, not just plain lists); it is now a
    // paragraph there rather than heading text.
    assert_eq!(
        carve::to_html("- one\n  :: term\n  :  # H\nlazy\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n      <dd>\n        <h1 id=\"H\">H</h1>\n        <p>lazy</p>\n      </dd>\n    </dl>\n  </li>\n</ul>"
    );
}

#[test]
fn blank_after_heading_still_ends_it() {
    // A blank line closes the heading; the following text is a separate block.
    assert_eq!(
        carve::to_html("- a\n  - # N\n\nsep\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>\n        <h1 id=\"N\">N</h1>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>sep</p>"
    );
}

#[test]
fn caption_ends_the_item_rather_than_folding_into_the_heading() {
    // A caption (`^ …`) is a heading/figure terminator, so it ends the item's
    // lazy continuation instead of folding into the heading; it becomes its own
    // top-level block, matching carve-js / carve-php.
    assert_eq!(
        carve::to_html("- text\n\n  # H\n^ cap\n"),
        "<ul>\n  <li>text\n    <h1 id=\"H\">H</h1>\n  </li>\n</ul>\n<p>^ cap</p>"
    );
}

#[test]
fn caption_ends_a_plain_paragraph_item_too() {
    assert_eq!(
        carve::to_html("- text\n^ cap\n"),
        "<ul>\n  <li>text</li>\n</ul>\n<p>^ cap</p>"
    );
}
