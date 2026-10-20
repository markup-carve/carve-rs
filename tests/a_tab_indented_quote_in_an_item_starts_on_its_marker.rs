//! A quote placed in a list item by a tab starts on its `>`.
//!
//! Stripping an authored block base advanced the column map by the stripped
//! width in columns, so a tab moved the span three codepoints past the marker
//! and off the end of the line. carve-php agrees on every case below.

use carve::ast::BlockNode;

fn quote_start(src: &str) -> (usize, usize, usize) {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(src, &options);
    let BlockNode::List(list) = &doc.children[0] else {
        panic!("no list");
    };
    let quote = list.items[0]
        .children
        .iter()
        .find_map(|child| match child {
            BlockNode::BlockQuote(quote) => quote.pos.clone(),
            _ => None,
        })
        .expect("quote pos");
    (quote.start_line, quote.start_column, quote.start_offset)
}

#[test]
fn a_tab_indented_quote_starts_on_its_marker() {
    for (src, expected) in [
        ("- a\n  \t> q\n", (2, 4, 7)),
        ("- a\n   \t> q\n", (2, 5, 8)),
        ("- a\n\t\t> q\n", (2, 3, 6)),
        ("- a\n\n  \t> q\n", (3, 4, 8)),
    ] {
        assert_eq!(quote_start(src), expected, "{src:?}");
    }
}
