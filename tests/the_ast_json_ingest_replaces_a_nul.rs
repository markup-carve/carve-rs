//! PART 12 §21: the AST-JSON ingest replaces every U+0000 with U+FFFD in every
//! string value, before it reads that value for anything else.
//!
//! `normalize_source` has always done this to Carve source, which is why PART 9
//! §29 carves the character out of the content class. The AST is a SECOND DOOR
//! into the same renderers and it had none, so an authored NUL and an ingested
//! one stood on different footings - one replaced, one content.
//!
//! THE DOOR IS NOT THE JSON PARSER. RFC 8259 forbids an unescaped U+0000 inside
//! a string, so `parse_string` refuses a raw control byte and still does. The
//! six-character escape is the only route in, and it is the one this normalizes;
//! a surrogate pair cannot produce U+0000, since its scalar starts at U+10000.
//!
//! HOW IT WAS FOUND. The HTML renderer's footnote-placement marker wrapped a
//! fixed string in NUL, on the claim that the character "cannot appear in
//! rendered HTML output". That was a claim about ONE of the two doors: through
//! this one, a text node carrying the marker pulled the endnotes section into
//! itself - `<p><section role="doc-endnotes">...</section></p>`, and no longer
//! at the document end (carve-rs#1217).
//!
//! THE CLAUSE DOES NOT DEPEND ON THAT MARKER, and the marker no longer spells a
//! NUL: it is picked per document now (carve-rs#1245). What is normative here
//! is that the two doors agree about U+0000, which is why the rows below are
//! about every target rather than about one renderer's internals.

use carve::{
    from_json, parse, render_ansi, render_carve, render_html, render_markdown, render_plain_text,
};

/// The escape, which is the only spelling JSON text can carry.
const NUL_ESCAPE: &str = r"\u0000";
/// A different C0 control, and the control for every row here. §29 still makes
/// U+000B ordinary content - the carve-out is U+0000 alone - so nothing about
/// this change may move it.
const VT_ESCAPE: &str = r"\u000b";

fn text_doc(escaped_value: &str) -> String {
    format!(
        r#"{{"type":"document","srcByteLength":3,"children":[{{"type":"paragraph","children":[{{"type":"text","value":"{escaped_value}"}}]}}]}}"#
    )
}

#[test]
fn every_target_renders_the_replacement_character() {
    // Before the clause: html, markdown and plain emitted the byte verbatim,
    // ANSI stripped it, and the canonical writer DELETED it - so `fmt` was
    // silently lossy on a document that had one.
    let doc = from_json(&text_doc(&format!("a{NUL_ESCAPE}b"))).expect("decodes");

    assert_eq!(render_html(&doc).unwrap(), "<p>a\u{fffd}b</p>");
    assert_eq!(render_markdown(&doc).unwrap(), "a\u{fffd}b\n");
    assert_eq!(render_plain_text(&doc).unwrap(), "a\u{fffd}b\n");
    assert_eq!(render_ansi(&doc).unwrap(), "a\u{fffd}b\n");
    assert_eq!(render_carve(&doc).unwrap(), "a\u{fffd}b\n");
}

#[test]
fn the_ingested_document_agrees_with_the_same_document_written_as_source() {
    // The whole of the rule: the two doors into the renderers take the same
    // doormat, so what an author writes and what a host hands over land in the
    // same place.
    let ingested = from_json(&text_doc(&format!("a{NUL_ESCAPE}b"))).expect("decodes");
    let parsed = parse("a\u{0}b\n");

    assert_eq!(
        render_html(&ingested).unwrap(),
        render_html(&parsed).unwrap()
    );
}

#[test]
fn the_writer_produces_source_the_parser_reads_back_unchanged() {
    // The writer deleted the byte, so `fmt` dropped a character with no
    // diagnostic. What it writes now survives a re-parse.
    let doc = from_json(&text_doc(&format!("a{NUL_ESCAPE}b"))).expect("decodes");
    let carve = render_carve(&doc).unwrap();

    assert_eq!(render_plain_text(&parse(&carve)).unwrap(), "a\u{fffd}b\n");
}

#[test]
fn the_footnotes_placement_sentinel_cannot_be_forged() {
    // THE TICKET'S MEASUREMENT. A text node whose value is the sentinel used to
    // relocate the endnotes section into the paragraph holding it:
    //
    //   <p><section role="doc-endnotes" ...>...</section><a id="fnref1" ...>
    //
    // A `<section>` inside a `<p>`, and the section no longer at the document
    // end. The marker is now ordinary text and the section stays where it
    // belongs.
    let payload = format!(
        r#"{{"type":"document","srcByteLength":30,"children":[{{"type":"paragraph","children":[{{"type":"text","value":"{NUL_ESCAPE}carve:footnotes-placement{NUL_ESCAPE}"}},{{"type":"footnote_ref","id":"a"}}]}},{{"type":"footnote","label":"a","children":[{{"type":"paragraph","children":[{{"type":"text","value":"note"}}]}}]}}]}}"#
    );

    let html = render_html(&from_json(&payload).expect("decodes")).unwrap();

    assert!(!html.contains("\u{0}"), "no NUL reaches the output: {html}");
    assert!(
        html.starts_with("<p>\u{fffd}carve:footnotes-placement\u{fffd}"),
        "the marker is text: {html}"
    );
    assert!(
        !html.contains("<p><section"),
        "the endnotes section is not inside the paragraph: {html}"
    );
    assert!(
        html.contains("</p>\n<section role=\"doc-endnotes\""),
        "the endnotes section is at the document end: {html}"
    );
}

#[test]
fn a_sentinel_in_a_document_with_no_footnotes_is_text_too() {
    // The other half of the same collision, and its own wrong output: with no
    // endnotes section to relocate, the sweep degraded the forged marker to
    // `<div class="footnotes"></div>` - a `<div>` inside a `<p>`, minted from
    // the author's own text.
    let html = render_html(
        &from_json(&text_doc(&format!(
            "{NUL_ESCAPE}carve:footnotes-placement{NUL_ESCAPE}"
        )))
        .expect("decodes"),
    )
    .unwrap();

    assert_eq!(html, "<p>\u{fffd}carve:footnotes-placement\u{fffd}</p>");
}

#[test]
fn a_string_that_is_not_a_text_value_is_replaced_too() {
    // "every string value it ingests", so an identifier, a class, an attribute
    // value and a code block's content are all in scope.
    let payload = format!(
        r#"{{"type":"document","srcByteLength":3,"children":[{{"type":"paragraph","attrs":{{"id":"i{NUL_ESCAPE}d","classes":["c{NUL_ESCAPE}k"],"keyValues":{{"title":"x{NUL_ESCAPE}y"}},"order":["title"]}},"children":[{{"type":"text","value":"q"}}]}},{{"type":"code_block","lang":"js","content":"a{NUL_ESCAPE}b"}}]}}"#
    );

    let html = render_html(&from_json(&payload).expect("decodes")).unwrap();

    assert!(!html.contains('\u{0}'), "{html}");
    assert!(html.contains("id=\"i\u{fffd}d\""), "{html}");
    assert!(html.contains("class=\"c\u{fffd}k\""), "{html}");
    assert!(html.contains("title=\"x\u{fffd}y\""), "{html}");
    assert!(html.contains("a\u{fffd}b"), "{html}");
}

#[test]
fn a_raw_byte_in_json_text_stays_a_syntax_error() {
    // §21 does not relax the JSON grammar: the byte never reaches a Carve rule.
    // Stated as a row so a later reading of "replaces on ingest" cannot be taken
    // for "accepts a raw control byte in a JSON document".
    let raw = text_doc("a\u{0}b");

    let error = from_json(&raw).expect_err("a raw control byte is refused");

    assert!(
        error
            .to_string()
            .contains("control character in JSON string"),
        "{error}"
    );
}

#[test]
fn the_other_c0_controls_stay_where_section_29_puts_them() {
    // THE CONTROL THAT MUST NOT MOVE. The carve-out is U+0000 alone, so U+000B
    // stays ordinary content on html, markdown, plain and the canonical writer,
    // and stays stripped on the terminal target where T4 strips the class.
    let doc = from_json(&text_doc(&format!("a{VT_ESCAPE}b"))).expect("decodes");

    assert_eq!(render_html(&doc).unwrap(), "<p>a\u{b}b</p>");
    assert_eq!(render_markdown(&doc).unwrap(), "a\u{b}b\n");
    assert_eq!(render_plain_text(&doc).unwrap(), "a\u{b}b\n");
    assert_eq!(render_carve(&doc).unwrap(), "a\u{b}b\n");
    assert_eq!(render_ansi(&doc).unwrap(), "ab\n");
}

#[test]
fn an_authored_replacement_character_and_an_ordinary_document_are_untouched() {
    // The other two controls: the replacement character a payload already
    // carries is content, and a payload with no NUL comes back the same.
    let authored = from_json(&text_doc(r"a\ufffdb")).expect("decodes");
    assert_eq!(render_html(&authored).unwrap(), "<p>a\u{fffd}b</p>");

    let ordinary = from_json(&text_doc("ab")).expect("decodes");
    assert_eq!(render_html(&ordinary).unwrap(), "<p>ab</p>");
}
