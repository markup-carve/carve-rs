//! A line block's breaks carry spans even when a tab unanchors the line
//! (PART 12 section 4, carve-rs#480).
//!
//! A tab expands to placeholders and shifts every offset WITHIN a line, so the
//! stanza's inline text is deliberately left unplaced - section 4 prefers no
//! span to a wrong one. A break is a different fact: it is the newline ENDING
//! its line, and a tab does not move a line's own end.

use carve::{parse_with_options, BlockNode, InlineNode, Options};

fn break_slices(src: &str) -> Vec<String> {
    let options = Options {
        positions: true,
        ..Default::default()
    };
    let doc = parse_with_options(src, &options);
    let cps: Vec<char> = src.chars().collect();

    let mut out = Vec::new();
    for block in &doc.children {
        let BlockNode::LineBlock(lb) = block else {
            continue;
        };
        for child in &lb.children {
            let BlockNode::Paragraph(p) = child else {
                continue;
            };
            for inline in &p.children {
                if let InlineNode::HardBreak(b) = inline {
                    let pos = b.pos.expect("a line block's break must carry a span");
                    out.push(cps[pos.start_offset..pos.end_offset].iter().collect());
                }
            }
        }
    }
    out
}

#[test]
fn a_tab_bearing_stanza_still_places_its_breaks() {
    // Before the fix these two arrived with no span at all: the tab unanchored
    // the whole line, and the break inherited that.
    let slices = break_slices("::: |\ntab\tgap\nwide\t\tgap\n\tlead\n:::\n");

    assert_eq!(slices, vec!["\n".to_string(), "\n".to_string()]);
}

#[test]
fn a_tab_free_stanza_is_unchanged() {
    let slices = break_slices("::: |\nRoses are red,\nViolets are blue.\n:::\n");

    assert_eq!(slices, vec!["\n".to_string()]);
}

#[test]
fn the_stanza_paragraph_spans_its_own_lines() {
    // The enclosing paragraph is line geometry too, and carve-rs already had
    // this right where carve-js did not. Pinned so it stays that way.
    let src = "::: |\ntab\tgap\nwide\t\tgap\n\tlead\n:::\n";
    let options = Options {
        positions: true,
        ..Default::default()
    };
    let doc = parse_with_options(src, &options);
    let cps: Vec<char> = src.chars().collect();

    let BlockNode::LineBlock(lb) = &doc.children[0] else {
        panic!("expected a line block");
    };
    let BlockNode::Paragraph(p) = &lb.children[0] else {
        panic!("expected a paragraph");
    };
    let pos = p.pos.expect("the stanza paragraph must carry a span");
    let slice: String = cps[pos.start_offset..pos.end_offset].iter().collect();

    assert_eq!(slice, "tab\tgap\nwide\t\tgap\n\tlead");
}
