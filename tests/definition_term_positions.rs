//! A `<dt>` and a `<dd>` carried no position, which was 29 of the 83 remaining
//! findings - the largest block after plain text (carve-rs#333).
//!
//! Two halves, and the second is the one that hides. The nodes were built with
//! `pos: None` even though the parser had the cursor; and once given a span,
//! `fill_offsets` reached their CHILDREN and skipped the nodes themselves, so
//! each carried a correct line and column with offsets of `0..0` - present, and
//! selecting nothing. That is the third node family in this engine to fail that
//! exact way, after figure captions and footnote definition bodies.

use carve::ast::BlockNode;

const SOURCE: &str = ":: color\n:: colour\n:  The visual property.\n:  A pigment.\n";

fn definition_list(source: &str) -> carve::ast::DefinitionList {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let BlockNode::DefinitionList(list) = &doc.children[0] else {
        panic!("the fixture did not parse as a definition list");
    };
    list.clone()
}

fn slice(source: &str, pos: carve::ast::Pos) -> String {
    source
        .chars()
        .skip(pos.start_offset)
        .take(pos.end_offset - pos.start_offset)
        .collect()
}

/// The span covers the marker, the way a heading's covers its `#`.
#[test]
fn a_term_spans_its_marker_and_content() {
    let list = definition_list(SOURCE);
    let terms = &list.items[0].terms;
    assert_eq!(terms.len(), 2, "consecutive terms share one entry");

    let spans: Vec<String> = terms
        .iter()
        .map(|t| slice(SOURCE, t.pos.expect("a term must carry a position")))
        .collect();
    assert_eq!(spans, vec![":: color", ":: colour"]);
}

#[test]
fn a_description_spans_its_marker_and_content() {
    let list = definition_list(SOURCE);
    let defs = &list.items[0].definitions;
    assert_eq!(defs.len(), 2);

    let spans: Vec<String> = defs
        .iter()
        .map(|d| slice(SOURCE, d.pos.expect("a description must carry a position")))
        .collect();
    assert_eq!(spans, vec![":  The visual property.", ":  A pigment."]);
}

/// The offsets are the half that was silently wrong: line and column were
/// already right while both offsets stayed 0.
#[test]
fn neither_publishes_a_zero_length_placeholder() {
    let list = definition_list(SOURCE);
    for term in &list.items[0].terms {
        let pos = term.pos.expect("a term position");
        assert_ne!(
            pos.start_offset, pos.end_offset,
            "the term span selects nothing"
        );
    }
    for def in &list.items[0].definitions {
        let pos = def.pos.expect("a description position");
        assert_ne!(
            pos.start_offset, pos.end_offset,
            "the description span selects nothing"
        );
    }
}

/// A description that folds continuation lines is ONE region, not just its
/// opening line - the cursor has already passed those lines when the span is
/// taken, so reading it too early would clip them off.
#[test]
fn a_multi_line_description_spans_every_line_it_folded() {
    let source = ":: term\n:  first line\n   second line\n";
    let list = definition_list(source);
    let pos = list.items[0].definitions[0]
        .pos
        .expect("a description position");

    let text = slice(source, pos);
    assert!(text.contains("first line"), "{text:?}");
    assert!(
        text.contains("second line"),
        "the folded line fell outside: {text:?}"
    );
}
