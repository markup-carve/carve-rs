//! Carve — a parser + HTML renderer for the [Carve](https://github.com/markup-carve/carve)
//! markup language.
//!
//! ## Quick start
//!
//! ```
//! let html = carve::to_html("# Hello\n\n/italic/ and *bold*.");
//! assert!(html.contains("<h1>Hello</h1>"));
//! assert!(html.contains("<em>italic</em>"));
//! ```
//!
//! Implementation status: MVP. Supports headings, paragraphs, fenced
//! code, inline code, lists (ordered / unordered / task), plain
//! blockquotes, links, images, and the full emphasis family (italic,
//! strong, underline, strike, super, sub, highlight, bold-italic).
//!
//! Deferred: tables, admonitions, captions / figures, attributes,
//! abbreviations, mentions, tags, extensions, frontmatter.

pub mod ast;
mod escape;
mod parse;
mod render;

pub use ast::*;
pub use parse::parse;
pub use render::render_html;

/// Parse a Carve source string and render it as HTML in one call.
pub fn to_html(source: &str) -> String {
    render_html(&parse(source))
}
