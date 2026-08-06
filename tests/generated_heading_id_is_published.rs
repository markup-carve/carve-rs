//! A generated heading id is a resolution result, so it reaches the wire.
//!
//! PART 12 §5: "A GENERATED HEADING ID IS A RESOLUTION RESULT -- NORMATIVE. A
//! `heading` whose id was slugged from its text rather than written carries
//! that id in `attrs.id`". The criterion is recomputability, and a heading id
//! is not a function of the heading: dedup assigns the next free suffix in
//! DOCUMENT ORDER, so a consumer holding one subtree cannot derive `Notes-2`.
//!
//! This engine assigned ids during the HTML render only, so a published heading
//! carried no `attrs` at all (carve#750). carve-js has always published it;
//! carve-php started in carve-php#901.
//!
//! The id takes no `order` slot - it was never written in an attribute block -
//! and the carve writer must not turn it back into source (carve-js#741).

const PLAIN: &str = "# Welcome\n";
const DEDUPED: &str = "# Notes\n\n# Notes\n\n# Notes\n";
const AUTHORED: &str = "{#chosen}\n# Welcome\n";

#[test]
fn a_slugged_id_is_published() {
    let json = carve::ast_json::to_json(&carve::parse(PLAIN));
    assert!(
        json.contains("\"attrs\":{\"id\":\"Welcome\"}"),
        "the slugged id is not on the wire: {json}"
    );
}

#[test]
fn dedup_suffixes_are_published_in_document_order() {
    // The case that cannot be recomputed from one subtree.
    let json = carve::ast_json::to_json(&carve::parse(DEDUPED));
    for want in [
        "\"id\":\"Notes\"",
        "\"id\":\"Notes-2\"",
        "\"id\":\"Notes-3\"",
    ] {
        assert!(json.contains(want), "missing {want} in {json}");
    }
}

#[test]
fn a_published_id_takes_no_order_slot() {
    let json = carve::ast_json::to_json(&carve::parse(PLAIN));
    assert!(
        !json.contains("\"order\""),
        "a generated id claimed a slot in an attribute block: {json}"
    );
}

#[test]
fn an_authored_id_wins_and_keeps_its_slot() {
    let json = carve::ast_json::to_json(&carve::parse(AUTHORED));
    assert!(json.contains("\"id\":\"chosen\""), "{json}");
    assert!(json.contains("\"order\":[\"#id\"]"), "{json}");
}

#[test]
fn the_html_uses_the_same_ids() {
    // The published tree and the rendered document must not disagree.
    let html = carve::render_html(&carve::parse(DEDUPED)).expect("render");
    assert!(html.contains("id=\"Notes\""), "{html}");
    assert!(html.contains("id=\"Notes-2\""), "{html}");
}

#[test]
fn the_writer_does_not_turn_a_generated_id_into_source() {
    // PART 11 §1 gives the document back. An id the parse would re-derive is
    // not the author's source (carve-js#741).
    let doc = carve::ast_json::from_json(&carve::ast_json::to_json(&carve::parse(PLAIN)))
        .expect("decode");
    assert_eq!(carve::render_carve(&doc).expect("write"), PLAIN);
}

#[test]
fn the_writer_keeps_an_authored_id() {
    let doc = carve::ast_json::from_json(&carve::ast_json::to_json(&carve::parse(AUTHORED)))
        .expect("decode");
    assert_eq!(carve::render_carve(&doc).expect("write"), AUTHORED);
}
