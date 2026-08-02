//! The no-break-space placeholder is a PUBLISHED value, so which code point it
//! is matters for interoperability (carve-rs#404).
//!
//! This engine used U+E001 where the reference publishes U+E000, so a consumer
//! that special-cased the reference's spelling got nothing here - and the shape
//! check cannot catch it, because both engines publish a `text` node with a
//! `value` and the schema does not constrain which code points go in one.
//!
//! U+E001 and U+E002 are now the WRITER's staging markers, moved to U+E010.. so
//! the published range is free. A test that only checked the published value
//! would not notice them colliding again, so the round-trip cases below matter
//! as much as the code-point one.

const NBSP_PLACEHOLDER: char = '\u{e000}';

fn first_text_value(source: &str) -> String {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    let carve::ast::BlockNode::Paragraph(para) = &doc.children[0] else {
        panic!("expected a paragraph");
    };
    for node in &para.children {
        if let carve::ast::InlineNode::Text(text) = node {
            return text.value.clone();
        }
    }
    panic!("no text node");
}

#[test]
fn an_escaped_space_publishes_the_reference_code_point() {
    let source = format!("a{} b\n", '\\');
    let value = first_text_value(&source);

    assert!(
        value.contains(NBSP_PLACEHOLDER),
        "expected U+E000, got {:?}",
        value.chars().map(|c| c as u32).collect::<Vec<_>>()
    );
    assert!(
        !value.contains('\u{e001}') && !value.contains('\u{e002}'),
        "a writer staging marker leaked into a published value: {:?}",
        value.chars().map(|c| c as u32).collect::<Vec<_>>()
    );
}

/// The placeholder exists to keep an authored escape distinct from a typed
/// no-break space. Both render `&nbsp;`, so only the formatter shows the
/// difference - which is the whole reason a real U+00A0 cannot be published
/// instead.
#[test]
fn the_placeholder_still_separates_an_escape_from_a_typed_space() {
    let escaped = format!("a{} b\n", '\\');
    let typed = "a\u{00a0}b\n";

    assert!(first_text_value(&escaped).contains(NBSP_PLACEHOLDER));
    assert!(first_text_value(typed).contains('\u{00a0}'));
    assert!(!first_text_value(typed).contains(NBSP_PLACEHOLDER));

    assert_eq!(
        carve::to_carve(&escaped),
        escaped,
        "the escape did not round-trip"
    );
    assert_eq!(
        carve::to_carve(typed),
        typed,
        "the typed space did not round-trip"
    );
}

/// Trailing whitespace is staged through the markers that moved. If they ever
/// collide with the published range again, this is where it shows: the staged
/// space would be indistinguishable from document content and survive into the
/// output.
#[test]
fn trailing_whitespace_still_round_trips_after_the_markers_moved() {
    for source in ["a  \nb\n", "a\t\nb\n", "text\n"] {
        let out = carve::to_carve(source);
        assert!(
            !out.contains('\u{e010}') && !out.contains('\u{e011}') && !out.contains('\u{e012}'),
            "a staging marker survived into the output for {source:?}: {out:?}"
        );
    }
}

/// A line-block indent uses the same published placeholder, so it moved too.
#[test]
fn a_line_block_indent_uses_the_published_placeholder() {
    let source = "::: |\nRoses are red,\n  Violets are blue.\n:::\n";
    let doc = carve::parse(source);

    let mut seen = false;
    fn walk(nodes: &[carve::ast::BlockNode], seen: &mut bool) {
        for block in nodes {
            match block {
                carve::ast::BlockNode::Paragraph(p) => {
                    for node in &p.children {
                        if let carve::ast::InlineNode::Text(t) = node {
                            if t.value.starts_with('\u{e000}') {
                                *seen = true;
                            }
                            assert!(
                                !t.value.contains('\u{e001}'),
                                "the indent still uses the old code point: {:?}",
                                t.value
                            );
                        }
                    }
                }
                carve::ast::BlockNode::Div(d) => walk(&d.children, seen),
                carve::ast::BlockNode::LineBlock(l) => walk(&l.children, seen),
                _ => {}
            }
        }
    }
    walk(&doc.children, &mut seen);
    assert!(seen, "no indented verse line carried the placeholder");
}
