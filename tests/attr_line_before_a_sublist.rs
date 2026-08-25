//! A block-attribute line before a NESTED LIST inside a list item attaches to
//! that list (markup-carve/carve-rs#1007, rule decided in markup-carve/carve#1238).
//!
//! The attributes used to be dropped - not rendered literally, not warned
//! about, just gone - and a nested list was the ONLY block in that position
//! that lost them. A paragraph, a block quote and a code fence written in the
//! same place all attached, and so did a list one nesting level up. The
//! controls at the bottom of this file are those neighbours: they had no test
//! of their own, so nothing said that the nested list was the odd one out.
//!
//! WHY IT WAS ONLY LISTS. `parse_blocks` owns the pending-attribute slot and
//! attaches a `{…}` line to the next block IN THE SAME STREAM. Inside an item
//! the continuation collector stops at a marker sitting at the item's content
//! column, so `parse_list` can own the sub-list and its looseness bookkeeping -
//! which put the attribute line at the end of one chunk and the list at the
//! start of another, each with its own slot.
//!
//! That break happens whether or not a blank line precedes the attributes, so
//! rows B and C of the matrix in markup-carve/carve#1238 are one defect and
//! both attach here. The blank line was never what decided it: with the same
//! spacing that dropped them for a nested list, a PARAGRAPH took them, which is
//! pinned as a control below.

#[test]
fn attributes_reach_a_nested_list() {
    assert_eq!(
        carve::to_html("- a\n\n  {.x}\n  - b\n"),
        "<ul>\n  <li>a\n    <ul class=\"x\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn no_blank_line_is_needed_before_them() {
    // Row C of the matrix in markup-carve/carve#1238. The blank line is not
    // what decides attachment: the same no-blank line in front of a PARAGRAPH
    // has always attached (the control below), and the ruling is that a nested
    // list is a block like any other. The orphaning is identical either way -
    // the continuation collector breaks on the marker whether or not a blank
    // preceded - so the same carry repairs both rows.
    assert_eq!(
        carve::to_html("- a\n  {.x}\n  - b\n"),
        "<ul>\n  <li>a\n    <ul class=\"x\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_deeper_sublist_takes_them_without_a_blank_too() {
    assert_eq!(
        carve::to_html("- a\n\n  - b\n    {.x}\n    - c\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <ul class=\"x\">\n          <li>c</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_nested_ordered_list_takes_them_too() {
    assert_eq!(
        carve::to_html("- a\n\n  {.x}\n  1. b\n"),
        "<ul>\n  <li>a\n    <ol class=\"x\">\n      <li>b</li>\n    </ol>\n  </li>\n</ul>"
    );
}

#[test]
fn a_multi_line_attribute_block_reaches_it() {
    // `{#id` / `.x}` is one block spanning two lines, so the split that carries
    // it across the chunk boundary has to take the whole run or none of it.
    assert_eq!(
        carve::to_html("- a\n\n  {#id\n  .x}\n  - b\n"),
        "<ul>\n  <li>a\n    <ul id=\"id\" class=\"x\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn stacked_attribute_blocks_merge_the_way_they_do_at_top_level() {
    // Two blocks in front of one target merge into a single set everywhere
    // else - `{.x}` / `{#i}` / `para` publishes `class="x" id="i"` at document
    // level and inside an item. Lifting only the LAST block off the chunk would
    // have made the nested list the one target that keeps just the final block.
    assert_eq!(
        carve::to_html("{.x}\n{#i}\n- b\n"),
        "<ul class=\"x\" id=\"i\">\n  <li>b</li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- a\n\n  {.x}\n  {#i}\n  - b\n"),
        "<ul>\n  <li>a\n    <ul class=\"x\" id=\"i\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_line_that_merely_ends_in_a_brace_keeps_its_paragraph() {
    // The split walks back to the line that OPENS the run and hands it to the
    // same reader `parse_blocks` uses, then takes the run only if that reader
    // consumed it WHOLE. Here it does not: `{.x}` is an attribute block and
    // `more text}` is a paragraph that happens to end in a brace. Splitting on
    // the walk-back alone would carry the attributes to the nested list AND
    // silently delete the paragraph.
    assert_eq!(
        carve::to_html("- a\n\n  {.x}\n  more text}\n  - b\n"),
        "<ul>\n  <li><p>a</p>\n    <p class=\"x\">more text}</p>\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn they_attach_after_a_paragraph_in_the_same_item() {
    // The attribute line is the TAIL of a chunk that also holds content, which
    // is the shape a fix keyed on "the whole chunk is an attribute block" would
    // miss. Same three lines at document level have always attached.
    assert_eq!(
        carve::to_html("- a\n\n  para\n  {.x}\n  - b\n"),
        "<ul>\n  <li><p>a</p>\n    <p>para</p>\n    <ul class=\"x\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_blank_line_does_not_break_the_attachment() {
    // `{.x}` / blank / `- b` attaches at document level, so it attaches here.
    assert_eq!(
        carve::to_html("- a\n\n  para\n\n  {.x}\n  - b\n"),
        "<ul>\n  <li><p>a</p>\n    <p>para</p>\n    <ul class=\"x\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_deeper_sublist_takes_its_own() {
    assert_eq!(
        carve::to_html("- a\n\n  - b\n\n    {.x}\n    - c\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b\n        <ul class=\"x\">\n          <li>c</li>\n        </ul>\n      </li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn they_land_on_the_nested_list_and_nowhere_else() {
    // The target is the nested `<ul>`, not the `<li>` that holds it and not the
    // outer `<ul>` - both of which are also in scope at that point and would
    // look plausible in a rendered page.
    let html = carve::to_html("- a\n\n  {.x}\n  - b\n- c\n");
    assert_eq!(
        html,
        "<ul>\n  <li>a\n    <ul class=\"x\">\n      <li>b</li>\n    </ul>\n  </li>\n  <li>c</li>\n</ul>"
    );
    assert!(
        !html.starts_with("<ul class="),
        "the outer list must not take them"
    );
    assert!(!html.contains("<li class="), "the item must not take them");
}

#[test]
fn the_item_stays_tight() {
    // §17 L2: a blank before an item's sub-block leaves the item tight, so `a`
    // renders bare. Carrying the attributes must not turn the sub-list into a
    // second paragraph's worth of looseness.
    let html = carve::to_html("- a\n\n  {.x}\n  - b\n");
    assert!(
        html.contains("<li>a\n"),
        "the item lost its tight rendering: {html}"
    );
    assert!(!html.contains("<p>a</p>"), "the item was loosened: {html}");
}

#[test]
fn attributes_with_nothing_after_them_are_still_dropped() {
    // Unchanged: a block-attribute line whose target never arrives renders
    // nothing and leaves no trace, exactly as at document level.
    assert_eq!(
        carve::to_html("- a\n\n  {.x}\n"),
        "<ul>\n  <li>a</li>\n</ul>"
    );
}

#[test]
fn a_later_sublist_does_not_inherit_them() {
    // The attributes are written in front of nothing in the first item. A
    // sibling item opening is what ends their reach - without that they would
    // travel down the list and land on the next sub-list they met, which is not
    // where the author put them.
    let html = carve::to_html("- a\n\n  {.x}\n- b\n\n  - c\n");
    assert!(
        !html.contains("class=\"x\""),
        "attributes leaked to a later item: {html}"
    );
}

#[test]
fn a_line_past_the_content_column_attributes_the_list_under_it() {
    assert_eq!(
        carve::to_html("- a\n\n   {.c}\n   - b\n"),
        "<ul>\n  <li>a\n    <ul class=\"c\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS: the same position, for the block types that always worked. They had
// no test before, which is why nothing recorded that the nested list diverged.

#[test]
fn a_paragraph_in_that_position_attaches() {
    assert_eq!(
        carve::to_html("- a\n\n  {.x}\n  para\n"),
        "<ul>\n  <li><p>a</p>\n    <p class=\"x\">para</p>\n  </li>\n</ul>"
    );
}

#[test]
fn a_paragraph_attaches_with_no_blank_line_either() {
    // The row that settles row C: with the SAME spacing that dropped the
    // attributes for a nested list, a paragraph took them - so there is no
    // "an unseparated attribute line is stricter" rule to appeal to. This
    // behavior predates the fix and must not move.
    assert_eq!(
        carve::to_html("- a\n  {.x}\n  para\n"),
        "<ul>\n  <li>a\n    <p class=\"x\">para</p>\n  </li>\n</ul>"
    );
}

#[test]
fn the_marker_abutting_form_still_attributes_the_item() {
    // The OTHER attribute spelling, out of scope for this fix and the one thing
    // that reaches an `<li>`. It must keep working at both nesting levels, or
    // carrying the line form across a chunk boundary has stolen its target.
    assert_eq!(
        carve::to_html("-{.x} item\n"),
        "<ul>\n  <li class=\"x\">item</li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- a\n\n  -{.x} b\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li class=\"x\">b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn a_code_fence_in_that_position_attaches() {
    assert_eq!(
        carve::to_html("- a\n\n  {.x}\n  ```\n  code\n  ```\n"),
        "<ul>\n  <li>a\n    <pre class=\"x\"><code>code\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn a_block_quote_in_that_position_attaches() {
    assert_eq!(
        carve::to_html("- a\n\n  {.x}\n  > q\n"),
        "<ul>\n  <li>a\n    <blockquote class=\"x\"><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn a_list_one_level_up_attaches() {
    assert_eq!(
        carve::to_html("{.x}\n- b\n"),
        "<ul class=\"x\">\n  <li>b</li>\n</ul>"
    );
}
