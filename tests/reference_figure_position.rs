//! A `figure` built over a REFERENCE image publishes the same span the inline
//! form does.
//!
//! A direct `![a](/p.png)` + `^ cap` becomes a `Figure` at parse time and is
//! placed there. A REFERENCE `![a][ok]` + `^ cap` cannot be: whether the label
//! resolves is not known until the definitions are collected, so the construct
//! arrives at `promote_block_images` as an ordinary `Paragraph` and is rebuilt
//! into a `Figure` afterwards. That rebuild passed `pos: None` and the span was
//! lost, while the paragraph it dismantled had carried the right one all along
//! (carve-rs#737).
//!
//! PART 12 §4 exempts a REASSEMBLED node, and this one does not qualify:
//! `docs/ast-json.md` narrows the exemption to nodes that CANNOT be placed, not
//! nodes that have not been placed yet. The two lines are contiguous, the same
//! engine publishes an honest span for the inline form of the same construct,
//! and carve-js and carve-php both place the reference form.
//!
//! markup-carve/carve#913 rules `pos` MARKUP-INCLUSIVE - a span covers the
//! construct as written - and makes the containment invariant part of the
//! ruling: a parent's span must contain every child's. Both are asserted below,
//! and separately, so a future change to the convention cannot silently drop
//! containment.
//!
//! THE TRAP THESE TESTS ARE WRITTEN AROUND: positions are OPT-IN in this
//! engine. A probe that forgets `Options { positions: true, .. }` reads `None`
//! everywhere and passes against the unfixed engine, comparing nothing to
//! nothing. Every assertion here unwraps the span with an explicit message
//! rather than comparing `Option`s, so an absent field fails loudly instead of
//! matching an expectation that is also absent.
//!
//! Measured against carve-js 3d95e94 and carve-php 876e312, which both give
//! `[0,14]` on the spec corpus document `207-a-reference-image-takes-a-caption`.

use carve::{parse_with_options, BlockNode, Figure, FigureTarget, Options, Pos};

fn parse(src: &str) -> carve::Document {
    parse_with_options(
        src,
        &Options {
            positions: true,
            ..Default::default()
        },
    )
}

/// The document's single figure, or a failure naming what was there instead.
fn figure(src: &str) -> Figure {
    let doc = parse(src);
    for block in doc.children {
        if let BlockNode::Figure(f) = block {
            return f;
        }
    }
    panic!("no figure in the parsed document");
}

/// Unwrap a span, failing loudly when the field is ABSENT rather than treating
/// a missing position as a comparable value. This is the whole point: an
/// `Option`-to-`Option` comparison passes on the unfixed engine.
fn require(pos: Option<Pos>, what: &str) -> Pos {
    pos.unwrap_or_else(|| panic!("{what} published NO position; the field must be present"))
}

fn offsets(pos: &Pos) -> (usize, usize) {
    (pos.start_offset, pos.end_offset)
}

/// Slice by codepoint offsets, the unit PART 12 §4 pins.
fn slice(src: &str, pos: &Pos) -> String {
    let chars: Vec<char> = src.chars().collect();
    chars[pos.start_offset..pos.end_offset.min(chars.len())]
        .iter()
        .collect()
}

const REF: &str = "![a][ok]\n^ cap\n\n[ok]: /p.png\n";
const INLINE: &str = "![a](/p.png)\n^ cap\n";

#[test]
fn a_reference_figure_publishes_a_position() {
    // The bare presence check, kept separate from the value check below so a
    // failure says which of the two went wrong. This is the assertion that was
    // red before the fix.
    let f = figure(REF);
    require(f.pos, "the reference figure");
}

#[test]
fn the_reference_figures_span_covers_the_image_and_the_caption() {
    // The VALUE, which is the assertion that matters: a span that is merely
    // present can point anywhere. `[0,14]` runs from the first `!` of the image
    // line through the last character of the caption line - the construct as
    // written, markup included (markup-carve/carve#913). carve-js and carve-php
    // give the same two numbers.
    let f = figure(REF);
    let pos = require(f.pos, "the reference figure");
    assert_eq!(offsets(&pos), (0, 14));
    assert_eq!(slice(REF, &pos), "![a][ok]\n^ cap");
}

#[test]
fn the_reference_figures_span_contains_both_of_its_children() {
    // CONTAINMENT, asserted on its own. markup-carve/carve#913 makes this part
    // of the ruling rather than a consequence of it: whichever convention wins,
    // a parent's span must contain every child's, or the span tree is not a
    // tree.
    let f = figure(REF);
    let parent = require(f.pos, "the reference figure");
    let FigureTarget::Image(image) = &*f.target else {
        panic!("the figure's target is not an image");
    };
    let child = require(image.pos, "the figure's image");
    assert!(
        parent.start_offset <= child.start_offset && child.end_offset <= parent.end_offset,
        "figure {:?} does not contain its image {:?}",
        offsets(&parent),
        offsets(&child)
    );
    let caption = f
        .caption
        .iter()
        .find_map(|n| match n {
            carve::InlineNode::Text(t) => t.pos,
            _ => None,
        })
        .expect("the caption published no positioned text");
    assert!(
        parent.start_offset <= caption.start_offset && caption.end_offset <= parent.end_offset,
        "figure {:?} does not contain its caption text {:?}",
        offsets(&parent),
        offsets(&caption)
    );
}

#[test]
fn the_caption_text_span_excludes_the_marker() {
    // CONTROL (holds before this fix too). Asserted because the promotion EDITS
    // this span (carve-rs#620): the `^ ` marker is stripped from the value, and
    // the start offset advances with it so the span still slices back to its own
    // content. A change that carried the paragraph's span onto the figure while
    // breaking that adjustment would pass every other test here - dropping the
    // adjustment kills this assertion and nothing else.
    let f = figure(REF);
    let caption = f
        .caption
        .iter()
        .find_map(|n| match n {
            carve::InlineNode::Text(t) => t.pos,
            _ => None,
        })
        .expect("the caption published no positioned text");
    assert_eq!(offsets(&caption), (11, 14));
    assert_eq!(slice(REF, &caption), "cap");
}

#[test]
fn control_the_inline_figure_is_unchanged() {
    // CONTROL. The direct-image form is placed at parse time on a different
    // path, and it was already right - it is what showed the reference form's
    // omission was not forced. `[0,18]` covers its own two lines; the numbers
    // differ from the reference form's only because `![a](/p.png)` is longer
    // than `![a][ok]`.
    let f = figure(INLINE);
    let pos = require(f.pos, "the inline figure");
    assert_eq!(offsets(&pos), (0, 18));
    assert_eq!(slice(INLINE, &pos), "![a](/p.png)\n^ cap");
}

#[test]
fn control_an_unresolved_reference_does_not_become_a_figure_at_all() {
    // CONTROL, and the boundary of the promotion this fix touches. With no
    // definition the image stays unresolved, keeps its raw source, and the
    // block remains a paragraph - so there is no figure to place and the fix
    // must not manufacture one.
    let doc = parse("![a][nope]\n^ cap\n");
    assert!(
        !doc.children
            .iter()
            .any(|b| matches!(b, BlockNode::Figure(_))),
        "an unresolved reference image must not promote to a figure"
    );
}

#[test]
fn a_multi_line_caption_extends_the_figures_span() {
    // The span is taken from the paragraph rather than recomputed, so it has to
    // follow the construct when the construct grows. A second caption line is
    // inside the same paragraph, so the figure must reach the end of it - a fix
    // that hard-coded "image line plus one" would stop short here.
    let src = "![a][ok]\n^ cap\n  more\n\n[ok]: /p.png\n";
    let f = figure(src);
    let pos = require(f.pos, "the reference figure with a two-line caption");
    assert_eq!(slice(src, &pos), "![a][ok]\n^ cap\n  more");
}
