//! The frontmatter block as the author wrote it (spec PART 12 section 2).
//!
//! The parsed `frontmatter` map cannot stand in for it: it is built by splitting
//! on the first colon, so key order, comments and any non-`key: value` line are
//! gone, and a typed (`---json`, `---toml`) block is not parsed into it at all -
//! for those documents the map is EMPTY while the source clearly has
//! frontmatter.

use carve::parse;

#[test]
fn a_bare_fence_is_yaml() {
    let doc = parse("---\ntitle: x\n---\n\nbody\n");
    let raw = doc.frontmatter_raw.expect("a frontmatter block");

    assert_eq!(raw.format, "yaml");
    assert_eq!(raw.content, "title: x");
}

#[test]
fn the_format_token_is_kept() {
    let doc = parse("---toml\ntitle = \"x\"\n---\n\nbody\n");
    let raw = doc.frontmatter_raw.expect("a frontmatter block");

    assert_eq!(raw.format, "toml");
    assert_eq!(raw.content, "title = \"x\"");
}

#[test]
fn a_typed_block_is_kept_even_though_the_map_is_empty() {
    // This is the case that makes the raw block load-bearing rather than
    // redundant: the map has nothing, so a consumer reading only the map
    // concludes the document has no frontmatter at all.
    let doc = parse("---json\n{\"title\": \"x\"}\n---\n\nbody\n");

    assert!(doc.frontmatter.is_empty());
    let raw = doc.frontmatter_raw.expect("a frontmatter block");
    assert_eq!(raw.format, "json");
    assert_eq!(raw.content, "{\"title\": \"x\"}");
}

#[test]
fn content_the_map_cannot_represent_survives() {
    let source = "---\n# a comment\ntitle: x\nlist:\n  - one\n---\n\nbody\n";
    let doc = parse(source);
    let raw = doc.frontmatter_raw.expect("a frontmatter block");

    assert_eq!(raw.content, "# a comment\ntitle: x\nlist:\n  - one");
    // The map kept the comment as a key and flattened the nesting, which is
    // exactly why it is not a serializable form.
    assert_eq!(doc.frontmatter.get("title"), Some(&"x".to_string()));
}

#[test]
fn a_document_without_frontmatter_has_none() {
    assert!(parse("# H\n\nbody\n").frontmatter_raw.is_none());
    // A `---` that is a thematic break, not an opening fence.
    assert!(parse("body\n\n---\n\nmore\n").frontmatter_raw.is_none());
}

#[test]
fn an_empty_block_is_still_a_block() {
    let doc = parse("---\n---\n\nbody\n");
    let raw = doc.frontmatter_raw.expect("an empty frontmatter block");

    assert_eq!(raw.format, "yaml");
    assert_eq!(raw.content, "");
}
