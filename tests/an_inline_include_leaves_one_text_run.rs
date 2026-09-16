//! PART 12 section 1a (CARVE-P12-002): a parsed tree holds no two adjacent
//! `text` nodes, and an inline include is where the expansion pass used to
//! leave three (markup-carve/carve-rs#1647).

use std::collections::HashMap;

use carve::{
    expand_includes, parse_with_options, BlockNode, Document, IncludeDenial, IncludeOptions,
    IncludeResolved, IncludeResolver, InlineNode, Options,
};

struct Files(HashMap<String, String>);

impl IncludeResolver for Files {
    fn resolve(
        &self,
        path: &str,
        _ctx: &carve::IncludeContext<'_>,
    ) -> Result<IncludeResolved, IncludeDenial> {
        self.0
            .get(path)
            .map(|source| IncludeResolved::with_id(source.clone(), path))
            .ok_or(IncludeDenial::NotFound)
    }
}

fn expand(source: &str, files: &[(&str, &str)]) -> Document {
    let resolver = Files(
        files
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
    );
    let doc = parse_with_options(source, &Options::default().with_positions(true));
    expand_includes(doc, source, &IncludeOptions::new().with_resolver(&resolver)).doc
}

fn paragraph(doc: &Document) -> &[InlineNode] {
    match &doc.children[0] {
        BlockNode::Paragraph(p) => &p.children,
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

#[test]
fn an_inline_include_leaves_one_text_run() {
    let doc = expand(
        "Root {{ sub/child.crv }} tail.\n",
        &[("sub/child.crv", "inlined text\n")],
    );
    let children = paragraph(&doc);
    assert_eq!(children.len(), 1, "{children:?}");
    let InlineNode::Text(text) = &children[0] else {
        panic!("{children:?}");
    };
    assert_eq!(text.value, "Root inlined text tail.");
}

#[test]
fn the_run_takes_the_host_file_s_span() {
    // Section 1a: the run spans the file that HOLDS it, from the first host
    // piece's start to the last one's end. An included piece's coordinates are
    // measured in another file.
    let doc = expand(
        "Root {{ sub/child.crv }} tail.\n",
        &[("sub/child.crv", "inlined text\n")],
    );
    let InlineNode::Text(text) = &paragraph(&doc)[0] else {
        panic!()
    };
    let pos = text.pos.as_ref().expect("the run is positioned");
    assert_eq!((pos.start_offset, pos.end_offset), (0, 30));
    assert!(pos.file.is_none(), "{:?}", pos.file);
}

#[test]
fn a_run_that_stops_at_another_node_stays_two_runs() {
    // CONTROL. Section 1a merges within one parent's child list and across
    // nothing: the emphasis between them keeps these two apart.
    let doc = expand("Root {{ c.crv }} tail /em/ x.\n", &[("c.crv", "inlined\n")]);
    let children = paragraph(&doc);
    assert_eq!(children.len(), 3, "{children:?}");
    assert!(
        matches!(children[1], InlineNode::Emphasis(_)),
        "{children:?}"
    );
}

#[test]
fn an_escaped_text_node_does_not_join_the_run() {
    // CONTROL. Section 1a is about `text` only, and PART 12 section 5 keeps
    // `escaped_text` distinct because an escape is authored form.
    let doc = expand("a \\* {{ c.crv }} b\n", &[("c.crv", "inlined\n")]);
    let children = paragraph(&doc);
    assert!(
        children
            .iter()
            .any(|n| matches!(n, InlineNode::EscapedText(_))),
        "{children:?}"
    );
}

#[test]
fn an_include_that_fills_the_paragraph_keeps_the_child_s_own_span() {
    // One piece is not a run, so nothing merges and the child's coordinates
    // stay with the file they were measured in.
    let doc = expand("{{ c.crv }}\n", &[("c.crv", "inlined\n")]);
    let children = paragraph(&doc);
    assert_eq!(children.len(), 1, "{children:?}");
    let InlineNode::Text(text) = &children[0] else {
        panic!()
    };
    assert_eq!(text.value, "inlined");
    assert_eq!(
        text.pos
            .as_ref()
            .and_then(|p| p.file.as_ref())
            .map(|f| f.as_str()),
        Some("c.crv")
    );
}

#[test]
fn a_run_ending_in_an_include_still_spans_the_host() {
    // The included piece is the LAST positioned one, so a span taken from
    // "whatever came last" would publish the child file's coordinates.
    let doc = expand("Root {{ c.crv }}\n", &[("c.crv", "inlined\n")]);
    let InlineNode::Text(text) = &paragraph(&doc)[0] else {
        panic!()
    };
    assert_eq!(text.value, "Root inlined");
    let pos = text.pos.as_ref().expect("the run is positioned");
    assert_eq!((pos.start_offset, pos.end_offset), (0, 16));
    assert!(pos.file.is_none(), "{:?}", pos.file);
}

#[test]
fn a_run_starting_with_an_include_still_spans_the_host() {
    // And the mirror: the included piece is the FIRST positioned one.
    let doc = expand("{{ c.crv }} tail.\n", &[("c.crv", "inlined\n")]);
    let InlineNode::Text(text) = &paragraph(&doc)[0] else {
        panic!()
    };
    assert_eq!(text.value, "inlined tail.");
    let pos = text.pos.as_ref().expect("the run is positioned");
    assert!(pos.file.is_none(), "{:?}", pos.file);
    // The host piece's own span covers the line, so the run does too.
    assert_eq!((pos.start_offset, pos.end_offset), (0, 17));
}
