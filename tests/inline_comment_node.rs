//! A trailing line comment is PUBLISHED, not dropped.
//!
//! `Also visible. %% tail` produced a paragraph holding one `text` node here,
//! where carve-js and carve-php publish `text` then `comment`. The rendered
//! output was right on every target - a comment renders to nothing, and the
//! canonical writer reproduced it from the source run - but a tree that records
//! what the author wrote cannot drop it, and a consumer that reads the tree
//! (carve-lsp, the pandoc bridge, anything formatting over the wire) lost it
//! (carve-rs#513).

use carve::{parse, to_carve, to_html, to_json};

#[test]
fn a_trailing_comment_is_a_node() {
    let json = to_json(&parse("Also visible. %% tail"));

    assert!(json.contains("\"type\":\"comment\""), "{json}");
    assert!(json.contains("\"block\":false"), "{json}");
    assert!(json.contains("\"content\":\"tail\""), "{json}");
}

#[test]
fn it_renders_to_nothing_on_every_target_but_carve() {
    assert_eq!(to_html("Also visible. %% tail"), "<p>Also visible.</p>");
    assert_eq!(
        to_carve("Also visible. %% tail").trim_end(),
        "Also visible. %% tail"
    );
}

#[test]
fn a_comment_opening_a_line_keeps_its_column() {
    // The space before `%%` is what makes it a comment mid-line; at the start
    // of a line there is none to put back.
    assert_eq!(to_carve("%% whole line").trim_end(), "%% whole line");
}

#[test]
fn it_survives_a_json_round_trip() {
    let doc = parse("Also visible. %% tail");
    let json = to_json(&doc);
    let decoded = carve::from_json(&json).expect("decodes");

    assert_eq!(to_carve_doc(&decoded).trim_end(), "Also visible. %% tail");
}

fn to_carve_doc(doc: &carve::Document) -> String {
    carve::render_carve(doc).expect("the tree under test is within the render ceiling")
}
