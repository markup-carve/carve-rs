//! An ingested `start` of 1 is the default, not an attribute.
//!
//! `resources/ast-schema.json` documents the field as the first number of an
//! ordered list "when it is not 1", so a parsed tree never carries 1 and the
//! only way to see it is an ingested payload that spells the default out.
//!
//! Writing it back as `start="1"` made ONE payload two different documents,
//! decided by which output was asked for: the HTML carried the attribute, while
//! the canonical Carve writer emitted a plain `1.` marker, which reads back
//! without it. carve-js and carve-php both render `<ol>` here.

/// A one-item ordered list whose `start` is spelled as the default.
const START_ONE: &str = r#"{"type":"document","srcByteLength":0,"children":[{"type":"list","ordered":true,"tight":true,"delim":".","start":1,"items":[{"type":"list_item","children":[{"type":"paragraph","children":[{"type":"text","value":"a"}]}]}]}]}"#;

/// The same list starting at 5, which IS an attribute.
const START_FIVE: &str = r#"{"type":"document","srcByteLength":0,"children":[{"type":"list","ordered":true,"tight":true,"delim":".","start":5,"items":[{"type":"list_item","children":[{"type":"paragraph","children":[{"type":"text","value":"a"}]}]}]}]}"#;

#[test]
fn the_html_carries_no_start_attribute() {
    let doc = carve::ast_json::from_json(START_ONE).expect("ingest");
    let html = carve::render_html(&doc).expect("render");

    assert!(html.contains("<ol>"), "expected a plain <ol>, got: {html}");
    assert!(
        !html.contains("start="),
        "expected no start attribute, got: {html}"
    );
}

/// The two outputs have to describe the same document, which is the defect the
/// attribute caused rather than a separate rule.
#[test]
fn the_two_outputs_agree_on_the_document() {
    let doc = carve::ast_json::from_json(START_ONE).expect("ingest");
    let source = carve::render_carve(&doc).expect("write");

    assert_eq!(source, "1. a\n");
    assert_eq!(
        carve::render_html(&doc).expect("render"),
        carve::to_html(&source)
    );
}

/// A start the schema DOES carry is still written.
#[test]
fn a_start_that_is_not_one_is_still_an_attribute() {
    let doc = carve::ast_json::from_json(START_FIVE).expect("ingest");

    assert!(carve::render_html(&doc)
        .expect("render")
        .contains("start=\"5\""));
}
