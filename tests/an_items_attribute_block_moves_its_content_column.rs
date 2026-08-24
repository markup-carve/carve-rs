//! Historical category-413 regression: marker attributes are now item metadata
//! and contribute zero to the content column (markup-carve/carve#1701). The
//! checkbox remains content and contributes zero as well; only the bare marker
//! and separator establish the column.

fn html(source: &str) -> String {
    carve::to_html(source)
}

#[test]
fn the_old_full_prefix_column_is_lazy_text() {
    assert_eq!(
        html("-{#k} [x] a\n      # h\n").trim(),
        "<ul>\n  <li id=\"k\"><input type=\"checkbox\" checked disabled aria-label=\"a # h\"> a\n\
         # h</li>\n</ul>"
    );
}

#[test]
fn a_continuation_at_the_bare_marker_column_is_inside() {
    assert_eq!(
        html("-{#k} [x] a\n  # h\n").trim(),
        "<ul>\n  <li id=\"k\"><input type=\"checkbox\" checked disabled aria-label=\"a\"> a\n    \
         <h1 id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

/// A sub-list reads the same bare-marker column through `marker_content_col`.
#[test]
fn a_sub_list_reads_the_same_column() {
    assert_eq!(
        html("-{#k} [x] a\n  - sub\n").trim(),
        "<ul>\n  <li id=\"k\"><input type=\"checkbox\" checked disabled aria-label=\"a\"> a\n    \
         <ul>\n      <li>sub</li>\n    </ul>\n  </li>\n</ul>"
    );
}

/// THE FIRST NEIGHBOUR A WRONG FIX BREAKS: without an attribute block the
/// column is the bullet's alone, so the checkbox still moves nothing and a
/// continuation at 2 is inside the item.
#[test]
fn a_plain_task_item_still_has_its_column_at_two() {
    assert_eq!(
        html("- [x] a\n  # h\n").trim(),
        "<ul>\n  <li><input type=\"checkbox\" checked disabled aria-label=\"a\"> a\n    \
         <h1 id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

/// Padding in front of the checkbox is not an attribute block and does not move
/// the column either: a fix that measured where the checkbox BEGINS rather than
/// counting the block would move this to 4, which is what carve-js does not do.
#[test]
fn padding_before_the_checkbox_still_moves_nothing() {
    let expected = "<ul>\n  <li><input type=\"checkbox\" disabled aria-label=\"item\"> item\n    \
                    <h1 id=\"H\">H</h1>\n  </li>\n</ul>";
    assert_eq!(html("-   [ ] item\n  # H\n").trim(), expected);
    assert_eq!(
        html("-   [ ] item\n    # H\n").trim(),
        "<ul>\n  <li><input type=\"checkbox\" disabled aria-label=\"item # H\"> item\n# H</li>\n</ul>"
    );
}

#[test]
fn a_plain_attributed_item_uses_column_two() {
    assert_eq!(
        html("-{#k} a\n  # h\n").trim(),
        "<ul>\n  <li id=\"k\">a\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

/// An ORDERED item's block counts the same way, and always did - the ordered
/// branch never carried the task constant. Pinned beside the bullet so a later
/// change to one is measured against the other.
#[test]
fn an_ordered_item_with_attributes_uses_its_bare_width() {
    assert_eq!(
        html("1.{#k} a\n   # h\n").trim(),
        "<ol>\n  <li id=\"k\">a\n    <h1 id=\"h\">h</h1>\n  </li>\n</ol>"
    );
}

/// THE LOOSENESS WALK READS THE COLUMN THROUGH ANOTHER DOOR. `marker_content_col`
/// is the second place the constant was spelled, and it answers for a SUB-LIST
/// rather than for the item loop: a blank line followed by a paragraph BELOW the
/// sub-list's content column is internal to the outer item and loosens it.
///
/// `after` sits at column 2 of the dedented sub-list. Under the constant the
/// sub-list's column was 2 as well, so the paragraph did not fall below it and
/// the outer item stayed TIGHT - carve-rs emitted bare text where carve-js
/// emitted paragraphs. The direct pairs above cannot reach this: they never
/// build a sub-list, so reverting that second site alone leaves every one of
/// them green.
#[test]
fn the_column_reaches_the_outer_items_looseness() {
    assert_eq!(
        html("- outer\n  -{#k} [x] inner\n\n  after\n").trim(),
        "<ul>\n  <li><p>outer</p>\n    <ul>\n      <li id=\"k\"><input type=\"checkbox\" checked disabled aria-label=\"inner\"> inner</li>\n    </ul>\n    <p>after</p>\n  </li>\n</ul>"
    );
}
