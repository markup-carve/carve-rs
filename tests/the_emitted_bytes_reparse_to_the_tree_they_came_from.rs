//! PART 11 §1: `to_html(fmt(x)) == to_html(x)`, and §1a: where spelling a
//! construct by its own rule would emit a document that does not re-parse to the
//! tree it was written from, §1 wins and the per-construct rule yields.
//!
//! `markup-carve/carve#994` measured six shapes across the fleet. carve-rs was
//! left unmeasured, and when it was measured all six had been closed here -
//! four by the byte-0 deviation `markup-carve/carve#961` records, one by
//! carve-rs#831 and one by carve-rs#841. Sweeping past the six found two more
//! that ARE open here and are closed in carve-js, which is what this file pins.
//!
//! 1. A hoisted definition promotes a PARAGRAPH whose first line is
//!    `---yaml`-shaped to byte 0. No head-of-document respelling repairs that
//!    one, because the paragraph's text is not the writer's to change - it is
//!    saved by respelling the CLOSER, which is why the fallback moves every
//!    HYPHEN break in the document rather than the one at the head. Only the
//!    hyphen spelling can be read as a fence, so carve-rs#843's authored markers
//!    survive it.
//!
//! 2. A run of `+`-attached blocks kept the marker on the first child only, so
//!    every later child sat two columns to the RIGHT of the block above it and
//!    was read as its lazy continuation.
//!
//! Both are the same clause and neither is a styling difference: each destroys
//! the document.

fn html(src: &str) -> String {
    carve::to_html(src)
}

fn fmt(src: &str) -> String {
    carve::to_carve(src)
}

/// PART 11 §1, asserted directly: formatting must not change the document.
fn round_trips(src: &str) -> bool {
    html(&fmt(src)) == html(src)
}

// ---------------------------------------------------------------------------
// 1. A block promoted to byte 0 by a hoisted definition.
// ---------------------------------------------------------------------------

#[test]
fn a_promoted_dashed_paragraph_does_not_open_frontmatter() {
    // The PARAGRAPH `---yaml` is the shape no BREAK respelling can reach: the
    // writer may not rewrite a paragraph's text. `fmt` emitted `---yaml` /
    // `k: v` / blank / `---` / blank / `[a]: /u`, and the next parse read the
    // whole document as a frontmatter block and rendered NOTHING.
    //
    // Since markup-carve/carve#1443 the head is repaired at the head after all:
    // the run is flag-shaped, so it is literal text rather than an em dash, and
    // the writer ESCAPES it. That is enough on its own, so the `---` break below
    // keeps the spelling the author wrote. carve-js and carve-php emit the same
    // bytes.
    let src = "[a]: /u\n\n---yaml\nk: v\n---\n";
    assert_eq!(
        html(src),
        "<p>---yaml\nk: v</p>\n<hr>",
        "the premise: the input has no frontmatter"
    );
    assert_eq!(fmt(src), "\\-\\-\\-yaml\nk: v\n\n---\n\n[a]: /u\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_footnote_definition_promotes_the_same_paragraph() {
    // The hoist is not specific to a link definition: PART 11 §7 moves every
    // definition below the body, and a footnote definition promotes whatever
    // stood second exactly as a link definition does.
    let src = "[^a]: n\n\n---toml\nx\n---\n";
    assert!(round_trips(src), "{}", fmt(src));
    // The HEAD is what moves (it is escaped), so the closer does not have to:
    // markup-carve/carve#1443 made the promoted run literal text.
    assert_eq!(fmt(src), "\\-\\-\\-toml\nx\n\n---\n\n[^a]: n\n");
}

#[test]
fn a_hyphen_break_promoted_to_the_head_is_written_star() {
    // carve-rs#843 narrowed which documents reach the fallback, and this is the
    // narrowing: with the author's marker preserved, a `---` break can only
    // arrive at byte 0 by being PROMOTED there. An authored one cannot - `---`
    // on line 1 with a `---` line below it is a frontmatter block in the source,
    // not two breaks, so `to_carve` never sees that tree from text.
    //
    // Two lines are enough to lose everything: written with hyphens this is an
    // EMPTY frontmatter block, rendering nothing where the input rendered two
    // rules.
    let src = "[a]: /u\n\n---\n\n---\n";
    assert_eq!(html(src), "<hr>\n<hr>");
    assert_eq!(fmt(src), "***\n\n***\n\n[a]: /u\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn every_hyphen_break_moves_not_only_the_one_at_the_head() {
    // The fallback is document-wide, which is what makes the promoted PARAGRAPH
    // fixable at all: there, the only line that can move is the closer.
    let src = "[a]: /u\n\n---\n\np\n\n---\n";
    assert_eq!(html(src), "<hr>\n<p>p</p>\n<hr>");
    assert_eq!(fmt(src), "***\n\np\n\n***\n\n[a]: /u\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn an_authored_star_break_at_the_head_needs_no_fallback() {
    // CONTROL for the narrowing. A `***` head cannot open a fence, so a later
    // `---` closes nothing and every authored marker survives.
    let src = "***\n\na\n\n---\n\nb\n";
    assert_eq!(fmt(src), "***\n\na\n\n---\n\nb\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_star_break_elsewhere_survives_the_fallback() {
    // THE DEPARTURE IS THE SMALLEST ONE, and since markup-carve/carve#1443 it is
    // smaller still: the escaped head saves this document, so NO break moves -
    // neither the `---` nor the `___` - and every authored marker survives.
    let src = "[a]: /u\n\n---yaml\nk: v\n---\n\n___\n";
    assert_eq!(fmt(src), "\\-\\-\\-yaml\nk: v\n\n---\n\n___\n\n[a]: /u\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_relocated_definition_promoting_a_bare_break_still_round_trips() {
    let src = "[a]: /u\n\n---\n\np\n\n---\n";
    assert_eq!(html(src), "<hr>\n<p>p</p>\n<hr>");
    assert!(round_trips(src), "{}", fmt(src));
}

// ---------------------------------------------------------------------------
// 1. Controls: the documents the fallback must NOT touch.
// ---------------------------------------------------------------------------

#[test]
fn a_leading_break_with_no_closer_below_keeps_the_canonical_spelling() {
    // CONTROL. The opener only fires when a CLOSER follows, so a leading `---`
    // with nothing below it to close a block is not misread and owes no
    // fallback. Corpus `132-thematic-break-requires-contiguous-markers-4` asks
    // for exactly this. No mutation of the fallback moves this row - stated as a
    // control rather than offered as proof.
    let src = "---\n\na\n";
    assert_eq!(fmt(src), "---\n\na\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_break_below_a_paragraph_keeps_the_canonical_spelling() {
    // CONTROL. Nothing is at byte 0 but the paragraph, so there is no opener to
    // collide with and §6 governs unchanged.
    let src = "p\n\n---\n\nq\n";
    assert_eq!(fmt(src), "p\n\n---\n\nq\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_document_with_real_frontmatter_keeps_the_canonical_spelling() {
    // Its own frontmatter is written by the frontmatter writer, whose closer is
    // not a break - so the break below it is never at byte 0 and never respelled.
    let src = "---yaml\nk: v\n---\n\n---\n\np\n";
    assert_eq!(fmt(src), "---yaml\nk: v\n---\n\n---\n\np\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_closer_that_is_not_a_break_keeps_the_canonical_spelling() {
    // What used to be the residual, now CLOSED and asserted as such. The `---`
    // closer here is inside a fenced block, so respelling every BREAK cannot
    // remove it - the document was misread with `***` too. Escaping the head
    // reaches what no break spelling could (markup-carve/carve#1443), so the
    // document round-trips and no fallback is paid.
    let src = "[a]: /u\n\n---yaml\nk: v\n---\n\n```\n---\n```\n";
    let out = fmt(src);
    assert!(
        out.starts_with("\\-\\-\\-yaml"),
        "the promoted paragraph is escaped, not respelled: {out}"
    );
    assert!(
        !out.contains("***"),
        "no fallback is paid when it would not help: {out}"
    );
    assert!(round_trips(src), "{}", out);
}

// ---------------------------------------------------------------------------
// 2. A run of `+`-attached blocks.
// ---------------------------------------------------------------------------

#[test]
fn every_block_in_an_attached_run_keeps_its_marker() {
    // The first image took the marker and the second was written at the item's
    // CONTENT column, two columns to the right of it - so the second image's
    // source came back as literal text inside the first image's paragraph.
    let src = "- x\n+\n![a](i.png)\n+\n![a](i.png)\n";
    assert_eq!(fmt(src), "- x\n+\n![a](i.png)\n+\n![a](i.png)\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_second_attached_figure_does_not_land_inside_the_first_caption() {
    // The same defect with captions is worse to read: the second figure's WHOLE
    // source was absorbed into the first one's `<figcaption>`.
    let src = "- x\n+\n![a](i.png)\n^ cap\n+\n![a](i.png)\n^ cap\n";
    assert_eq!(html(src).matches("<figure>").count(), 2, "the premise");
    assert_eq!(html(&fmt(src)).matches("<figure>").count(), 2);
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_three_block_attached_run_keeps_every_marker() {
    let src = "- x\n+\n![a](i.png)\n+\n![b](j.png)\n+\n![c](k.png)\n";
    assert_eq!(fmt(src), src);
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_block_opener_after_a_marked_child_takes_the_marker_too() {
    // A quote OPENS its own block, so on its own it needs no marker - which is
    // why the condition cannot be the child's kind. Here it follows a child
    // already at the marker column, and at the content column it would be
    // indented relative to that child.
    let src = "- x\n+\n![a](i.png)\n+\n> q\n";
    assert_eq!(fmt(src), "- x\n+\n![a](i.png)\n+\n> q\n");
    assert!(round_trips(src), "{}", fmt(src));
}

// ---------------------------------------------------------------------------
// 2. Controls: the runs that must NOT gain a marker.
// ---------------------------------------------------------------------------

#[test]
fn a_run_that_never_reached_the_marker_column_is_untouched() {
    // CONTROL. A fence opens its own block at the item's content column, so no
    // child in this run is at the marker column and none gains a marker. This is
    // the shape the corpus pins, and no mutation of the run rule moves it.
    let src = "- x\n+\n```\nc\n```\n";
    assert_eq!(fmt(src), "- x\n  ```\n  c\n  ```\n");
    assert!(round_trips(src), "{}", fmt(src));
}

#[test]
fn a_loose_item_is_untouched() {
    // CONTROL. The run rule lives in the TIGHT item writer; a loose item
    // separates its children with blank lines and reaches none of it.
    let src = "- x\n\n  p\n\n  q\n";
    assert!(round_trips(src), "{}", fmt(src));
    assert!(!fmt(src).contains('+'), "{}", fmt(src));
}

#[test]
fn an_ordinary_tight_item_is_untouched() {
    // CONTROL.
    let src = "- x\n  y\n";
    assert!(!fmt(src).contains('+'), "{}", fmt(src));
    assert!(round_trips(src), "{}", fmt(src));
}

// ---------------------------------------------------------------------------
// The six shapes `markup-carve/carve#994` measured, kept as regression rows.
// ---------------------------------------------------------------------------

#[test]
fn all_six_shapes_from_the_fleet_ticket_round_trip() {
    for src in [
        // 1. a leading break gains a closer
        "***\n\na\n\n---\n\nb\n",
        // 2. a relocated definition promotes the next block to byte 0
        "[a]: /u\n\n---\n\np\n\n---\n",
        // 3. a `+`-attached image loses its caption
        "- x\n+\n![a](i.png)\n^ cap\n",
        // 4. only the LAST line of a `+`-attached block is indented
        "- x\n+\n---yaml\nk: v\n---\n",
        // 5. a header cell's first character is read as alignment
        "| ~x~ |\n|---|\n| y |\n",
        // 6. an empty footnote body
        "[^f]: {x}\n\nr[^f]\n",
    ] {
        assert!(round_trips(src), "{src:?} formatted to {:?}", fmt(src));
        assert_eq!(fmt(&fmt(src)), fmt(src), "not idempotent: {src:?}");
    }
}
