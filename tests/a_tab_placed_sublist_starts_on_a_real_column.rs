//! A sublist placed by a tab starts on a real 1-based column (carve-rs#2967).
//!
//! The placing indentation is counted in tab-expanded columns, but the tab that
//! supplies it is one source character, so reclaiming the full width ran the
//! start to column 0. carve-php gives offset 4, column 1 for the ticket input.

use carve::ast::{BlockNode, Pos};

fn nested_list_and_item(src: &str) -> (Pos, Pos) {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(src, &options);
    let BlockNode::List(outer) = &doc.children[0] else {
        panic!("no outer list");
    };
    let nested = outer.items[0]
        .children
        .iter()
        .find_map(|child| match child {
            BlockNode::List(list) => Some(list),
            _ => None,
        })
        .expect("nested list");
    (
        nested.pos.clone().expect("list pos"),
        nested.items[0].pos.clone().expect("item pos"),
    )
}

#[test]
#[ignore = "rule A ledger: carve-rs#3524"]
fn a_tab_placed_sublist_starts_at_the_tab() {
    for pos in <[Pos; 2]>::from(nested_list_and_item("- a\n\t- b\n\tc\n")) {
        assert_eq!(
            (pos.start_line, pos.start_column, pos.start_offset),
            (2, 1, 4)
        );
    }
}

#[test]
fn a_space_placed_sublist_still_starts_at_its_marker() {
    for pos in <[Pos; 2]>::from(nested_list_and_item("- a\n  - b\n  c\n")) {
        assert_eq!(
            (pos.start_line, pos.start_column, pos.start_offset),
            (2, 3, 6)
        );
    }
}
