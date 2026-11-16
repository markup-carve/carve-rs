//! Carve 0.2 engine, stage 1: parse (source to tree), resolve and render HTML,
//! built from the 0.2 specification.
//!
//! The tree has two faces. [`parse_in`] and [`parse_resolved_in`] build it in
//! an [`Arena`]: nodes and child lists live in the arena, and text that is a
//! slice of the input borrows the input, so the tree lives as long as both.
//! This is the fast path, and [`render`] takes it. [`parse`] and
//! [`parse_resolved`] return an owned copy ([`ast::owned`]) for callers that
//! keep trees or deserialize them; `in_arena` and `owned` convert between the
//! two. Both serialize to the same JSON.
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

mod arena;
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

pub use arena::Arena;
pub use options::Options;

/// Parses source into the pre-resolve tree (stage parse), in `arena`. Text
/// values that are slices of the input borrow it.
#[must_use]
pub fn parse_in<'a>(input: &'a str, arena: &'a Arena) -> ast::Document<'a> {
    let src = source::Source::new(input);
    let out = block::scan(&src);
    tree::build(&src, arena, out.events, &out.own_starts)
}

/// Parses and resolves in `arena`: the tree `--tree` prints.
#[must_use]
pub fn parse_resolved_in<'a>(
    input: &'a str,
    opts: &Options,
    arena: &'a Arena,
) -> ast::Document<'a> {
    let mut doc = parse_in(input, arena);
    resolve::resolve(&mut doc, opts, arena);
    doc
}

/// Parses source into an owned pre-resolve tree.
#[must_use]
pub fn parse(input: &str) -> ast::owned::Document {
    let arena = Arena::for_input(input);
    parse_in(input, &arena).owned()
}

/// Parses and resolves into an owned tree.
#[must_use]
pub fn parse_resolved(input: &str, opts: &Options) -> ast::owned::Document {
    let arena = Arena::for_input(input);
    parse_resolved_in(input, opts, &arena).owned()
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
    let arena = Arena::for_input(input);
    let mut doc = parse_in(input, &arena);
    let ordered = resolve::resolve_ordered(&mut doc, opts, &arena);
    let (out, losses) = html::render_source(&doc, opts, input, ordered);
    if opts.strict_losses && !losses.is_empty() {
        return Err(RenderError::Losses(losses));
    }
    Ok(out)
}

/// The JSON of a tree (C2-A-165).
#[must_use]
pub fn to_json(doc: &ast::Document<'_>) -> String {
    #[derive(serde::Serialize)]
    #[serde(tag = "type")]
    enum Root<'r, 'a> {
        #[serde(rename = "document")]
        Document(&'r ast::Document<'a>),
    }
    serde_json::to_string(&Root::Document(doc)).unwrap_or_default()
}
