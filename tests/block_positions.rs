//! Source spans on block nodes (PART 12 section 4, carve-rs#333).
//!
//! The check that matters is not "a position is present" but "slicing the
//! source by the reported offsets returns the block the author wrote". A span
//! that is merely present can still point at unrelated text, which is worse
//! than no span at all - section 4 requires an implementation that cannot
//! produce a position to omit it rather than invent one.

use carve::{parse_with_options, BlockNode, Options, Pos};

fn positions(src: &str) -> Vec<(&'static str, Option<Pos>)> {
    let options = Options {
        positions: true,
        ..Default::default()
    };
    parse_with_options(src, &options)
        .children
        .iter()
        .map(|b| match b {
            BlockNode::Heading(n) => ("heading", n.pos.clone()),
            BlockNode::Paragraph(n) => ("paragraph", n.pos.clone()),
            BlockNode::ThematicBreak(n) => ("thematic_break", n.pos.clone()),
            BlockNode::CodeBlock(n) => ("code_block", n.pos.clone()),
            BlockNode::RawBlock(n) => ("raw_block", n.pos.clone()),
            BlockNode::Comment(n) => ("comment", n.pos.clone()),
            BlockNode::Div(n) => ("div", n.pos.clone()),
            BlockNode::Admonition(n) => ("admonition", n.pos.clone()),
            _ => ("other", None),
        })
        .collect()
}

/// Slice `src` by codepoint offsets, the unit PART 12 section 4 pins.
fn slice(src: &str, pos: &Pos) -> String {
    let chars: Vec<char> = src.chars().collect();
    chars[pos.start_offset..pos.end_offset.min(chars.len())]
        .iter()
        .collect()
}

#[test]
fn every_span_slices_back_to_its_own_block() {
    let src = "# H\n\n---\n\n```rust\nlet x = 1;\n```\n\n::: note\nbody\n:::\n\n%% a comment\n";
    let expected = [
        ("heading", "# H"),
        ("thematic_break", "---"),
        ("code_block", "```rust\nlet x = 1;\n```"),
        ("admonition", "::: note\nbody\n:::"),
        ("comment", "%% a comment"),
    ];

    let found = positions(src);
    for (name, want) in expected {
        let (_, pos) = found
            .iter()
            .find(|(n, _)| *n == name)
            .unwrap_or_else(|| panic!("no {name} in the tree"));
        let pos = pos
            .clone()
            .unwrap_or_else(|| panic!("{name} has no position"));
        assert_eq!(slice(src, &pos), want, "{name} span points elsewhere");
    }
}

#[test]
fn a_fence_span_covers_the_opener_and_closer_not_just_the_body() {
    let src = "```\nbody\n```\n";
    let (_, pos) = positions(src).into_iter().next().unwrap();
    let pos = pos.expect("code block has a position");
    assert_eq!(pos.start_line, 1);
    assert_eq!(pos.end_line, 3);
    assert_eq!(slice(src, &pos), "```\nbody\n```");
}

#[test]
fn a_raw_block_reports_its_span() {
    let src = "```=html\n<b>x</b>\n```\n";
    let found = positions(src);
    let (_, pos) = found
        .iter()
        .find(|(n, _)| *n == "raw_block")
        .expect("raw block");
    assert_eq!(slice(src, &pos.clone().unwrap()), "```=html\n<b>x</b>\n```");
}

#[test]
fn offsets_count_codepoints_not_bytes() {
    // An astral character ahead of the block: a byte-based offset would land
    // four bytes late and slice into the middle of the fence.
    let src = "\u{1F600}\u{1F600}\n\n---\n";
    let found = positions(src);
    let (_, pos) = found
        .iter()
        .find(|(n, _)| *n == "thematic_break")
        .expect("thematic break");
    assert_eq!(slice(src, &pos.clone().unwrap()), "---");
}

#[test]
fn positions_are_absent_when_the_option_is_off() {
    // Section 4 allows position tracking to be opt-in; what it forbids is a
    // serialized document without positions, not a parse without them.
    let doc = carve::parse("# H\n\n---\n");
    for block in &doc.children {
        match block {
            BlockNode::Heading(n) => assert!(n.pos.is_none()),
            BlockNode::ThematicBreak(n) => assert!(n.pos.is_none()),
            _ => {}
        }
    }
}
