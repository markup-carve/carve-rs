//! Carve 0.2 engine, stage 1: parse (source to tree), resolve and render HTML,
//! built from the 0.2 specification.
#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
// No panics on any input: the quality bar of the stage, enforced.
#![deny(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo
)]
// Parser offsets and byte markers are clearer as short names in their local scopes.
#![allow(clippy::many_single_char_names)]
// Generated schema literals use their source formatting.
#![allow(clippy::unreadable_literal)]
// Generated AST visitors and parser dispatch functions follow the schema shape.
#![allow(clippy::too_many_lines)]
// Three parser states require distinct nested option values.
#![allow(clippy::option_option)]
// Generated AST constructors preserve their current field grouping.
#![allow(clippy::inconsistent_struct_constructor)]

pub mod ast;
pub mod attr;
pub mod block;
pub mod chars;
pub mod hash;
pub mod html;
pub mod inline;
pub mod options;
#[cfg(feature = "profile")]
pub mod profile;
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
    tree::build(&src, out.events, &out.own_starts)
}

/// Parses and resolves: the tree `--tree` prints.
#[must_use]
pub fn parse_resolved(input: &str, opts: &Options) -> ast::Document {
    let mut doc = parse(input);
    resolve::resolve(&mut doc, opts);
    doc
}

/// Parses, resolves and renders HTML. A refused render (`try_render`)
/// gives no output, the empty string.
#[must_use]
pub fn render(input: &str, opts: &Options) -> String {
    try_render(input, opts).unwrap_or_default()
}

/// Why a render returned no output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// C2-H-163: the input is longer than the profile's `max_length` bytes.
    TooLong { profile: &'static str, limit: usize },
    /// C2-H-130: `strictLosses` and the render lost content: one code per
    /// loss, in document order.
    Losses(Vec<&'static str>),
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLong { profile, limit } => write!(
                f,
                "the input is longer than the {limit} bytes the profile {profile} accepts"
            ),
            Self::Losses(codes) => write!(f, "the render lost content: {}", codes.join(", ")),
        }
    }
}

impl std::error::Error for RenderError {}

/// Parses, resolves and renders HTML, or says why the render gives no
/// output: an input over `max_length` is refused before it is parsed
/// (C2-H-163), and under `strictLosses` a render loss is an error (C2-H-130).
///
/// # Errors
///
/// [`RenderError::TooLong`] and [`RenderError::Losses`].
pub fn try_render(input: &str, opts: &Options) -> Result<String, RenderError> {
    let profile = opts.profile.name();
    let limit = tables::PROFILES
        .iter()
        .find(|p| p.name == profile)
        .and_then(|p| p.max_length);
    if let Some(limit) = limit.filter(|&l| input.len() > l) {
        return Err(RenderError::TooLong { profile, limit });
    }
    let doc = parse_resolved(input, opts);
    let (out, losses) = html::render_with_losses_source(&doc, opts, input);
    if opts.strict_losses && !losses.is_empty() {
        return Err(RenderError::Losses(losses));
    }
    Ok(out)
}

/// The JSON of a tree (C2-A-165).
#[must_use]
pub fn to_json(doc: &ast::Document) -> String {
    serde_json::to_string(&ast::Node::Document(Box::new(doc.clone()))).unwrap_or_default()
}
