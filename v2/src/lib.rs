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
pub mod diagnostic;
pub mod hash;
pub mod html;
pub mod include;
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
///
/// # Errors
///
/// [`RenderError::TooLong`] when positions cannot fit in `u32`.
pub fn parse_in<'a>(input: &'a str, arena: &'a Arena) -> Result<ast::Document<'a>, RenderError> {
    check_source_length(input.len())?;
    let src = source::Source::new(input);
    let out = block::scan(&src);
    Ok(tree::build(&src, arena, out.events, &out.own_starts))
}

/// Parses and resolves in `arena`: the tree `--tree` prints.
///
/// # Errors
///
/// [`RenderError::TooLong`] when positions cannot fit in `u32`.
pub fn parse_resolved_in<'a>(
    input: &'a str,
    opts: &Options,
    arena: &'a Arena,
) -> Result<ast::Document<'a>, RenderError> {
    let mut doc = parse_in(input, arena)?;
    resolve::resolve(&mut doc, opts, arena);
    Ok(doc)
}

fn check_source_length(len: usize) -> Result<(), RenderError> {
    if len > u32::MAX as usize {
        return Err(RenderError::TooLong {
            profile: "source",
            limit: u32::MAX as usize,
        });
    }
    Ok(())
}

/// C2-B-218: parses bytes with replacement of malformed UTF-8 and original byte offsets.
///
/// # Errors
///
/// [`RenderError::TooLong`] when positions cannot fit in `u32`.
pub fn try_parse_resolved_bytes_in<'a>(
    input: &'a [u8],
    opts: &Options,
    arena: &'a Arena,
) -> Result<ast::Document<'a>, RenderError> {
    check_source_length(input.len())?;
    let src = source::Source::from_bytes(input);
    check_source_length(src.text().len())?;
    let out = block::scan(&src);
    let mut doc = tree::build(&src, arena, out.events, &out.own_starts);
    resolve::resolve(&mut doc, opts, arena);
    Ok(doc)
}

/// Parses source into an owned pre-resolve tree.
///
/// # Errors
///
/// [`RenderError::TooLong`] when positions cannot fit in `u32`.
pub fn parse(input: &str) -> Result<ast::owned::Document, RenderError> {
    check_source_length(input.len())?;
    let arena = Arena::for_input(input);
    Ok(parse_in(input, &arena)?.owned())
}

/// Parses and resolves into an owned tree.
///
/// # Errors
///
/// [`RenderError::TooLong`] when positions cannot fit in `u32`.
pub fn parse_resolved(input: &str, opts: &Options) -> Result<ast::owned::Document, RenderError> {
    check_source_length(input.len())?;
    let arena = Arena::for_input(input);
    Ok(parse_resolved_in(input, opts, &arena)?.owned())
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
    let limit = limit.unwrap_or(u32::MAX as usize).min(u32::MAX as usize);
    if input.len() > limit {
        return Err(RenderError::TooLong { profile, limit });
    }
    let arena = Arena::for_input(input);
    let mut diags = Vec::new();
    let (out, losses) = render_parts(input, opts, &arena, &mut diags, &mut Vec::new())?;
    if opts.strict_losses && !losses.is_empty() {
        return Err(RenderError::Losses(losses));
    }
    Ok(out)
}

/// Parses, expands (C2-N-006) and resolves; true in the second place when
/// the resolution order is needed, in the third when an include was expanded.
fn prepare<'a>(
    input: &'a str,
    opts: &Options,
    arena: &'a Arena,
    diags: &mut Vec<diagnostic::Diagnostic>,
) -> Result<(ast::Document<'a>, bool, Option<resolve::files::Files<'a>>), RenderError> {
    check_source_length(input.len())?;
    let src = source::Source::new(input);
    let out = block::scan(&src);
    diags.extend(
        out.findings
            .iter()
            .map(|f| diagnostic::Diagnostic::from_finding(f, &src, None)),
    );
    let (mut doc, scope) = tree::build_scoped(&src, arena, out.events, None);
    diags.extend(
        scope
            .findings
            .iter()
            .map(|f| diagnostic::Diagnostic::from_finding(f, &src, None)),
    );
    let expanded = opts.includes.is_some();
    if let Some(cfg) = &opts.includes {
        if input.contains("{{") {
            include::expand(&mut doc, &scope, cfg, opts, arena, diags);
        }
    } else if input.contains("{{") {
        include::no_resolver(doc.children, diags);
    }
    let files = (expanded && input.contains("{{")).then(|| resolve::files::Files::build(&doc));
    let mut dups = Vec::new();
    let ordered = resolve::resolve_files(&mut doc, opts, arena, files.as_ref(), &mut dups);
    for (id, pos, at) in dups {
        let file = files.as_ref().and_then(|f| f.file_at(at));
        diags.push(diagnostic::Diagnostic::new(
            "duplicate-id",
            &[("explicit", &id)],
            file,
            pos,
        ));
    }
    if expanded {
        doc.stages = Some(&resolve::STAGES_EXPANDED);
    }
    Ok((doc, ordered, files))
}

/// `places` gets the file of each unexpanded include of an expanded tree.
fn render_parts(
    input: &str,
    opts: &Options,
    arena: &Arena,
    diags: &mut Vec<diagnostic::Diagnostic>,
    places: &mut Vec<Option<String>>,
) -> Result<(String, Vec<&'static str>), RenderError> {
    let (mut doc, ordered, files) = prepare(input, opts, arena, diags)?;
    if let Some(files) = files {
        include::unexpanded(doc.children, None, places);
        let extra = if opts.roundtrip {
            include::roundtrip_losses(doc.children)
        } else {
            Vec::new()
        };
        if matches!(
            opts.profile,
            options::Profile::Full | options::Profile::Article
        ) {
            include::flatten(&mut doc, arena);
        }
        let src = source::Source::new(input);
        let (out, mut losses) = html::render_files(&doc, opts, src.text(), &files);
        losses.extend(extra);
        return Ok((out, losses));
    }
    let (out, losses) = if opts.profile != options::Profile::Full && input.contains('\0') {
        let src = source::Source::new(input);
        html::render_mapped_source(&doc, opts, src.render_view(), ordered)
    } else {
        html::render_source(&doc, opts, input, ordered)
    };
    Ok((out, losses))
}

/// The HTML of a render and the findings of its parse and its render.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub html: String,
    pub diagnostics: Vec<diagnostic::Diagnostic>,
}

impl Rendered {
    /// The codes of the findings, in order.
    #[must_use]
    pub fn codes(&self) -> Vec<String> {
        self.diagnostics
            .iter()
            .map(|d| d.code.to_string())
            .collect()
    }
}

/// Renders as [`render`] does and returns the findings of the parse
/// channel (C2-N-061) and the render losses (C2-H-130) beside the HTML.
#[must_use]
pub fn render_with_diagnostics(input: &str, opts: &Options) -> Rendered {
    let arena = Arena::for_input(input);
    let mut diags = Vec::new();
    let mut places = Vec::new();
    let html = match render_parts(input, opts, &arena, &mut diags, &mut places) {
        Ok((html, losses)) => {
            let mut places = places.into_iter();
            diags.extend(losses.iter().map(|c| {
                // C2-N-063: the loss of an unexpanded include names its file.
                let file = if *c == "include-unresolved" {
                    places.next().flatten()
                } else {
                    None
                };
                diagnostic::Diagnostic::new(c, &[], file.as_deref(), None)
            }));
            if opts.strict_losses && !losses.is_empty() {
                String::new()
            } else {
                html
            }
        }
        Err(_) => String::new(),
    };
    Rendered {
        html,
        diagnostics: diags,
    }
}

/// C2-N-083: the dependencies of a document: its include nodes in tree
/// order, each with its path as written, its including file and, when the
/// file was read, its id. With no resolver every directive is unread.
#[must_use]
pub fn dependencies(input: &str, opts: &Options) -> Vec<include::Dependency> {
    let arena = Arena::for_input(input);
    let mut out = Vec::new();
    if let Ok((doc, _, _)) = prepare(input, opts, &arena, &mut Vec::new()) {
        let top = opts.includes.as_ref().and_then(|c| c.file.as_deref());
        include::dependencies(doc.children, top, &mut out);
    }
    out
}

/// The JSON of the expanded and resolved tree (C2-A-165), every position of
/// an included node naming its file (C2-N-019).
#[must_use]
pub fn tree_json(input: &str, opts: &Options) -> String {
    let arena = Arena::for_input(input);
    let Ok((doc, _, files)) = prepare(input, opts, &arena, &mut Vec::new()) else {
        return String::new();
    };
    if files.is_none() && opts.includes.is_none() {
        return to_json(&doc);
    }
    let mut v = serde_json::to_value(&doc).unwrap_or_default();
    if let Some(m) = v.as_object_mut() {
        m.insert("type".into(), "document".into());
    }
    include::stamp_files(&mut v, None);
    v.to_string()
}

/// C2-B-218: renders bytes after replacing malformed UTF-8 subparts.
///
/// # Errors
///
/// [`RenderError::TooLong`] and [`RenderError::Losses`].
pub fn try_render_bytes(input: &[u8], opts: &Options) -> Result<String, RenderError> {
    check_source_length(input.len())?;
    if let Ok(valid) = std::str::from_utf8(input) {
        return try_render(valid, opts);
    }
    let profile = opts.profile.name();
    let limit = tables::PROFILES
        .iter()
        .find(|p| p.name == profile)
        .and_then(|p| p.max_length)
        .unwrap_or(u32::MAX as usize)
        .min(u32::MAX as usize);
    if input.len() > limit {
        return Err(RenderError::TooLong { profile, limit });
    }
    let src = source::Source::from_bytes(input);
    let arena = Arena::for_input(src.text());
    let out = block::scan(&src);
    let mut doc = tree::build(&src, &arena, out.events, &out.own_starts);
    let ordered = resolve::resolve_ordered(&mut doc, opts, &arena);
    let (out, losses) = html::render_mapped_source(&doc, opts, src.render_view(), ordered);
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

#[cfg(test)]
mod source_limit_tests {
    #[test]
    fn source_larger_than_u32_positions_is_refused() {
        assert!(super::check_source_length(u32::MAX as usize).is_ok());
        if let Some(too_long) = (u32::MAX as usize).checked_add(1) {
            assert_eq!(
                super::check_source_length(too_long),
                Err(super::RenderError::TooLong {
                    profile: "source",
                    limit: u32::MAX as usize,
                })
            );
        }
    }
}
