//! A caption's text span covers the caption, not the `^ ` marker
//! (PART 12, carve-rs#620).
//!
//! A reference image with a caption is promoted to a figure AFTER parsing:
//! it arrives as `Paragraph[Image, SoftBreak, Text("^ cap")]` and the promotion
//! strips the marker from the text. It stripped the VALUE and left the POSITION
//! covering the marker, so the node's span no longer sliced back to its own
//! content - `9..14` reading `^ cap` for the value `cap`.
//!
//! The direct-image form never had this. It parses the caption from the text
//! after the marker, so its anchor is right to begin with; only this
//! post-parse promotion edits a node the parser has already positioned.
//!
//! carve-php reports `off 11 14 col 3` for the same input, which is what this
//! now matches.

use carve::ast::{BlockNode, InlineNode};

/// Every text node's span, sliced back out of the source it came from.
///
/// Offsets count CODEPOINTS (PART 12), so the slice is taken over a char
/// vector rather than bytes - the same way `position_spans_match_source` does.
fn text_spans(src: &str) -> Vec<(String, String)> {
    let options = carve::Options {
        positions: true,
        ..Default::default()
    };
    let doc = carve::parse_with_options(src, &options);
    let codepoints: Vec<char> = src.chars().collect();
    let mut out = Vec::new();

    fn inlines(ns: &[InlineNode], cp: &[char], out: &mut Vec<(String, String)>) {
        for n in ns {
            if let InlineNode::Text(t) = n {
                if let Some(p) = &t.pos {
                    let sliced: String = cp[p.start_offset..p.end_offset].iter().collect();
                    out.push((t.value.clone(), sliced));
                }
            }
        }
    }

    for b in &doc.children {
        match b {
            BlockNode::Figure(f) => inlines(&f.caption, &codepoints, &mut out),
            BlockNode::Paragraph(p) => inlines(&p.children, &codepoints, &mut out),
            _ => {}
        }
    }
    out
}

#[test]
fn a_reference_image_caption_span_is_the_caption() {
    let src = "![a][ok]\n^ cap\n\n[ok]: /p.png\n";
    for (value, sliced) in text_spans(src) {
        assert_eq!(
            value, sliced,
            "a text node's span does not slice back to its own value"
        );
    }
}

#[test]
fn the_direct_image_form_still_agrees() {
    // The path that was always right, kept so a repair of the promotion cannot
    // quietly change it.
    let src = "![a](/p.png)\n^ cap\n";
    for (value, sliced) in text_spans(src) {
        assert_eq!(value, sliced, "direct-image caption span moved");
    }
}

#[test]
fn a_multi_word_caption_keeps_its_whole_span() {
    // The marker is stripped once, from the front. A longer caption must not
    // lose more than the marker.
    let src = "![a][ok]\n^ two words here\n\n[ok]: /p.png\n";
    let spans = text_spans(src);
    assert!(
        spans.iter().any(|(v, _)| v == "two words here"),
        "caption text was truncated: {spans:?}"
    );
    for (value, sliced) in spans {
        assert_eq!(value, sliced, "span does not match its value");
    }
}

#[test]
fn an_unresolved_reference_image_is_untouched() {
    // No promotion happens, so nothing edits the text node and its span is
    // whatever the parser set. Pinned because the fix sits inside the
    // promotion branch and must not reach outside it.
    let src = "![a][missing]\n^ cap\n";
    for (value, sliced) in text_spans(src) {
        assert_eq!(value, sliced, "unresolved-reference span moved");
    }
}
