//! PART 12 §18: a `[@key]: entry` bibliography line is a `citation_definition`.
//!
//! WHY THESE ASSERTIONS ARE ON THE TREE AND NOT ON THE HTML.
//!
//! A definition renders nothing where it sits. carve-rs consumed the line in the
//! Citations extension's `after_parse` hook, so it was not in the published tree
//! at all; carve-js left it as a paragraph whose first child is a
//! `citation_group` followed by the literal text of the separator. Both render
//! the same references list, which is why the two engines published different
//! documents for the same source for as long as the feature has existed and
//! every fixture agreed: no fixture was looking at the tree
//! (markup-carve/carve#1276).
//!
//! So an HTML assertion is structurally incapable of catching this, and the
//! ones below are on `parse_with_options` output. `parse` is the stage that
//! matters: it is what `to_json` serializes, and PART 12 §3a makes the
//! serialized tree the PRE-RESOLVE one. carve-js's collect pass runs in a hook
//! `parse` does not call; carve-rs's `after_parse` runs INSIDE
//! `parse_with_options`, so the node is on the wire.
//!
//! The last two tests are the other half of the clause: no rendered output
//! moves, on any target.

use carve::{BlockNode, Citations, InlineNode, Options};

const WITH_METADATA: &str = "Smith [@smith2020] agrees.\n\n[@smith2020]: {author=\"Smith\" year=\"2020\"} Smith, J. (2020). A Study. Pub.\n";

const WITHOUT_METADATA: &str =
    "Smith [@smith2020] agrees.\n\n[@smith2020]: Smith, J. (2020). A Study. Pub.\n";

const WITH_INLINE_MARKUP: &str =
    "Smith [@smith2020] agrees.\n\n[@smith2020]: Smith, J. */A Study/*. `code`. Pub.\n";

const TWO_DEFINITIONS: &str = "Both [@smith2020] and [@jones2019] agree.\n\n[@smith2020]: Smith, J. (2020). A Study. Pub.\n\n[@jones2019]: Jones, A. (2019). Notes. Pub.\n";

fn parsed(source: &str) -> carve::Document {
    let citations = Citations::new();
    let options = Options::new()
        .with_extension(&citations)
        .with_positions(true);
    carve::parse_with_options(source, &options)
}

fn definitions(doc: &carve::Document) -> Vec<&carve::CitationDefinition> {
    doc.children
        .iter()
        .filter_map(|block| match block {
            BlockNode::CitationDefinition(def) => Some(def),
            _ => None,
        })
        .collect()
}

fn text_of(children: &[InlineNode]) -> String {
    children
        .iter()
        .map(|node| match node {
            InlineNode::Text(t) => t.value.clone(),
            InlineNode::Code(c) => c.value.clone(),
            InlineNode::Emphasis(e) => text_of(&e.children),
            _ => String::new(),
        })
        .collect()
}

fn html(source: &str) -> String {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    carve::to_html_with_options(source, &options)
}

#[test]
fn the_definition_line_is_a_node_at_the_parse_stage() {
    let doc = parsed(WITH_METADATA);
    let defs = definitions(&doc);
    assert_eq!(defs.len(), 1, "tree: {:?}", doc.children);
    assert_eq!(defs[0].key, "smith2020");
    // The paragraph of citation-shaped literal text is NOT what survives.
    assert_eq!(doc.children.len(), 2);
    assert!(matches!(doc.children[0], BlockNode::Paragraph(_)));
    assert!(matches!(doc.children[1], BlockNode::CitationDefinition(_)));
}

#[test]
fn the_node_reaches_the_wire() {
    // The serialization, not just the enum: `to_json` is what another engine
    // reads, and it is the layer the divergence was invisible at.
    let json = carve::to_json(&parsed(WITH_METADATA));
    assert!(
        json.contains("\"type\":\"citation_definition\""),
        "no citation_definition on the wire: {json}"
    );
    assert!(json.contains("\"key\":\"smith2020\""), "{json}");
}

#[test]
fn the_entry_is_the_inline_content_after_the_separator() {
    let defs_doc = parsed(WITH_METADATA);
    let defs = definitions(&defs_doc);
    assert_eq!(
        text_of(&defs[0].children),
        "Smith, J. (2020). A Study. Pub."
    );
}

#[test]
fn the_metadata_block_lands_in_attrs() {
    let doc = parsed(WITH_METADATA);
    let defs = definitions(&doc);
    let attrs = defs[0].attrs.as_ref().expect("metadata block is missing");
    assert_eq!(
        attrs.key_values.get("author").map(String::as_str),
        Some("Smith")
    );
    assert_eq!(
        attrs.key_values.get("year").map(String::as_str),
        Some("2020")
    );
}

#[test]
fn a_definition_without_a_metadata_block_carries_no_attrs() {
    let doc = parsed(WITHOUT_METADATA);
    let defs = definitions(&doc);
    assert_eq!(defs.len(), 1);
    assert!(defs[0].attrs.is_none(), "{:?}", defs[0].attrs);
    assert_eq!(
        text_of(&defs[0].children),
        "Smith, J. (2020). A Study. Pub."
    );
}

#[test]
fn the_entry_keeps_its_inline_markup() {
    // §18 shapes this after §10's link reference definition rather than after
    // the footnote: the entry is INLINE content, so emphasis and a code span
    // are nodes rather than the flat string a `label`-style field would hold.
    let doc = parsed(WITH_INLINE_MARKUP);
    let defs = definitions(&doc);
    let kinds: Vec<&str> = defs[0]
        .children
        .iter()
        .map(|node| match node {
            InlineNode::Text(_) => "text",
            InlineNode::Emphasis(_) => "emphasis",
            InlineNode::Code(_) => "code",
            other => panic!("unexpected inline node: {other:?}"),
        })
        .collect();
    assert!(kinds.contains(&"emphasis"), "{kinds:?}");
    assert!(kinds.contains(&"code"), "{kinds:?}");
    // And no BLOCK ever appears in there - the field holds inline nodes only,
    // which the type already guarantees; what this pins is that the entry was
    // parsed rather than kept as one literal run.
    assert!(defs[0].children.len() > 1, "{:?}", defs[0].children);
}

#[test]
fn two_definitions_are_two_nodes_in_source_order() {
    let doc = parsed(TWO_DEFINITIONS);
    let defs = definitions(&doc);
    assert_eq!(defs.len(), 2);
    assert_eq!(defs[0].key, "smith2020");
    assert_eq!(defs[1].key, "jones2019");
    let first = defs[0].pos.expect("first definition has no pos");
    let second = defs[1].pos.expect("second definition has no pos");
    assert!(
        first.start_offset < second.start_offset,
        "pos runs backwards: {first:?} then {second:?}"
    );
}

#[test]
fn pos_spans_the_whole_definition_line() {
    // Losing `pos` is the specific defect: consuming the line at parse time
    // discarded it, so the line could not be reproduced and an AST round trip
    // deleted it. The span has to start at the `[` of `[@key]`, which no inline
    // node on the line carries - the citation group is built by the extension.
    let doc = parsed(WITH_METADATA);
    let defs = definitions(&doc);
    let pos = defs[0].pos.expect("the definition has no pos");
    let line = WITH_METADATA.lines().nth(2).expect("no third line");
    assert_eq!(pos.start_line, 3);
    assert_eq!(pos.end_line, 3);
    assert_eq!(pos.start_column, 1);
    assert_eq!(
        pos.start_offset,
        WITH_METADATA.find("[@smith2020]:").unwrap()
    );
    assert_eq!(pos.end_offset, pos.start_offset + line.chars().count());
    assert_eq!(pos.end_column, line.chars().count() + 1);
    // The slice the span selects IS the definition line.
    assert_eq!(&WITH_METADATA[pos.start_offset..pos.end_offset], line);
}

#[test]
fn a_default_profile_parse_never_produces_the_node() {
    // Tier-2: with the extension off, `[@key]: entry` is not a citation
    // definition and not a reference definition either, so the line is ordinary
    // paragraph text.
    let doc = carve::parse_with_options(WITH_METADATA, &Options::new().with_positions(true));
    assert!(definitions(&doc).is_empty(), "{:?}", doc.children);
    assert!(!carve::to_json(&doc).contains("citation_definition"));
}

#[test]
fn the_rendered_bibliography_is_unchanged() {
    // The definition still feeds the references list - it now does so from a
    // node in the tree rather than from parse-time state that dropped the line.
    let rendered = html(WITH_METADATA);
    assert!(
        rendered.contains("<li id=\"ref-smith2020\">Smith, J. (2020). A Study. Pub.</li>"),
        "{rendered}"
    );
    assert!(rendered.contains("<ol class=\"references\">"), "{rendered}");
}

#[test]
fn the_definition_renders_nothing_where_it_sits_on_every_target() {
    let citations = Citations::new();
    let options = Options::new().with_extension(&citations);
    // The document with the definition and the same document without it differ
    // only in the references list, which the with-definition one gains. What no
    // target may show is the line itself, in the position it was written.
    let rendered = carve::to_html_with_options(WITH_METADATA, &options);
    let before_refs = rendered.split("<ol class=\"references\">").next().unwrap();
    assert!(
        !before_refs.contains("author="),
        "the metadata block reached the output: {rendered}"
    );
    assert!(
        !before_refs.contains("A Study"),
        "the entry rendered where the line was written: {rendered}"
    );
    assert_eq!(
        before_refs,
        "<p>Smith [<a data-cite-key=\"smith2020\" href=\"#ref-smith2020\">1</a>] agrees.</p>\n"
    );

    for other in [
        carve::to_markdown_with_options(WITH_METADATA, &options),
        carve::to_plain_text_with_options(WITH_METADATA, &options),
        carve::to_ansi_with_options(WITH_METADATA, &options),
    ] {
        assert!(!other.contains("author="), "{other}");
    }
}
