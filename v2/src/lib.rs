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
mod fast_plain;
pub mod hash;
pub mod html;
pub mod include;
pub mod inline;
pub mod lint;
pub mod options;
#[cfg(feature = "profile")]
pub mod profile;
pub mod reader;
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
    if opts.strict_losses && !losses.is_empty() {
        return Err(RenderError::Losses(losses));
    }
    Ok(html)
}

/// Renders a supplied tree and returns the render-loss record (C2-T-005).
///
/// # Errors
///
/// [`RenderError::Losses`] under `strictLosses`.
pub fn render_tree_with_diagnostics(
    tree: &ast::owned::Document,
    opts: &Options,
) -> Result<Rendered, RenderError> {
    let arena = Arena::new();
    let mut doc = tree.in_arena(&arena);
    resolve::clear_results(&mut doc);
    resolve::resolve(&mut doc, opts, &arena);
    let (html, losses, derived) = html::render_with_details(&doc, opts);
    if opts.strict_losses && !losses.is_empty() {
        return Err(RenderError::Losses(losses));
    }
    let diagnostics = losses
        .into_iter()
        .map(|code| {
            let mut finding = diagnostic::Diagnostic::new(code, &[], None, None);
            if code == "derived-text-budget" {
                finding.path = derived.and_then(|target| node_path(doc.children, target));
            }
            finding
        })
        .collect();
    Ok(Rendered { html, diagnostics })
}

fn node_path(nodes: &[ast::Node<'_>], target: usize) -> Option<String> {
    let mut pending = vec![(nodes, String::new())];
    while let Some((children, parent)) = pending.pop() {
        for (i, node) in children.iter().enumerate().rev() {
            let path = format!("{parent}/children/{i}");
            if resolve::order::addr(node) == target {
                return Some(path);
            }
            if let Some(next) = node.children() {
                pending.push((next, path));
            }
        }
    }
    None
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
        &mut Vec::new(),
        &mut Vec::new(),
        &mut Vec::new(),
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
        let pos = if file.is_none() {
            id_range(&src, &id, pos).or(pos)
        } else {
            pos
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
                let (_, end) = src.line_range(p.start_line.saturating_sub(1) as usize);
                src.pos(p.start(), end)
            } else {
                p
            }
        });
        diags.push(diagnostic::Diagnostic::new(code, &[], None, pos));
    }
}

fn id_range(src: &source::Source<'_>, id: &str, pos: Option<ast::Pos>) -> Option<ast::Pos> {
    let pos = pos?;
    let (start, end) = src.line_range(pos.start_line.saturating_sub(1) as usize);
    let line = src.text().get(start..end)?;
    if !line.trim_start_matches([' ', '\t']).starts_with('{') {
        return None;
    }
    let needle = format!("#{id}");
    let offset = line.find(&needle)?;
    Some(src.pos(start + offset, start + offset + needle.len()))
}

/// `places` gets the file of each unexpanded include of an expanded tree.
fn render_parts(
    input: &str,
    opts: &Options,
    arena: &Arena,
    diags: &mut Vec<diagnostic::Diagnostic>,
    places: &mut Vec<Option<String>>,
    destinations: &mut Vec<html::DestinationLoss>,
    raws: &mut Vec<html::RawLoss>,
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
        let (out, mut losses) = html::render_files(
            &doc,
            opts,
            src.text(),
            &files,
            Some(destinations),
            Some(raws),
        );
        losses.extend(extra);
        return Ok((out, losses));
    }
    let (out, losses) = if opts.profile != options::Profile::Full && input.contains('\0') {
        let src = source::Source::new(input);
        html::render_mapped_source(
            &doc,
            opts,
            src.render_view(),
            ordered,
            Some(destinations),
            Some(raws),
        )
    } else {
        html::render_source(&doc, opts, input, ordered, Some(destinations), Some(raws))
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
    let mut destinations = Vec::new();
    let mut raws = Vec::new();
    let html = match render_parts(
        input,
        opts,
        &arena,
        &mut diags,
        &mut places,
        &mut destinations,
        &mut raws,
    ) {
        Ok((html, losses)) => {
            let mut places = places.into_iter();
            let source = source::Source::new(input);
            let mut raw_losses = raws.iter();
            let mut destination_losses = destinations.iter();
            let loss_diags: Vec<_> = losses
                .iter()
                .map(|c| {
                    // C2-N-063: the loss of an unexpanded include names its file.
                    let file = if *c == "include-unresolved" {
                        places.next().flatten()
                    } else {
                        None
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
                    let pos = raw
                        .and_then(|r| r.pos)
                        .or_else(|| destination.and_then(|d| destination_loss_position(&source, d)))
                        .or_else(|| destination.and_then(|d| d.pos))
                        .or_else(|| loss_position(c, &source, opts, diags.as_slice()));
                    let (variant, values) = loss_message(c, source.text(), pos, raw, destination);
                    let values: Vec<_> = values
                        .iter()
                        .map(|(name, value)| (*name, value.as_str()))
                        .collect();
                    diagnostic::Diagnostic::nth(c, variant, &values, file.as_deref(), pos)
                })
                .collect();
            diags.extend(loss_diags);
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

fn loss_message(
    code: &str,
    source: &str,
    pos: Option<ast::Pos>,
    raw: Option<&html::RawLoss>,
    destination: Option<&html::DestinationLoss>,
) -> (usize, Vec<(&'static str, String)>) {
    match code {
        "carrier-attribute-dropped" => {
            let key = pos
                .and_then(|position| {
                    source.get(
                        usize::try_from(position.start_offset).ok()?
                            ..usize::try_from(position.end_offset).ok()?,
                    )
                })
                .and_then(|attribute| attribute.split_once('='))
                .map_or_else(String::new, |(key, _)| key.to_string());
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
    let (out, losses) =
        html::render_mapped_source(&doc, opts, src.render_view(), ordered, None, None);
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
