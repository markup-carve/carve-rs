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
//!
//! The public, stable library is the module [`api`]: the calls of
//! `tooling.md`, typed errors, option validation, the writer registry and the
//! JSON boundary for bindings. Every other module and item of the crate root
//! is an engine internal that the engine's own tests reach; it is hidden from
//! the documentation and may change without notice.
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
// Existing short names and range expressions are intentional in parser and schema code.
#![allow(
    clippy::similar_names,
    clippy::inefficient_to_string,
    clippy::range_plus_one
)]

pub mod api;
mod arena;
#[doc(hidden)]
pub mod ast;
#[doc(hidden)]
pub mod attr;
#[doc(hidden)]
pub mod block;
#[doc(hidden)]
pub mod chars;
#[doc(hidden)]
pub mod diagnostic;
pub mod export;
mod fast_plain;
#[doc(hidden)]
pub mod hash;
#[doc(hidden)]
pub mod html;
mod import;
#[doc(hidden)]
pub mod include;
#[doc(hidden)]
pub mod inline;
#[doc(hidden)]
pub mod lint;
#[doc(hidden)]
pub mod options;
#[cfg(feature = "profile")]
pub mod profile;
#[doc(hidden)]
pub mod reader;
#[doc(hidden)]
pub mod resolve;
#[doc(hidden)]
pub mod source;
#[doc(hidden)]
pub mod tables;
#[doc(hidden)]
pub mod tree;
// The canonical Carve writer (`writer.md`), reached through `api`.
mod write;

#[doc(hidden)]
pub use arena::Arena;

// The examples of README.md run as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeExamples;
#[doc(hidden)]
pub use options::Options;

#[doc(hidden)]
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

#[doc(hidden)]
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

#[doc(hidden)]
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

#[doc(hidden)]
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

#[doc(hidden)]
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

#[doc(hidden)]
/// Renders a supplied tree after running resolution anew (C2-A-135).
///
/// # Errors
///
/// [`RenderError::Losses`] under `strictLosses`.
pub fn render_tree(tree: &ast::owned::Document, opts: &Options) -> Result<String, RenderError> {
    let arena = Arena::new();
    let mut doc = tree.in_arena(&arena);
    resolve::clear_results(&mut doc);
    resolve::resolve(&mut doc, opts, &arena);
    let (html, losses) = html::render_with_losses(&doc, opts);
    let losses = unallowed(losses, opts);
    if opts.strict_losses && !losses.is_empty() {
        return Err(RenderError::Losses(losses));
    }
    Ok(html)
}

#[doc(hidden)]
/// Renders a supplied tree and returns the render-loss record (C2-T-005).
pub fn render_tree_with_diagnostics(
    tree: &ast::owned::Document,
    opts: &Options,
) -> Result<Rendered, RenderError> {
    let arena = Arena::new();
    let mut doc = tree.in_arena(&arena);
    let files = resolve_tree(&mut doc, opts, &arena);
    let (html, losses, derived, extension_nodes, destinations) =
        html::render_tree_parts(&doc, opts, files.as_ref(), None);
    let losses = unallowed(losses, opts);
    let lost = !losses.is_empty();
    let mut diagnostics =
        tree_loss_diagnostics(&doc, losses, derived, &extension_nodes, &destinations);
    if opts.strict_losses {
        for diagnostic in &mut diagnostics {
            diagnostic.severity = "error";
        }
    }
    let dropped_losses = cap_losses(&mut diagnostics, opts);
    Ok(Rendered {
        html: if opts.strict_losses && lost {
            String::new()
        } else {
            html
        },
        diagnostics,
        dropped_losses,
    })
}

/// The resolved view as an owned tree, for extension transforms that already
/// consume typed nodes. Avoids a JSON round trip between resolution and them.
pub(crate) fn resolved_owned(tree: &ast::owned::Document, opts: &Options) -> ast::owned::Document {
    let arena = Arena::new();
    let mut doc = tree.in_arena(&arena);
    let _ = resolve_tree_unflattened(&mut doc, opts, &arena);
    doc.owned()
}

/// Resolves a supplied tree again (C2-A-135). The children of an expanded
/// include keep their own file's definitions (C2-N-053), as in a render
/// from source, and render in its place (C2-N-075).
pub(crate) fn resolve_tree<'a>(
    doc: &mut ast::Document<'a>,
    opts: &Options,
    arena: &'a Arena,
) -> Option<resolve::files::Files<'a>> {
    let files = resolve_tree_unflattened(doc, opts, arena)?;
    if matches!(
        opts.profile,
        options::Profile::Full | options::Profile::Article
    ) {
        include::flatten(doc, arena);
    }
    Some(files)
}

/// [`resolve_tree`] with the include nodes left in place.
pub(crate) fn resolve_tree_unflattened<'a>(
    doc: &mut ast::Document<'a>,
    opts: &Options,
    arena: &'a Arena,
) -> Option<resolve::files::Files<'a>> {
    resolve::clear_results(doc);
    if !include::has_expanded(doc.children) {
        resolve::resolve(doc, opts, arena);
        return None;
    }
    let files = resolve::files::Files::build(doc, std::collections::HashMap::new());
    resolve::resolve_files(doc, opts, arena, Some(&files), &mut Vec::new());
    Some(files)
}

/// [`render_tree_with_diagnostics`] with a host's extension renderers
/// (C2-X-052). Losses are not refused here; the caller applies `strictLosses`.
pub(crate) fn render_tree_hooked(
    tree: &ast::owned::Document,
    opts: &Options,
    hook: &dyn html::ExtensionHook,
) -> Rendered {
    let arena = Arena::new();
    let mut doc = tree.in_arena(&arena);
    let files = resolve_tree(&mut doc, opts, &arena);
    let (html, losses, derived, extension_nodes, destinations) =
        html::render_tree_parts(&doc, opts, files.as_ref(), Some(hook));
    let mut diagnostics = tree_loss_diagnostics(
        &doc,
        unallowed(losses, opts),
        derived,
        &extension_nodes,
        &destinations,
    );
    if opts.strict_losses {
        for diagnostic in &mut diagnostics {
            diagnostic.severity = "error";
        }
    }
    let dropped_losses = cap_losses(&mut diagnostics, opts);
    Rendered {
        html,
        diagnostics,
        dropped_losses,
    }
}

/// C2-H-204: the losses whose code the host has not allowed.
fn unallowed(mut losses: Vec<&'static str>, opts: &Options) -> Vec<&'static str> {
    if !opts.allow_losses.is_empty() {
        losses.retain(|code| !opts.allows(code));
    }
    losses
}

/// C2-H-205: keeps the first `max_losses` render-loss entries and returns
/// the codes of the others.
fn cap_losses(diags: &mut Vec<diagnostic::Diagnostic>, opts: &Options) -> Vec<&'static str> {
    let mut kept = 0usize;
    let mut dropped = Vec::new();
    diags.retain(|d| {
        if d.channel != "render-loss" {
            return true;
        }
        kept += 1;
        if kept <= opts.max_losses {
            return true;
        }
        dropped.push(d.code);
        false
    });
    dropped
}

enum PathItem<'n, 'a> {
    Array(&'n [ast::Node<'a>], String),
    One(&'n ast::Node<'a>, String),
}

fn secondary_node_paths<'n, 'a>(
    node: &'n ast::Node<'a>,
    path: &str,
    pending: &mut Vec<PathItem<'n, 'a>>,
) {
    use PathItem::{Array, One};
    match node {
        ast::Node::Admonition(e) => {
            if let Some(title) = e.title.as_deref() {
                pending.push(Array(title, format!("{path}/title")));
            }
        }
        ast::Node::BlockExtension(e) => pending.push(Array(e.fallback, format!("{path}/fallback"))),
        ast::Node::InlineExtension(e) => {
            if let Some(fallback) = e.fallback.as_deref() {
                pending.push(Array(fallback, format!("{path}/fallback")));
            }
        }
        ast::Node::BlockQuote(e) => {
            if let Some(caption) = e.caption.as_deref() {
                pending.push(One(caption, format!("{path}/caption")));
            }
        }
        ast::Node::Caption(e) => {
            if let Some(short) = e.short.as_deref() {
                pending.push(Array(short, format!("{path}/short")));
            }
        }
        _ => secondary_node_paths_middle(node, path, pending),
    }
}

fn secondary_node_paths_middle<'n, 'a>(
    node: &'n ast::Node<'a>,
    path: &str,
    pending: &mut Vec<PathItem<'n, 'a>>,
) {
    use PathItem::{Array, One};
    match node {
        ast::Node::Citation(e) => {
            if let Some(prefix) = e.prefix.as_deref() {
                pending.push(Array(prefix, format!("{path}/prefix")));
            }
            if let Some(suffix) = e.suffix.as_deref() {
                pending.push(Array(suffix, format!("{path}/suffix")));
            }
        }
        ast::Node::CodeBlock(e) => {
            if let Some(caption) = e.caption.as_deref() {
                pending.push(One(caption, format!("{path}/caption")));
            }
            if let Some(title) = e.title.as_deref() {
                pending.push(Array(title, format!("{path}/title")));
            }
        }
        ast::Node::Directive(e) => {
            if let Some(title) = e.title.as_deref() {
                pending.push(Array(title, format!("{path}/title")));
            }
        }
        ast::Node::Div(e) => {
            if let Some(title) = e.title.as_deref() {
                pending.push(Array(title, format!("{path}/title")));
            }
        }
        _ => secondary_node_paths_tail(node, path, pending),
    }
}

fn secondary_node_paths_tail<'n, 'a>(
    node: &'n ast::Node<'a>,
    path: &str,
    pending: &mut Vec<PathItem<'n, 'a>>,
) {
    use PathItem::{Array, One};
    match node {
        ast::Node::FigureGroup(e) => {
            if let Some(caption) = e.caption.as_deref() {
                pending.push(One(caption, format!("{path}/caption")));
            }
        }
        ast::Node::Image(e) => {
            if let Some(description) = e.long_description.as_deref() {
                pending.push(Array(description, format!("{path}/longDescription")));
            }
        }
        ast::Node::Include(e) => {
            if let Some(caption) = e.caption.as_deref() {
                pending.push(One(caption, format!("{path}/caption")));
            }
        }
        ast::Node::Paragraph(e) => {
            if let Some(caption) = e.caption.as_deref() {
                pending.push(One(caption, format!("{path}/caption")));
            }
        }
        ast::Node::Ruby(e) => pending.push(Array(e.pairs, format!("{path}/pairs"))),
        ast::Node::RubyPair(e) => {
            pending.push(Array(e.base, format!("{path}/base")));
            pending.push(Array(e.annotation, format!("{path}/annotation")));
        }
        ast::Node::Table(e) => {
            if let Some(caption) = e.caption.as_deref() {
                pending.push(One(caption, format!("{path}/caption")));
            }
            pending.push(Array(e.columns, format!("{path}/columns")));
        }
        _ => {}
    }
}

fn node_path(nodes: &[ast::Node<'_>], target: usize) -> Option<String> {
    use PathItem::{Array, One};
    let mut pending = vec![Array(nodes, "/children".to_string())];
    while let Some(item) = pending.pop() {
        match item {
            Array(nodes, prefix) => {
                for (i, node) in nodes.iter().enumerate().rev() {
                    pending.push(One(node, format!("{prefix}/{i}")));
                }
            }
            One(node, path) => {
                if resolve::order::addr(node) == target {
                    return Some(path);
                }
                if let Some(children) = node.children() {
                    pending.push(Array(children, format!("{path}/children")));
                }
                secondary_node_paths(node, &path, &mut pending);
            }
        }
    }
    None
}

fn tree_loss_diagnostics(
    doc: &ast::Document<'_>,
    losses: Vec<&'static str>,
    derived: Option<usize>,
    extension_nodes: &[(usize, String)],
    destination_losses: &[html::DestinationLoss],
) -> Vec<diagnostic::Diagnostic> {
    let mut extensions = extension_nodes.iter();
    let mut destinations = destination_losses.iter();
    losses
        .into_iter()
        .map(|code| {
            let name = if code == "extension-fallback-rendered" {
                extensions.next()
            } else {
                None
            };
            let mut finding = diagnostic::Diagnostic::new(
                code,
                &name.map_or(Vec::new(), |(_, name)| vec![("name", name.as_str())]),
                None,
                None,
            );
            finding.path = match code {
                "extension-fallback-rendered" => {
                    name.and_then(|(target, _)| node_path(doc.children, *target))
                }
                "destination-denied" => destinations
                    .next()
                    .and_then(|loss| loss.site.as_ref())
                    .and_then(|(target, field)| {
                        node_path(doc.children, *target).map(|path| format!("{path}{field}"))
                    }),
                "derived-text-budget" => derived.and_then(|target| node_path(doc.children, target)),
                _ => None,
            };
            finding
        })
        .collect()
}

#[doc(hidden)]
/// Parses, resolves and renders HTML. A refused render (`try_render`)
/// gives no output, the empty string.
#[must_use]
pub fn render(input: &str, opts: &Options) -> String {
    try_render(input, opts).unwrap_or_default()
}

#[doc(hidden)]
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

#[doc(hidden)]
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
    if let Some(html) = fast_plain::render(input, opts) {
        return Ok(html);
    }
    let arena = Arena::for_input(input);
    let mut diags = Vec::new();
    let (out, losses) = render_parts(
        input,
        opts,
        &arena,
        &mut diags,
        &mut RenderDetails::default(),
    )?;
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
    duplicate_definitions(&doc, &src, diags);
    diags.extend(
        scope
            .findings
            .iter()
            .map(|f| diagnostic::Diagnostic::from_finding(f, &src, None)),
    );
    let expanded = opts.includes.is_some();
    let mut included_sources = std::collections::HashMap::new();
    if let Some(cfg) = &opts.includes {
        if input.contains("{{") {
            included_sources = include::expand(&mut doc, &scope, cfg, opts, arena, diags);
        }
    } else if input.contains("{{") {
        include::no_resolver(doc.children, diags);
    }
    let files = (expanded && input.contains("{{"))
        .then(|| resolve::files::Files::build(&doc, included_sources));
    let mut dups = Vec::new();
    let ordered = resolve::resolve_files(&mut doc, opts, arena, files.as_ref(), &mut dups);
    for (id, pos, at) in dups {
        let file = files.as_ref().and_then(|f| f.file_at(at));
        let pos = if let Some(name) = file {
            files
                .as_ref()
                .and_then(|f| f.sources().find(|(id, _)| *id == name))
                .and_then(|(_, text)| id_range(&source::Source::new(text), &id, pos))
                .or(pos)
        } else {
            id_range(&src, &id, pos).or(pos)
        };
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

pub(crate) fn parse_diagnostics(input: &str, opts: &Options) -> Vec<diagnostic::Diagnostic> {
    let arena = Arena::for_input(input);
    let mut diagnostics = Vec::new();
    let _ = prepare(input, opts, &arena, &mut diagnostics);
    diagnostics
}

/// C2-B-083, C2-B-085: only the first definition of each label wins.
fn duplicate_definitions(
    doc: &ast::Document<'_>,
    src: &source::Source<'_>,
    diags: &mut Vec<diagnostic::Diagnostic>,
) {
    let mut links = std::collections::HashSet::new();
    let mut abbreviations = std::collections::HashSet::new();
    let mut footnotes = std::collections::HashSet::new();
    for node in doc.children.iter() {
        let code = match node {
            ast::Node::LinkReferenceDefinition(x)
                if !links.insert(crate::chars::fold_label(x.key)) =>
            {
                "duplicate-link-reference-definition"
            }
            ast::Node::AbbreviationDef(x)
                if !abbreviations.insert(crate::chars::fold_label(x.key)) =>
            {
                "duplicate-abbreviation-definition"
            }
            ast::Node::Footnote(x) if !footnotes.insert(crate::chars::fold_label(x.key)) => {
                "duplicate-footnote-definition"
            }
            _ => continue,
        };
        let pos = node.pos().copied().map(|p| {
            if code == "duplicate-footnote-definition" {
                let (start, end) = src.line_range(p.start_line.saturating_sub(1) as usize);
                let marker_end = src
                    .text()
                    .get(start..end)
                    .and_then(|line| {
                        let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
                        block::shapes::footnote_marker_len(&line[indent..])
                            .map(|marker| start + indent + marker)
                    })
                    .unwrap_or(end);
                src.pos(p.start(), marker_end)
            } else {
                p
            }
        });
        diags.push(diagnostic::Diagnostic::new(code, &[], None, pos));
    }
}

fn id_range(src: &source::Source<'_>, id: &str, pos: Option<ast::Pos>) -> Option<ast::Pos> {
    let pos = pos?;
    if let Some(range) = id_attribute_range(src, id, pos.start(), pos.end()) {
        return Some(range);
    }
    let previous = pos.start_line.saturating_sub(2) as usize;
    let (start, end) = src.line_range(previous);
    let line = src.text().get(start..end)?;
    if line.trim_start_matches([' ', '\t']).starts_with('{') {
        return id_attribute_range(src, id, start, end);
    }
    None
}

fn id_attribute_range(
    src: &source::Source<'_>,
    id: &str,
    start: usize,
    end: usize,
) -> Option<ast::Pos> {
    let raw = src.text().get(start..end)?;
    let first_line_end = raw.find('\n').unwrap_or(raw.len());
    let mut on_first_line = None;
    let mut last = None;
    for (open, _) in raw.match_indices('{') {
        let Some((block, close)) = attr::parse_block(raw, open) else {
            continue;
        };
        if block.attrs.id.as_deref() != Some(id) {
            continue;
        }
        let body = raw.get(open + 1..close - 1)?;
        if let Some((from, to)) = authored_id_token(body) {
            let range = src.pos(start + open + 1 + from, start + open + 1 + to);
            if open < first_line_end && on_first_line.is_none() {
                on_first_line = Some(range);
            }
            last = Some(range);
        }
    }
    if raw.contains('\n') && !raw.starts_with('[') {
        on_first_line.or(last)
    } else {
        last
    }
}

fn authored_id_token(body: &str) -> Option<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut index = 0;
    let mut found = None;
    while index < bytes.len() {
        while bytes.get(index) == Some(&b' ') {
            index += 1;
        }
        let start = index;
        let mut quote = None;
        let mut escaped = false;
        while let Some(&byte) = bytes.get(index) {
            if byte == b' ' && quote.is_none() {
                break;
            }
            if matches!(byte, b'"' | b'\'') && !escaped {
                if quote == Some(byte) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(byte);
                }
            }
            escaped = byte == b'\\' && !escaped;
            index += 1;
        }
        if start == index {
            break;
        }
        let token = body.get(start..index)?;
        if token.starts_with('#')
            || token
                .split_once('=')
                .is_some_and(|(key, _)| key.eq_ignore_ascii_case("id"))
        {
            found = Some((start, index));
        }
    }
    found
}

/// `places` gets the file and the path of each unexpanded include.
#[derive(Default)]
struct RenderDetails {
    places: Vec<(Option<String>, String)>,
    destinations: Vec<html::DestinationLoss>,
    raws: Vec<html::RawLoss>,
    carriers: Vec<html::CarrierLoss>,
    included_sources: std::collections::HashMap<String, String>,
}

fn render_parts(
    input: &str,
    opts: &Options,
    arena: &Arena,
    diags: &mut Vec<diagnostic::Diagnostic>,
    details: &mut RenderDetails,
) -> Result<(String, Vec<&'static str>), RenderError> {
    let (mut doc, ordered, files) = prepare(input, opts, arena, diags)?;
    include::unexpanded(doc.children, None, &mut details.places);
    if let Some(files) = files {
        details.included_sources.extend(
            files
                .sources()
                .map(|(id, text)| (id.to_string(), text.to_string())),
        );
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
        let (out, mut losses) = html::render_files(
            &doc,
            opts,
            src.text(),
            &files,
            Some(&mut details.destinations),
            Some(&mut details.raws),
            Some(&mut details.carriers),
        );
        losses.extend(extra);
        return Ok((out, unallowed(losses, opts)));
    }
    let (out, losses) = if opts.profile != options::Profile::Full && input.contains('\0') {
        let src = source::Source::new(input);
        html::render_mapped_source(
            &doc,
            opts,
            src.render_view(),
            ordered,
            Some(&mut details.destinations),
            Some(&mut details.raws),
            Some(&mut details.carriers),
        )
    } else {
        html::render_source(
            &doc,
            opts,
            input,
            ordered,
            Some(&mut details.destinations),
            Some(&mut details.raws),
            Some(&mut details.carriers),
        )
    };
    Ok((out, unallowed(losses, opts)))
}

#[doc(hidden)]
/// The HTML of a render and the findings of its parse and its render.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub html: String,
    pub diagnostics: Vec<diagnostic::Diagnostic>,
    /// C2-H-205: the codes of the render losses past `max_losses`, in order.
    pub dropped_losses: Vec<&'static str>,
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

#[doc(hidden)]
/// Renders as [`render`] does and returns the findings of the parse
/// channel (C2-N-061) and the render losses (C2-H-130) beside the HTML.
#[must_use]
pub fn render_with_diagnostics(input: &str, opts: &Options) -> Rendered {
    let arena = Arena::for_input(input);
    let mut diags = Vec::new();
    let mut details = RenderDetails::default();
    let html = match render_parts(input, opts, &arena, &mut diags, &mut details) {
        Ok((html, losses)) => {
            diags.extend(loss_diagnostics(input, opts, &diags, &details, &losses));
            if opts.strict_losses && !losses.is_empty() {
                String::new()
            } else {
                html
            }
        }
        Err(_) => String::new(),
    };
    let dropped_losses = cap_losses(&mut diags, opts);
    Rendered {
        html,
        diagnostics: diags,
        dropped_losses,
    }
}

fn loss_diagnostics(
    input: &str,
    opts: &Options,
    parse_diags: &[diagnostic::Diagnostic],
    details: &RenderDetails,
    losses: &[&'static str],
) -> Vec<diagnostic::Diagnostic> {
    let mut places = details.places.iter().cloned();
    let source = source::Source::new(input);
    let mut raw_losses = details.raws.iter();
    let mut destination_losses = details.destinations.iter();
    let mut carrier_losses = details.carriers.iter();
    let mut carrier_cursors = std::collections::HashMap::new();
    let included_sources = &details.included_sources;
    let loss_diags: Vec<_> = losses
        .iter()
        .map(|c| {
            // C2-N-063: the loss of an unexpanded include names its file.
            let (include_file, include_path) = if *c == "include-unresolved" {
                places.next().map_or((None, None), |(f, p)| (f, Some(p)))
            } else {
                (None, None)
            };
            let raw = if *c == "raw-format-dropped" {
                raw_losses.next()
            } else {
                None
            };
            let destination = if *c == "destination-denied" {
                destination_losses.next()
            } else {
                None
            };
            let carrier = if *c == "carrier-attribute-dropped" {
                carrier_losses.next()
            } else {
                None
            };
            let file = include_file.or_else(|| {
                raw.and_then(|r| r.file.clone())
                    .or_else(|| destination.and_then(|d| d.file.clone()))
                    .or_else(|| carrier.and_then(|c| c.file.clone()))
            });
            let included_source = file
                .as_deref()
                .and_then(|id| included_sources.get(id))
                .map(|text| source::Source::new(text));
            let loss_source = included_source.as_ref().unwrap_or(&source);
            let pos = raw
                .and_then(|r| r.pos)
                .or_else(|| destination.and_then(|d| destination_loss_position(loss_source, d)))
                .or_else(|| destination.and_then(|d| d.pos))
                .or_else(|| {
                    carrier
                        .and_then(|c| carrier_loss_position(loss_source, c, &mut carrier_cursors))
                })
                .or_else(|| carrier.and_then(|c| c.pos))
                .or_else(|| {
                    file.is_none()
                        .then(|| loss_position(c, &source, opts, parse_diags))
                        .flatten()
                });
            let (variant, values) = loss_message(c, raw, destination, carrier, include_path);
            let values: Vec<_> = values
                .iter()
                .map(|(name, value)| (*name, value.as_str()))
                .collect();
            let mut diagnostic =
                diagnostic::Diagnostic::nth(c, variant, &values, file.as_deref(), pos);
            if opts.strict_losses {
                diagnostic.severity = "error";
            }
            diagnostic
        })
        .collect();
    loss_diags
}

fn destination_loss_position(
    source: &source::Source<'_>,
    loss: &html::DestinationLoss,
) -> Option<ast::Pos> {
    let node = loss.pos?;
    let start = usize::try_from(node.start_offset).ok()?;
    let end = usize::try_from(node.end_offset).ok()?;
    let raw = source.text().get(start..end)?;
    let at = if loss.attribute {
        raw.rfind(&loss.value)?
    } else if let Some(mark) = raw.rfind("](") {
        let from = mark + 2;
        let to = from + raw.get(from..)?.find(')')?;
        from + raw.get(from..to)?.find(&loss.value)?
    } else {
        raw.find(&loss.value)?
    } + start;
    Some(source.pos(at, at + loss.value.len()))
}

fn carrier_loss_position(
    source: &source::Source<'_>,
    loss: &html::CarrierLoss,
    cursors: &mut std::collections::HashMap<(Option<String>, u32, u32), usize>,
) -> Option<ast::Pos> {
    let node = loss.pos?;
    let id = (loss.file.clone(), node.start_offset, node.end_offset);
    let cursor = cursors.get(&id).copied().unwrap_or(0);
    let at = loss.regions.iter().find_map(|(from, to)| {
        let start = usize::try_from(*from).ok()?.max(cursor);
        let end = usize::try_from(*to).ok()?;
        source
            .text()
            .get(start..end)?
            .find(&loss.key)
            .map(|at| at + start)
    })?;
    let end = usize::try_from(node.end_offset).ok()?;
    let tail = source.text().get(at..end)?;
    let length = if tail.get(loss.key.len()..)?.starts_with('=') {
        tail.find([' ', '\t', '\n', '}']).unwrap_or(tail.len())
    } else {
        loss.key.len()
    };
    cursors.insert(id, at + length);
    Some(source.pos(at, at + length))
}

fn loss_message(
    code: &str,
    raw: Option<&html::RawLoss>,
    destination: Option<&html::DestinationLoss>,
    carrier: Option<&html::CarrierLoss>,
    include_path: Option<String>,
) -> (usize, Vec<(&'static str, String)>) {
    match code {
        "include-unresolved" => (0, vec![("path", include_path.unwrap_or_default())]),
        "carrier-attribute-dropped" => {
            let key = carrier.map_or_else(String::new, |loss| loss.key.clone());
            (0, vec![("key", key)])
        }
        "destination-denied" => (
            usize::from(destination.is_some_and(|d| d.image)),
            Vec::new(),
        ),
        "raw-format-dropped" => (
            0,
            vec![
                ("kind", raw.map_or("", |r| r.kind).to_string()),
                ("format", raw.map_or("", |r| r.format.as_str()).to_string()),
                ("target", "HTML".to_string()),
            ],
        ),
        _ => (0, Vec::new()),
    }
}

fn loss_position(
    code: &str,
    source: &source::Source<'_>,
    opts: &Options,
    parse_diags: &[diagnostic::Diagnostic],
) -> Option<ast::Pos> {
    let text = source.text();
    let span = match code {
        "carrier-attribute-dropped" => {
            let start = text.find("data-carve-")?;
            let len = text
                .get(start..)?
                .bytes()
                .take_while(|b| !matches!(b, b' ' | b'\t' | b'}'))
                .count();
            (start, start + len)
        }
        "destination-denied" => {
            let start = text.find("](")? + 2;
            let end = start + text.get(start..)?.find(')')?;
            (start, end)
        }
        "raw-format-dropped" => return raw_loss_position(source, parse_diags),
        "profile-construct-degraded" => {
            let start = text.find(|c: char| !c.is_whitespace())?;
            (start, start + 1)
        }
        "include-unresolved" | "include-expanded-roundtrip" => {
            let start = text.find("{{")?;
            (start, start + 2)
        }
        "derived-text-budget" => return derived_overflow_pos(text, opts),
        _ => return None,
    };
    Some(source.pos(span.0, span.1))
}

fn raw_loss_position(
    source: &source::Source<'_>,
    parse_diags: &[diagnostic::Diagnostic],
) -> Option<ast::Pos> {
    if let Some(pos) = parse_diags
        .iter()
        .find(|d| d.code == "unclosed-code-fence")
        .and_then(|d| d.pos)
    {
        return Some(pos);
    }
    let start = source.text().find('`')?;
    Some(source.pos(start, start + 1))
}

fn derived_overflow_pos(input: &str, opts: &Options) -> Option<ast::Pos> {
    let arena = Arena::for_input(input);
    let doc = parse_resolved_in(input, opts, &arena).ok()?;
    let order = resolve::order::Order::build(&doc);
    let mut remaining =
        tables::DERIVED_TEXT_BYTES.max(input.len().saturating_mul(tables::DERIVED_TEXT_FACTOR));
    for visit in order.visits {
        if let ast::Node::Abbreviation(abbr) = visit.node
            && let Some(expansion) = abbr.expansion
        {
            if expansion.len() > remaining {
                return abbr.pos;
            }
            remaining -= expansion.len();
        }
    }
    None
}

#[doc(hidden)]
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

#[doc(hidden)]
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
    stamped_value(&doc).to_string()
}

#[doc(hidden)]
/// [`tree_json`] together with the owned tree of the same parse, for a
/// caller that needs both without reading its own output back through the
/// tree reader. `None` where [`tree_json`] gives no tree.
#[must_use]
pub fn tree_parts(input: &str, opts: &Options) -> Option<(String, ast::owned::Document)> {
    let arena = Arena::for_input(input);
    let (doc, _, files) = prepare(input, opts, &arena, &mut Vec::new()).ok()?;
    if files.is_none() && opts.includes.is_none() {
        return Some((to_json(&doc), doc.owned()));
    }
    let mut v = stamped_value(&doc);
    let json = v.to_string();
    if let Some(m) = v.as_object_mut() {
        m.remove("type");
    }
    // The owned positions carry the file stamps, which `doc.owned()` lacks.
    let owned = serde_json::from_value(v).ok()?;
    Some((json, owned))
}

fn stamped_value(doc: &ast::Document<'_>) -> serde_json::Value {
    let mut v = serde_json::to_value(doc).unwrap_or_default();
    if let Some(m) = v.as_object_mut() {
        m.insert("type".into(), "document".into());
    }
    include::stamp_files(&mut v, None);
    v
}

#[doc(hidden)]
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
    let (out, losses) =
        html::render_mapped_source(&doc, opts, src.render_view(), ordered, None, None, None);
    let losses = unallowed(losses, opts);
    if opts.strict_losses && !losses.is_empty() {
        return Err(RenderError::Losses(losses));
    }
    Ok(out)
}

#[doc(hidden)]
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
