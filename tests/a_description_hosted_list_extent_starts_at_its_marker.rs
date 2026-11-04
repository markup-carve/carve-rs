//! A list directly in a definition body includes placing indentation from its
//! owner's content column (CARVE-P12-013, markup-carve/carve#5063).
//!
//! Corpus doc `461-...-sibling-4` is the "opener one column shy of the note's
//! floor" case:
//!
//! ```text
//! :: t
//! :  [^f]: note
//!
//!     - nested
//! tail
//! ```
//!
//! The `[^f]: note` line collects out as a footnote; after the blank line the
//! `- nested` line sits one column shy of the note's body floor, so it stays a
//! list inside the description. Without the blank it is a marker under the
//! note's paragraph and folds into the note as text (carve#4206, corpus 800).
//! The description's content column is 4. The list and list item start there,
//! one space before the marker at column 5. The merged spec position checker
//! requires offset 23 and rejects the former marker offset 24.

use carve::ast::BlockNode;

const SIBLING_4: &str = ":: t\n:  [^f]: note\n\n    - nested\ntail\n";

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
fn a_description_hosted_list_and_item_anchor_on_the_owner_column() {
    let list = description_hosted_list(SIBLING_4);
    let list_pos = list.pos.clone().expect("the list carries a position");
    assert_eq!(
        (
            list_pos.start_line,
            list_pos.start_column,
            list_pos.start_offset
        ),
        (4, 4, 23),
        "the list extent must begin at the description's content column",
    );
    assert_eq!(
        list_pos.end_offset, 37,
        "the list extent still ends at its last owned line",
    );

    let item = list.items.first().expect("the list has one item");
    let item_pos = item.pos.clone().expect("the list item carries a position");
    assert_eq!(
        (
            item_pos.start_line,
            item_pos.start_column,
            item_pos.start_offset
        ),
        (4, 4, 23),
        "the list item extent must begin at the description's content column",
    );
    assert_eq!(item_pos.end_offset, 37);
}

#[test]
fn the_owner_anchor_leaves_the_rendered_html_unchanged() {
    // The ruling moved only the span; the tree and HTML are unanimous across
    // engines. Its `<ol>`/`<ul>` body is the proof the node is still a list.
    let html = carve::to_html(SIBLING_4);
    assert!(
        html.contains("<li>nested"),
        "the description still hosts a one-item list: {html}",
    );
}
