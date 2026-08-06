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
//!
//! THE ANSWER IS NOW REFUSAL, not a silent drop. PART 12 section 11 generalized
//! this one field to every property the schema does not name, and rules dropping
//! out for the reason section 9(b) gives about depth: a caller told the tree was
//! accepted learns nothing about what went missing (carve-rs#691). So the
//! assertions below say `from_json` FAILS where they said the field vanished.
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

/// SOURCE's published tree with `refId` injected on the references.
fn tree_with_injected_ref_id() -> String {
    let json = carve::ast_json::to_json(&carve::parse(SOURCE));
    let injected = json.replace(
        "\"type\":\"footnote_ref\"",
        "\"type\":\"footnote_ref\",\"refId\":\"fnref9\"",
    );
    assert!(
        injected.contains("refId"),
        "the fixture failed to inject anything: {injected}"
    );

    injected
}

#[test]
fn a_fresh_parse_produces_none() {
    // The baseline the whole issue rests on.
    let json = carve::ast_json::to_json(&carve::parse(SOURCE));
    assert!(!json.contains("refId"), "{json}");
}

#[test]
fn a_payload_carrying_one_is_refused() {
    let error = carve::ast_json::from_json(&tree_with_injected_ref_id())
        .expect_err("a payload carrying refId was accepted");

    // Named, not merely rejected: a caller cannot act on "something was wrong".
    assert!(error.to_string().contains("refId"), "{error}");
}

#[test]
fn a_tree_without_one_still_round_trips() {
    // The control. The assertion above passes for a decoder that refuses
    // everything, and this is what such a decoder would break.
    let json = carve::ast_json::to_json(&carve::parse(SOURCE));
    let back = carve::ast_json::from_json(&json).expect("this engine's own tree is readable");
    let out = carve::ast_json::to_json(&back);

    assert!(out.contains("\"footnote_ref\""), "{out}");
    assert!(out.contains("\"id\":\"a\""), "{out}");
    assert!(out.contains("\"number\":1"), "{out}");
    assert!(!out.contains("refId"), "{out}");
}

#[test]
fn an_inline_footnote_is_treated_the_same_way() {
    // Both node types carried the field, so both have to refuse it.
    let json = carve::ast_json::to_json(&carve::parse("a ^[note] b\n"));
    let injected = json.replace(
        "\"type\":\"inline_footnote\"",
        "\"type\":\"inline_footnote\",\"refId\":\"fnref1\"",
    );
    assert!(injected.contains("refId"), "fixture injected nothing");

    let error = carve::ast_json::from_json(&injected).expect_err("refId was accepted");
    assert!(error.to_string().contains("refId"), "{error}");
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
