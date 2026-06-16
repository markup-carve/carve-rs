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
//! Implementation status: passes every `.crv` / `.html` pair currently
//! checked into this crate's `tests/spec` submodule, including tables,
//! captions / figures, admonitions, abbreviations, mentions, tags,
//! inline extensions, attributes, and frontmatter.

pub mod ast;
mod escape;
mod extension;
pub mod extensions;
mod parse;
mod render;
mod render_ansi;
mod render_markdown;
mod render_plain;
mod render_text;

pub use ast::*;
pub use extension::{
    BlockMatch, CarveExtension, InlineMatch, MatcherContext, Options, RenderContext,
};
pub use extensions::{
    Autolink, AutolinkOptions, ExternalLinks, ExternalLinksOptions, HeadingPermalinks,
    HeadingPermalinksOptions, ListType, Mermaid, MermaidOptions, Position, TabNormalize,
    TableOfContents, TableOfContentsOptions, UrlGenerator, Wikilinks, WikilinksOptions,
};
pub use parse::{parse, parse_with_options};
pub use render::{render_html, render_html_with_options};
pub use render_ansi::render_ansi;
pub use render_markdown::render_markdown;
pub use render_plain::render_plain_text;

/// Parse a Carve source string and render it as HTML in one call.
pub fn to_html(source: &str) -> String {
    render_html(&parse(source))
}

/// Parse a Carve source string and render it as Markdown in one call.
pub fn to_markdown(source: &str) -> String {
    render_markdown(&parse(source))
}

/// Parse a Carve source string and render it as plain text in one call.
pub fn to_plain_text(source: &str) -> String {
    render_plain_text(&parse(source))
}

/// Parse a Carve source string and render it as ANSI-styled text in one call.
pub fn to_ansi(source: &str) -> String {
    render_ansi(&parse(source))
}

/// Parse, run opt-in extension hooks, and render to HTML.
pub fn to_html_with_options(source: &str, options: &Options<'_>) -> String {
    let mut doc = parse_with_options(source, options);
    for ext in &options.extensions {
        doc = ext.before_render(doc);
    }
    render_html_with_options(&doc, options)
}
