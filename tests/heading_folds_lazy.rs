//! A heading folds trailing flush-left plain text as continuation (PART 2
//! multi-line headings) no matter how deeply the heading is nested. carve-rs
//! previously let the flush-left line escape to a top-level paragraph when the
//! heading was an indented block, or the item's own block preceded by a blank,
//! or the tail of a nested sub-list. All now fold into the heading, matching
//! carve-php (carve#326).

#[test]
fn indented_item_heading_after_blank_folds_lazy() {
    assert_eq!(
        carve::to_html("- text\n\n  # N\nlazy\n"),
        "<ul>\n  <li>text\n    <h1 id=\"N-lazy\">N\nlazy</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn nested_marker_line_heading_folds_lazy() {
    assert_eq!(
        carve::to_html("- a\n  - # N\nlazy\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>\n        <h1 id=\"N-lazy\">N\nlazy</h1>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn deeply_nested_indented_heading_folds_lazy() {
    assert_eq!(
        carve::to_html("- a\n  - b\n    # N\nlazy\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <h1 id=\"N-lazy\">N\nlazy</h1>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn heading_ending_a_definition_body_folds_lazy() {
    // A heading that ends a definition list's definition body also folds the
    // following flush-left line into it (the recursive check descends through
    // the definition list, not just plain lists).
    assert_eq!(
        carve::to_html("- one\n  :: term\n  :  # H\nlazy\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n      <dd>\n        <h1 id=\"H-lazy\">H\nlazy</h1>\n      </dd>\n    </dl>\n  </li>\n</ul>"
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
