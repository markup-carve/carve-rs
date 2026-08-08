//! PART 11 §7b: a footnote definition with no blocks is written `[^f]: {empty}`.
//!
//! The body empties whenever the definition line's whole body is a
//! block-attribute run: the line collects it as attributes and, with no
//! following block inside the note, drops it. The writer then has a definition
//! to spell and nothing to put after the colon.
//!
//! `[^f]:` is the wrong answer and it is the one this engine gave
//! (carve-rs#819, shape 3). That line is not a definition at all -- MARKER
//! REQUIRES CONTENT (PART 2) -- so formatting the document lost BOTH halves:
//! the definition came back as a paragraph and the reference to it came back as
//! literal text. §1a is the rule that licenses departing from the per-construct
//! spelling: the emitted bytes have to re-parse to the tree they were written
//! from, and where the construct's own rule cannot do that, §1 wins and the
//! writer takes the SMALLEST departure that restores it.
//!
//! WHY NOT `{ }` OR `{}` -- the two spellings a reader reaches for first, and
//! the reason the spec pins the spelling rather than leaving it per engine.
//! Neither is an attribute block on a block line: `block_attributes` requires
//! at least one attribute and there is no block-level blessed-empty form, so
//! both stay CONTENT and the note's body then holds a text node the author
//! never wrote. That is the same §1 failure in a different shape, which is why
//! the rows below read the rendered BODY rather than merely asserting that an
//! endnote section was produced -- the weaker check passes for every candidate,
//! including the two that do not work and including an ordinary body.
//!
//! THE MUTATION THESE ROWS EXIST FOR is emitting the bare marker. Reverting
//! `render_footnote_def_source` breaks every row below except the ones marked
//! CONTROL and the one that pins the token's behavior one node over.

use carve::{to_carve, to_html};

/// A definition whose body is a block-attribute line, so the body empties.
const EMPTIED: &str = "[^f]: {x}\n\nr[^f]\n";

/// The endnote list item, which is the part of the output these rows are about.
fn endnote(html: &str) -> String {
    let start = html.find("<li id=").expect("an endnote item");
    let end = html.find("</li>").expect("an endnote item's end") + "</li>".len();
    html[start..end].to_string()
}

#[test]
fn an_empty_body_is_written_with_the_sentinel() {
    assert_eq!(to_carve(EMPTIED), "r[^f]\n\n[^f]: {empty}\n");
}

#[test]
fn the_written_source_still_defines_and_still_resolves() {
    // The BODY, not just the presence of a section: the note holds the backlink
    // and nothing else, and the reference above it is a numbered noteref.
    let written = to_carve(EMPTIED);
    let html = to_html(&written);
    assert!(
        html.contains("<a id=\"fnref1\" href=\"#fn1\" role=\"doc-noteref\"><sup>1</sup></a>"),
        "the reference degraded to text: {html}"
    );
    assert_eq!(
        endnote(&html),
        "<li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\">\u{21a9}</a></p>\n    </li>"
    );
}

#[test]
fn formatting_preserves_the_document() {
    // PART 11 §1, in the form a person can see. The bare marker fails here: it
    // renders `<p>r[^f]</p>` plus `<p>[^f]:</p>`, both halves literal.
    assert_eq!(to_html(&to_carve(EMPTIED)), to_html(EMPTIED));
}

#[test]
fn the_writer_settles() {
    let once = to_carve(EMPTIED);
    assert_eq!(to_carve(&once), once);
}

#[test]
fn an_ingested_empty_body_is_written_with_the_sentinel() {
    // The second route to a zero-block body (the first is the source above, and
    // the third is a profile whose Strip emptied it). The encoder publishes
    // `"children": []`, so this is the engine reading back its own output.
    let json = carve::to_json(&carve::parse(EMPTIED));
    assert!(
        json.contains("\"type\":\"footnote\"") && json.contains("\"children\":[]"),
        "the premise: the encoder publishes an empty body - {json}"
    );
    let decoded = carve::from_json(&json).expect("decode");
    assert_eq!(
        carve::render_carve(&decoded).expect("render"),
        "r[^f]\n\n[^f]: {empty}\n"
    );
}

#[test]
fn the_sentinel_reaches_nothing_it_passes_over_on_the_way_out() {
    // `{empty}` is a BOOLEAN ATTRIBUTE and renders `empty=""` wherever
    // attributes survive. It is inert on this node because a footnote body is
    // its own container, and that has to hold for the neighbours too: neither
    // the next definition nor a following paragraph may collect it.
    let html = to_html(&to_carve("[^f]: {x}\n[^g]: g body\n\nr[^f] s[^g]\n"));
    assert!(!html.contains("empty="), "the sentinel leaked: {html}");
    assert!(
        html.contains("g body"),
        "the next definition was lost: {html}"
    );
    assert_eq!(html.matches("doc-backlink").count(), 2, "{html}");
}

#[test]
fn a_bare_word_attribute_is_otherwise_load_bearing() {
    // Why the clause calls the inertness a PARSE RULE rather than a property of
    // the word. Same token, one node over, and it renders.
    assert!(
        to_html("{empty}\npara\n").contains("<p empty=\"\">para</p>"),
        "{}",
        to_html("{empty}\npara\n")
    );
}

#[test]
fn control_an_ordinary_body_is_written_unchanged() {
    // CONTROL. Passes today, and no mutation of this defect touches it. Without
    // it the rows above are equally satisfied by a writer that stamps `{empty}`
    // on every footnote definition it ever writes.
    assert_eq!(to_carve("[^f]: t\n\nr[^f]\n"), "r[^f]\n\n[^f]: t\n");
}

#[test]
fn control_a_body_that_is_only_a_comment_is_not_an_empty_body() {
    // CONTROL, and the boundary the fix must not cross: one block that RENDERS
    // nothing is not a body with no blocks. It is written as the comment the
    // author wrote, never as the sentinel.
    assert_eq!(to_carve("[^f]: %% c\n\nr[^f]\n"), "r[^f]\n\n[^f]: %% c\n");
}
