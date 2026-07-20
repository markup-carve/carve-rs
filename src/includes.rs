//! Processor-level file inclusion (`{{ path }}`), spec PART 9 §19 (I1-I11).
//!
//! The core parser is untouched and performs NO file I/O: it does not know the
//! directive exists and leaves `{{ … }}` as ordinary inline text. Expansion is
//! this separate, opt-in pass over an already-parsed [`Document`]. With no
//! resolver configured the directive stays literal, which is the behavior the
//! conformance corpus pins.
//!
//! ```
//! use carve::{expand_includes, parse, render_html, IncludeOptions, IncludeResolved};
//!
//! let source = "Before.\n\n{{ child.crv }}\n";
//! let doc = parse(source);
//! let resolver = |path: &str, _ctx: &carve::IncludeContext<'_>| {
//!     (path == "child.crv").then(|| IncludeResolved::from("Included body."))
//! };
//! let opts = IncludeOptions::new().with_resolver(&resolver);
//! let result = expand_includes(doc, source, &opts);
//! assert!(render_html(&result.doc).contains("<p>Included body.</p>"));
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::ast::{BlockNode, Document, FigureTarget, Heading, InlineNode, Paragraph};
use crate::parse::{parse, slugify_parse};
use crate::render::plain_inlines;

/// Default transitive include depth limit (spec I6 recommends at least 16).
pub const DEFAULT_MAX_DEPTH: usize = 16;
/// Byte-budget floor, matching the §25 amplification bound `max(1 MB, 8 x input)`.
const MIN_BUDGET: usize = 1024 * 1024;

/// A degradation or rename reported by [`expand_includes`].
///
/// Every failure mode in spec I7 produces one of these and leaves the offending
/// directive LITERAL; inclusion never silently drops a directive.
///
/// Unlike carve-js this carries no line/column: the Rust AST does not retain
/// source positions, so no position is reported rather than a fabricated one.
/// Full position remapping (spec I4) is out of scope in every engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeWarning {
    /// Stable rule id, e.g. `"include-cycle"`.
    pub rule: String,
    /// Human-readable explanation.
    pub message: String,
    /// Identity of the file the warning AROSE in: the canonical id a resolver
    /// returned for that file, or the raw directive path when the resolver
    /// returned plain source. A directive that failed to resolve is attributed
    /// to the document CONTAINING it, not to the target it names; a warning
    /// raised while expanding a child (a heading clamp, a rename, a nested
    /// cycle) is attributed to that child.
    ///
    /// `None` for a top-level document the caller gave no `source_path` for -
    /// there is no identity to report, and none is invented.
    pub file: Option<String>,
}

/// Context handed to a resolver for one directive.
#[derive(Debug, Clone)]
pub struct IncludeContext<'a> {
    /// Identity of the root document, when the host supplied one.
    pub source_path: Option<&'a str>,
    /// Include chain, root first. Each entry is the canonical id a resolver
    /// returned, or the raw directive path when it returned plain source.
    /// The last entry is the file containing the directive being resolved,
    /// which is what relative resolution keys off (spec I1).
    pub stack: &'a [String],
    /// Zero-based include depth of the directive being resolved.
    pub depth: usize,
}

/// What a resolver produces: source text, optionally with a canonical id.
///
/// The id feeds cycle detection (spec I6) and dependency identity (I11), and
/// becomes the parent entry in [`IncludeContext::stack`] for nested resolves.
/// Resolvers that map paths to files (filesystem, VFS) SHOULD supply one;
/// without it two spellings of the same file (`b.crv` vs `./b.crv`) defeat the
/// cycle guard and only the depth limit stops the recursion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeResolved {
    pub source: String,
    pub id: Option<String>,
}

impl IncludeResolved {
    /// Source text with a canonical id.
    pub fn with_id(source: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            id: Some(id.into()),
        }
    }
}

impl From<String> for IncludeResolved {
    fn from(source: String) -> Self {
        Self { source, id: None }
    }
}

impl From<&str> for IncludeResolved {
    fn from(source: &str) -> Self {
        Self {
            source: source.to_string(),
            id: None,
        }
    }
}

/// Host-supplied path resolution (spec I3). The core never touches the
/// filesystem; a host that wants inclusion supplies one of these.
///
/// Returning `None` means "unresolvable" and covers missing files, unreadable
/// files, and containment denials alike - a host wants to re-check any of them
/// if the tree changes, so all three are reported as attempted dependencies.
pub trait IncludeResolver {
    fn resolve(&self, path: &str, ctx: &IncludeContext<'_>) -> Option<IncludeResolved>;
}

impl<F> IncludeResolver for F
where
    F: Fn(&str, &IncludeContext<'_>) -> Option<IncludeResolved>,
{
    fn resolve(&self, path: &str, ctx: &IncludeContext<'_>) -> Option<IncludeResolved> {
        self(path, ctx)
    }
}

/// Expansion knobs. Default-constructed options carry NO resolver, so
/// [`expand_includes`] is a no-op that leaves every directive literal.
#[derive(Default)]
pub struct IncludeOptions<'a> {
    resolver: Option<&'a dyn IncludeResolver>,
    source_path: Option<String>,
    max_depth: Option<usize>,
    max_bytes: Option<usize>,
}

impl<'a> IncludeOptions<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_resolver(mut self, resolver: &'a dyn IncludeResolver) -> Self {
        self.resolver = Some(resolver);
        self
    }

    /// Identity of the root document. Seeds the cycle-guard stack and the
    /// `file` attribution on warnings raised in the root.
    pub fn with_source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = Some(path.into());
        self
    }

    /// Maximum transitive include depth (spec I6). Defaults to
    /// [`DEFAULT_MAX_DEPTH`].
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Total expanded-source byte budget. Defaults to `max(1 MB, 8 x root
    /// source bytes)`, the §25 amplification bound.
    pub fn with_max_bytes(mut self, bytes: usize) -> Self {
        self.max_bytes = Some(bytes);
        self
    }
}

/// One include target touched during expansion (spec I11).
///
/// Unresolved targets are reported too: a host that watched only the files it
/// successfully read would never learn that a previously-missing target now
/// EXISTS, so a preview would stay stale at exactly the moment the author
/// fixes the problem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeDependency {
    /// The resolver's canonical id when it supplied one (the identity the
    /// cycle guard uses), otherwise the directive path as written.
    pub id: String,
    /// True when the resolver produced source text that was actually merged.
    pub resolved: bool,
}

/// Outcome of [`expand_includes`].
#[derive(Debug, Clone)]
pub struct IncludeResult {
    pub doc: Document,
    pub warnings: Vec<IncludeWarning>,
    /// Every include target touched during the whole recursive expansion,
    /// de-duplicated, in first-encounter order. Empty without a resolver.
    pub dependencies: Vec<IncludeDependency>,
}

// ---------------------------------------------------------------------------
// Directive syntax (spec I1 / PART 6)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
enum Shift {
    By(i32),
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Directive {
    path: String,
    section: Option<String>,
    lines: Option<(usize, usize)>,
    shift: Shift,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_rest(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// Bare path: stops at space, `#`, `@`, `}` (PART 6), plus the quote
/// characters that introduce the quoted form.
fn is_bare_path_char(c: char) -> bool {
    !matches!(c, '#' | '@' | '}' | '"' | '\u{201c}') && !c.is_whitespace()
}

/// Match a directive starting exactly at `start`, returning the byte index one
/// past the closing `}}` plus the parsed shape.
///
/// Mirrors carve-js's directive regex: `{{`, whitespace, a bare / straight- or
/// curly-quoted path, an optional `#section`, a lazily-matched option tail, at
/// least one whitespace, `}}`. The tail is bounded by the FIRST `}}` that has
/// whitespace immediately before it, which is what the lazy quantifier picks.
fn match_directive_at(text: &str, start: usize) -> Option<(usize, RawDirective)> {
    let bytes = text.as_bytes();
    if !text[start..].starts_with("{{") {
        return None;
    }
    let mut i = start + 2;
    // `\s+` after the opening braces.
    let ws_start = i;
    while i < bytes.len() && text[i..].chars().next()?.is_whitespace() {
        i += text[i..].chars().next()?.len_utf8();
    }
    if i == ws_start {
        return None;
    }

    // Path: straight-quoted, curly-quoted (smart typography already rewrote
    // the source), or bare.
    let rest = &text[i..];
    let (path, after_path) = if let Some(body) = rest.strip_prefix('"') {
        // `"((?:\\.|[^"\\])*)"`: a backslash escapes the following character,
        // and only `\"` / `\\` unescape (every other pair stays verbatim).
        let mut out = String::new();
        let mut chars = body.char_indices();
        let mut close = None;
        while let Some((off, c)) = chars.next() {
            match c {
                '\\' => {
                    let (_, next) = chars.next()?;
                    if next == '"' || next == '\\' {
                        out.push(next);
                    } else {
                        out.push('\\');
                        out.push(next);
                    }
                }
                '"' => {
                    close = Some(off);
                    break;
                }
                _ => out.push(c),
            }
        }
        (out, i + 1 + close? + 1)
    } else if let Some(body) = rest.strip_prefix('\u{201c}') {
        let close = body.find('\u{201d}')?;
        (
            body[..close].to_string(),
            i + '\u{201c}'.len_utf8() + close + '\u{201d}'.len_utf8(),
        )
    } else {
        let end = rest
            .char_indices()
            .find(|(_, c)| !is_bare_path_char(*c))
            .map(|(o, _)| o)
            .unwrap_or(rest.len());
        if end == 0 {
            return None;
        }
        (rest[..end].to_string(), i + end)
    };
    i = after_path;

    // Optional ` #section`, exactly one, immediately after the path.
    let mut section = None;
    {
        let tail = &text[i..];
        let ws = tail.len() - tail.trim_start_matches(|c: char| c.is_whitespace()).len();
        if ws > 0 {
            let after_ws = &tail[ws..];
            if let Some(name) = after_ws.strip_prefix('#') {
                let mut it = name.chars();
                if let Some(first) = it.next() {
                    if is_ident_start(first) {
                        let end = name
                            .char_indices()
                            .find(|(_, c)| !is_ident_rest(*c))
                            .map(|(o, _)| o)
                            .unwrap_or(name.len());
                        section = Some(name[..end].to_string());
                        i += ws + 1 + end;
                    }
                }
            }
        }
    }

    // Lazy option tail terminated by `\s+}}`: the first `}}` with whitespace
    // right before it wins.
    let tail = &text[i..];
    let mut search = 0usize;
    loop {
        let hit = tail[search..].find("}}")? + search;
        let before = &tail[..hit];
        let trimmed = before.trim_end_matches(|c: char| c.is_whitespace());
        if trimmed.len() < before.len() {
            return Some((
                i + hit + 2,
                RawDirective {
                    path,
                    section,
                    options: trimmed.to_string(),
                },
            ));
        }
        search = hit + 1;
    }
}

struct RawDirective {
    path: String,
    section: Option<String>,
    options: String,
}

/// Outcome of turning a raw match into a directive: options are validated here
/// so an unknown key or malformed value degrades to a warning + literal (I1/I7).
enum ParsedDirective {
    Ok(Box<Directive>),
    /// An `@…`-shaped option that is unknown or malformed; carries the offending token.
    BadOption(String),
    /// Not directive-shaped at all; no warning, stays literal silently.
    NotADirective,
}

fn parse_options(raw: RawDirective) -> ParsedDirective {
    let mut lines = None;
    let mut shift = Shift::By(0);
    for part in raw.options.split_whitespace() {
        let bad = || {
            if part.starts_with('@') {
                ParsedDirective::BadOption(part.to_string())
            } else {
                ParsedDirective::NotADirective
            }
        };
        let Some(body) = part.strip_prefix('@') else {
            return bad();
        };
        let Some((key, value)) = body.split_once(':') else {
            return bad();
        };
        let key_ok = {
            let mut it = key.chars();
            it.next().is_some_and(is_ident_start) && it.all(is_ident_rest)
        };
        let value_ok = !value.is_empty()
            && value
                .chars()
                .all(|c| !matches!(c, '#' | '@' | '}') && !c.is_whitespace());
        if !key_ok || !value_ok {
            return bad();
        }
        match key {
            "lines" => {
                let Some((a_raw, b_raw)) = value.split_once('-') else {
                    return bad();
                };
                // `[1-9]\d*` on both sides: 1-based, so a leading zero is
                // malformed rather than silently normalized.
                let positive = |s: &str| {
                    !s.is_empty()
                        && !s.starts_with('0')
                        && s.chars().all(|c| c.is_ascii_digit())
                        && s.parse::<usize>().is_ok()
                };
                if !positive(a_raw) || !positive(b_raw) {
                    return bad();
                }
                let (a, b) = (
                    a_raw.parse::<usize>().unwrap_or(0),
                    b_raw.parse::<usize>().unwrap_or(0),
                );
                // An inverted range is an error, not an empty selection.
                if b < a {
                    return bad();
                }
                lines = Some((a, b));
            }
            "shift" => {
                if value == "auto" {
                    shift = Shift::Auto;
                } else {
                    let digits = value.strip_prefix(['+', '-']).unwrap_or(value);
                    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
                        return bad();
                    }
                    match value.strip_prefix('+').unwrap_or(value).parse::<i32>() {
                        Ok(n) => shift = Shift::By(n),
                        Err(_) => return bad(),
                    }
                }
            }
            // Every other `@key` is RESERVED (I1).
            _ => return bad(),
        }
    }
    ParsedDirective::Ok(Box::new(Directive {
        path: raw.path,
        section: raw.section,
        lines,
        shift,
    }))
}

/// Parse a whole string that must be exactly one directive (block form).
fn parse_full_directive(text: &str) -> ParsedDirective {
    match match_directive_at(text, 0) {
        Some((end, raw)) if end == text.len() => parse_options(raw),
        _ => ParsedDirective::NotADirective,
    }
}

/// Loose directive shape: one whole-paragraph `{{…}}` token, valid or not.
fn is_directive_shaped(text: &str) -> bool {
    let t = text.trim();
    t.starts_with("{{")
        && t.ends_with("}}")
        && t.len() >= 4
        && !t[2..t.len() - 2].contains('{')
        && !t[2..t.len() - 2].contains('}')
}

// ---------------------------------------------------------------------------
// Expansion state
// ---------------------------------------------------------------------------

struct State<'a> {
    opts: &'a IncludeOptions<'a>,
    warnings: Vec<IncludeWarning>,
    max_depth: usize,
    max_bytes: usize,
    used_bytes: usize,
    stack: Vec<String>,
    depth: usize,
    /// Identity of the document whose content is currently being expanded.
    file: Option<String>,
    used_heading_ids: HashSet<String>,
    /// Include targets in first-encounter order, plus an index for dedup.
    dependencies: Vec<IncludeDependency>,
    dep_index: HashMap<String, usize>,
    /// Footnote definitions of each document on the expansion stack; the last
    /// entry belongs to the document currently being expanded.
    footnotes: Vec<BTreeMap<String, Vec<BlockNode>>>,
    /// Spec I8 context level C: the level of the nearest preceding heading in
    /// the directive's own block container or an enclosing one, 0 when there is
    /// none. Containers save and restore it, so a CLOSED sibling container does
    /// not set context.
    ///
    /// Held in the coordinate system of the content being expanded: a child
    /// that will later be shifted by N sees `C - N` here, so once the shift
    /// lands the effective context is the parent's actual level again.
    context_level: i32,
}

impl State<'_> {
    fn warn(&mut self, rule: &str, message: String) {
        let file = self.file.clone();
        self.warn_for(rule, message, file);
    }

    fn warn_for(&mut self, rule: &str, message: String, file: Option<String>) {
        self.warnings.push(IncludeWarning {
            rule: rule.to_string(),
            message,
            file,
        });
    }

    /// Record an include target for host file watching. Deduplicated by id,
    /// first encounter fixes the order, and a later success upgrades an entry
    /// first seen unresolved.
    fn note(&mut self, id: &str, resolved: bool) {
        match self.dep_index.get(id) {
            Some(&idx) => {
                if resolved {
                    self.dependencies[idx].resolved = true;
                }
            }
            None => {
                self.dep_index
                    .insert(id.to_string(), self.dependencies.len());
                self.dependencies.push(IncludeDependency {
                    id: id.to_string(),
                    resolved,
                });
            }
        }
    }

    /// Force an entry back to unresolved, bypassing `note`'s upgrade rule.
    fn note_forced_unresolved(&mut self, id: &str) {
        if let Some(&idx) = self.dep_index.get(id) {
            self.dependencies[idx].resolved = false;
        } else {
            self.note(id, false);
        }
    }
}

fn slice_lines(source: &str, range: (usize, usize)) -> String {
    let mut lines: Vec<&str> = source.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let start = range.0.saturating_sub(1);
    let end = range.1.min(lines.len());
    if start >= end {
        return String::new();
    }
    lines[start..end].join("\n")
}

fn resolve_child(d: &Directive, state: &mut State<'_>) -> Option<(String, String)> {
    let resolver = state.opts.resolver?;
    // I1: the two SELECTION mechanisms are mutually exclusive.
    if d.section.is_some() && d.lines.is_some() {
        state.warn(
            "include-selection-conflict",
            format!(
                "Include \"{}\" cannot use both #section and @lines.",
                d.path
            ),
        );
        return None;
    }
    if state.depth >= state.max_depth {
        // Never handed to the resolver, but still a target the host may want
        // to watch, so it is reported as unresolved rather than dropped.
        state.note(&d.path, false);
        state.warn(
            "include-depth",
            format!(
                "Include depth limit of {} exceeded for \"{}\".",
                state.max_depth, d.path
            ),
        );
        return None;
    }

    let ctx = IncludeContext {
        source_path: state.opts.source_path.as_deref(),
        stack: &state.stack,
        depth: state.depth,
    };
    let Some(resolved) = resolver.resolve(&d.path, &ctx) else {
        // Covers missing files and containment denials alike.
        state.note(&d.path, false);
        state.warn(
            "include-unresolved",
            format!("Include \"{}\" could not be resolved.", d.path),
        );
        return None;
    };
    let id = resolved.id.unwrap_or_else(|| d.path.clone());
    let source = resolved.source;
    // I7: binary content warns and stays literal.
    if source.contains('\0') {
        state.note(&id, false);
        state.warn(
            "include-non-text",
            format!("Include \"{}\" did not resolve to text.", d.path),
        );
        return None;
    }
    state.note(&id, true);
    // The cycle guard compares canonical ids AFTER resolution, so a resolver
    // that supplies ids catches "b.crv" vs "./b.crv" spellings of one file.
    if state.stack.iter().any(|e| e == &id) {
        state.warn(
            "include-cycle",
            format!("Include cycle detected for \"{}\".", d.path),
        );
        return None;
    }
    let bytes = source.len();
    if state.used_bytes + bytes > state.max_bytes {
        state.warn(
            "include-budget",
            format!("Include byte budget exceeded by \"{}\".", d.path),
        );
        return None;
    }
    state.used_bytes += bytes;
    let selected = match d.lines {
        Some(range) => slice_lines(&source, range),
        None => source,
    };
    Some((selected, id))
}

fn heading_id(h: &Heading) -> String {
    h.attrs
        .as_ref()
        .and_then(|a| a.id.clone())
        .unwrap_or_else(|| slugify_parse(&plain_inlines(&h.children), false))
}

/// The subtree rooted at the heading whose id equals `section`: that heading
/// through content up to the next same-or-higher-level heading (I1).
fn select_section(children: &[BlockNode], section: &str) -> Option<Vec<BlockNode>> {
    let start = children
        .iter()
        .position(|b| matches!(b, BlockNode::Heading(h) if heading_id(h) == section))?;
    let BlockNode::Heading(head) = &children[start] else {
        return None;
    };
    let level = head.level;
    let mut end = start + 1;
    while end < children.len() {
        if let BlockNode::Heading(h) = &children[end] {
            if h.level <= level {
                break;
            }
        }
        end += 1;
    }
    Some(children[start..end].to_vec())
}

fn shift_blocks(blocks: &mut [BlockNode], shift: i32, state: &mut State<'_>) {
    if shift == 0 {
        return;
    }
    walk_blocks_mut(blocks, &mut |block| {
        if let BlockNode::Heading(h) = block {
            let shifted = h.level as i32 + shift;
            let clamped = shifted.clamp(1, 6);
            if clamped != shifted {
                // The heading is KEPT, never dropped (I8).
                state.warn(
                    "include-heading-clamp",
                    format!("Included heading level {shifted} was clamped to {clamped}."),
                );
            }
            h.level = clamped as u8;
        }
    });
}

/// Spec I8 `@shift:auto`: N = (C + 1) - T, where C is the context level at the
/// include site and T the MINIMUM heading level in the resolved content.
///
/// The minimum rather than the first heading's level, so the child's internal
/// relative structure survives. Content with no headings is a no-op (N = 0)
/// and warns about nothing, which also covers inline includes.
///
/// Called AFTER the child's own includes are expanded, so headings a child
/// contributes only by including another file still count.
fn auto_shift(children: &[BlockNode], context_level: i32) -> i32 {
    let mut top: Option<u8> = None;
    walk_blocks(children, &mut |block| {
        if let BlockNode::Heading(h) = block {
            // `Option::is_none_or` is newer than this crate's MSRV (1.75).
            if top.map_or(true, |t| h.level < t) {
                top = Some(h.level);
            }
        }
    });
    match top {
        None => 0,
        Some(t) => context_level + 1 - t as i32,
    }
}

/// Merge-time collision pass for EXPLICIT heading ids (spec I5): parent ids and
/// earlier includes win, a later duplicate gets the least free `-N`, and the
/// child's own crossrefs follow the rename so they keep resolving within the
/// child's scope. Runs depth-first at merge time because after splicing, file
/// provenance is gone.
fn rename_child_heading_ids(
    children: &mut [BlockNode],
    footnote_bodies: &mut BTreeMap<String, Vec<BlockNode>>,
    state: &mut State<'_>,
) {
    let mut rename: HashMap<String, String> = HashMap::new();
    walk_blocks_mut(children, &mut |block| {
        let BlockNode::Heading(h) = block else { return };
        let Some(id) = h.attrs.as_ref().and_then(|a| a.id.clone()) else {
            return;
        };
        if state.used_heading_ids.insert(id.clone()) {
            return;
        }
        let renamed = next_free(&id, |c| state.used_heading_ids.contains(c));
        if let Some(attrs) = h.attrs.as_mut() {
            attrs.id = Some(renamed.clone());
        }
        state.used_heading_ids.insert(renamed.clone());
        state.warn(
            "include-heading-id-rename",
            format!("Heading id \"{id}\" was renamed to \"{renamed}\"."),
        );
        rename.insert(id, renamed);
    });
    if !rename.is_empty() {
        let empty = HashMap::new();
        rename_in_blocks(children, &empty, &rename);
        for body in footnote_bodies.values_mut() {
            rename_in_blocks(body, &empty, &rename);
        }
    }
}

fn next_free(base: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut n = 2u32;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

fn normalize_ref_label(label: &str) -> String {
    label.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `child_file` is the CHILD's identity: the renamed label is the child's own,
/// and the merge runs after expansion has already restored the parent as the
/// current file, so attribution is passed in explicitly.
fn merge_footnotes(
    child_defs: BTreeMap<String, Vec<BlockNode>>,
    child_children: &mut [BlockNode],
    state: &mut State<'_>,
    child_file: Option<String>,
) {
    if child_defs.is_empty() {
        return;
    }
    let mut rename: HashMap<String, String> = HashMap::new();
    for (label, body) in child_defs {
        let target = state
            .footnotes
            .last()
            .expect("footnote stack is never empty during expansion");
        let taken = target
            .keys()
            .any(|existing| normalize_ref_label(existing) == normalize_ref_label(&label));
        let final_label = if taken {
            let existing: HashSet<String> = target.keys().cloned().collect();
            next_free(&label, |c| existing.contains(c))
        } else {
            label.clone()
        };
        if final_label != label {
            state.warn_for(
                "include-footnote-rename",
                format!("Footnote label \"{label}\" was renamed to \"{final_label}\"."),
                child_file.clone(),
            );
            rename.insert(label.clone(), final_label.clone());
        }
        if let Some(target) = state.footnotes.last_mut() {
            target.insert(final_label, body);
        }
    }
    if !rename.is_empty() {
        let empty = HashMap::new();
        rename_in_blocks(child_children, &rename, &empty);
    }
}

// ---------------------------------------------------------------------------
// AST walking
// ---------------------------------------------------------------------------

fn walk_blocks(blocks: &[BlockNode], f: &mut impl FnMut(&BlockNode)) {
    for block in blocks {
        f(block);
        match block {
            BlockNode::BlockQuote(b) => walk_blocks(&b.children, f),
            BlockNode::Div(d) => walk_blocks(&d.children, f),
            BlockNode::Admonition(a) => walk_blocks(&a.children, f),
            BlockNode::List(l) => {
                for item in &l.items {
                    walk_blocks(&item.children, f);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for item in &dl.items {
                    for def in &item.definitions {
                        walk_blocks(&def.children, f);
                    }
                }
            }
            BlockNode::Figure(fig) => {
                if let FigureTarget::BlockQuote(b) = &fig.target {
                    walk_blocks(&b.children, f);
                }
            }
            _ => {}
        }
    }
}

fn walk_blocks_mut(blocks: &mut [BlockNode], f: &mut impl FnMut(&mut BlockNode)) {
    for block in blocks {
        f(block);
        match block {
            BlockNode::BlockQuote(b) => walk_blocks_mut(&mut b.children, f),
            BlockNode::Div(d) => walk_blocks_mut(&mut d.children, f),
            BlockNode::Admonition(a) => walk_blocks_mut(&mut a.children, f),
            BlockNode::List(l) => {
                for item in &mut l.items {
                    walk_blocks_mut(&mut item.children, f);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        walk_blocks_mut(&mut def.children, f);
                    }
                }
            }
            BlockNode::Figure(fig) => {
                if let FigureTarget::BlockQuote(b) = &mut fig.target {
                    walk_blocks_mut(&mut b.children, f);
                }
            }
            _ => {}
        }
    }
}

fn rename_inlines(
    nodes: &mut [InlineNode],
    footnotes: &HashMap<String, String>,
    headings: &HashMap<String, String>,
) {
    for node in nodes {
        match node {
            InlineNode::Footnote(f) => {
                if let Some(id) = &f.id {
                    if let Some(new) = footnotes.get(id) {
                        f.id = Some(new.clone());
                    }
                }
                if let Some(inline) = &mut f.inline {
                    rename_inlines(inline, footnotes, headings);
                }
            }
            // An UNRESOLVED cross-reference (a programmatically built AST, or
            // one the parser could not bind).
            InlineNode::CrossRef(c) => {
                if let Some(new) = headings.get(&c.target) {
                    c.target = new.clone();
                }
            }
            InlineNode::Emphasis(e) => rename_inlines(&mut e.children, footnotes, headings),
            InlineNode::Link(l) => {
                // Unlike carve-js, this engine binds `</#id>` during PARSE, so
                // by include time the child's own cross-reference is already a
                // Link carrying `#id`. The rename has to follow it there or a
                // renamed heading would keep an href pointing at the id the
                // parent kept. `from_crossref` is exactly the flag that marks
                // an auto-filled cross-reference, so an ordinary authored
                // `[text](#dup)` link is left alone.
                if l.from_crossref {
                    if let Some(old) = l.href.strip_prefix('#') {
                        if let Some(new) = headings.get(old) {
                            l.href = format!("#{new}");
                        }
                    }
                }
                rename_inlines(&mut l.children, footnotes, headings);
            }
            InlineNode::Span(s) => rename_inlines(&mut s.children, footnotes, headings),
            InlineNode::Extension(e) => rename_inlines(&mut e.children, footnotes, headings),
            InlineNode::CriticInsert(c) => rename_inlines(&mut c.children, footnotes, headings),
            InlineNode::CriticDelete(c) => rename_inlines(&mut c.children, footnotes, headings),
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    for part in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        rename_inlines(part, footnotes, headings);
                    }
                }
            }
            _ => {}
        }
    }
}

fn rename_in_blocks(
    blocks: &mut [BlockNode],
    footnotes: &HashMap<String, String>,
    headings: &HashMap<String, String>,
) {
    walk_blocks_mut(blocks, &mut |block| match block {
        BlockNode::Heading(h) => rename_inlines(&mut h.children, footnotes, headings),
        BlockNode::Paragraph(p) => rename_inlines(&mut p.children, footnotes, headings),
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                rename_inlines(caption, footnotes, headings);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    rename_inlines(&mut cell.children, footnotes, headings);
                }
            }
        }
        BlockNode::Figure(fig) => {
            rename_inlines(&mut fig.caption, footnotes, headings);
            match &mut fig.target {
                FigureTarget::Paragraph(p) => rename_inlines(&mut p.children, footnotes, headings),
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        rename_inlines(caption, footnotes, headings);
                    }
                }
                _ => {}
            }
        }
        _ => {}
    });
}

// ---------------------------------------------------------------------------
// Child expansion
// ---------------------------------------------------------------------------

struct ExpandedChild {
    children: Vec<BlockNode>,
    footnotes: BTreeMap<String, Vec<BlockNode>>,
    file: Option<String>,
}

fn expand_child(d: &Directive, state: &mut State<'_>) -> Option<ExpandedChild> {
    let (source, id) = resolve_child(d, state)?;
    // I4 fragment containment: the child is PARSED as a self-contained
    // document, never spliced as source. A construct still open at the end of
    // the child closes at child EOF and can never swallow parent content.
    let child = parse(&source);
    let mut children = child.children;
    let mut footnotes = child.footnote_defs;
    // Select BEFORE expanding: nested includes outside the wanted section must
    // not be resolved (no budget charge) and must not move section boundaries.
    if let Some(section) = &d.section {
        match select_section(&children, section) {
            Some(selected) => children = selected,
            None => {
                // Same attempt, not a second one: the file was read but the
                // include did not expand, so the entry is forced back to
                // unresolved rather than going through note()'s upgrade rule.
                state.note_forced_unresolved(&id);
                state.warn(
                    "include-section",
                    format!("Include \"{}\" has no section \"#{section}\".", d.path),
                );
                return None;
            }
        }
    }

    // Everything from here on operates on the child's own content, so warnings
    // it raises name the CHILD rather than the document that included it.
    let outer_file = state.file.replace(id.clone());
    rename_child_heading_ids(&mut children, &mut footnotes, state);

    let auto = d.shift == Shift::Auto;
    let stated = match d.shift {
        Shift::By(n) => n,
        Shift::Auto => 0,
    };
    state.stack.push(id.clone());
    state.depth += 1;
    state.footnotes.push(footnotes);
    // The child is shifted only AFTER its own includes are expanded, so inside
    // it the inherited context is expressed in pre-shift coordinates: a stated
    // shift is known now and translated out, and once it lands a nested `auto`
    // sits where the assembled document says it should.
    //
    // `auto` is NOT translated because its offset is not known yet - it is
    // measured over the assembled content below, which is exactly what makes
    // it self-consistent.
    let outer_context = state.context_level;
    state.context_level = outer_context - stated;
    expand_blocks(&mut children, state);
    let mut footnotes = state
        .footnotes
        .pop()
        .expect("footnote stack push/pop are balanced");
    // A footnote body is its own container: no heading precedes it.
    state.footnotes.push(BTreeMap::new());
    for body in footnotes.values_mut() {
        state.context_level = 0;
        expand_blocks(body, state);
    }
    let nested = state
        .footnotes
        .pop()
        .expect("footnote stack push/pop are balanced");
    for (label, body) in nested {
        footnotes.entry(label).or_insert(body);
    }
    state.context_level = outer_context;
    state.depth -= 1;
    state.stack.pop();
    // Measured after expansion so a child that only passes through to nested
    // includes is levelled by the headings those actually contributed.
    let shift = if auto {
        auto_shift(&children, state.context_level)
    } else {
        stated
    };
    shift_blocks(&mut children, shift, state);
    state.file = outer_file;
    Some(ExpandedChild {
        children,
        footnotes,
        file: Some(id),
    })
}

// ---------------------------------------------------------------------------
// Inline expansion (I2 inline form, I9a run recognition)
// ---------------------------------------------------------------------------

fn is_run_node(node: &InlineNode) -> bool {
    matches!(
        node,
        InlineNode::Text(_) | InlineNode::Mention(_) | InlineNode::Tag(_)
    )
}

/// Source form of a run node. A directive's own syntax overlaps constructs the
/// core already parses (`#section` is TAG syntax, `@key:value` is MENTION
/// syntax), so recognition reassembles the run before matching (I9a).
fn run_node_text(node: &InlineNode) -> String {
    match node {
        InlineNode::Text(t) => t.clone(),
        InlineNode::Mention(m) => format!("@{}", m.user),
        InlineNode::Tag(t) => format!("#{}", t.name),
        _ => String::new(),
    }
}

/// Return the run nodes covering `[from, to)` of the run's reassembled text.
/// Directive matches start with `{{` and end with `}}`, which the core always
/// parses as text, so a boundary can only fall inside a text node; mention and
/// tag nodes are either fully kept or fully consumed by a directive span.
fn slice_run(run: &[InlineNode], from: usize, to: usize) -> Vec<InlineNode> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for node in run {
        let text = run_node_text(node);
        let start = offset;
        let end = offset + text.len();
        offset = end;
        if end <= from || start >= to {
            continue;
        }
        let InlineNode::Text(value) = node else {
            out.push(node.clone());
            continue;
        };
        let lo = from.max(start) - start;
        let hi = to.min(end) - start;
        let slice = &value[lo..hi];
        if slice == value {
            out.push(node.clone());
        } else if !slice.is_empty() {
            out.push(InlineNode::Text(slice.to_string()));
        }
    }
    out
}

fn expand_run(run: &[InlineNode], state: &mut State<'_>) -> Vec<InlineNode> {
    let full: String = run.iter().map(run_node_text).collect();
    let mut spans: Vec<(usize, usize, Vec<InlineNode>)> = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = full[cursor..].find("{{") {
        let start = cursor + rel;
        let Some((end, raw)) = match_directive_at(&full, start) else {
            cursor = start + 2;
            continue;
        };
        cursor = end;
        let d = match parse_options(raw) {
            ParsedDirective::Ok(d) => d,
            ParsedDirective::BadOption(part) => {
                state.warn(
                    "include-unknown-option",
                    format!("Unknown include option \"{part}\"."),
                );
                continue;
            }
            ParsedDirective::NotADirective => continue,
        };
        let Some(expanded) = expand_child(&d, state) else {
            continue;
        };
        let mut children = expanded.children;
        // I2: resolved content in INLINE position must parse to inline-only
        // content - a single paragraph, or nothing.
        let inline_only = match children.len() {
            0 => true,
            1 => matches!(children[0], BlockNode::Paragraph(_)),
            _ => false,
        };
        if !inline_only {
            state.warn(
                "include-block-in-inline",
                format!("Inline include \"{}\" resolved to block content.", d.path),
            );
            continue;
        }
        // Merge BEFORE lifting the paragraph's inlines out: a footnote-label
        // rename rewrites the references inside `children`, so taking them
        // first would rename the definition while leaving the spliced
        // reference pointing at the label the parent kept.
        merge_footnotes(expanded.footnotes, &mut children, state, expanded.file);
        let replacement = match children.first_mut() {
            Some(BlockNode::Paragraph(p)) => std::mem::take(&mut p.children),
            _ => Vec::new(),
        };
        spans.push((start, end, replacement));
    }
    if spans.is_empty() {
        return run.to_vec();
    }
    let mut out = Vec::new();
    let mut at = 0usize;
    for (start, end, replacement) in spans {
        out.extend(slice_run(run, at, start));
        out.extend(replacement);
        at = end;
    }
    out.extend(slice_run(run, at, full.len()));
    out
}

fn expand_inlines(nodes: &mut Vec<InlineNode>, state: &mut State<'_>) {
    let mut out: Vec<InlineNode> = Vec::with_capacity(nodes.len());
    let mut i = 0usize;
    while i < nodes.len() {
        if is_run_node(&nodes[i]) {
            // A directive split across other inline structures (emphasis, a
            // link, a code span) stays literal by design: the run STOPS at any
            // node that is not literal-text-shaped (I9a), which is also what
            // gives code spans their verbatim protection (I9).
            let mut j = i;
            while j < nodes.len() && is_run_node(&nodes[j]) {
                j += 1;
            }
            out.extend(expand_run(&nodes[i..j], state));
            i = j;
            continue;
        }
        let mut node = nodes[i].clone();
        match &mut node {
            InlineNode::Emphasis(e) => expand_inlines(&mut e.children, state),
            InlineNode::Link(l) => expand_inlines(&mut l.children, state),
            InlineNode::Span(s) => expand_inlines(&mut s.children, state),
            InlineNode::Extension(e) => expand_inlines(&mut e.children, state),
            InlineNode::CriticInsert(c) => expand_inlines(&mut c.children, state),
            InlineNode::CriticDelete(c) => expand_inlines(&mut c.children, state),
            InlineNode::Footnote(f) => {
                if let Some(inline) = &mut f.inline {
                    expand_inlines(inline, state);
                }
            }
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    for part in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        expand_inlines(part, state);
                    }
                }
            }
            // Code, RawInline and Math are VERBATIM (I9): never traversed, so
            // a directive inside them is never handed to the resolver.
            _ => {}
        }
        out.push(node);
        i += 1;
    }
    *nodes = out;
}

// ---------------------------------------------------------------------------
// Block expansion (I2 block form)
// ---------------------------------------------------------------------------

/// Reassemble a paragraph's inlines into directive source, or `None` if the
/// paragraph holds anything that is not literal-text-shaped.
fn directive_source(nodes: &[InlineNode]) -> Option<String> {
    let mut out = String::new();
    for node in nodes {
        if !is_run_node(node) {
            return None;
        }
        out.push_str(&run_node_text(node));
    }
    Some(out)
}

fn expand_paragraph(block: &mut Paragraph, state: &mut State<'_>) -> Option<Vec<BlockNode>> {
    if let Some(source) = directive_source(&block.children) {
        let parsed = parse_full_directive(&source);
        match parsed {
            ParsedDirective::Ok(d) => {
                if let Some(expanded) = expand_child(&d, state) {
                    let mut children = expanded.children;
                    merge_footnotes(expanded.footnotes, &mut children, state, expanded.file);
                    return Some(children);
                }
                // I7: degrade to literal - the original inline nodes render
                // exactly as the core does with no resolver.
                return None;
            }
            ParsedDirective::BadOption(part) => {
                state.warn(
                    "include-unknown-option",
                    format!("Unknown include option \"{part}\"."),
                );
                return None;
            }
            ParsedDirective::NotADirective => {
                // A whole-paragraph directive that failed to parse was already
                // reported here; skip the inline scan so it is not warned twice.
                if is_directive_shaped(&source) {
                    // Recheck: a shaped-but-unparsable token may still carry a
                    // bad option worth reporting, which parse_full_directive
                    // only sees when the overall shape matched.
                    return None;
                }
            }
        }
    }
    expand_inlines(&mut block.children, state);
    None
}

fn expand_blocks(blocks: &mut Vec<BlockNode>, state: &mut State<'_>) {
    // Spec I8: this block list is ONE container. Headings in it set the context
    // for later blocks and for containers nested inside it, but the entry value
    // is restored on exit so a CLOSED sibling container never sets context.
    let entry_context = state.context_level;
    let mut i = 0usize;
    while i < blocks.len() {
        let mut replacement: Option<Vec<BlockNode>> = None;
        match &mut blocks[i] {
            BlockNode::Paragraph(p) => replacement = expand_paragraph(p, state),
            BlockNode::BlockQuote(b) => expand_blocks(&mut b.children, state),
            BlockNode::Div(d) => expand_blocks(&mut d.children, state),
            BlockNode::Admonition(a) => expand_blocks(&mut a.children, state),
            BlockNode::List(l) => {
                for item in &mut l.items {
                    expand_blocks(&mut item.children, state);
                }
            }
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        expand_blocks(&mut def.children, state);
                    }
                }
            }
            BlockNode::Figure(fig) => {
                match &mut fig.target {
                    FigureTarget::BlockQuote(b) => expand_blocks(&mut b.children, state),
                    FigureTarget::Paragraph(p) => expand_inlines(&mut p.children, state),
                    _ => {}
                }
                expand_inlines(&mut fig.caption, state);
            }
            BlockNode::Heading(h) => {
                expand_inlines(&mut h.children, state);
                state.context_level = h.level as i32;
            }
            BlockNode::Table(t) => {
                if let Some(caption) = &mut t.caption {
                    expand_inlines(caption, state);
                }
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        expand_inlines(&mut cell.children, state);
                    }
                }
            }
            // CodeBlock and RawBlock are VERBATIM (I9).
            _ => {}
        }
        if let Some(replacement) = replacement {
            let len = replacement.len();
            blocks.splice(i..i + 1, replacement);
            // The merged blocks are now part of THIS container, so a heading
            // they contribute at this level sets the context for what follows -
            // "the document as assembled" (I8).
            for merged in &blocks[i..i + len] {
                if let BlockNode::Heading(h) = merged {
                    state.context_level = h.level as i32;
                }
            }
            i += len;
        } else {
            i += 1;
        }
    }
    state.context_level = entry_context;
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Expand processor-level `{{ … }}` include directives in an already-parsed AST.
///
/// With no resolver configured, directives remain ordinary text and no warnings
/// are emitted - the pinned core behavior.
pub fn expand_includes(doc: Document, source: &str, options: &IncludeOptions<'_>) -> IncludeResult {
    let mut doc = doc;
    let mut state = State {
        opts: options,
        warnings: Vec::new(),
        max_depth: options.max_depth.unwrap_or(DEFAULT_MAX_DEPTH),
        max_bytes: options
            .max_bytes
            .unwrap_or_else(|| MIN_BUDGET.max(source.len().saturating_mul(8))),
        used_bytes: 0,
        stack: options.source_path.clone().into_iter().collect(),
        depth: 0,
        file: options.source_path.clone(),
        used_heading_ids: HashSet::new(),
        dependencies: Vec::new(),
        dep_index: HashMap::new(),
        footnotes: Vec::new(),
        context_level: 0,
    };
    // Recognition needs no extra parse, but a document whose source contains no
    // "{{" at all cannot hold a directive in any position, so the AST walk is
    // skipped outright. Directive-free documents stay at parse cost.
    if options.resolver.is_some() && source.contains("{{") {
        // Parent explicit ids are claimed FIRST (I5: parent before child), so
        // an included duplicate is the one renamed - even against a parent
        // heading that appears after the include site.
        walk_blocks(&doc.children, &mut |block| {
            if let BlockNode::Heading(h) = block {
                if let Some(id) = h.attrs.as_ref().and_then(|a| a.id.clone()) {
                    state.used_heading_ids.insert(id);
                }
            }
        });
        state.footnotes.push(std::mem::take(&mut doc.footnote_defs));
        expand_blocks(&mut doc.children, &mut state);
        let mut defs = state
            .footnotes
            .pop()
            .expect("footnote stack push/pop are balanced");
        // Each footnote body is its own container, with no preceding heading.
        state.footnotes.push(BTreeMap::new());
        for body in defs.values_mut() {
            state.context_level = 0;
            expand_blocks(body, &mut state);
        }
        let nested = state
            .footnotes
            .pop()
            .expect("footnote stack push/pop are balanced");
        for (label, body) in nested {
            defs.entry(label).or_insert(body);
        }
        doc.footnote_defs = defs;
    }
    IncludeResult {
        doc,
        warnings: state.warnings,
        dependencies: state.dependencies,
    }
}

// ---------------------------------------------------------------------------
// Filesystem resolver (spec I10)
// ---------------------------------------------------------------------------

/// Filesystem resolver with canonical root-containment checks, for TRUSTED
/// hosts (a CLI, a static-site build). Not for untrusted input.
///
/// Every candidate is canonicalized (symlinks resolved) and only then checked
/// against the canonical root. This is deliberately NOT a lexical ban on `..`,
/// which is wrong on both sides: TOO STRICT (a document in `chapters/`
/// including `../shared/glossary.crv` is a normal book layout whose canonical
/// target is inside the root) and TOO WEAK (a symlink inside the root pointing
/// out of it, or an absolute path, escapes with no `..` present at all).
pub struct FileSystemResolver {
    root_real: PathBuf,
    allow_absolute: bool,
}

impl FileSystemResolver {
    /// Canonicalizes `root` up front; fails if it does not exist.
    pub fn new(root: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            root_real: std::fs::canonicalize(root)?,
            allow_absolute: false,
        })
    }

    /// Allow absolute include paths. They are STILL subject to the same
    /// canonical containment check, so this only widens spelling, not reach.
    pub fn allow_absolute(mut self, allow: bool) -> Self {
        self.allow_absolute = allow;
        self
    }

    /// Containment test over CANONICAL paths.
    ///
    /// `Path::strip_prefix` compares whole components, so a sibling directory
    /// legitimately named `..foo` (or `rootother` next to `root`) is not
    /// misread as an escape the way a string-prefix test would be. Both sides
    /// are already canonical, so no `..` component survives to be re-walked.
    fn contains(&self, candidate: &Path) -> bool {
        candidate.strip_prefix(&self.root_real).is_ok()
    }
}

impl IncludeResolver for FileSystemResolver {
    fn resolve(&self, include_path: &str, ctx: &IncludeContext<'_>) -> Option<IncludeResolved> {
        let requested = Path::new(include_path);
        if !self.allow_absolute && requested.is_absolute() {
            return None;
        }
        // ONE ROOT PER EXPANSION (I10): relative paths resolve against the
        // INCLUDING file, but containment is checked against the single
        // top-level root. The root must NOT re-base per child, or a nested
        // document could never reach a sibling directory of the project.
        //
        // The stack carries the canonical path of each ancestor, so a nested
        // relative include resolves against its actual parent directory.
        let base = match ctx.stack.last() {
            Some(parent) => self
                .root_real
                .join(parent)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.root_real.clone()),
            None => self.root_real.clone(),
        };
        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            base.join(requested)
        };
        // CANONICALIZE-THEN-CONTAIN. `canonicalize` requires the path to
        // EXIST; a missing target therefore lands in the Err arm, which denies.
        // The failure path must never be able to report "contained" - every
        // error here returns None, so an unreadable or nonexistent target is
        // reported unresolved rather than being read through an unchecked path.
        let real = std::fs::canonicalize(&candidate).ok()?;
        if !self.contains(&real) {
            return None;
        }
        // Read through the CANONICAL path: it holds no symlink components, so
        // the check that just passed describes the bytes actually read. A
        // residual TOCTOU window remains if a directory component is swapped
        // between the two syscalls, which is why this resolver is for trusted
        // trees only.
        let source = std::fs::read_to_string(&real).ok()?;
        Some(IncludeResolved::with_id(
            source,
            real.to_string_lossy().into_owned(),
        ))
    }
}
