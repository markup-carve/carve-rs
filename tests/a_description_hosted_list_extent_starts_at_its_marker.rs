//! A LIST DIRECTLY IN A DEFINITION BODY BEGINS ITS EXTENT ON THE MARKER
//! (markup-carve/carve#1980, converging with carve-js's
//! `sublistsCarryAuthoredBase`).
//!
//! Corpus doc `461-...-sibling-4` is the "opener one column shy of the note's
//! floor" case:
//!
//! ```text
//! :: t
//! :  [^f]: note
//!     - nested
//! tail
//! ```
//!
//! The `[^f]: note` line collects out as a footnote; the `- nested` line sits
//! one column shy of the note's body floor, so it stays a list inside the
//! description rather than folding into the note. A footnote body and a
//! definition body rebase over-indented blocks with sublists included, so a
//! marker written past the body's content column carries its own authored base
//! (PART 9 §24 C3): the list and list item begin ON the marker, and the run of
//! placing indentation before it is NOT reclaimed into the span.
//!
//! This engine reclaimed it unconditionally, so the list and list item started
//! one leading-indentation column too early (col 4 / offset 22 instead of col 5
//! / offset 23). carve-js and carve-php both anchor on the marker.
//!
//! ORACLE: carve-js main, which gives col 5 / offset 23 for both nodes with the
//! rest of the tree and the rendered HTML byte-identical.

use carve::ast::BlockNode;

const SIBLING_4: &str = ":: t\n:  [^f]: note\n    - nested\ntail\n";

/// The `list` node hosted directly by the definition's description body.
fn description_hosted_list(src: &str) -> carve::ast::List {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(src, &options);
    for block in &doc.children {
        let BlockNode::DefinitionList(list) = block else {
            continue;
        };
        for item in &list.items {
            for def in &item.definitions {
                for child in &def.children {
                    if let BlockNode::List(inner) = child {
                        return inner.clone();
                    }
                }
            }
        }
    }
    panic!("no list found in the description body");
}

#[test]
fn a_description_hosted_list_and_item_anchor_on_the_marker() {
    let list = description_hosted_list(SIBLING_4);
    let list_pos = list.pos.expect("the list carries a position");
    assert_eq!(
        (
            list_pos.start_line,
            list_pos.start_column,
            list_pos.start_offset
        ),
        (3, 5, 23),
        "the list extent must begin on the `-` marker, not the placing indent",
    );
    assert_eq!(
        list_pos.end_offset, 36,
        "the list extent still ends where it did before the marker anchor",
    );

    let item = list.items.first().expect("the list has one item");
    let item_pos = item.pos.expect("the list item carries a position");
    assert_eq!(
        (
            item_pos.start_line,
            item_pos.start_column,
            item_pos.start_offset
        ),
        (3, 5, 23),
        "the list item extent must begin on the `-` marker too",
    );
    assert_eq!(item_pos.end_offset, 36);
}

#[test]
fn the_marker_anchor_leaves_the_rendered_html_unchanged() {
    // The ruling moved only the span; the tree and HTML are unanimous across
    // engines. Its `<ol>`/`<ul>` body is the proof the node is still a list.
    let html = carve::to_html(SIBLING_4);
    assert!(
        html.contains("<li>nested"),
        "the description still hosts a one-item list: {html}",
    );
}
