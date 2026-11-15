//! Carve 0.2 engine, stage 1: parse (source to tree), resolve and render HTML,
//! built from the 0.2 specification.
#![forbid(unsafe_code)]

pub mod ast;
pub mod attr;
pub mod block;
pub mod chars;
pub mod html;
pub mod inline;
pub mod options;
pub mod resolve;
pub mod source;
pub mod tables;
pub mod tree;

pub use options::Options;

/// Parses source into the pre-resolve tree (stage parse).
#[must_use]
pub fn parse(input: &str) -> ast::Document {
    let src = source::Source::new(input);
    let out = block::scan(&src);
    tree::build(&src, out.events)
}

/// Parses and resolves: the tree `--tree` prints.
#[must_use]
pub fn parse_resolved(input: &str, opts: &Options) -> ast::Document {
    let mut doc = parse(input);
    resolve::resolve(&mut doc, opts);
    doc
}

/// Parses, resolves and renders HTML.
#[must_use]
pub fn render(input: &str, opts: &Options) -> String {
    let doc = parse_resolved(input, opts);
    html::render(&doc, opts)
}

/// The JSON of a tree (C2-A-165).
#[must_use]
pub fn to_json(doc: &ast::Document) -> String {
    serde_json::to_string(&ast::Node::Document(doc.clone())).unwrap_or_default()
}
