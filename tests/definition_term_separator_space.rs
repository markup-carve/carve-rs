//! The SEPARATOR after `::` is the space, and whitespace past it is not
//! content (carve#513, carve#530, carve-rs#511 item 3).
//!
//! `::  u` rendered `<dt> u</dt>` here and `<dt>u</dt>` in carve-js and
//! carve-php. The rule is a property of the separator, not of any one marker:
//! this engine already stripped the extra column for `#`, `>` and `-`, and the
//! definition term was the one marker that kept it. That is the same shape
//! carve#513 and carve#530 rejected twice.

use carve::ast::{BlockNode, InlineNode};

fn html(source: &str) -> String {
    carve::render_html(&carve::parse(source))
}

fn term_text(source: &str) -> String {
    let doc = carve::parse(source);
    let BlockNode::DefinitionList(list) = doc.children.first().expect("a block") else {
        panic!("expected a definition list, got {:?}", doc.children.first());
    };
    let term = list
        .items
        .first()
        .expect("an item")
        .terms
        .first()
        .expect("a term");
    term.children
        .iter()
        .map(|node| match node {
            InlineNode::Text(t) => t.value.clone(),
            other => panic!("expected text, got {other:?}"),
        })
        .collect()
}

#[test]
fn extra_columns_after_the_separator_are_not_term_content() {
    assert_eq!(term_text("::  u\n:  d\n"), "u");
    assert_eq!(term_text("::    spaced\n:  d\n"), "spaced");
}

#[test]
fn the_single_separator_space_is_unchanged() {
    assert_eq!(term_text(":: u\n:  d\n"), "u");
}

#[test]
fn a_term_keeps_its_interior_and_loses_its_trailing_run() {
    // Only the run against the marker is separator; spaces the author wrote
    // inside the term are content, and the trailing run was already dropped.
    assert_eq!(term_text("::   a  b   \n:  d\n"), "a  b");
}

#[test]
fn every_term_in_an_item_strips_its_own_padding() {
    // An item can hold several terms, and each `::` line carries its own
    // separator. Trimming only the first left the second `<dt>` padded - found
    // in review, not by the first three tests, which all used one term.
    let doc = carve::parse(":: a\n::  b\n:  d\n");
    let BlockNode::DefinitionList(list) = doc.children.first().expect("a block") else {
        panic!("expected a definition list");
    };
    let terms = &list.items.first().expect("an item").terms;
    assert_eq!(terms.len(), 2, "{terms:?}");
    let text = |term: &carve::ast::DefinitionTerm| -> String {
        term.children
            .iter()
            .map(|node| match node {
                InlineNode::Text(t) => t.value.clone(),
                other => panic!("expected text, got {other:?}"),
            })
            .collect()
    };
    assert_eq!(text(&terms[0]), "a");
    assert_eq!(text(&terms[1]), "b");
}

#[test]
fn the_rendered_term_matches_the_other_two_engines() {
    assert!(
        html("::  u\n:  d\n").contains("<dt>u</dt>"),
        "got {}",
        html("::  u\n:  d\n")
    );
}

#[test]
fn the_terms_own_text_is_positioned_past_the_separator() {
    // The column has to move with the trim, or the term's text selects the
    // separator. `stripped_col` matches the slice as a SUFFIX of the raw line,
    // so passing the trimmed slice is what carries the column along.
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options("::  u\n:  d\n", &options);
    let BlockNode::DefinitionList(list) = doc.children.first().expect("a block") else {
        panic!("expected a definition list");
    };
    let term = list
        .items
        .first()
        .expect("an item")
        .terms
        .first()
        .expect("a term");
    let InlineNode::Text(text) = term.children.first().expect("term text") else {
        panic!("expected text");
    };
    let pos = text.pos.as_ref().expect("a position");
    // `::  u` - the `u` sits in column 5, which is where carve-js puts it too.
    assert_eq!(pos.start_column, 5, "{pos:?}");
    assert_eq!(pos.start_offset, 4, "{pos:?}");
}
