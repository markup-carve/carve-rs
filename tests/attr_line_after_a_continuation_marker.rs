//! A block-attribute line written after a `+` continuation marker is an
//! attribute block, and it attaches to the block that follows it INSIDE the
//! item (markup-carve/carve-rs#1020, rule decided in markup-carve/carve#1238).
//!
//! It used to read as ordinary paragraph text, and the block the attributes
//! were written for then fell OUTSIDE the item, where carve-js and carve-php
//! put it inside carrying the attributes.
//!
//! WHY. `parse_blocks` owns the only pending-attribute slot. A `+` attaches its
//! block through `parse_continuation_block`, which calls `parse_block` - the
//! SINGLE-block parser, which has no slot - so the `{…}` line arrived with
//! nothing to read it and fell through to a paragraph. PART 2 lists
//! `block_attributes` among the alternatives of `block` and PART 11 spells
//! `continuation_marker_block = continuation_marker, block`, so the marker
//! admits one; PART 9 §15 then floats it to the next block.
//!
//! THIS IS NOT THE PATH markup-carve/carve-rs#1007 TOOK. That one was a chunk
//! boundary inside `collect_item_continuation_block_mapped`, repaired by
//! lifting a TRAILING attribute block off the chunk (`split_trailing_attrs`)
//! and carrying it into the sub-list branch. The `+` marker never reaches that
//! collector: it builds its own sub-cursor over a verbatim slice. Same rule,
//! different mechanism, and neither fix reaches the other's shape - which is
//! why both need their own tests.

// ---------------------------------------------------------------------------
// The reported shape.
// ---------------------------------------------------------------------------

#[test]
fn a_quote_after_the_marker_takes_the_attributes_and_stays_in_the_item() {
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n> q\n"),
        "<ul>\n  <li>a\n    <blockquote class=\"x\"><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn a_paragraph_after_the_marker_takes_them_too() {
    // The same document with a paragraph as the target. Before the fix the
    // whole run folded into the item's opening paragraph as text, `{.x}` and
    // `para` on two lines of one paragraph.
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\npara\n"),
        "<ul>\n  <li>a\n    <p class=\"x\">para</p>\n  </li>\n</ul>"
    );
}

#[test]
fn the_indented_spelling_of_the_same_document_is_unchanged() {
    // The control that made the marker form the odd one out: written with
    // indentation instead of a `+`, this always attached. Both spellings now
    // publish the same tree.
    assert_eq!(
        carve::to_html("- a\n  {.x}\n  para\n"),
        "<ul>\n  <li>a\n    <p class=\"x\">para</p>\n  </li>\n</ul>"
    );
}

// ---------------------------------------------------------------------------
// Every block kind in that position, and the marker's other spelling.
// ---------------------------------------------------------------------------

#[test]
fn a_code_fence_after_the_marker_takes_them() {
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n```\ncode\n```\n"),
        "<ul>\n  <li>a\n    <pre class=\"x\"><code>code\n</code></pre>\n  </li>\n</ul>"
    );
}

#[test]
fn a_heading_after_the_marker_takes_them() {
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n# h\n"),
        "<ul>\n  <li>a\n    <h1 class=\"x\" id=\"h\">h</h1>\n  </li>\n</ul>"
    );
}

#[test]
fn a_sublist_after_the_marker_takes_them() {
    // The block kind markup-carve/carve-rs#1007 was about, reached down the
    // OTHER path. Before the fix the marker line folded the whole run into the
    // item's paragraph, so the list was not even built.
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n  - b\n"),
        "<ul>\n  <li>a\n    <ul class=\"x\">\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn the_first_block_form_takes_them() {
    // `- +` attaches the item's FIRST block through the same function, so it
    // carried the same defect and is repaired by the same change.
    assert_eq!(
        carve::to_html("- +\n{.x}\n> q\n"),
        "<ul>\n  <li>\n    <blockquote class=\"x\"><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn a_marker_inside_a_nested_list_attaches_nothing() {
    // THIS CASE INVERTED with markup-carve/carve#1436, and the comment it used
    // to carry - "flush has to be measured against the marker's own column
    // rather than against column 0" - is the loose reading the clause names and
    // rejects. §17 L3 says the marker attaches a block beginning at DOCUMENT
    // COLUMN 0 and nothing else.
    //
    // So the `+` at column 2 attaches nothing: no column-0 block follows it.
    // The attribute line and the quote below are at the OUTER item's content
    // column, which is where the ordinary column rules put them - after the
    // nested list, as blocks of the outer item.
    //
    // carve-js and carve-php both produce exactly this, independently, since
    // they took the same ruling.
    assert_eq!(
        carve::to_html("- a\n  - b\n  +\n  {.x}\n  > q\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n    <blockquote class=\"x\"><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

// ---------------------------------------------------------------------------
// The attribute block's own shapes.
// ---------------------------------------------------------------------------

#[test]
fn consecutive_attribute_lines_merge_into_one_set() {
    // Attribute blocks STACK. Reading only the last line off would make the
    // `+` target the one place that keeps just the final block.
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n{#i}\n> q\n"),
        "<ul>\n  <li>a\n    <blockquote class=\"x\" id=\"i\"><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn a_block_that_spans_lines_is_read_whole() {
    assert_eq!(
        carve::to_html("- a\n+\n{#id\n.foo}\n> q\n"),
        "<ul>\n  <li>a\n    <blockquote id=\"id\" class=\"foo\"><p>q</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn a_paragraph_that_merely_ends_in_a_brace_is_not_swallowed() {
    // The reader consumes `{.x}` and stops; `more text}` is the paragraph the
    // attributes land on, not part of the attribute block. It stays.
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\nmore text}\n> q\n"),
        "<ul>\n  <li>a\n    <p class=\"x\">more text}</p>\n  </li>\n</ul>\n<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn attributes_that_reach_nothing_are_dropped() {
    // §17 L3 bounds the attachment at the blank line, so nothing follows the
    // attribute block inside it. A set that reaches nothing is dropped, exactly
    // as one at the end of any other stream is - it does not travel past the
    // boundary to the quote below, and it does not come back as text.
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n\n> q\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<blockquote><p>q</p></blockquote>"
    );
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n"),
        "<ul>\n  <li>a</li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n- b\n"),
        "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>"
    );
}

// ---------------------------------------------------------------------------
// Controls that must not move.
// ---------------------------------------------------------------------------

#[test]
fn control_an_indented_line_after_the_marker_is_not_the_marker_s() {
    // The marker reaches column 0 and nothing else (§17 L3), so it attaches
    // nothing here and the indented line is left to its own column - the item's
    // content column, where a brace line is the attribute line for a block this
    // item does not have. It attaches to nothing and renders nothing; `> q` is
    // the quote's own, not the attribute's.
    //
    // This control used to read the line as TEXT inside a block the marker had
    // attached, which is the attachment the clause refuses. carve-js and the
    // executable spec both produce what is asserted here (corpus 435-13).
    assert_eq!(
        carve::to_html("- a\n+\n  {.x}\n> q\n"),
        "<ul>\n  <li>a</li>\n</ul>\n<blockquote><p>q</p></blockquote>"
    );
}

#[test]
fn control_the_marker_with_no_attribute_line_is_unchanged() {
    // The `+` marker's own job. Nothing about reading an attribute block may
    // change what the marker does when there is none to read.
    assert_eq!(
        carve::to_html("- a\n+\n> q\n"),
        "<ul>\n  <li>a\n    <blockquote><p>q</p></blockquote>\n  </li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- a\n+\npara\n"),
        "<ul>\n  <li>a\n    para\n  </li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- a\n+\n  - b\n"),
        "<ul>\n  <li>a\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

#[test]
fn control_a_further_marker_still_ends_the_attachment() {
    // §17 L3's terminators are untouched: the second `+` attaches its own
    // block, and the attribute line in front of it belongs to that one.
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n> q1\n+\n{#i}\n> q2\n"),
        "<ul>\n  <li>a\n    <blockquote class=\"x\"><p>q1</p></blockquote>\n    <blockquote id=\"i\"><p>q2</p></blockquote>\n  </li>\n</ul>"
    );
}

#[test]
fn control_the_marker_abutting_form_is_untouched() {
    // `-{.x} item` is the only spelling that reaches an `<li>`, and it does not
    // go through the continuation marker at all.
    assert_eq!(
        carve::to_html("-{.x} item\n"),
        "<ul>\n  <li class=\"x\">item</li>\n</ul>"
    );
}

#[test]
fn control_item_tightness_is_unchanged() {
    // PART 9 §17 L2. A `+` attachment does not loosen the item, with or without
    // an attribute line in front of the block it attaches.
    assert_eq!(
        carve::to_html("- a\n+\n{.x}\n> q\n- b\n"),
        "<ul>\n  <li>a\n    <blockquote class=\"x\"><p>q</p></blockquote>\n  </li>\n  <li>b</li>\n</ul>"
    );
    assert_eq!(
        carve::to_html("- a\n+\n> q\n- b\n"),
        "<ul>\n  <li>a\n    <blockquote><p>q</p></blockquote>\n  </li>\n  <li>b</li>\n</ul>"
    );
}
