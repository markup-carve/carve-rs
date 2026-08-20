//! A footnote body with NO blocks at all still owes the reader a way back.
//!
//! PART 9 S16 hangs the backlink on the body's last block, and synthesizes a
//! wrapping paragraph when that block is not a paragraph
//! (markup-carve/carve#688). A body with zero blocks has no last block to hang
//! it on, so the whole paragraph is synthesized. This engine read "no blocks"
//! as "nothing to do" and emitted a bare `<li id="fn1"></li>`, while the
//! reference above it still rendered and still pointed at the note - so a
//! reader who followed it was stranded (carve-rs#826).
//!
//! THE TICKET NAMED ONE ROUTE to a zero-block body. There are THREE, and they
//! all arrive at the same site in `render_footnotes_section`:
//!
//! 1. source, where the body's only line is a block-attribute line and is
//!    consumed as attributes;
//! 2. AST-JSON ingest, where `"type":"footnote"` carries an empty `children`;
//! 3. a profile whose disallowed action is Strip removing every block of the
//!    body, which leaves the label mapped to an empty vector.
//!
//! Routes that are NOT zero-block and were already correct, measured before
//! this was written: a body that is only a comment (one Comment block, backlink
//! already synthesized), a body that is only a link-reference or abbreviation
//! definition line (those stay paragraph text inside the note), and a reference
//! whose label has no definition at all (it degrades to literal `[^f]` text, so
//! there is no endnote and nobody to strand).
//!
//! Every expectation below was compared byte for byte against carve-js at
//! `76dadb6`, which already synthesizes the paragraph, as does carve-php.

use carve::profile::{DisallowedAction, Profile};

fn html(src: &str) -> String {
    carve::to_html(src)
}

/// The endnote list item, the only part of the output this file is about.
fn endnote(html: &str) -> String {
    let start = html.find("<li id=").expect("an endnote item");
    let end = html.find("</li>").expect("an endnote item's end") + "</li>".len();
    html[start..end].to_string()
}

#[test]
fn a_body_emptied_by_a_block_attribute_line_carries_the_backlink() {
    // The ticket's shape. `{x}` is consumed as the definition's attributes, so
    // the body holds no blocks.
    assert_eq!(
        endnote(&html("r[^f]\n\n[^f]: {x}\n")),
        "<li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a></p>\n    </li>"
    );
}

#[test]
fn an_ingested_empty_body_carries_the_backlink() {
    // Route 2. The encoder publishes `"children": []` for the shape above, so
    // this is the engine reading back its own output: without the fix
    // `carve --json | carve --from-json` dropped the backlink a second time.
    let json = carve::to_json(&carve::parse("r[^f]\n\n[^f]: {x}\n"));
    assert!(
        json.contains("\"type\":\"footnote\"") && json.contains("\"children\":[]"),
        "the premise: the encoder publishes an empty body - {json}"
    );
    let decoded = carve::from_json(&json).expect("decode");
    let rendered = carve::render_html(&decoded).expect("render");
    assert_eq!(
        endnote(&rendered),
        "<li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a></p>\n    </li>"
    );
}

#[test]
fn a_body_a_profile_emptied_carries_the_backlink() {
    // Route 3. Strip REMOVES rather than degrades, so denying the body's only
    // block leaves the label mapped to an empty vector while the reference
    // survives - the exact state the other two routes reach.
    let doc = carve::parse("r[^f]\n\n[^f]: | a |\n      |---|\n      | b |\n");
    let profile = Profile::full()
        .deny_block(&["table"])
        .on_disallowed(DisallowedAction::Strip);
    let filtered = carve::profile_filter::apply_profile(doc, &profile, None)
        .expect("the profile is in collect mode, not error mode")
        .doc;
    assert_eq!(
        filtered.footnote_defs.get("f").map(Vec::len),
        Some(0),
        "the premise: the filter left the label with an empty body"
    );
    let rendered = carve::render_html(&filtered).expect("render");
    assert_eq!(
        endnote(&rendered),
        "<li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a></p>\n    </li>"
    );
}

#[test]
fn an_empty_body_referenced_twice_carries_both_numbered_backlinks() {
    // The synthesized paragraph carries whatever `render_backlinks` produces,
    // not a hardcoded single arrow - so N references still get N distinct ways
    // back.
    assert_eq!(
        endnote(&html("r[^f] s[^f]\n\n[^f]: {x}\n")),
        "<li id=\"fn1\">\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference 1\">\u{21a9}<sup>1</sup></a> \
         <a href=\"#fnref1-2\" role=\"doc-backlink\" aria-label=\"Back to reference 2\">\u{21a9}<sup>2</sup></a></p>\n    </li>"
    );
}

// ---------------------------------------------------------------------------
// CONTROLS. These pass on the UNFIXED engine and no mutation of this defect
// touches them. They are here so a fix that reaches the ordinary body - putting
// the backlink in a second paragraph, or losing the body text - fails loudly.
// ---------------------------------------------------------------------------

#[test]
fn control_an_ordinary_body_keeps_the_backlink_inside_its_own_paragraph() {
    assert_eq!(
        endnote(&html("r[^f]\n\n[^f]: t\n")),
        "<li id=\"fn1\">\n      <p>t<a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a></p>\n    </li>"
    );
}

#[test]
fn control_a_body_ending_in_a_quote_keeps_its_synthesized_paragraph() {
    // The neighbouring branch of the same rule: one block, not a paragraph. The
    // backlink goes in a paragraph AFTER it, never inside the quotation.
    assert_eq!(
        endnote(&html("r[^f]\n\n[^f]: > q\n")),
        "<li id=\"fn1\">\n      <blockquote><p>q</p></blockquote>\n      \
         <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a></p>\n    </li>"
    );
}

#[test]
fn control_a_body_that_is_only_a_comment_was_already_correct() {
    // Not a zero-block body: one Comment block that renders nothing. It already
    // took the synthesized-paragraph branch before this fix, blank line and
    // all, and carve-js emits the same blank line.
    assert_eq!(
        endnote(&html("r[^f]\n\n[^f]: %% c\n")),
        "<li id=\"fn1\">\n\n      <p><a href=\"#fnref1\" role=\"doc-backlink\" aria-label=\"Back to reference\">\u{21a9}</a></p>\n    </li>"
    );
}

#[test]
fn control_a_reference_with_no_definition_strands_nobody() {
    // There is no endnote to be stranded in: the reference degrades to the
    // literal text the author typed.
    let out = html("r[^f]\n");
    assert!(!out.contains("doc-endnotes"), "{out}");
    assert!(out.contains("r[^f]"), "{out}");
}
