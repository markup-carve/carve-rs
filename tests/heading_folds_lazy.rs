//! Trailing flush-left plain text after a heading stays INSIDE the item the
//! heading belongs to, no matter how deeply that heading is nested - carve-rs
//! once let it escape to a top-level paragraph (carve#326). What it no longer
//! does is fold into the heading itself: a heading ends at its newline (PART 2
//! SINGLE-LINE HEADINGS, carve#451), so the line lands beside the heading as
//! the item's own content. Matches carve-js / carve-php.
//!
//! THE MARKER-LINE HALF OF THIS FILE MOVED. markup-carve/carve#1280 ruled PART 1
//! S4 uniform - lazy continuation extends an OPEN PARAGRAPH and nothing else -
//! and a heading written as a marker's content leaves none, so `- # H` / `tail`
//! ends the item at any depth. What survives here is the CONTENT-COLUMN half,
//! which the clause leaves deliberately open because corpus
//! 75-list-nesting-and-looseness-4 pins the folding answer for it.

#[test]
fn indented_item_heading_after_blank_keeps_the_lazy_line_in_the_item() {
    assert_eq!(
        carve::to_html("- text\n\n  # N\nlazy\n"),
        "<ul>\n  <li>text\n    <h1 id=\"N\">N</h1>\n    lazy\n  </li>\n</ul>"
    );
}

#[test]
fn nested_marker_line_heading_ends_the_item_like_an_unnested_one() {
    // Depth is not a parameter (carve-rs#1025). This engine used to fold here
    // and END on `- - # H` / `tail`, two documents that differ only in how many
    // items wrap the heading. Ending is the answer the ruling settled on, so
    // this one moved to meet the other rather than the reverse.
    assert_eq!(
        carve::to_html("- a\n  - # N\nlazy\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>\n        <h1 id=\"N\">N</h1>\n      </li>\n    </ul>\n  </li>\n</ul>\n<p>lazy</p>"
    );
}

#[test]
fn deeply_nested_indented_heading_keeps_the_lazy_line_in_the_item() {
    // Corpus 73-list-nesting-and-looseness-4: the line is a paragraph in the
    // item, rendered unwrapped because the list is tight.
    assert_eq!(
        carve::to_html("- a\n  - b\n    # N\nlazy\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <h1 id=\"N\">N</h1>\n        lazy\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn heading_ending_a_definition_body_keeps_the_lazy_line_in_the_body() {
    // A heading that ends a definition list's definition body also keeps the
    // following flush-left line inside it (the recursive check descends through
    // the definition list, not just plain lists) -- as a paragraph, since the
    // definition body is loose.
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
