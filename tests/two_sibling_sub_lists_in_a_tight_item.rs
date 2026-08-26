//! PART 9 §11 N1a's boundary applies at EVERY level, so an item can hold two
//! sibling sub-lists - and the canonical writer could not spell one.
//!
//! A tight item joins its children so the re-parse stays tight, and where two of
//! them would merge it wrote both behind §17 L3's `+` marker at the item's
//! MARKER column. That column is column 0, which is where the list the item
//! belongs to writes its own markers: a sub-list put there is not attached to
//! the item, it is dissolved into the list around it. The ticket document came
//! back as one flat list of three items, with both sub-lists and the boundary
//! between them gone, so `to_html(fmt(x)) == to_html(x)` failed
//! (markup-carve/carve#1501).
//!
//! The remedy is that a sub-list is written at the item's CONTENT column, with
//! whatever separator the block above it needs: the boundary when that block is
//! a list it would merge with, one blank line when it is a block that would read
//! the sub-list as its own continuation, and nothing at all otherwise.
//!
//! THE ASSERTIONS COMPARE RE-PARSES, not bytes of HTML with the escaping
//! forgiven: `shape` is the tree the reader gets back, and an equal-HTML check
//! alone is exactly what let the sibling defects in this area sit unnoticed.
//!
//! The spellings are byte-identical to carve-js (markup-carve/carve-js#1299) and
//! carve-php, which are the oracles this port was measured against.

use carve::ast::BlockNode;

fn fmt(source: &str) -> String {
    carve::to_carve(source)
}

fn html(source: &str) -> String {
    carve::to_html(source)
}

/// The block tree as nested type names - inline content and positions dropped,
/// so only the nesting the reader gets back is compared.
fn shape_blocks(blocks: &[BlockNode]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        match block {
            BlockNode::List(list) => {
                let items: Vec<String> = list
                    .items
                    .iter()
                    .map(|item| {
                        let inner = shape_blocks(&item.children);
                        if inner.is_empty() {
                            "list_item".to_string()
                        } else {
                            format!("list_item({inner})")
                        }
                    })
                    .collect();
                parts.push(format!("list({})", items.join(",")));
            }
            BlockNode::BlockQuote(quote) => {
                let inner = shape_blocks(&quote.children);
                parts.push(if inner.is_empty() {
                    "block_quote".to_string()
                } else {
                    format!("block_quote({inner})")
                });
            }
            _ => {}
        }
    }
    parts.join(",")
}

fn shape(source: &str) -> String {
    format!("document({})", shape_blocks(&carve::parse(source).children))
}

/// Every property PART 11 §1 asks of the writer, on one document.
fn round_trips(source: &str) {
    let once = fmt(source);
    assert_eq!(shape(&once), shape(source), "shape of {source:?}");
    assert_eq!(html(&once), html(source), "html of {source:?}");
    assert_eq!(fmt(&once), once, "idempotence of {source:?}");
}

/// A line of nothing but spaces or tabs is not a form the writer may emit (PART
/// 11 §7), and it is what the first attempt at this fix produced above the
/// second list. A line with a TRAILING run is the same tooling hazard, and is
/// what this engine wrote for the boundary inside a nested quote.
fn has_no_stray_whitespace(text: &str) {
    for line in text.split('\n') {
        assert!(
            line.is_empty() || !line.trim().is_empty(),
            "whitespace-only line in {text:?}"
        );
        assert_eq!(
            line.trim_end_matches([' ', '\t']),
            line,
            "trailing whitespace in {text:?}"
        );
    }
}

#[test]
fn writes_the_ticket_document_back_as_the_author_wrote_it() {
    let source = "- outer\n\n  - a\n\n\n\n  - b\n";

    assert_eq!(
        shape(source),
        "document(list(list_item(list(list_item),list(list_item))))"
    );
    assert_eq!(fmt(source), "- outer\n  - a\n\n\n\n  - b\n");
    round_trips(source);
}

#[test]
fn does_not_put_the_sub_lists_at_the_marker_column() {
    // The failure was not "some other spelling": at column 0 the `- b` is an
    // item of the OUTER list, so the document loses a level of nesting.
    let written = fmt("- outer\n\n  - a\n\n\n\n  - b\n");

    assert!(!written.contains("\n+\n"), "{written:?}");
    assert_eq!(
        written
            .split('\n')
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>(),
        vec!["- outer"]
    );
}

#[test]
fn leaves_no_stray_whitespace_above_the_second_list() {
    has_no_stray_whitespace(&fmt("- outer\n\n  - a\n\n\n\n  - b\n"));
    has_no_stray_whitespace(&fmt("> - outer\n>\n>   - a\n>\n>\n>\n>   - b\n"));
    has_no_stray_whitespace(&fmt("> > - a\n> >\n> >\n> >\n> > - b\n"));
    has_no_stray_whitespace(&fmt("- x\n\n  > - a\n  >\n  >\n  >\n  > - b\n"));
}

#[test]
fn spells_the_boundary_as_exactly_three_blank_lines() {
    // §10i fixes the length at three, whatever run the author wrote.
    assert!(fmt("- outer\n\n  - a\n\n\n\n  - b\n").contains("- a\n\n\n\n  - b"));
}

#[test]
fn collapses_a_longer_run_to_three_inside_an_item_too() {
    // The nested analogue of corpus 395: a decorative run still normalizes, and
    // the boundary is not a decorative run.
    let six = "- outer\n\n  - a\n\n\n\n\n\n\n  - b\n";

    assert_eq!(fmt(six), "- outer\n  - a\n\n\n\n  - b\n");
    round_trips(six);
}

#[test]
fn separates_a_third_and_a_fourth_sub_list_the_same_way() {
    let three = "- outer\n\n  - a\n\n\n\n  - b\n\n\n\n  - c\n";

    assert_eq!(fmt(three), "- outer\n  - a\n\n\n\n  - b\n\n\n\n  - c\n");
    round_trips(three);
    round_trips("- o\n\n  - a\n\n\n\n  - b\n\n\n\n  - c\n\n\n\n  - d\n");
}

#[test]
fn separates_sub_lists_that_hold_more_than_one_item() {
    round_trips("- outer\n\n  - a\n  - a2\n\n\n\n  - b\n  - b2\n");
}

#[test]
fn carries_the_boundary_through_ordered_bullet_and_task_markers() {
    round_trips("1. outer\n\n   1. a\n\n\n\n   1. b\n");
    round_trips("1. outer\n\n   - a\n\n\n\n   - b\n");
    round_trips("- outer\n\n  - [ ] a\n\n\n\n  - [ ] b\n");
}

#[test]
fn separates_sub_lists_two_levels_down() {
    let source = "- L1\n\n  - L2\n\n    - a\n\n\n\n    - b\n";

    assert_eq!(fmt(source), "- L1\n  - L2\n    - a\n\n\n\n    - b\n");
    round_trips(source);
}

#[test]
fn separates_sub_lists_in_the_second_item_of_a_list() {
    round_trips("- one\n- two\n\n  - a\n\n\n\n  - b\n");
}

#[test]
fn separates_sub_lists_below_a_fence_or_a_quote_in_the_same_item() {
    round_trips("- x\n\n  ```\n  c\n  ```\n\n  - a\n\n\n\n  - b\n");
    round_trips("- x\n\n  > q\n\n  - a\n\n\n\n  - b\n");
}

#[test]
fn separates_sub_lists_in_a_loose_item() {
    // The loose path is `render_blocks`, a different branch from the tight join.
    let source = "- outer\n\n  para\n\n  - a\n\n\n\n  - b\n";

    assert_eq!(fmt(source), "- outer\n\n  para\n\n  - a\n\n\n\n  - b\n");
    round_trips(source);
    round_trips("- outer\n\n  - a\n\n\n\n  - b\n\n  tail\n");
}

#[test]
fn spells_the_boundary_with_the_host_prefix_inside_a_blockquote() {
    // A blockquote writes its own blank line as `>`, so the three blank lines
    // the boundary opens are `>` lines - an empty line would end the quote and
    // take the second list out of it.
    let source = "> - outer\n>\n>   - a\n>\n>\n>\n>   - b\n";

    assert_eq!(fmt(source), "> - outer\n>   - a\n>\n>\n>\n>   - b\n");
    round_trips(source);
}

#[test]
fn spells_the_boundary_with_every_host_prefix_however_deep() {
    // The prefix is read off the line the marker stands on, so no host has to
    // know the boundary exists - and a host nested inside another gets both
    // halves. A nested quote writes `> >` and it used to write `> > ` with a
    // trailing space, which no other engine does and PART 11 §7 rules out.
    assert_eq!(
        fmt("> > - a\n> >\n> >\n> >\n> > - b\n"),
        "> > - a\n> >\n> >\n> >\n> > - b\n"
    );
    round_trips("> > - a\n> >\n> >\n> >\n> > - b\n");
    round_trips("- x\n\n  > - a\n  >\n  >\n  >\n  > - b\n");
    round_trips("- x\n\n  > - o\n  >\n  >   - a\n  >\n  >\n  >\n  >   - b\n");
    assert_eq!(
        fmt(":: t\n: - a\n\n\n\n   - b\n"),
        ":: t\n: - a\n\n\n\n  - b\n"
    );
    round_trips(":: t\n: - a\n\n\n\n   - b\n");
    round_trips("::: note\n- a\n\n\n\n- b\n:::\n");
}

#[test]
fn keeps_the_top_level_boundary_exactly_as_it_was() {
    // The control for the mechanism change: nothing at document level may move
    // with the tight-item join.
    assert_eq!(
        fmt("- apples\n\n\n\n- oranges\n"),
        "- apples\n\n\n\n- oranges\n"
    );
    assert_eq!(fmt("1. a\n\n  1. b\n"), "1. a\n\n\n\n1. b\n");
}

#[test]
fn writes_one_blank_line_below_a_block_at_the_marker_column() {
    // §17 L3 puts the attached paragraph at column 0, and a sub-list at the
    // item's content column below it is INDENTED under an open paragraph, so it
    // reads as that paragraph's lazy continuation and never opens.
    let source = "- x\n+\np2\n\n  - b\n";

    assert_eq!(fmt(source), "- x\n+\np2\n\n  - b\n");
    round_trips(source);
}

#[test]
fn writes_one_blank_line_below_a_blockquote() {
    // A quote takes a non-blank line below it as lazy continuation. That shape
    // carries no §11 N1a boundary at all - it is the same question and the same
    // answer, and the spelling now matches carve-js and carve-php.
    let source = "- x\n  > q\n\n  - b\n";

    assert_eq!(fmt(source), "- x\n  > q\n\n  - b\n");
    round_trips(source);
    round_trips("- x\n\n  - a\n\n  > q\n\n  - b\n");
}

#[test]
fn writes_one_blank_line_below_every_kind_that_leaves_a_paragraph_open() {
    // Each member of the set is load-bearing rather than carried along for
    // symmetry: with a sub-list already open at the item's content column, all
    // four of these lose the second sub-list without the blank line.
    for above in ["para", "![a](i.png)", "![a](i.png)\n  ^ cap", ":: t\n  : d"] {
        round_trips(&format!("- o\n  - z\n  | t |\n  {above}\n\n  - s1\n"));
    }
}

#[test]
fn writes_no_separator_where_nothing_above_reaches_down() {
    // The bound on the rule: a heading, fence, table, break, div or admonition
    // closes at its last line, so the sub-list opens on the next one and owes
    // nothing. A blank line here would be a construct the document did not have.
    assert_eq!(fmt("- x\n\n  # h\n\n  - b\n"), "- x\n  # h\n  - b\n");
    assert_eq!(fmt("- x\n\n  | a |\n\n  - b\n"), "- x\n  | a |\n  - b\n");
    assert_eq!(fmt("- x\n\n  ***\n\n  - b\n"), "- x\n  ***\n  - b\n");
    assert_eq!(fmt("- outer\n\n  - a\n"), "- outer\n  - a\n");
}

#[test]
fn leaves_the_marker_column_to_the_kinds_that_still_need_it() {
    // Two sibling blockquotes, tables, line blocks and definition lists merge
    // when written adjacent and CAN be attached at column 0, because none of
    // them opens there in preference to being attached. They keep the `+`.
    assert!(fmt("- outer\n\n  > a\n\n  > b\n").contains("\n+\n"));
    assert!(fmt("- outer\n\n  | a |\n\n  | a |\n").contains("\n+\n"));
    round_trips("- outer\n\n  > a\n\n  > b\n");
    round_trips("- outer\n\n  | a |\n\n  | a |\n");
}

#[test]
fn owes_nothing_to_sub_lists_whose_markers_already_differ() {
    // carve#286's axis: different markers separate on their own, so no boundary
    // is written and the author's adjacency survives.
    assert_eq!(
        fmt("- outer\n\n  - a\n\n  * b\n"),
        "- outer\n  - a\n  * b\n"
    );
    round_trips("- outer\n\n  - a\n\n\n\n  * b\n");
}

#[test]
fn keeps_an_ordinary_blank_run_above_a_non_list() {
    // A boundary above a block that is NOT a list is a decorative run and still
    // normalizes to one blank line - the control that the boundary is written
    // only where §11 N1's merge rule would otherwise apply.
    round_trips("- x\n\n  para\n\n\n\n  - b\n");
    round_trips("- x\n\n  ```\n  c\n  ```\n\n\n\n  - b\n");
}
