//! A first-block marker may read a block at the opener's authored column or
//! document column zero. A later marker uses its open item's marker column.

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
        "<ul>\n  <li>\n    <ul>\n      <li>+\nx</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_line_at_the_outer_content_column_belongs_to_the_outer_item() {
    // Column 2 is the OUTER item's content column. The marker never reached it,
    // so the ordinary rules place it - which is what made outer content
    // unwritable after a nested marker.
    assert_eq!(
        html("* * +\n  x\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>x</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_line_below_every_content_column_reaches_no_container() {
    // Column 1 is below the outer item's content column and above the base, so
    // no container names it. An empty first-block item leaves no open paragraph
    // for it to fold into either.
    assert_eq!(
        html("* * +\n x\n"),
        "<ul>\n  <li>\n    <ul>\n      <li>+\nx</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn the_single_level_form_is_unaffected() {
    // Under `- +` a line at column 2 is the item's OWN content column, so the
    // ordinary rules put it in the item whether the marker reached it or not.
    // That is why the loose reading went unnoticed for so long.
    assert_eq!(html("- +\nx\n"), "<ul>\n  <li>x</li>\n</ul>");
    assert_eq!(html("- +\n  x\n"), "<ul>\n  <li>+\nx</li>\n</ul>");
}

#[test]
fn a_nested_marker_attaches_at_its_item_column() {
    let with_marker = html("- a\n  - b\n  +\n  c\n");
    assert_eq!(
        with_marker,
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        c\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}
