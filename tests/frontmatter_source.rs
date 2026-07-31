//! Frontmatter is retained as written, not only as parsed key/values.
//!
//! PART 12 section 7 requires a serialized document to carry frontmatter RAW.
//! This engine parses the bare/yaml form into key/values and drops everything
//! else on the floor, so a consumer had nothing conformant to emit (carve#411).

use carve::parse;

#[test]
fn keeps_the_raw_block_and_its_format() {
    let doc = parse("---\ntitle: T\nnum: 3\n---\n\nBody\n");
    let source = doc.frontmatter_source.expect("frontmatter retained");
    assert_eq!(source.format, "yaml");
    assert_eq!(source.content, "title: T\nnum: 3");
}

#[test]
fn defaults_the_format_to_yaml_when_the_fence_carries_none() {
    let doc = parse("---\ntitle: T\n---\n\nBody\n");
    assert_eq!(doc.frontmatter_source.unwrap().format, "yaml");
}

#[test]
fn keeps_a_typed_block_the_parsed_map_cannot_hold() {
    // The parsed map only handles the bare/yaml key:value form, so a TOML block
    // parses to nothing at all. Without the raw text, serializing it would
    // claim the document had no frontmatter.
    let doc = parse("---toml\nx = 1\ny = [2, 3]\n---\n\nBody\n");
    assert!(doc.frontmatter.is_empty(), "toml is not key/value parsed");
    let source = doc.frontmatter_source.expect("frontmatter retained");
    assert_eq!(source.format, "toml");
    assert_eq!(source.content, "x = 1\ny = [2, 3]");
}

#[test]
fn distinguishes_an_empty_block_from_no_block() {
    // "has frontmatter, and it is empty" is a different claim from "has none",
    // and only one of them should serialize a frontmatter node.
    let empty = parse("---\n---\n\nBody\n");
    let source = empty
        .frontmatter_source
        .expect("an empty block is still a block");
    assert_eq!(source.content, "");

    let none = parse("Body only\n");
    assert!(none.frontmatter_source.is_none());
}

#[test]
fn leaves_a_thematic_break_alone() {
    // `---` that is not a frontmatter fence must not be captured as one.
    let doc = parse("Body\n\n---\n\nMore\n");
    assert!(doc.frontmatter_source.is_none());
}
