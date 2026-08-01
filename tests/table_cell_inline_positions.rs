//! A table cell's inline content had no positions, and 168 of the 287 unplaced
//! text nodes in the corpus were inside one (carve-rs#333).
//!
//! The cause was not the cell parser but the row SPLITTER: it resolved `\|` to
//! a bare `|`, so the cell text was no longer a verbatim slice of the row and
//! nothing inside it could be mapped back. That also lost the `escaped_text`
//! node the vocabulary defines, which carve-js publishes - so the two engines
//! disagreed on the tree, not only on positions.

use carve::ast::{BlockNode, InlineNode};

fn cells(source: &str) -> Vec<InlineNode> {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("the fixture did not parse as a table");
    };
    table.rows[0].cells[0].children.clone()
}

/// The escape stays a node rather than being resolved into the text, which is
/// what carve-js publishes for the same input.
#[test]
fn an_escaped_pipe_stays_its_own_node() {
    let children = cells("| a \\| b | c |\n|---|---|\n| d | e |\n");

    let kinds: Vec<&str> = children
        .iter()
        .map(|n| match n {
            InlineNode::Text(_) => "text",
            InlineNode::EscapedText(_) => "escaped_text",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["text", "escaped_text", "text"]);
}

/// Every inline in a cell carries a span, and each span slices back to itself.
#[test]
fn cell_inlines_carry_spans_that_slice_back() {
    let source = "| a \\| b | c |\n|---|---|\n| d | e |\n";
    let codepoints: Vec<char> = source.chars().collect();

    let children = cells(source);
    assert!(!children.is_empty(), "the cell parsed with no content");

    let mut checked = 0usize;
    for node in &children {
        let (value, pos) = match node {
            InlineNode::Text(t) => (t.value.clone(), t.pos),
            InlineNode::EscapedText(e) => (format!("\\{}", e.value), e.pos),
            _ => continue,
        };
        let pos = pos.unwrap_or_else(|| panic!("no position on {value:?}"));
        let slice: String = codepoints[pos.start_offset..pos.end_offset].iter().collect();
        assert_eq!(slice, value, "the span points somewhere else");
        checked += 1;
    }
    assert_eq!(checked, 3, "expected three placed inlines");
}

/// A cell with no escape must be placed too - the escape case is the reason the
/// text was unanchorable, not the only shape that needed anchoring.
#[test]
fn an_ordinary_cell_is_placed_as_well() {
    let source = "| Heading | Other |\n|---|---|\n| body | cell |\n";
    let codepoints: Vec<char> = source.chars().collect();

    let children = cells(source);
    let InlineNode::Text(text) = &children[0] else {
        panic!("expected a text node");
    };
    let pos = text.pos.expect("an ordinary cell's text must carry a position");
    let slice: String = codepoints[pos.start_offset..pos.end_offset].iter().collect();
    assert_eq!(slice, text.value);
    assert_eq!(pos.start_line, 1);
}

/// The anchor has to come from the CELL's own column, not the row's: getting
/// this wrong places every cell after the first at the row's start.
#[test]
fn a_later_cell_is_anchored_at_its_own_column() {
    let source = "| a | second |\n|---|---|\n| c | d |\n";
    let codepoints: Vec<char> = source.chars().collect();

    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let BlockNode::Table(table) = &doc.children[0] else {
        panic!("the fixture did not parse as a table");
    };

    let InlineNode::Text(text) = &table.rows[0].cells[1].children[0] else {
        panic!("expected a text node in the second cell");
    };
    let pos = text.pos.expect("the second cell's text must carry a position");
    let slice: String = codepoints[pos.start_offset..pos.end_offset].iter().collect();
    assert_eq!(slice, text.value, "the second cell anchored at the wrong column");
}
