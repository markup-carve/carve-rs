//! A paragraph produced by the over-cap degrade publishes a position, and so do
//! the inlines in it.
//!
//! PART 9 §25: past the nesting cap an opener "becomes literal paragraph text" -
//! it degrades, it does not vanish. The flattened run that results is ONE
//! paragraph of contiguous, verbatim source lines. Nothing about it is
//! REASSEMBLED in PART 12 §4's sense, so §4's exemption does not reach it:
//! `docs/ast-json.md` narrows that to nodes which CANNOT be placed, and this one
//! can - carve-js places it, and slicing its span returns exactly the flattened
//! text (carve-rs#716).
//!
//! markup-carve/carve#913 rules `pos` MARKUP-INCLUSIVE and makes the containment
//! invariant part of the ruling: a parent's span must contain every child's.
//! Both are asserted here, and separately.
//!
//! THE TRAP: positions are OPT-IN in this engine. A probe that forgets
//! `Options { positions: true, .. }` reads `None` everywhere and passes against
//! the unfixed engine. Every assertion below requires the field to be PRESENT
//! before comparing anything, and `require` panics by name when it is not.
//!
//! TWO PRODUCERS, not one. The ticket named the colon-fence document and warned
//! that the `DepthGuard::enter()` else-branch in `parse_blocks` is NOT the site
//! that builds it - which is correct, and is why the first attempt at this fix
//! changed nothing. That branch is a SECOND producer all the same: a deep quote
//! ladder and a deep list ladder both arrive there with positions on, and it
//! published no span either. Both paths are asserted.
//!
//! A debug `cargo test` build has much larger un-inlined frames than a release
//! build, so a worst-case-depth probe runs on a generous worker stack, the same
//! convention `ast_json_roundtrip_depth.rs` uses. The property under test is the
//! SPAN, not the frame size.

use carve::{parse_with_options, BlockNode, InlineNode, Options, Pos};

fn on_big_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn worker")
        .join()
        .expect("worker must return, not abort");
}

fn parse(src: &str) -> carve::Document {
    parse_with_options(
        src,
        &Options {
            positions: true,
            ..Default::default()
        },
    )
}

/// Fail loudly when the field is ABSENT rather than comparing `None` to `None`.
fn require(pos: Option<Pos>, what: &str) -> Pos {
    pos.unwrap_or_else(|| panic!("{what} published NO position; the field must be present"))
}

fn slice(src: &str, pos: &Pos) -> String {
    let chars: Vec<char> = src.chars().collect();
    chars[pos.start_offset..pos.end_offset.min(chars.len())]
        .iter()
        .collect()
}

/// Walk to the single deepest paragraph, whatever containers wrap it.
fn deepest_paragraph(doc: &carve::Document) -> carve::Paragraph {
    fn walk(blocks: &[BlockNode], out: &mut Vec<carve::Paragraph>) {
        for b in blocks {
            match b {
                BlockNode::Paragraph(p) => out.push(p.clone()),
                BlockNode::Admonition(a) => walk(&a.children, out),
                BlockNode::Div(d) => walk(&d.children, out),
                BlockNode::BlockQuote(q) => walk(&q.children, out),
                BlockNode::List(l) => {
                    for item in &l.items {
                        walk(&item.children, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    walk(&doc.children, &mut found);
    found.pop().expect("no paragraph in the document")
}

/// Every placed child must sit inside the parent's span.
fn assert_contains(parent: &Pos, children: &[InlineNode]) {
    for child in children {
        let child_pos = match child {
            InlineNode::Text(t) => t.pos,
            InlineNode::SoftBreak(b) => b.pos,
            _ => None,
        };
        if let Some(c) = child_pos {
            assert!(
                parent.start_offset <= c.start_offset && c.end_offset <= parent.end_offset,
                "parent [{},{}] does not contain child [{},{}]",
                parent.start_offset,
                parent.end_offset,
                c.start_offset,
                c.end_offset
            );
        }
    }
}

/// The corpus shape: openers past the cap, then a body line.
fn colon_ladder(n: usize) -> String {
    let mut s = String::new();
    for _ in 0..n {
        s.push_str(":::: note\n");
    }
    s.push_str("x\n");
    s
}

#[test]
fn the_over_cap_paragraph_publishes_a_position() {
    on_big_stack(|| {
        let src = colon_ladder(203);
        let para = deepest_paragraph(&parse(&src));
        require(para.pos, "the over-cap paragraph");
    });
}

#[test]
fn the_over_cap_paragraphs_span_slices_back_to_the_flattened_run() {
    on_big_stack(|| {
        // THE VALUE. A span that is merely present can point anywhere; this is
        // the assertion carve#913 asks for. The three openers past the cap plus
        // the body line, exactly - which is what carve-js reports on the corpus
        // document, offsets 2000..2031 over its 200-deep ladder.
        let src = colon_ladder(203);
        let para = deepest_paragraph(&parse(&src));
        let pos = require(para.pos, "the over-cap paragraph");
        assert_eq!(
            slice(&src, &pos),
            ":::: note\n:::: note\n:::: note\nx",
            "the span must cover the degraded openers AND the body line"
        );
    });
}

#[test]
fn every_inline_in_the_over_cap_paragraph_is_placed_and_contained() {
    on_big_stack(|| {
        // The ticket names `paragraph` and `soft_break`; the same site drops the
        // `text` runs too - one degrade path, one fix - so all eight nodes on
        // the corpus document are asserted together rather than in two groups.
        let src = colon_ladder(203);
        let para = deepest_paragraph(&parse(&src));
        let pos = require(para.pos, "the over-cap paragraph");

        let mut texts = 0;
        let mut breaks = 0;
        for child in &para.children {
            match child {
                InlineNode::Text(t) => {
                    let p = require(t.pos, "a text run in the over-cap paragraph");
                    assert_eq!(slice(&src, &p), t.value, "a text span must slice to itself");
                    texts += 1;
                }
                InlineNode::SoftBreak(b) => {
                    require(b.pos, "a soft break in the over-cap paragraph");
                    breaks += 1;
                }
                other => panic!("unexpected inline in the flattened run: {other:?}"),
            }
        }
        assert_eq!((texts, breaks), (4, 3), "the shape the ticket measured");
        assert_contains(&pos, &para.children);
    });
}

#[test]
fn the_second_producer_places_a_deep_quote_ladder() {
    on_big_stack(|| {
        // The `DepthGuard::enter()` else-branch in `parse_blocks`. The ticket
        // correctly warns it is NOT the site that builds the colon-fence
        // document - patching it there changed nothing - but a quote ladder
        // reaches it with positions on and it published no span either. Without
        // this assertion the second producer stays broken behind a fixed first
        // one, which is how this repo's duplicated-state bugs have survived.
        let src = format!("{}x\n", "> ".repeat(300));
        let para = deepest_paragraph(&parse(&src));
        let pos = require(para.pos, "the over-cap paragraph of a quote ladder");
        let sliced = slice(&src, &pos);
        assert!(
            sliced.ends_with("> x"),
            "the span must reach the body line: {:?}",
            &sliced[sliced.len().saturating_sub(12)..]
        );
        assert!(
            sliced.starts_with("> "),
            "the span must start at a degraded marker, markup included"
        );
        assert_contains(&pos, &para.children);
    });
}

#[test]
fn the_second_producer_places_a_deep_list_ladder() {
    on_big_stack(|| {
        // The other shape that reaches the same branch, through a different
        // container. Kept because the quote path and the list path arrive with
        // different amounts stripped from the front of each line, which is what
        // the column half of the span is computed from.
        let mut src = String::from("- x\n");
        for i in 1..300 {
            src.push_str(&format!("{}- d\n", "  ".repeat(i)));
        }
        let para = deepest_paragraph(&parse(&src));
        let pos = require(para.pos, "the over-cap paragraph of a list ladder");
        assert!(
            slice(&src, &pos).ends_with("- d"),
            "the span must reach the last degraded line"
        );
        assert_contains(&pos, &para.children);
    });
}

#[test]
fn control_a_ladder_inside_the_cap_is_unchanged() {
    on_big_stack(|| {
        // CONTROL. Below the cap nothing degrades, the body is an ordinary
        // paragraph parsed the ordinary way, and it was already placed. This is
        // what must not move - and it is what shows the assertions above are
        // about the degrade path rather than about paragraphs in general.
        let src = colon_ladder(3);
        let para = deepest_paragraph(&parse(&src));
        let pos = require(para.pos, "the paragraph inside the cap");
        assert_eq!(slice(&src, &pos), "x");
    });
}

#[test]
fn control_positions_off_still_publishes_none() {
    on_big_stack(|| {
        // CONTROL, and the other half of the opt-in trap. With positions OFF the
        // field must stay absent: a fix that placed nodes unconditionally would
        // pass every assertion above and quietly cost every caller who did not
        // ask for spans.
        let src = colon_ladder(203);
        let doc = parse_with_options(&src, &Options::default());
        let para = deepest_paragraph(&doc);
        assert!(para.pos.is_none(), "positions were not requested");
        for child in &para.children {
            match child {
                InlineNode::Text(t) => assert!(t.pos.is_none(), "text span without a request"),
                InlineNode::SoftBreak(b) => {
                    assert!(b.pos.is_none(), "break span without a request")
                }
                _ => {}
            }
        }
    });
}
