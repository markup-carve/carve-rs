//! `refId` does not cross the wire in either direction.
//!
//! It is a RENDERING convention - `fnref1`, the anchor an endnotes section links
//! back to - not a resolution result. `resources/ast-schema.json` declared it on
//! `footnote_ref` and `inline_footnote`, no engine ever produced one, and
//! carve#762 removed it. With `additionalProperties: false`, a tree carrying it
//! is now invalid.
//!
//! This engine never wrote one on the AST path - carve-rs#639 threaded a flag
//! specifically so numbering could run without assigning it. It ECHOED one: the
//! codec read `refId` off a payload and wrote it back, so a document read and
//! re-published here became one the published format rejects (carve-rs#648).
//! carve-php already refuses such a payload outright.
//!
//! THE FIELD ITSELF STAYS on the node. The HTML renderer assigns it while
//! numbering footnotes and builds the backlinks from it; the last test here is
//! what keeps "do not publish it" from being satisfied by never computing it.
//!
//! NO ingest-then-render case, because there is no public entry point that takes
//! a Document and returns HTML - every renderer here takes a source string, and
//! the render module is private. A consumer therefore cannot reach the path where
//! a payload's anchor could survive into rendered output, so a test for it would
//! be exercising something no caller can do.

const SOURCE: &str = "Text[^a] and[^a].\n\n[^a]: note\n";

/// SOURCE's published tree with `refId` injected on the references, read back
/// and re-serialized.
fn round_tripped_with_injected_ref_id() -> String {
    let json = carve::ast_json::to_json(&carve::parse(SOURCE));
    let injected = json.replace(
        "\"type\":\"footnote_ref\"",
        "\"type\":\"footnote_ref\",\"refId\":\"fnref9\"",
    );
    assert!(
        injected.contains("refId"),
        "the fixture failed to inject anything: {injected}"
    );
    let back =
        carve::ast_json::from_json(&injected).expect("a payload with an extra field decodes");

    carve::ast_json::to_json(&back)
}

#[test]
fn a_fresh_parse_produces_none() {
    // The baseline the whole issue rests on.
    let json = carve::ast_json::to_json(&carve::parse(SOURCE));
    assert!(!json.contains("refId"), "{json}");
}

#[test]
fn an_injected_one_is_not_echoed_back() {
    let out = round_tripped_with_injected_ref_id();
    assert!(!out.contains("refId"), "{out}");
}

#[test]
fn the_rest_of_the_reference_survives() {
    // The boundary. Dropping the node, or its id or number, would also satisfy
    // the assertion above.
    let out = round_tripped_with_injected_ref_id();
    assert!(out.contains("\"footnote_ref\""), "{out}");
    assert!(out.contains("\"id\":\"a\""), "{out}");
    assert!(out.contains("\"number\":1"), "{out}");
}

#[test]
fn an_inline_footnote_is_treated_the_same_way() {
    // Both node types carried the field, so both have to drop it.
    let json = carve::ast_json::to_json(&carve::parse("a ^[note] b\n"));
    let injected = json.replace(
        "\"type\":\"inline_footnote\"",
        "\"type\":\"inline_footnote\",\"refId\":\"fnref1\"",
    );
    assert!(injected.contains("refId"), "fixture injected nothing");
    let back = carve::ast_json::from_json(&injected).expect("decodes");
    let out = carve::ast_json::to_json(&back);

    assert!(out.contains("\"inline_footnote\""), "{out}");
    assert!(!out.contains("refId"), "{out}");
}

#[test]
fn the_html_still_builds_its_backlinks() {
    // The other half: the renderer computes the anchor from the number, so the
    // endnotes section still links back - twice, since the note is used twice.
    let html = carve::to_html(SOURCE);
    assert!(html.contains("id=\"fnref1\""), "{html}");
    assert!(html.contains("id=\"fnref1-2\""), "{html}");
    assert!(html.contains("id=\"fn1\""), "{html}");
}
