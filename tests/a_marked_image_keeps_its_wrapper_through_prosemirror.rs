//! A paragraph holding one image comes back a BLOCK image, because that is what
//! Carve spells and the editor's wrapper is not the author's. An image carrying
//! MARKS is not that shape: its marks are the wrapper - a link, a span, an
//! emphasis - and a block image has nowhere to carry them.
//!
//! Unwrapping one anyway returned the image alone and dropped the wrapper and the
//! paragraph with it, and the forward direction reported no loss at all, so the
//! round trip read as exact while it lost the link. Corpus 525-5 and 525-9 are
//! the shapes (markup-carve/carve#2586), which is what surfaced it.

use carve::{from_prosemirror, parse_with_options, render_html, to_prosemirror, Options};

fn round_trip(source: &str) -> (String, String, bool) {
    let doc = parse_with_options(source, &Options::default().with_positions(true));
    let pm = to_prosemirror(&doc);
    let back = from_prosemirror(&pm.json).expect("the bridge reads its own tree");
    (
        render_html(&doc).unwrap().trim().to_string(),
        render_html(&back).unwrap().trim().to_string(),
        pm.dropped.is_empty() && pm.degraded.is_empty(),
    )
}

/// Corpus 525-9 and 525-5. Each reports nothing, so each has to be exact.
#[test]
fn a_marked_image_keeps_its_wrapper() {
    for source in ["[![a](/i)](/u)\n", "[![a](/i)]{.c}\n"] {
        let (before, after, strict) = round_trip(source);
        assert!(strict, "{source:?} reports a loss: {before}");
        assert_eq!(after, before, "{source:?}");
        assert!(after.contains("<img"), "{after}");
    }
}

/// Every mark, since the rule is about carrying marks and not about links.
#[test]
fn every_mark_on_the_image_survives() {
    for source in [
        "[![a](/i)](/u)\n",
        "[![a](/i)]{.c}\n",
        "/![a](/i)/\n",
        "*![a](/i)*\n",
    ] {
        let (before, after, _) = round_trip(source);
        assert_eq!(after, before, "{source:?}");
    }
}

/// The unwrap itself is the behavior this must not take away: an UNMARKED lone
/// image is a block image in Carve and comes back as one.
#[test]
fn an_unmarked_lone_image_is_still_unwrapped() {
    let (before, after, _) = round_trip("![a](/i)\n");
    assert_eq!(before, "<img src=\"/i\" alt=\"a\">");
    assert_eq!(after, before);
}

/// The other exception the unwrap already had, which the mark test sits beside:
/// an image whose reference never resolved renders as literal text, so it is
/// inline content and needs its paragraph.
#[test]
fn an_unresolved_reference_image_still_keeps_its_paragraph() {
    let (before, after, _) = round_trip("![a][gone]\n");
    assert_eq!(before, "<p>![a][gone]</p>");
    assert_eq!(after, before);
}

/// An image beside other content was never the unwrap's case, and stays put.
#[test]
fn an_image_beside_text_is_untouched() {
    let (before, after, _) = round_trip("see ![a](/i) now\n");
    assert_eq!(after, before);
    assert!(before.starts_with("<p>"), "{before}");
}
