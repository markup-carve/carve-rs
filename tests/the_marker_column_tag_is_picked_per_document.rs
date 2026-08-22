//! The list writer's marker-column tag is PICKED per document, not fixed.
//!
//! §17 L3 puts an item's `+` continuation marker, and the block it attaches, at
//! the item's MARKER column rather than its content column. The block is
//! produced deep inside the item body, where the prefix is not yet known, so
//! the writer TAGS its lines and the prefix loop strips the tag back off BY
//! POSITION - a line that starts with it.
//!
//! The tag used to be the fixed `U+E005`, the one writer marker left out of the
//! scheme markup-carve/carve#678 settled. A continuation line the AUTHOR opened
//! with `U+E005` therefore answered that test: the character was eaten AND the
//! line was written at the marker column, which moved the block out of its item
//! (markup-carve/carve-rs#1226). carve-js moved the same marker into its own
//! picked run in markup-carve/carve-js#1289, carve-php in
//! markup-carve/carve-php#1087.
//!
//! PICKING IS NOT RELOCATING. Moving the tag to a different FIXED code point
//! answers the two inputs above and leaves the defect where it was, one address
//! along - which is what `the_run_moves_when_the_document_occupies_it` is here
//! to catch: it occupies every default the writer prefers, so a tag that is not
//! actually picked is still standing on document content.

fn fmt(source: &str) -> String {
    carve::render_carve(&carve::parse(source)).unwrap()
}

/// PART 11 §1: the writer's output renders to the same HTML as its input.
fn round_trips(source: &str) {
    assert_eq!(
        carve::to_html(&fmt(source)),
        carve::to_html(source),
        "to_html(fmt(x)) != to_html(x) for {source:?} -> {:?}",
        fmt(source)
    );
}

#[test]
fn an_authored_tag_opening_a_continuation_line_survives() {
    // Before: `- a` / `x` - the character eaten and the line at column 0, so one
    // item holding two lines came back as a list plus a paragraph.
    let source = "- a\n  \u{e005}x\n";
    assert_eq!(fmt(source), source);
    round_trips(source);
}

#[test]
fn an_authored_tag_after_a_blank_line_keeps_its_paragraph_in_the_item() {
    let source = "- a\n\n  \u{e005}text\n";
    round_trips(source);
    // The failure was a change of BLOCK STRUCTURE, not only a lost character:
    // the paragraph left the item. Assert the shape, not just the bytes.
    let written = carve::to_html(&fmt(source));
    assert!(
        written.contains("<li>") && !written.contains("</ul>\n<p>"),
        "the paragraph left its item: {written}"
    );
}

/// Every writer-only sentinel, in the one context that consumes each of them,
/// against a private-use control no mechanism in the crate claims.
#[test]
fn every_writer_sentinel_survives_the_context_that_consumes_it() {
    const CONTROL: char = '\u{e0ff}';
    // U+E000 is out of scope: it is the PARSER's marker for a non-breaking
    // space, published in a text node, so an authored one is indistinguishable
    // from a parsed nbsp before any writer runs (the other half of carve#678).
    for code in 0xe001..=0xe014u32 {
        let cp = char::from_u32(code).unwrap();
        for shape in [
            "a{}b\n",               // inline text
            "```\na{}z\n```\n",     // verbatim content
            "- item\n\n  {}cont\n", // a list item's continuation line
        ] {
            let source = shape.replace("{}", &cp.to_string());
            let control = shape.replace("{}", &CONTROL.to_string());
            // The payload is normalized away, so only STRUCTURE can differ
            // between the candidate and the control.
            assert_eq!(
                fmt(&source).replace(cp, "\u{fffd}"),
                fmt(&control).replace(CONTROL, "\u{fffd}"),
                "U+{code:04X} is not written like an ordinary character in {shape:?}"
            );
            round_trips(&source);
        }
    }
}

#[test]
fn the_run_moves_when_the_document_occupies_it() {
    // Occupy every code point the writer prefers, so no slot can answer from
    // its default and the marker column has to move with the rest.
    let occupied: String = ('\u{e001}'..='\u{e020}').collect();
    let source = format!("{occupied}\n\n- a\n  \u{e005}x\n");
    assert_eq!(fmt(&source), source);
    round_trips(&source);
}

/// BOUND: the tag still does its job, whichever code point carries it.
///
/// A run that satisfied the cases above by DELETING the tag mechanism would
/// write the continuation marker at the item's content column instead, which is
/// markup-carve/carve#861, and would put a later child in the run inside the
/// block above it, which is markup-carve/carve-rs#819.
#[test]
fn the_continuation_marker_still_reaches_the_marker_column() {
    for source in [
        "- a\n\n+ b\n",
        "- a\n  + x\n",
        "- x\n+\n![i](a.png)\n+\n![j](b.png)\n",
    ] {
        assert_eq!(fmt(source), source, "the marker column moved");
        round_trips(source);
    }
}
