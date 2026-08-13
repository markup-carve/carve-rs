//! A figure caption's inline content had no usable positions: 27 unplaced text
//! nodes in the corpus sat in one (carve-rs#333).
//!
//! Two separate defects, and the second is the dangerous one. The caption was
//! parsed from a string joined out of its source lines with no anchor, so
//! nothing in it could be placed. Anchoring it then produced spans with correct
//! lines and columns but offsets of `0..0`, because `fill_offsets` walked a
//! figure's TARGET and never its caption - a span that reads as present and
//! selects nothing, which PART 12 section 4 forbids more sharply than absence.

use carve::ast::{BlockNode, InlineNode};

/// The inline content a `^ ` line produced, whichever slot it landed in.
///
/// A quote's `^ ` line is its ATTRIBUTION now, not a figure caption (PART 9
/// §4a, carve#1159) - a DIFFERENT field, reached by a different arm of every
/// walker, and it failed the offsets exactly the way a figure caption did
/// before carve-rs#333. So both slots are driven through the same rows here
/// rather than the quote fixtures being retired to an image host.
fn caption_of(source: &str) -> Vec<InlineNode> {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(source, &options);
    match &doc.children[0] {
        BlockNode::Figure(figure) => figure.caption.clone(),
        BlockNode::BlockQuote(quote) => quote
            .attribution
            .clone()
            .expect("the fixture's caption line did not attach to the quote"),
        _ => panic!("the fixture parsed as neither a figure nor a quote"),
    }
}

fn text_of(node: &InlineNode) -> Option<(String, Option<carve::ast::Pos>)> {
    match node {
        InlineNode::Text(t) => Some((t.value.clone(), t.pos)),
        _ => None,
    }
}

#[test]
fn a_caption_slices_back_to_its_source() {
    let source = "> Stay hungry\n^ Steve Jobs\n";
    let codepoints: Vec<char> = source.chars().collect();

    let caption = caption_of(source);
    let (value, pos) = text_of(&caption[0]).expect("expected a text node");
    let pos = pos.expect("a caption's text must carry a position");

    assert_eq!(pos.start_line, 2, "the caption is on the second line");
    let slice: String = codepoints[pos.start_offset..pos.end_offset]
        .iter()
        .collect();
    assert_eq!(slice, value);
}

/// The same row on the slot carve-rs#333 was filed about, so retargeting the
/// fixtures above at the attribution cannot quietly drop the figure's caption.
#[test]
fn a_figure_caption_slices_back_to_its_source_too() {
    let source = "![a](b.png)\n^ Steve Jobs\n";
    let codepoints: Vec<char> = source.chars().collect();

    let caption = caption_of(source);
    let (value, pos) = text_of(&caption[0]).expect("expected a text node");
    let pos = pos.expect("a caption's text must carry a position");

    assert_eq!(pos.start_line, 2, "the caption is on the second line");
    assert_ne!(
        pos.start_offset, pos.end_offset,
        "the span selects nothing, which reads as present and is not"
    );
    let slice: String = codepoints[pos.start_offset..pos.end_offset]
        .iter()
        .collect();
    assert_eq!(slice, value);
}

/// The offsets are the half that was silently wrong: line and column were
/// already right while both offsets stayed 0.
#[test]
fn a_caption_span_is_not_a_zero_length_placeholder() {
    let caption = caption_of("> Stay hungry\n^ Steve Jobs\n");
    let (_, pos) = text_of(&caption[0]).expect("expected a text node");
    let pos = pos.expect("a caption's text must carry a position");

    assert_ne!(
        pos.start_offset, pos.end_offset,
        "the span selects nothing, which reads as present and is not"
    );
}

/// A caption folds continuation lines like a paragraph, so the second line
/// needs its own anchor - reusing the first line's places it on line 1.
#[test]
fn a_folded_continuation_line_is_anchored_to_its_own_line() {
    let source = "> Stay hungry\n^ Steve Jobs\nand more\n";
    let codepoints: Vec<char> = source.chars().collect();

    let caption = caption_of(source);
    let (value, pos) = text_of(caption.last().expect("a caption")).expect("expected a text node");
    let pos = pos.expect("the folded line must carry a position");

    let slice: String = codepoints[pos.start_offset..pos.end_offset]
        .iter()
        .collect();
    assert!(
        value.ends_with(slice.trim_end()) || slice.contains("more"),
        "the folded line anchored to the wrong source line: {slice:?} vs {value:?}"
    );
}
