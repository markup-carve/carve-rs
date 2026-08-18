//! A heading at an item's content column is a bounded block and leaves no open
//! paragraph. PART 1 S4 therefore closes that item before a flush-left line
//! (markup-carve/carve#1377), just as for a marker-line heading.
//!
//! THE MARKER-LINE HALF OF THIS FILE MOVED. markup-carve/carve#1280 ruled PART 1
//! S4 uniform - lazy continuation extends an OPEN PARAGRAPH and nothing else -
//! and a heading written as a marker's content leaves none, so `- # H` / `tail`
//! ends the item at any depth. What survives here is the CONTENT-COLUMN half,
//! which the clause leaves deliberately open because corpus
//! 75-list-nesting-and-looseness-4 pins the folding answer for it. A DEFINITION
//! BODY'S marker line answers the same way as a list item's since
//! carve-rs#1049, and the row below is what that changed here.

#[test]
fn indented_item_heading_after_blank_ends_the_item() {
    assert_eq!(
        carve::to_html("- text\n\n  # N\nlazy\n"),
        "<ul>\n  <li>text\n    <h1 id=\"N\">N</h1>\n  </li>\n</ul>\n<p>lazy</p>"
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
fn deeply_nested_indented_heading_closes_the_inner_item() {
    // Corpus 75-list-nesting-and-looseness-4: the outer item remains available.
    assert_eq!(
        carve::to_html("- a\n  - b\n    # N\nlazy\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <h1 id=\"N\">N</h1>\n      </li>\n    </ul>\n    lazy\n  </li>\n</ul>"
    );
}

#[test]
fn heading_on_a_definition_marker_line_leaves_no_outer_paragraph_either() {
    // `:  # H` leaves no paragraph open in the definition, and the definition
    // list has already interrupted the item's earlier prose. No container in
    // the open stack can therefore take the flush-left line (PART 1 S4).
    assert_eq!(
        carve::to_html("- one\n  :: term\n  :  # H\nlazy\n"),
        "<ul>\n  <li>one\n    <dl>\n      <dt>term</dt>\n      <dd>\n        <h1 id=\"H\">H</h1>\n      </dd>\n    </dl>\n  </li>\n</ul>\n<p>lazy</p>"
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
