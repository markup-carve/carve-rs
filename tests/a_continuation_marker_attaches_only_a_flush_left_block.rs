//! A CONTINUATION MARKER ATTACHES ONLY A FLUSH-LEFT BLOCK
//! (markup-carve/carve#1436).
//!
//! §17 L3: the marker attaches a block that begins at DOCUMENT COLUMN 0 and
//! nothing else. A line at any other column is not attached at all - it falls
//! through to the ordinary column rules, which give it to whichever container
//! its own column names, exactly as if the marker line had been a comment.
//!
//! This engine had the loose reading twice over: it attached a line at any
//! indentation, AND it dropped the column-0 line the marker actually reaches
//! for, because a flush-left line is a dedent that normally ends an item.

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn a_nested_marker_attaches_the_column_zero_block() {
    // The block the marker names is written flush left, which the collector
    // used to treat as the dedent that ends the item - so nothing was attached
    // and the line became a document paragraph.
    assert_eq!(
        html("* * +\nx\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>x</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_line_at_the_outer_content_column_belongs_to_the_outer_item() {
    // Column 2 is the OUTER item's content column. The marker never reached it,
    // so the ordinary rules place it - which is what made outer content
    // unwritable after a nested marker.
    assert_eq!(
        html("* * +\n  x\n"),
        "<ul>\n  <li>\n    <ul>\n      <li></li>\n    </ul>\n    x\n  </li>\n</ul>"
    );
}

#[test]
fn a_line_below_every_content_column_reaches_no_container() {
    // Column 1 is below the outer item's content column and above the base, so
    // no container names it. An empty first-block item leaves no open paragraph
    // for it to fold into either.
    assert_eq!(
        html("* * +\n x\n"),
        "<ul>\n  <li>\n    <ul>\n      <li></li>\n    </ul>\n  </li>\n</ul>\n<p>x</p>"
    );
}

#[test]
fn the_single_level_form_is_unaffected() {
    // Under `- +` a line at column 2 is the item's OWN content column, so the
    // ordinary rules put it in the item whether the marker reached it or not.
    // That is why the loose reading went unnoticed for so long.
    assert_eq!(html("- +\nx\n"), "<ul>\n  <li>x</li>\n</ul>");
    assert_eq!(html("- +\n  x\n"), "<ul>\n  <li>x</li>\n</ul>");
}

#[test]
fn a_marker_that_cannot_reach_column_zero_attaches_nothing() {
    // Inside an item body the collector is stripping, document column 0 is
    // unreachable by construction, so this marker reaches for a block that
    // cannot be written here. It renders nothing and does not break the lazy
    // fold - the same document without the marker line gives the same tree.
    let with_marker = html("- a\n  - b\n  +\n  c\n");
    let without_marker = html("- a\n  - b\n  c\n");
    assert_eq!(with_marker, without_marker);
    assert_eq!(
        with_marker,
        "<ul>\n  <li>a\n    <ul>\n      <li>b\nc</li>\n    </ul>\n  </li>\n</ul>"
    );
}
