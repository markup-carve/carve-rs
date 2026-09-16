//! `render_carve` writes a frontmatter block as the author wrote it. It used to
//! rebuild the block from the parsed key/value map, which cannot hold JSON or
//! TOML at all and forgets a YAML block's key order (markup-carve/carve-rs#1666).

use std::collections::BTreeMap;

use carve::{parse, render_carve, to_carve, Document};

fn written(src: &str) -> String {
    render_carve(&parse(src)).expect("within the render ceiling")
}

#[test]
fn a_json_block_survives_the_tree_taking_writer() {
    let src = "---json\n{\"title\": \"My Document\"}\n---\n\nContent begins here.\n";
    assert_eq!(written(src), src);
}

#[test]
fn a_toml_block_survives_the_tree_taking_writer() {
    let src = "---toml\ntitle = \"My Document\"\n---\n\nContent begins here.\n";
    assert_eq!(written(src), src);
}

#[test]
fn a_yaml_block_keeps_its_key_order() {
    let src = "---yaml\ntitle: A\nauthor: B\ndate: C\n---\n\nContent.\n";
    assert_eq!(written(src), src);
}

#[test]
fn to_carve_writes_the_block_once() {
    // CONTROL for the entry point that already held the raw block: it prepends
    // its own copy, so the tree must not carry one into the writer as well.
    let src = "---toml\na = 1\n---\n\nBody.\n";
    assert_eq!(to_carve(src).matches("---toml").count(), 1);
}

#[test]
fn a_document_built_with_only_a_map_still_writes_the_map() {
    // CONTROL. No raw block to prefer, so the map is the fallback.
    let mut doc: Document = parse("Body.\n");
    doc.frontmatter = BTreeMap::from([("t".to_string(), "1".to_string())]);
    doc.frontmatter_raw = None;
    let out = render_carve(&doc).expect("within the render ceiling");
    assert!(out.starts_with("---\nt: 1\n---"), "{out:?}");
}
