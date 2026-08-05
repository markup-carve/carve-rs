//! `attrs.order` is the source-appearance order of the SLOTS in a `{…}` block.
//!
//! The schema says exactly that - `attrs` is "The `{#id .class key=value}` block
//! attached to the node", and `order` is "Source-appearance order of the slots:
//! `#id`, `.class`, or a bare key name".
//!
//! A code fence's title is written as fence metadata (``` ``` rust "Example" ```),
//! not as a slot in an attribute block, so it has no source appearance to
//! record. Publishing `order: ["title"]` claims a position in a block the author
//! never wrote (carve#785). carve-js has never published it; carve-php stopped
//! in carve-php#898.
//!
//! The attribute itself is unaffected: it reaches the wire and renders as
//! `<pre title="…">`.

const FENCE_TITLE: &str = "``` rust \"Example\"\ncode\n```\n";
const AUTHORED_TITLE: &str = "{title=\"Written\"}\n``` rust\ncode\n```\n";

#[test]
fn a_fence_title_is_published_without_an_order_slot() {
    let json = carve::ast_json::to_json(&carve::parse(FENCE_TITLE));
    assert!(
        json.contains("\"title\":\"Example\""),
        "the title left the wire entirely: {json}"
    );
    assert!(
        !json.contains("\"order\""),
        "a synthesized title claimed a slot in an attribute block: {json}"
    );
}

#[test]
fn an_authored_title_attribute_keeps_its_slot() {
    // The control, and the reason this is not "code blocks have no order": a
    // title written in a real attribute block DID appear in one.
    let json = carve::ast_json::to_json(&carve::parse(AUTHORED_TITLE));
    assert!(
        json.contains("\"order\":[\"title\"]"),
        "an authored title lost its slot: {json}"
    );
}

#[test]
fn the_title_still_renders() {
    let html = carve::render_html(&carve::parse(FENCE_TITLE)).expect("render");
    assert!(html.contains("title=\"Example\""), "{html}");
}
