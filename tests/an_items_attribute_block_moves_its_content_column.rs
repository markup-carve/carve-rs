//! AN ITEM'S ATTRIBUTE BLOCK MOVES ITS CONTENT COLUMN; ITS CHECKBOX DOES NOT.
//!
//! THE GRAMMAR SAYS WHAT THE BLOCK BINDS TO, so this is not an inference from a
//! neighbouring ruling. PART 9 §15 A8: "a `-{…} text` with no space after the
//! marker attributes the LIST ITEM", and `docs/divergence-from-djot.md` §17
//! states it outright - "the attribute block binds to the MARKER". Part of the
//! marker counts toward the marker's width, so `-{#k} [x] a` is the marker
//! `-{#k} `, six wide, and then the checkbox: the item's content column is 6.
//!
//! Djot has no such construct to appeal to. There `-{#k} [x] item` is a
//! PARAGRAPH - the `{#k}` is an inline attribute on a literal `-`, because a
//! bullet needs its separator first - and djot attributes a list through a
//! preceding attribute line that attaches to the LIST rather than the item.
//! §17 records the divergence as a deliberate extension.
//!
//! markup-carve/carve#1690 is the other half and still holds: a task item's
//! `[x] ` is CONTENT, so it does not move the column. The two together are why
//! the column is the bullet plus the block and nothing else.
//!
//! This engine read 2, the bare bullet width, which treats the block as though
//! it were not there and lands INSIDE it. A content column pointing into the
//! marker is not a content column - A8 notes the marker still needs content of
//! its own - so the two readings were never symmetric alternatives
//! (markup-carve/carve#1692, carve-rs#1372).
//!
//! BOTH SPELLINGS ARE PINNED HERE, not one. Before this fix each engine read
//! exactly one of the two as a continuation and they disagreed about which, so
//! a test covering a single column passes on the wrong engine: at 6 this engine
//! answered lazy text where carve-js answered a heading, and at 2 it answered a
//! heading where carve-js answered lazy text. Reverting the fix swaps them back,
//! which is what makes the pair discriminating and either one alone useless.
//!
//! Every expectation below was measured against carve-js at its `main` before
//! being written here. The corpus pins the same four shapes as category 413.

fn html(source: &str) -> String {
    carve::to_html(source)
}

/// The attribute block counts, so column 6 is where the item's content is and a
/// heading written there opens inside the item.
#[test]
fn a_continuation_at_the_attributed_content_column_is_inside_the_item() {
    assert_eq!(
        html("-{#k} [x] a\n      # h\n").trim(),
        "<ul>\n  <li id=\"k\"><input type=\"checkbox\" checked disabled aria-label=\"a\"> a\n    \
         <h1 id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

/// The other spelling. Column 2 is where the bare-bullet reading put the
/// column; it is below the real one, so the line is lazy paragraph text and the
/// `#` survives literally.
#[test]
fn a_continuation_below_it_is_lazy_paragraph_text() {
    assert_eq!(
        html("-{#k} [x] a\n  # h\n").trim(),
        "<ul>\n  <li id=\"k\"><input type=\"checkbox\" checked disabled aria-label=\"a # h\"> a\n\
         # h</li>\n</ul>"
    );
}

/// A SUB-LIST reads the same column, through `marker_content_col` rather than
/// through the item loop - the second place the constant was spelled. At 6 the
/// marker nests; at 2 it is text of the item's paragraph.
#[test]
fn a_sub_list_reads_the_same_column() {
    assert_eq!(
        html("-{#k} [x] a\n      - sub\n").trim(),
        "<ul>\n  <li id=\"k\"><input type=\"checkbox\" checked disabled aria-label=\"a\"> a\n    \
         <ul>\n      <li>sub</li>\n    </ul>\n  </li>\n</ul>"
    );
    assert_eq!(
        html("-{#k} [x] a\n  - sub\n").trim(),
        "<ul>\n  <li id=\"k\"><input type=\"checkbox\" checked disabled aria-label=\"a - sub\"> a\n\
         - sub</li>\n</ul>"
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

/// THE SECOND NEIGHBOUR: a plain item carrying attributes was already 6,
/// because its column is its marker's measured width. A task-only change must
/// leave it exactly there.
#[test]
fn a_plain_item_with_attributes_was_already_at_six() {
    assert_eq!(
        html("-{#k} a\n      # h\n").trim(),
        "<ul>\n  <li id=\"k\">a\n    <h1 id=\"h\">h</h1>\n  </li>\n</ul>"
    );
    assert_eq!(
        html("-{#k} a\n  # h\n").trim(),
        "<ul>\n  <li id=\"k\">a\n# h</li>\n</ul>"
    );
}

/// An ORDERED item's block counts the same way, and always did - the ordered
/// branch never carried the task constant. Pinned beside the bullet so a later
/// change to one is measured against the other.
#[test]
fn an_ordered_item_with_attributes_reads_seven() {
    assert_eq!(
        html("1.{#k} a\n       # h\n").trim(),
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
        html("- outer\n  -{#k} [x] inner\n\n    after\n").trim(),
        "<ul>\n  <li><p>outer</p>\n    <ul>\n      <li id=\"k\"><input type=\"checkbox\" checked disabled aria-label=\"inner\"> inner</li>\n    </ul>\n    <p>after</p>\n  </li>\n</ul>"
    );
}
