//! PART 9 §11 N1a (markup-carve/carve#1430): a run of THREE OR MORE blank lines
//! before a compatible sibling marker ends the list -- the marker after it opens
//! a new sibling list.
//!
//! One blank line stays the loose-item separator, and so does two. Three is the
//! threshold because two fires on runs documents already hold: sampled across
//! 8000 Markdown files, every one of the 30 sites at two blank lines was
//! changelog spacing or generator output rather than an author separating two
//! lists, and there were NO sites at three.
//!
//! The clause is unrestricted, so these pin the boundary at every level -- top
//! level, inside a quote, and inside a list item. A boundary that fired only at
//! the top level would make one spelling mean two things depending on where it
//! sits.
//!
//! The run closes nothing on its own; it denies a following sibling marker the
//! right to join. `a_continuation_still_continues_the_item` is that half.

/// Top-level node kinds, as a comma-joined string. Only the kinds these cases
/// produce are named, so anything else shows up as `other` and fails the
/// comparison loudly rather than silently matching.
fn top_kinds(source: &str) -> String {
    carve::parse(source)
        .children
        .iter()
        .map(|block| match block {
            carve::BlockNode::List(_) => "list",
            carve::BlockNode::Paragraph(_) => "paragraph",
            carve::BlockNode::Comment(_) => "comment",
            _ => "other",
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn written(source: &str) -> String {
    carve::render_carve(&carve::parse(source)).unwrap()
}

#[test]
fn one_blank_line_still_loosens() {
    assert_eq!(top_kinds("- apples\n\n- oranges\n"), "list");
    assert_eq!(
        carve::to_html("- apples\n\n- oranges\n"),
        "<ul>\n  <li><p>apples</p></li>\n  <li><p>oranges</p></li>\n</ul>"
    );
}

#[test]
fn two_blank_lines_still_loosen_rather_than_separate() {
    // The threshold is three precisely so that the run documents already
    // contain keeps meaning what it meant.
    assert_eq!(top_kinds("- apples\n\n\n- oranges\n"), "list");
    assert_eq!(
        carve::to_html("- apples\n\n\n- oranges\n"),
        "<ul>\n  <li><p>apples</p></li>\n  <li><p>oranges</p></li>\n</ul>"
    );
    // And the writer normalizes that decorative run back to one blank line.
    assert_eq!(
        written("- apples\n\n\n- oranges\n"),
        "- apples\n\n- oranges\n"
    );
}

#[test]
fn three_blank_lines_open_a_new_sibling_list() {
    let source = "- apples\n\n\n\n- oranges\n";
    assert_eq!(top_kinds(source), "list,list");
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li>apples</li>\n</ul>\n<ul>\n  <li>oranges</li>\n</ul>"
    );
    // Both lists come back TIGHT: the run separated them instead of loosening
    // one list, so neither item wraps its text in a paragraph.
    assert_eq!(written(source), source);
    assert_eq!(written(&written(source)), source);
}

#[test]
fn an_ordered_pair_separates_the_same_way() {
    let source = "1. a\n\n\n\n1. b\n";
    assert_eq!(top_kinds(source), "list,list");
    assert_eq!(
        carve::to_html(source),
        "<ol>\n  <li>a</li>\n</ol>\n<ol>\n  <li>b</li>\n</ol>"
    );
    assert_eq!(written(source), source);
}

/// A LONGER run is still one boundary, and the writer spells the boundary at
/// its canonical length: six blank lines round-trip to three, still two lists.
#[test]
fn a_longer_run_is_the_same_boundary_and_writes_back_as_three() {
    let six = "- apples\n\n\n\n\n\n\n- oranges\n";
    assert_eq!(top_kinds(six), "list,list");
    assert_eq!(written(six), "- apples\n\n\n\n- oranges\n");
    assert_eq!(top_kinds(&written(six)), "list,list");
    assert_eq!(carve::to_html(six), carve::to_html(&written(six)));
}

/// The run denies a following SIBLING MARKER the right to join. It is not an
/// item terminator, so content at the item's content column still belongs to
/// the item at any run length -- §17's content-column model, unchanged.
#[test]
fn a_continuation_still_continues_the_item() {
    let source = "- a\n\n\n\n  still a\n";
    assert_eq!(top_kinds(source), "list");
    assert_eq!(
        carve::to_html(source),
        "<ul>\n  <li><p>a</p>\n    <p>still a</p>\n  </li>\n</ul>"
    );
}

#[test]
fn the_boundary_applies_inside_a_block_quote() {
    let source = "> - a\n>\n>\n>\n> - b\n";
    assert_eq!(
        carve::to_html(source),
        "<blockquote>\n  <ul>\n    <li>a</li>\n  </ul>\n  <ul>\n    <li>b</li>\n  </ul>\n</blockquote>"
    );
    // The written form re-parses to the same document: the quote's own blank
    // lines carry the boundary.
    assert_eq!(carve::to_html(&written(source)), carve::to_html(source));
}

/// THE PARSER SPLITS AT EVERY LEVEL, the nested case included. Only the WRITER
/// cannot spell this shape yet -- a tight item's join writes both sub-lists at
/// the item's marker column, where they merge back. That is
/// markup-carve/carve#1501, shared with carve-js, and deliberately out of scope
/// here; this case pins the half that is settled.
#[test]
fn the_boundary_applies_to_a_list_nested_in_an_item() {
    assert_eq!(
        carve::to_html("- outer\n\n  - a\n\n\n\n  - b\n"),
        "<ul>\n  <li>outer\n    <ul>\n      <li>a</li>\n    </ul>\n    <ul>\n      <li>b</li>\n    </ul>\n  </li>\n</ul>"
    );
}

/// A run of three inside an item that holds a MARKER-LINE block still splits:
/// the blank separator such an item swallows is not what the boundary counts.
#[test]
fn the_boundary_applies_after_a_marker_line_block() {
    assert_eq!(top_kinds("- # H\n\n\n\n- b\n"), "list,list");
}

/// BOUND, recorded so the next reader does not mistake it for the threshold at
/// work: an invisible line between two items already opened a second list
/// before §11 N1a existed, and it still does. The count is of BLANK lines only,
/// so a run broken by a comment is not a run and this shape is decided
/// elsewhere.
#[test]
fn a_comment_between_two_items_separates_them_for_its_own_reason() {
    assert_eq!(top_kinds("- a\n\n%% c\n\n- b\n"), "list,comment,list");
}

/// BOUND: three blank lines before an INCOMPATIBLE marker change nothing --
/// those lists separated on the marker axis already (carve#286).
#[test]
fn an_incompatible_marker_was_already_separate() {
    assert_eq!(top_kinds("- a\n\n\n\n* b\n"), "list,list");
    assert_eq!(top_kinds("- a\n\n* b\n"), "list,list");
}

/// BOUND: the boundary is owed between two LISTS and nowhere else. A hoisted
/// footnote definition is a non-list entry, so the pair state the two lists
/// above it raised must not reach it -- the writer applies the boundary where
/// an entry is pushed, so a definition arriving with that state still raised
/// came out behind three blank lines it was never owed.
#[test]
fn a_hoisted_definition_after_a_separated_pair_takes_no_boundary() {
    let source = "- a\n\n\n\n- b[^f]\n\n[^f]: note\n";
    assert_eq!(written(source), source);
    assert_eq!(carve::to_html(&written(source)), carve::to_html(source));
}
