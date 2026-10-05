//! Processor-level file inclusion (`{{ path }}`), spec PART 9 §19 (I1-I11).
//!
//! The core parser is untouched and performs NO file I/O: it does not know the
//! directive exists and leaves `{{ … }}` as ordinary inline text. Expansion is
//! this separate, opt-in pass over an already-parsed [`Document`]. With no
//! resolver configured the directive stays literal, which is the behavior the
//! conformance corpus pins.
//!
//! ```
//! use carve::{expand_includes, parse, render_html, IncludeDenial, IncludeOptions, IncludeResolved};
//!
//! let source = "Before.\n\n{{ child.crv }}\n";
//! let doc = parse(source);
//! let resolver = |path: &str, _ctx: &carve::IncludeContext<'_>| {
//!     (path == "child.crv")
//!         .then(|| IncludeResolved::from("Included body."))
//!         .ok_or(IncludeDenial::NotFound)
//! };
//! let opts = IncludeOptions::new().with_resolver(&resolver);
//! let result = expand_includes(doc, source, &opts);
//! assert!(render_html(&result.doc).unwrap().contains("<p>Included body.</p>"));
//! ```

use std::collections::{BTreeMap, HashMap, HashSet};
// Only the filesystem resolver deals in paths; nothing else in the pass does.
#[cfg(feature = "fs")]
use std::path::{Path, PathBuf};

use crate::ast::{
    AttrSlot, Attrs, BlockNode, Document, FigureTarget, Heading, Image, InlineNode, Paragraph,
};
use crate::extension::CarveExtension;
use crate::include_walk::SubtreeVisitor;
use crate::parse::slugify_parse;
use crate::render::plain_inlines;

/// Default transitive include depth limit (spec I6 recommends at least 16).
pub const DEFAULT_MAX_DEPTH: usize = 16;

/// Resolver calls allowed for one expansion.
///
/// The byte budget bounds expanded OUTPUT; it does not bound the WORK done to
/// produce it, because a target is resolved before its size is known and a
/// document may carry one directive per dozen bytes. Section 19 requires a
/// finite bound here, and without one a megabyte of directives was tens of
/// thousands of file reads for a bounded expansion.
pub const DEFAULT_MAX_RESOLVER_CALLS: usize = 1000;

/// Include warnings retained for one expansion.
///
/// Warnings are per-directive, so a document of refused directives otherwise
/// allocates one per directive. One warning per distinct rule always survives
/// the cap, and the suppressed count says how many did not.
pub const DEFAULT_MAX_WARNINGS: usize = 100;

fn spent_message(rule: &str, path: &str) -> String {
    if rule == "include-call-limit" {
        format!("Include resolver call limit exceeded for \"{path}\".")
    } else {
        format!("Include byte budget exceeded by \"{path}\".")
    }
}
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

/// Why a resolver refused, in the classes PART 9 section 19 names.
///
/// A refusal is still an attempted dependency whatever its class: a host wants
/// to re-check any of them if the tree changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeDenial {
    /// The target resolves outside the configured root.
    OutsideRoot,
    /// Nothing is there to read.
    NotFound,
    /// No root is configured, so containment cannot be decided.
    NoRoot,
    /// A policy of the resolver's own refused it: an absolute path where those
    /// are not allowed, a file past a size cap, an unreadable target.
    Denied,
    /// Refused for a reason the resolver does not classify.
    Unresolved,
}

impl IncludeDenial {
    /// The portable class name (spec section 19).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OutsideRoot => "outside-root",
            Self::NotFound => "not-found",
            Self::NoRoot => "no-root",
            Self::Denied => "include-denied",
            Self::Unresolved => "include-unresolved",
        }
    }
}

/// Host-supplied path resolution (spec I3). The core never touches the
/// filesystem; a host that wants inclusion supplies one of these.
///
/// The error carries the class, which reaches the caller on the attempted
/// dependency ([`IncludeDependency::denial`]).
pub trait IncludeResolver {
    fn resolve(
        &self,
        path: &str,
        ctx: &IncludeContext<'_>,
    ) -> Result<IncludeResolved, IncludeDenial>;

    /// Where a refused target WOULD appear, when this resolver can say (spec
    /// I11). The id reaches the caller on the attempted dependency, so a host
    /// watches that path and rebuilds when the file arrives. `None` leaves the
    /// dependency spelled as the directive wrote it.
    fn unresolved_id(&self, _path: &str, _ctx: &IncludeContext<'_>) -> Option<String> {
        None
    }
}

impl<F> IncludeResolver for F
where
    F: Fn(&str, &IncludeContext<'_>) -> Result<IncludeResolved, IncludeDenial>,
{
    fn resolve(
        &self,
        path: &str,
        ctx: &IncludeContext<'_>,
    ) -> Result<IncludeResolved, IncludeDenial> {
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
    max_resolver_calls: Option<usize>,
    max_warnings: Option<usize>,
    extensions: Vec<&'a dyn CarveExtension>,
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

    /// Resolver calls allowed for one expansion. Defaults to
    /// [`DEFAULT_MAX_RESOLVER_CALLS`].
    ///
    /// This is the bound on the pass's own WORK, which the byte budget does not
    /// give: a target is resolved before its size is known.
    pub fn with_max_resolver_calls(mut self, calls: usize) -> Self {
        self.max_resolver_calls = Some(calls);
        self
    }

    /// Include warnings retained. Defaults to [`DEFAULT_MAX_WARNINGS`]. One per
    /// distinct rule always survives, and the result reports how many did not.
    pub fn with_max_warnings(mut self, warnings: usize) -> Self {
        self.max_warnings = Some(warnings);
        self
    }

    /// Parse each child with this extension too. Pass the set the PARENT was
    /// parsed with, so the same text means the same thing in either file.
    /// [`crate::prepare_doc_with_includes`] forwards its own parse options' set
    /// and does not read this one.
    pub fn with_extension(mut self, extension: &'a dyn CarveExtension) -> Self {
        self.extensions.push(extension);
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
    /// True when the target's SOURCE WAS READ - and nothing more (spec I11).
    ///
    /// It is NOT a report on whether the content was merged. A target that was
    /// read and then rejected (a missing `#section`, a cycle, an exhausted
    /// byte budget) stays `true`, because the file exists and a host must keep
    /// watching it: editing the child to fix the problem has to invalidate the
    /// preview. Only a target whose source was never obtained - unresolvable,
    /// denied by containment, or refused before any read at the depth limit -
    /// is `false`.
    pub resolved: bool,
    /// Why the resolver refused, when it refused and said so. `None` for a
    /// dependency that resolved, and for a refusal the engine made itself.
    pub denial: Option<IncludeDenial>,
}

/// Outcome of [`expand_includes`].
#[derive(Debug, Clone)]
pub struct IncludeResult {
    pub doc: Document,
    pub warnings: Vec<IncludeWarning>,
    /// Warnings raised but not retained once the cap was reached. Zero on every
    /// uncapped run; non-zero means `warnings` is a sample, not the whole
    /// report, so a capped one is never mistaken for a clean one.
    pub suppressed_warnings: usize,
    /// Every include target touched during the whole recursive expansion,
    /// de-duplicated, in first-encounter order. Empty without a resolver.
    pub dependencies: Vec<IncludeDependency>,
    /// Bytes CHARGED against the byte budget: the size of every target the
    /// resolver handed back, whether or not expansion went on to admit it.
    ///
    /// Published because the budget's own arithmetic is otherwise unobservable
    /// from outside, and the conformance corpus pins it. PART 9 section 19 is
    /// explicit that the budget bounds the expanded OUTPUT and not the WORK -
    /// a target is resolved before its size is known - so a counter of bytes
    /// ADMITTED could not tell "read nothing" from "read a file and refused
    /// it", which is the one thing a reader of this number wants to know.
    pub charged_bytes: usize,
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

/// PART 6 `include_section` takes an `explicit_identifier`, which may also open
/// on a digit (`#2024-plan`).
fn is_explicit_ident_start(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
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
/// `{{`, whitespace, a bare / straight- or curly-quoted path, an optional
/// `#section`, an option tail, at least one whitespace, `}}`. THE CLOSER is
/// the first such `}}` that falls OUTSIDE any quoted run (PART 6): a
/// `quoted_include_path` or an option's `quoted_value` may carry the pair.
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

    // A quoted form can spell an empty or whitespace-only path (`{{ "" }}`,
    // `{{ "   " }}`); the maintainer ruled such a token is NOT a directive at
    // all. Rejecting it HERE is what converges the serializer and the expander
    // on one recognizer: the spec requires the serializer's preserved set and
    // the expander's recognized set to be identical, and every recognition
    // path in this module funnels through `match_directive_at`. Bare paths
    // cannot be empty or hold whitespace, so this only ever fires on the
    // quoted forms.
    if path.trim().is_empty() {
        return None;
    }

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
                    if is_explicit_ident_start(first) {
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

    // Option tail closed by the first `\s+}}` OUTSIDE any quoted run. A
    // `quoted_value` (PART 4) excludes only its own quote, the backslash and
    // the newline, so a `}}` between the quotes belongs to the run; an
    // UNTERMINATED quote opens no run, and the first `}}` wins again.
    //
    // One forward pass, never a rewind: the first candidate passed while a run
    // is PROVISIONALLY open is remembered, so an unterminated run already has
    // its fallback in hand. Matching the quote to its closer first and then
    // searching for `}}` is what would rescan the run once per opener.
    let tail = &text[i..];
    let tail_bytes = tail.as_bytes();
    // A `}}` whose preceding character is whitespace, i.e. `whitespace+, "}}"`.
    let is_closer = |j: usize| {
        tail_bytes[j] == b'}'
            && tail_bytes.get(j + 1) == Some(&b'}')
            && tail[..j]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_whitespace())
    };
    let mut j = 0usize;
    let mut open_quote: Option<u8> = None;
    let mut fallback: Option<usize> = None;
    let hit = loop {
        if j >= tail_bytes.len() {
            break fallback?;
        }
        let c = tail_bytes[j];
        let Some(quote) = open_quote else {
            if c == b'"' || c == b'\'' {
                open_quote = Some(c);
            } else if is_closer(j) {
                break j;
            }
            j += 1;
            continue;
        };
        if c == b'\\' {
            // `escaped_char`: the next character cannot close the run.
            j += 2;
        } else if c == quote {
            // The run closed, so every candidate inside it belonged to it.
            open_quote = None;
            fallback = None;
            j += 1;
        } else if c == b'\n' {
            // A quoted value stops at the newline, so no run ever opened.
            if let Some(f) = fallback {
                break f;
            }
            open_quote = None;
        } else {
            if fallback.is_none() && is_closer(j) {
                fallback = Some(j);
            }
            j += 1;
        }
    };

    let before = &tail[..hit];
    Some((
        i + hit + 2,
        RawDirective {
            path,
            section,
            options: before
                .trim_end_matches(|c: char| c.is_whitespace())
                .to_string(),
        },
    ))
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

/// Byte range of the first SHAPE-well-formed directive at or after `from`.
///
/// "Shape-well-formed" is deliberately WIDER than what expansion accepts: the
/// run opens `{{`, closes `}}`, and carries a non-empty path token - exactly
/// what [`match_directive_at`] recognizes. Section existence and option
/// validity are NOT required here; they are expansion-time DIAGNOSTICS
/// (`include-section`, `include-unknown-option`), and preservation must not
/// depend on them.
///
/// Gating preservation on full validity was the earlier behavior and it was
/// actively harmful: `{{ a.crv @bogus:1 }}` is a one-character typo, but
/// escaping it to `\{\{ … \}\}` froze the typo into permanent literal text and
/// destroyed the very warning that would have told the author what to fix. A
/// malformed directive must stay a malformed DIRECTIVE, not become prose.
///
/// A run that is not shape-well-formed - `{{ oops` with no close, or `{{ }}`
/// / `{{ "" }}` with an empty or whitespace-only path - is ordinary text and
/// is escaped as before. The empty-path exclusion lives in
/// [`match_directive_at`] so the serializer and the expander share exactly one
/// recognizer; this scanner needs no path check of its own.
///
/// This exists for the Carve serializer (spec I1): a directive must survive
/// `carve fmt` verbatim, because escaping it to `\{\{ … \}\}` renders the same
/// today but silently deletes the include the moment a resolver is wired up.
/// A serializer cannot tell an authored literal `{{` from a directive - both
/// parse to the same text - so a literal that happens to be directive-shaped
/// loses its escaping. That is accepted: per I9 an author who needs a
/// guaranteed literal puts it in a code span or fence, where the directive is
/// inert by construction and this scanner is never consulted.
fn find_directive(text: &str, from: usize) -> Option<(usize, usize)> {
    let mut i = from;
    while let Some(offset) = text[i..].find("{{") {
        let start = i + offset;
        if let Some((end, _raw)) = match_directive_at(text, start) {
            return Some((start, end));
        }
        // `{{` is ASCII, so this stays on a char boundary.
        i = start + 2;
    }
    None
}

/// One piece of a run split at well-formed directive boundaries.
pub(crate) enum RunPiece {
    /// Original nodes, to be rendered normally.
    Nodes(Vec<InlineNode>),
    /// Verbatim directive source, to be emitted unescaped.
    Directive(String),
}

/// Split a maximal run of literal-text-shaped nodes at every well-formed
/// directive, or `None` when it holds none.
///
/// Recognition MUST happen at run level, exactly as [`expand_run`] does it: a
/// directive's own syntax overlaps constructs the core already parses, so
/// `{{ book.crv #intro }}` arrives as Text + Tag + Text and `@shift:2` as a
/// Mention (I9a). Scanning individual text nodes would miss precisely the
/// sectioned and optioned forms.
pub(crate) fn split_run_directives(run: &[InlineNode]) -> Option<Vec<RunPiece>> {
    let full: String = run.iter().map(run_node_text).collect();
    if !full.contains("{{") {
        return None;
    }
    let slicer = RunSlicer::new(run);
    let mut pieces = Vec::new();
    let mut at = 0usize;
    let mut cursor = 0usize;
    while let Some((start, end)) = find_directive(&full, cursor) {
        if start > at {
            pieces.push(RunPiece::Nodes(slicer.slice(at, start)));
        }
        pieces.push(RunPiece::Directive(full[start..end].to_string()));
        at = end;
        cursor = end;
    }
    if pieces.is_empty() {
        return None;
    }
    if at < full.len() {
        pieces.push(RunPiece::Nodes(slicer.slice(at, full.len())));
    }
    Some(pieces)
}

/// The id the renderer derives from a heading's own text.
fn heading_slug(children: &[InlineNode]) -> String {
    slugify_parse(
        &plain_inlines(children),
        crate::extension::HeadingIdOptions::default(),
    )
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
    /// Extensions each child is parsed with.
    extensions: &'a [&'a dyn CarveExtension],
    warnings: Vec<IncludeWarning>,
    max_depth: usize,
    max_bytes: usize,
    used_bytes: usize,
    max_resolver_calls: usize,
    resolver_calls: usize,
    max_warnings: usize,
    suppressed_warnings: usize,
    /// Rules already represented in `warnings`, so a cap never hides a class.
    seen_rules: std::collections::BTreeSet<String>,
    /// Whether the PARENT document carries source positions, and therefore
    /// whether each child is parsed with them.
    ///
    /// A child parsed without them contributes nodes with no position at all,
    /// and the writer orders collected definitions BY position - so merged
    /// footnote definitions from two files tied and fell back to the label.
    /// It also silently drops the source mapping section 19 asks for, since
    /// there is no `pos` to stamp a file onto.
    track_positions: bool,
    /// Which whole-expansion total is spent, if either.
    ///
    /// Both only ever grow, so once one is spent no later directive can
    /// succeed. Latching stops the pass resolving the rest of the document
    /// only to refuse each directive individually - the refusal is already
    /// decided, and resolving to reach it is what turns a megabyte of
    /// directives into tens of thousands of file reads (spec S6).
    spent: Option<&'static str>,
    stack: Vec<String>,
    depth: usize,
    /// Identity of the document whose content is currently being expanded.
    file: Option<String>,
    /// Every element id claimed so far (I5), mapped to the file inclusion that
    /// claimed it first (0 is the root document, each child inclusion takes the
    /// next number) and the number of elements carrying it. A duplicate is
    /// renamed only against ANOTHER inclusion's claim; the count lets an
    /// element that never reaches the output give its id back without freeing
    /// one a surviving element still carries.
    used_ids: HashMap<String, (usize, usize)>,
    /// The last inclusion number handed out.
    inclusions: usize,
    /// Explicit-id renames waiting for their final `-N`, in the order they were
    /// made. The suffix has to skip every id in the ASSEMBLED document,
    /// including ones later includes have not contributed yet, so the merge
    /// writes a placeholder and [`finish_renames`] picks the name at the end.
    pending_renames: Vec<PendingRename>,
    /// The inclusion whose content is being expanded, 0 for the root.
    owner: usize,
    /// Open reservation frames, one per include being expanded (spec I7).
    ///
    /// Every identifier claimed while a child is being processed is journalled
    /// into the innermost frame. The frame is committed only when the child's
    /// content actually merges, and released otherwise, so a REJECTED
    /// directive leaves the document byte-identical to one that had the
    /// directive written as literal text from the start. Frames nest: an outer
    /// rejection releases everything its successful descendants claimed.
    reservation_frames: Vec<Vec<String>>,
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
        // A rule not yet represented is always kept, so a capped report still
        // shows every distinct failure class; only repeats of a class already
        // shown are counted instead of stored.
        if self.warnings.len() >= self.max_warnings && self.seen_rules.contains(rule) {
            self.suppressed_warnings += 1;
            return;
        }
        self.seen_rules.insert(rule.to_string());
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
        self.note_denied(id, resolved, None);
    }

    fn note_denied(&mut self, id: &str, resolved: bool, denial: Option<IncludeDenial>) {
        match self.dep_index.get(id) {
            Some(&idx) => {
                if resolved {
                    self.dependencies[idx].resolved = true;
                    self.dependencies[idx].denial = None;
                } else if self.dependencies[idx].denial.is_none() {
                    self.dependencies[idx].denial = denial;
                }
            }
            None => {
                self.dep_index
                    .insert(id.to_string(), self.dependencies.len());
                self.dependencies.push(IncludeDependency {
                    id: id.to_string(),
                    resolved,
                    denial,
                });
            }
        }
    }

    /// Claim an element id for inclusion `owner`, journalling it into the
    /// innermost open reservation frame. Returns `true` when the id was free.
    ///
    /// Claims made outside any frame - the root document's own ids - are
    /// permanent and never journalled.
    fn reserve_id(&mut self, id: &str, owner: usize) -> bool {
        let entry = self.used_ids.entry(id.to_string()).or_insert((owner, 0));
        entry.1 += 1;
        if let Some(frame) = self.reservation_frames.last_mut() {
            frame.push(id.to_string());
        }
        entry.1 == 1
    }

    /// The inclusion that claimed `id` first, if any did.
    fn id_owner(&self, id: &str) -> Option<usize> {
        self.used_ids.get(id).map(|&(owner, _)| owner)
    }

    /// Rename one occurrence of `id` to a placeholder that [`finish_renames`]
    /// replaces, claim it for `owner` and warn.
    fn defer_rename(&mut self, id: &str, owner: usize) -> String {
        let placeholder = format!("{RENAME_MARK}{}{RENAME_MARK}", self.pending_renames.len());
        self.reserve_id(&placeholder, owner);
        let before = self.warnings.len();
        self.warn(
            "include-heading-id-rename",
            format!("Id \"{id}\" was renamed to \"{placeholder}\"."),
        );
        self.pending_renames.push(PendingRename {
            placeholder: placeholder.clone(),
            base: id.to_string(),
            warning: (self.warnings.len() > before).then_some(before),
        });
        placeholder
    }

    /// Give back one element's claim on `id`, for an element that never
    /// reaches the output.
    fn release_id(&mut self, id: &str) {
        if let Some(entry) = self.used_ids.get_mut(id) {
            entry.1 = entry.1.saturating_sub(1);
            if entry.1 == 0 {
                self.used_ids.remove(id);
            }
        }
    }

    fn open_reservations(&mut self) {
        self.reservation_frames.push(Vec::new());
    }

    /// Keep the frame's claims by folding them into the enclosing frame, so an
    /// outer rejection can still release them.
    fn commit_reservations(&mut self) {
        let frame = self
            .reservation_frames
            .pop()
            .expect("reservation frames are balanced");
        if let Some(outer) = self.reservation_frames.last_mut() {
            outer.extend(frame);
        }
    }

    /// Release every identifier the frame claimed: the child's content is not
    /// being merged, so nothing it named may influence the parent (I7).
    fn rollback_reservations(&mut self) {
        let frame = self
            .reservation_frames
            .pop()
            .expect("reservation frames are balanced");
        for id in frame {
            self.release_id(&id);
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

struct ResolvedChild {
    source: String,
    id: String,
    position_base: Option<(String, usize)>,
}

fn resolve_child(d: &Directive, state: &mut State<'_>) -> Option<ResolvedChild> {
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

    // A whole-expansion total is already spent, so this directive cannot expand
    // whatever it resolves to. Refuse it WITHOUT resolving: the target is never
    // read, and it is reported unresolved because it genuinely was not.
    if let Some(rule) = state.spent {
        state.note(&d.path, false);
        state.warn(rule, spent_message(rule, &d.path));
        return None;
    }
    if state.resolver_calls >= state.max_resolver_calls {
        state.spent = Some("include-call-limit");
        state.note(&d.path, false);
        state.warn(
            "include-call-limit",
            spent_message("include-call-limit", &d.path),
        );
        return None;
    }
    state.resolver_calls += 1;

    let ctx = IncludeContext {
        source_path: state.opts.source_path.as_deref(),
        stack: &state.stack,
        depth: state.depth,
    };
    let resolved = match resolver.resolve(&d.path, &ctx) {
        Ok(resolved) => resolved,
        Err(denial) => {
            // I11: a resolver that can say where the target would be reports
            // that path; anything else keeps the directive's spelling.
            let id = resolver
                .unresolved_id(&d.path, &ctx)
                .unwrap_or_else(|| d.path.clone());
            state.note_denied(&id, false, Some(denial));
            state.warn(
                "include-unresolved",
                format!("Include \"{}\" could not be resolved.", d.path),
            );
            return None;
        }
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
    // CHARGED BEFORE THE COMPARISON. By this line the resolver has already run
    // and `source` is in hand, so the read has happened whatever the budget
    // says. PART 9 section 19 names and accepts that: the budget bounds the
    // expanded OUTPUT, not the WORK, "because a target is resolved before its
    // size is known". Charging only what expansion ADMITS would make the
    // counter unable to tell "read nothing" from "read a file and refused it".
    // Refusing before reading is ruled out by the same clause; what bounds the
    // I/O is the separate resolver-call bound.
    state.used_bytes += source.len();
    if state.used_bytes > state.max_bytes {
        state.spent = Some("include-budget");
        state.warn("include-budget", spent_message("include-budget", &d.path));
        return None;
    }
    let (selected, position_base) = match d.lines {
        Some(range) => (
            slice_lines(&source, range),
            Some((source, range.0.saturating_sub(1))),
        ),
        None => (source, None),
    };
    Some(ResolvedChild {
        source: selected,
        id,
        position_base,
    })
}

fn heading_id(h: &Heading) -> String {
    h.attrs
        .as_ref()
        .and_then(|a| a.id.clone())
        .unwrap_or_else(|| {
            slugify_parse(
                &plain_inlines(&h.children),
                crate::extension::HeadingIdOptions::default(),
            )
        })
}

/// What `#name` selects from a child's own parse (I1a), or `None`.
///
/// Step 1: the first heading, at any depth, whose id matches: it and the blocks
/// after it in the same container, up to the next same-or-higher-level heading.
/// Step 2: otherwise the first other block carrying the explicit id. Document
/// order, a container before the blocks inside it. Footnote bodies are never
/// searched; they are not in `children`.
fn select_fragment(children: &[BlockNode], section: &str) -> Option<Vec<BlockNode>> {
    let heading = first_block(
        children,
        &|b| matches!(b, BlockNode::Heading(h) if heading_id(h) == section),
    );
    if let Some((seq, start)) = heading {
        let BlockNode::Heading(head) = &seq[start] else {
            return None;
        };
        let level = head.level;
        let end = seq[start + 1..]
            .iter()
            .position(|b| matches!(b, BlockNode::Heading(h) if h.level <= level))
            .map_or(seq.len(), |at| start + 1 + at);
        return Some(seq[start..end].to_vec());
    }
    let (seq, at) = first_block(children, &|b| {
        selectable_block_attrs(b)
            .and_then(|a| a.id.as_deref())
            .is_some_and(|id| id == section)
    })?;
    Some(vec![seq[at].clone()])
}

/// The first block in document order that passes `test`, as its sequence and
/// index.
fn first_block<'a>(
    seq: &'a [BlockNode],
    test: &impl Fn(&BlockNode) -> bool,
) -> Option<(&'a [BlockNode], usize)> {
    for (i, block) in seq.iter().enumerate() {
        if test(block) {
            return Some((seq, i));
        }
        for inner in block_sequences(block) {
            if let Some(hit) = first_block(inner, test) {
                return Some(hit);
            }
        }
    }
    None
}

/// The block sequences directly inside `block`: container bodies, list items,
/// definitions and block table cells. A line block holds lines, not blocks.
fn block_sequences(block: &BlockNode) -> Vec<&[BlockNode]> {
    fn cells(t: &crate::ast::Table) -> Vec<&[BlockNode]> {
        t.rows
            .iter()
            .flat_map(|r| &r.cells)
            .filter_map(|c| c.blocks.as_deref())
            .collect()
    }
    match block {
        BlockNode::BlockQuote(b) => vec![&b.children],
        BlockNode::Admonition(a) => vec![&a.children],
        BlockNode::Directive(d) => vec![&d.children],
        BlockNode::Div(d) => vec![&d.children],
        BlockNode::Section(d) => vec![&d.children],
        BlockNode::FigureGroup(g) => vec![&g.children],
        BlockNode::ExtensionCarrier(e) => vec![&e.children],
        BlockNode::BlockExtension(e) => vec![e.fallback_slice()],
        BlockNode::List(l) => l.items.iter().map(|i| i.children.as_slice()).collect(),
        BlockNode::DefinitionList(d) => d
            .items
            .iter()
            .flat_map(|i| &i.definitions)
            .map(|def| def.children.as_slice())
            .collect(),
        BlockNode::Table(t) => cells(t),
        BlockNode::Figure(f) => match &*f.target {
            FigureTarget::BlockQuote(b) => vec![&b.children],
            FigureTarget::Table(t) => cells(t),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The attributes of a block step 2 may select. Definitions that render
/// nothing (reference, abbreviation and citation definitions, comments) and
/// headings (step 1's business) are never selected by a block id.
fn selectable_block_attrs(block: &BlockNode) -> Option<&Attrs> {
    match block {
        BlockNode::Paragraph(b) => b.attrs.as_ref(),
        BlockNode::CodeBlock(b) => b.attrs.as_ref(),
        BlockNode::List(b) => b.attrs.as_ref(),
        BlockNode::BlockQuote(b) => b.attrs.as_ref(),
        BlockNode::Table(b) => b.attrs.as_ref(),
        BlockNode::Admonition(b) => b.attrs.as_ref(),
        BlockNode::Directive(b) => b.attrs.as_ref(),
        BlockNode::Div(b) => b.attrs.as_ref(),
        BlockNode::Section(b) => b.attrs.as_ref(),
        BlockNode::LineBlock(b) => b.attrs.as_ref(),
        BlockNode::DefinitionList(b) => b.attrs.as_ref(),
        BlockNode::Figure(b) => b.attrs.as_ref(),
        BlockNode::FigureGroup(b) => b.attrs.as_ref(),
        BlockNode::BlockExtension(b) => b.attrs.as_ref(),
        BlockNode::ExtensionCarrier(b) => b.attrs.as_ref(),
        BlockNode::BlockImage(b) => b.attrs.as_ref(),
        BlockNode::ThematicBreak(b) => b.attrs.as_ref(),
        BlockNode::Heading(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::LinkReferenceDefinition(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_) => None,
    }
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

/// Brackets a rename placeholder. A private-use character cannot come out of an
/// `{#id}` the author wrote, so a placeholder never meets a real id.
const RENAME_MARK: char = '\u{E000}';

struct PendingRename {
    placeholder: String,
    base: String,
    /// Index of the warning that reports it, unless the warning cap dropped it.
    warning: Option<usize>,
}

/// Give every deferred rename its final name (I5): the least `-N`, N >= 2, that
/// no id anywhere in the assembled document uses and no earlier rename took.
/// A renamed id that never reached the output (its include was rejected, or an
/// inline include spliced its paragraph away) renamed nothing: its warning is
/// dropped and references to it point at the original id again.
fn finish_renames(doc: &mut Document, state: &mut State<'_>) {
    if state.pending_renames.is_empty() {
        return;
    }
    let mut present: HashSet<String> = HashSet::new();
    let mut gather = |attrs: &mut Option<Attrs>, _: Option<&[InlineNode]>| {
        if let Some(id) = attrs.as_ref().and_then(|a| a.id.as_deref()) {
            present.insert(id.to_string());
        }
    };
    for_each_id_site(&mut doc.children, &mut gather);
    for body in doc.footnote_defs.values_mut() {
        for_each_id_site(body, &mut gather);
    }
    let mut taken: HashSet<String> = present
        .iter()
        .filter(|id| !id.contains(RENAME_MARK))
        .cloned()
        .collect();
    let mut cursor: HashMap<&str, u32> = HashMap::new();
    let mut rename: HashMap<String, String> = HashMap::new();
    let mut unsaid: Vec<usize> = Vec::new();
    let pending = std::mem::take(&mut state.pending_renames);
    for p in &pending {
        if !present.contains(&p.placeholder) {
            unsaid.extend(p.warning);
            rename.insert(p.placeholder.clone(), p.base.clone());
            continue;
        }
        // The claim it collided with did not survive (a block include
        // directive's own paragraph, for one): nothing to rename around.
        if !taken.contains(&p.base) {
            unsaid.extend(p.warning);
            taken.insert(p.base.clone());
            rename.insert(p.placeholder.clone(), p.base.clone());
            continue;
        }
        let from = cursor.get(p.base.as_str()).copied().unwrap_or(2);
        let (name, n) = next_free_from(&p.base, from, |c| taken.contains(c));
        cursor.insert(&p.base, n + 1);
        taken.insert(name.clone());
        if let Some(w) = p.warning.and_then(|i| state.warnings.get_mut(i)) {
            w.message = w.message.replace(&p.placeholder, &name);
        }
        rename.insert(p.placeholder.clone(), name);
    }
    for i in unsaid.into_iter().rev() {
        state.warnings.remove(i);
    }
    let mut apply = |attrs: &mut Option<Attrs>, _: Option<&[InlineNode]>| {
        if let Some(a) = attrs.as_mut() {
            if let Some(new) = a.id.as_ref().and_then(|id| rename.get(id)) {
                a.id = Some(new.clone());
            }
        }
    };
    for_each_id_site(&mut doc.children, &mut apply);
    for body in doc.footnote_defs.values_mut() {
        for_each_id_site(body, &mut apply);
    }
    let mut follow = FollowRename { rename: &rename };
    follow.blocks(&mut doc.children);
    for body in doc.footnote_defs.values_mut() {
        follow.blocks(body);
    }
}

/// Merge-time collision pass for element ids (spec I5), run on one child
/// inclusion before its own includes expand.
///
/// An explicit id on any element is renamed when another inclusion (the parent,
/// an earlier include, an earlier inclusion of the same file) already claimed
/// it; every further copy of that id in this file is renamed too, each to its
/// own name. A duplicate the file holds on its own is left alone. Names compare
/// exactly, as HTML ids do. The final `-N` is picked by [`finish_renames`]. An
/// automatic heading id is re-derived from its slug against the assembled
/// document, silently (rule 4). References in this inclusion to a renamed id
/// follow its first renamed copy.
fn rename_child_ids(
    children: &mut [BlockNode],
    footnote_bodies: &mut BTreeMap<String, Vec<BlockNode>>,
    state: &mut State<'_>,
) -> usize {
    state.inclusions += 1;
    let me = state.inclusions;
    // An automatic id never lands on a name this file writes further down.
    let own = crate::document_ids::authored_ids(
        std::iter::once(&*children).chain(footnote_bodies.values().map(Vec::as_slice)),
    );
    // The last suffix handed out per slug, so a run of equal headings does not
    // restart its search at -2 each time.
    let mut cursor: HashMap<String, u32> = HashMap::new();
    let mut rename: HashMap<String, String> = HashMap::new();
    let mut renamed_explicit: HashSet<String> = HashSet::new();
    let mut visit = |attrs: &mut Option<Attrs>, heading: Option<&[InlineNode]>| {
        let Some(a) = attrs.as_mut() else { return };
        let Some(id) = a.id.clone() else { return };
        // A generated heading id carries no `order` slot; an authored one does.
        let generated = heading.filter(|_| !a.order.contains(&AttrSlot::Id));
        let new = if let Some(text) = generated {
            // The child's own suffix (`Overview-2` from its second `# Overview`)
            // is dropped, so the assembled document numbers it afresh.
            let base = heading_slug(text);
            let taken = |c: &str| state.used_ids.contains_key(c) || own.contains(c);
            let assigned = if taken(&base) {
                let from = cursor.get(&base).copied().unwrap_or(2);
                let (name, n) = next_free_from(&base, from, taken);
                cursor.insert(base, n + 1);
                name
            } else {
                base
            };
            state.reserve_id(&assigned, me);
            assigned
        } else if renamed_explicit.contains(&id) {
            state.defer_rename(&id, me)
        } else {
            match state.id_owner(&id) {
                Some(owner) if owner != me => {
                    renamed_explicit.insert(id.clone());
                    state.defer_rename(&id, me)
                }
                _ => {
                    state.reserve_id(&id, me);
                    id.clone()
                }
            }
        };
        if new != id {
            a.id = Some(new.clone());
            rename.entry(id).or_insert(new);
        }
    };
    for_each_id_site(children, &mut visit);
    for body in footnote_bodies.values_mut() {
        for_each_id_site(body, &mut visit);
    }
    if !rename.is_empty() {
        let mut follow = FollowRename { rename: &rename };
        for block in children.iter_mut() {
            follow.block(block);
        }
        for body in footnote_bodies.values_mut() {
            follow.blocks(body);
        }
    }
    me
}

/// Rewrites this inclusion's references to a renamed id: a `</#id>`
/// cross-reference and a link or image destination that is exactly `#id`,
/// inline or through the file's own reference definition (I5).
struct FollowRename<'a> {
    rename: &'a HashMap<String, String>,
}

impl FollowRename<'_> {
    /// The shared walker treats a citation entry as a leaf; its text holds
    /// links all the same.
    fn block(&mut self, block: &mut BlockNode) {
        if let BlockNode::CitationDefinition(d) = block {
            self.inlines(&mut d.children);
        } else {
            crate::include_walk::visit_block_children(block, self);
        }
    }

    /// Whether `href` was rewritten.
    fn follow(&self, href: &mut String) -> bool {
        let Some(new) = href.strip_prefix('#').and_then(|id| self.rename.get(id)) else {
            return false;
        };
        *href = format!("#{new}");
        true
    }
}

impl SubtreeVisitor for FollowRename<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks.iter_mut() {
            self.block(block);
        }
    }

    fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
        for node in inlines.iter_mut() {
            match node {
                InlineNode::Link(l) => {
                    // The writer spells a reference link from its label, which
                    // still names the old destination: write it inline instead.
                    if self.follow(&mut l.href) {
                        l.ref_label = None;
                        l.raw_ref = None;
                    }
                }
                // A parsed `</#id>` that resolved is already a Link. One that
                // did not resolve in its own file is literal text, left as is.
                InlineNode::CrossRef(c) => {
                    if let Some(href) = c.href.as_mut() {
                        self.follow(href);
                        if let Some(new) = self.rename.get(&c.target) {
                            c.target = new.clone();
                        }
                    }
                }
                _ => {}
            }
            crate::include_walk::visit_inline_children(node, self);
        }
    }

    fn image(&mut self, image: &mut Image) {
        if self.follow(&mut image.src) {
            image.ref_label = None;
            image.raw_ref = None;
        }
    }
}

/// Call `f` on every attribute slot that can carry an element id, in document
/// order, with the heading's text when the slot is a heading's. Mirrors the
/// renderer's id seeding (`document_ids`).
fn for_each_id_site(
    blocks: &mut [BlockNode],
    f: &mut impl FnMut(&mut Option<Attrs>, Option<&[InlineNode]>),
) {
    for block in blocks {
        match block {
            BlockNode::Heading(h) => {
                f(&mut h.attrs, Some(&h.children));
                id_sites_in_inlines(&mut h.children, f);
            }
            BlockNode::Paragraph(p) => {
                f(&mut p.attrs, None);
                id_sites_in_inlines(&mut p.children, f);
            }
            BlockNode::CitationDefinition(d) => {
                f(&mut d.attrs, None);
                id_sites_in_inlines(&mut d.children, f);
            }
            BlockNode::CodeBlock(c) => f(&mut c.attrs, None),
            BlockNode::List(l) => {
                f(&mut l.attrs, None);
                for item in &mut l.items {
                    f(&mut item.attrs, None);
                    for_each_id_site(&mut item.children, f);
                }
            }
            BlockNode::BlockQuote(b) => {
                f(&mut b.attrs, None);
                for_each_id_site(&mut b.children, f);
            }
            BlockNode::Table(t) => id_sites_in_table(t, f),
            BlockNode::Admonition(a) => {
                f(&mut a.attrs, None);
                if let Some(title) = &mut a.title {
                    id_sites_in_inlines(title, f);
                }
                for_each_id_site(&mut a.children, f);
            }
            BlockNode::Directive(d) => {
                f(&mut d.attrs, None);
                if let Some(title) = &mut d.title {
                    id_sites_in_inlines(title, f);
                }
                for_each_id_site(&mut d.children, f);
            }
            BlockNode::Div(d) => {
                f(&mut d.attrs, None);
                for_each_id_site(&mut d.children, f);
            }
            BlockNode::Section(d) => {
                f(&mut d.attrs, None);
                for_each_id_site(&mut d.children, f);
            }
            BlockNode::LineBlock(b) => {
                f(&mut b.attrs, None);
                for_each_id_site(&mut b.children, f);
            }
            BlockNode::DefinitionList(d) => {
                f(&mut d.attrs, None);
                for item in &mut d.items {
                    for term in &mut item.terms {
                        f(&mut term.attrs, None);
                        id_sites_in_inlines(&mut term.children, f);
                    }
                    for def in &mut item.definitions {
                        for_each_id_site(&mut def.children, f);
                    }
                }
            }
            BlockNode::Figure(fig) => {
                f(&mut fig.attrs, None);
                match &mut *fig.target {
                    FigureTarget::Image(i) => f(&mut i.attrs, None),
                    FigureTarget::CodeBlock(c) => f(&mut c.attrs, None),
                    FigureTarget::BlockQuote(b) => {
                        f(&mut b.attrs, None);
                        for_each_id_site(&mut b.children, f);
                    }
                    FigureTarget::Table(t) => id_sites_in_table(t, f),
                    FigureTarget::Paragraph(p) => {
                        f(&mut p.attrs, None);
                        id_sites_in_inlines(&mut p.children, f);
                    }
                }
                id_sites_in_inlines(&mut fig.caption, f);
            }
            BlockNode::FigureGroup(g) => {
                f(&mut g.attrs, None);
                for_each_id_site(&mut g.children, f);
                if let Some(caption) = &mut g.caption {
                    id_sites_in_inlines(caption, f);
                }
            }
            BlockNode::BlockExtension(e) => {
                f(&mut e.attrs, None);
                for_each_id_site(e.fallback_slice_mut(), f);
            }
            BlockNode::ExtensionCarrier(e) => {
                f(&mut e.attrs, None);
                for_each_id_site(&mut e.children, f);
            }
            BlockNode::BlockImage(i) => f(&mut i.attrs, None),
            BlockNode::ThematicBreak(t) => f(&mut t.attrs, None),
            BlockNode::LinkReferenceDefinition(_)
            | BlockNode::AbbreviationDef(_)
            | BlockNode::RawBlock(_)
            | BlockNode::Comment(_) => {}
        }
    }
}

fn id_sites_in_table(
    t: &mut crate::ast::Table,
    f: &mut impl FnMut(&mut Option<Attrs>, Option<&[InlineNode]>),
) {
    f(&mut t.attrs, None);
    if let Some(caption) = &mut t.caption {
        id_sites_in_inlines(caption, f);
    }
    for row in &mut t.rows {
        f(&mut row.attrs, None);
        for cell in &mut row.cells {
            f(&mut cell.attrs, None);
            id_sites_in_inlines(&mut cell.children, f);
            if let Some(blocks) = &mut cell.blocks {
                for_each_id_site(blocks, f);
            }
        }
    }
}

fn id_sites_in_inlines(
    nodes: &mut [InlineNode],
    f: &mut impl FnMut(&mut Option<Attrs>, Option<&[InlineNode]>),
) {
    for node in nodes {
        match node {
            InlineNode::Emphasis(e) => {
                f(&mut e.attrs, None);
                id_sites_in_inlines(&mut e.children, f);
            }
            InlineNode::Code(c) => f(&mut c.attrs, None),
            InlineNode::LiteralInline(l) => f(&mut l.attrs, None),
            InlineNode::Link(l) => {
                f(&mut l.attrs, None);
                id_sites_in_inlines(&mut l.children, f);
            }
            InlineNode::Image(i) => f(&mut i.attrs, None),
            InlineNode::Span(s) => {
                f(&mut s.attrs, None);
                id_sites_in_inlines(&mut s.children, f);
            }
            InlineNode::Ruby(r) => {
                f(&mut r.attrs, None);
                for pair in &mut r.pairs {
                    id_sites_in_inlines(&mut pair.base, f);
                    id_sites_in_inlines(&mut pair.annotation, f);
                }
            }
            InlineNode::Math(m) => f(&mut m.attrs, None),
            InlineNode::AutoLink(a) => f(&mut a.attrs, None),
            InlineNode::Extension(e) => {
                f(&mut e.attrs, None);
                id_sites_in_inlines(&mut e.children, f);
            }
            InlineNode::Footnote(n) => {
                f(&mut n.attrs, None);
                if let Some(inline) = &mut n.inline {
                    id_sites_in_inlines(inline, f);
                }
            }
            InlineNode::CriticInsert(c) => id_sites_in_inlines(&mut c.children, f),
            InlineNode::CriticDelete(c) => id_sites_in_inlines(&mut c.children, f),
            InlineNode::CriticSubstitute(c) => {
                id_sites_in_inlines(&mut c.old, f);
                id_sites_in_inlines(&mut c.new, f);
            }
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    for inlines in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        id_sites_in_inlines(inlines, f);
                    }
                }
            }
            _ => {}
        }
    }
}

/// The least `base-N` with `N >= from` that `taken` refuses, and its `N`.
fn next_free_from(base: &str, from: u32, taken: impl Fn(&str) -> bool) -> (String, u32) {
    let mut n = from;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return (candidate, n);
        }
        n += 1;
    }
}

fn next_free(base: &str, taken: impl Fn(&str) -> bool) -> String {
    next_free_from(base, 2, taken).0
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
    let mut inserted: Vec<String> = Vec::new();
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
            inserted.push(final_label.clone());
            target.insert(final_label, body);
        }
    }
    if !rename.is_empty() {
        rename_in_blocks(child_children, &rename);
        if let Some(target) = state.footnotes.last_mut() {
            for label in inserted {
                if let Some(body) = target.get_mut(&label) {
                    rename_in_blocks(body, &rename);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AST walking
// ---------------------------------------------------------------------------

pub(crate) fn walk_blocks(blocks: &[BlockNode], f: &mut impl FnMut(&BlockNode)) {
    for block in blocks {
        f(block);
        match block {
            BlockNode::BlockQuote(b) => walk_blocks(&b.children, f),
            BlockNode::Directive(d) => walk_blocks(&d.children, f),
            BlockNode::Div(d) => walk_blocks(&d.children, f),
            BlockNode::Section(d) => walk_blocks(&d.children, f),
            BlockNode::Admonition(a) => walk_blocks(&a.children, f),
            BlockNode::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        if let Some(blocks) = &cell.blocks {
                            walk_blocks(blocks, f);
                        }
                    }
                }
            }
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
            BlockNode::Figure(fig) => match &*fig.target {
                FigureTarget::BlockQuote(b) => walk_blocks(&b.children, f),
                FigureTarget::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            if let Some(blocks) = &cell.blocks {
                                walk_blocks(blocks, f);
                            }
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn walk_blocks_mut(blocks: &mut [BlockNode], f: &mut impl FnMut(&mut BlockNode)) {
    for block in blocks {
        f(block);
        match block {
            BlockNode::BlockQuote(b) => walk_blocks_mut(&mut b.children, f),
            BlockNode::Directive(d) => walk_blocks_mut(&mut d.children, f),
            BlockNode::Div(d) => walk_blocks_mut(&mut d.children, f),
            BlockNode::Section(d) => walk_blocks_mut(&mut d.children, f),
            BlockNode::Admonition(a) => walk_blocks_mut(&mut a.children, f),
            BlockNode::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        if let Some(blocks) = &mut cell.blocks {
                            walk_blocks_mut(blocks, f);
                        }
                    }
                }
            }
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
            BlockNode::Figure(fig) => match &mut *fig.target {
                FigureTarget::BlockQuote(b) => walk_blocks_mut(&mut b.children, f),
                FigureTarget::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            if let Some(blocks) = &mut cell.blocks {
                                walk_blocks_mut(blocks, f);
                            }
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn rename_inlines(nodes: &mut [InlineNode], footnotes: &HashMap<String, String>) {
    for node in nodes {
        match node {
            InlineNode::Footnote(f) => {
                if let Some(id) = &f.id {
                    if let Some(new) = footnotes.get(id) {
                        f.id = Some(new.clone());
                    }
                }
                if let Some(inline) = &mut f.inline {
                    rename_inlines(inline, footnotes);
                }
            }
            InlineNode::Emphasis(e) => rename_inlines(&mut e.children, footnotes),
            InlineNode::Link(l) => rename_inlines(&mut l.children, footnotes),
            InlineNode::Span(s) => rename_inlines(&mut s.children, footnotes),
            InlineNode::Ruby(r) => {
                for pair in &mut r.pairs {
                    rename_inlines(&mut pair.base, footnotes);
                    rename_inlines(&mut pair.annotation, footnotes);
                }
            }
            InlineNode::Extension(e) => rename_inlines(&mut e.children, footnotes),
            InlineNode::CriticInsert(c) => rename_inlines(&mut c.children, footnotes),
            InlineNode::CriticDelete(c) => rename_inlines(&mut c.children, footnotes),
            InlineNode::CriticSubstitute(c) => {
                rename_inlines(&mut c.old, footnotes);
                rename_inlines(&mut c.new, footnotes);
            }
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    for part in [&mut item.prefix, &mut item.locator, &mut item.suffix]
                        .into_iter()
                        .flatten()
                    {
                        rename_inlines(part, footnotes);
                    }
                }
            }
            _ => {}
        }
    }
}

fn rename_in_blocks(blocks: &mut [BlockNode], footnotes: &HashMap<String, String>) {
    walk_blocks_mut(blocks, &mut |block| match block {
        BlockNode::Heading(h) => rename_inlines(&mut h.children, footnotes),
        BlockNode::Paragraph(p) => rename_inlines(&mut p.children, footnotes),
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.caption {
                rename_inlines(caption, footnotes);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    rename_inlines(&mut cell.children, footnotes);
                }
            }
        }
        BlockNode::Figure(fig) => {
            rename_inlines(&mut fig.caption, footnotes);
            match &mut *fig.target {
                FigureTarget::Paragraph(p) => rename_inlines(&mut p.children, footnotes),
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.caption {
                        rename_inlines(caption, footnotes);
                    }
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            rename_inlines(&mut cell.children, footnotes);
                        }
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

/// Resolve one directive and hand the result to `merge`, which decides whether
/// the content can actually land at the directive's position.
///
/// This is the single entry point for expanding a directive, and it enforces
/// spec I7 structurally rather than case by case: a reservation frame is open
/// for the whole of resolution AND merging, and is released unless `merge`
/// returns `Some`. Any rejection - unresolvable, binary, both selections,
/// cycle, depth, size, or content that cannot merge here - therefore leaves no
/// identifier claimed, so the document is byte-identical to one where the
/// directive was literal text from the start. Side effects added later are
/// covered by construction as long as they are journalled into the frame.
fn with_child<T>(
    d: &Directive,
    state: &mut State<'_>,
    inline: bool,
    merge: impl FnOnce(ExpandedChild, &mut State<'_>) -> Option<T>,
) -> Option<T> {
    state.open_reservations();
    let merged = expand_child(d, state, inline).and_then(|child| merge(child, state));
    if merged.is_some() {
        state.commit_reservations();
    } else {
        state.rollback_reservations();
    }
    merged
}

/// Record which file a position is measured in, for every node a resolved child
/// contributed (spec section 19, source mapping).
///
/// Runs AFTER the child's own includes are expanded and only where no identity
/// is set yet, so a grandchild keeps the file IT came from rather than being
/// overwritten by the file that pulled its parent in.
///
/// Without it an included span is ambiguous: a child's first paragraph and the
/// parent's first paragraph both report line 1, and a source-mapped host has no
/// way to tell them apart.
/// Whether this document was parsed with position tracking on, read off the
/// tree rather than passed in: the caller hands over a parsed document, not the
/// options it was parsed with.
fn doc_carries_positions(doc: &Document) -> bool {
    let mut found = false;
    walk_blocks(&doc.children, &mut |block| {
        if !found && crate::ast_json::block_pos(block).is_some() {
            found = true;
        }
    });
    found
}

fn stamp_source_file(blocks: &mut [BlockNode], file: &str) {
    // One identity for the whole child, handed to every node in it.
    let file = crate::ast::SourceFile::new(file);
    let mut stamp = Stamp { file: &file };
    for block in blocks.iter_mut() {
        crate::include_walk::visit_block_children(block, &mut stamp);
    }
}

/// Restore positions parsed from an `@lines` slice to the coordinates of the
/// complete source. This runs before nested includes are expanded, so a parent
/// slice base cannot be applied to a grandchild.
fn shift_source_positions(
    blocks: &mut [BlockNode],
    complete_source: &str,
    slice: &str,
    line_base: usize,
) {
    let mut shift = ShiftPositions {
        complete_source,
        slice,
        line_base,
    };
    for block in blocks.iter_mut() {
        crate::include_walk::visit_block_children(block, &mut shift);
    }
}

struct ShiftPositions<'a> {
    complete_source: &'a str,
    slice: &'a str,
    line_base: usize,
}

impl crate::include_walk::SubtreeVisitor for ShiftPositions<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks.iter_mut() {
            crate::include_walk::visit_block_children(block, self);
        }
    }

    fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
        for node in inlines.iter_mut() {
            crate::include_walk::visit_inline_children(node, self);
        }
    }

    fn position(&mut self, pos: &mut crate::ast::Pos) {
        pos.start_line += self.line_base;
        pos.end_line += self.line_base;
        pos.start_offset = source_offset(
            self.complete_source,
            self.slice,
            pos.start_line,
            pos.start_offset,
            self.line_base,
        );
        pos.end_offset = source_offset(
            self.complete_source,
            self.slice,
            pos.end_line,
            pos.end_offset,
            self.line_base,
        );
    }
}

fn source_offset(
    complete_source: &str,
    slice: &str,
    complete_line: usize,
    offset: usize,
    line_base: usize,
) -> usize {
    let slice_line = complete_line.saturating_sub(line_base);
    line_start_offset(complete_source, complete_line) + offset
        - line_start_offset(slice, slice_line)
}

/// Codepoint offset of the given 1-based physical line in source.
fn line_start_offset(source: &str, line: usize) -> usize {
    if line <= 1 {
        return 0;
    }

    let mut offset = 0;
    let mut current_line = 1;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        offset += 1;
        if ch == '\r' {
            if chars.next_if_eq(&'\n').is_some() {
                offset += 1;
            }
        } else if ch != '\n' {
            continue;
        }
        current_line += 1;
        if current_line == line {
            return offset;
        }
    }

    offset
}

struct Stamp<'a> {
    file: &'a crate::ast::SourceFile,
}

impl crate::include_walk::SubtreeVisitor for Stamp<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks.iter_mut() {
            crate::include_walk::visit_block_children(block, self);
        }
    }

    fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
        for node in inlines.iter_mut() {
            crate::include_walk::visit_inline_children(node, self);
        }
    }

    fn position(&mut self, pos: &mut crate::ast::Pos) {
        if pos.file.is_none() {
            pos.file = Some(self.file.clone());
        }
    }
}

fn expand_child(d: &Directive, state: &mut State<'_>, inline: bool) -> Option<ExpandedChild> {
    let ResolvedChild {
        source,
        id,
        position_base,
    } = resolve_child(d, state)?;
    // I4 fragment containment: the child is PARSED as a self-contained
    // document, never spliced as source. A construct still open at the end of
    // the child closes at child EOF and can never swallow parent content.
    let mut child_options = crate::Options::default().with_positions(state.track_positions);
    for extension in state.extensions {
        child_options = child_options.with_extension(*extension);
    }
    let mut child = crate::parse_with_options(&source, &child_options);
    let mut children = std::mem::take(&mut child.children);
    let mut footnotes = std::mem::take(&mut child.footnote_defs);
    if let Some((complete_source, line_base)) = position_base {
        shift_source_positions(&mut children, &complete_source, &source, line_base);
        for body in footnotes.values_mut() {
            shift_source_positions(body, &complete_source, &source, line_base);
        }
    }
    // Select BEFORE expanding: nested includes outside the wanted section must
    // not be resolved (no budget charge) and must not move section boundaries.
    if let Some(section) = &d.section {
        match select_fragment(&children, section) {
            Some(selected) => children = selected,
            None => {
                // I11: `resolved` reports whether the target's SOURCE WAS READ,
                // nothing more. The read succeeded here - only the section
                // selection failed - so the entry stays resolved and the host
                // keeps watching the file. Downgrading it would stop the watch
                // at exactly the wrong moment: adding the missing section to
                // the child would then never invalidate the preview.
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
    // An inline include splices a lone paragraph's inlines and leaves its
    // attributes behind, so its id must not claim a name it never takes.
    if inline {
        if let [BlockNode::Paragraph(p)] = children.as_mut_slice() {
            p.attrs = None;
        }
    }
    let me = rename_child_ids(&mut children, &mut footnotes, state);
    let outer_owner = std::mem::replace(&mut state.owner, me);

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
    state.footnotes.push(footnotes);
    let labels: Vec<String> = state.footnotes.last().unwrap().keys().cloned().collect();
    for label in labels {
        let mut body = state.footnotes.last_mut().unwrap().remove(&label).unwrap();
        state.context_level = 0;
        expand_blocks(&mut body, state);
        state.footnotes.last_mut().unwrap().insert(label, body);
    }
    footnotes = state
        .footnotes
        .pop()
        .expect("footnote stack push/pop are balanced");
    state.context_level = outer_context;
    state.depth -= 1;
    state.stack.pop();
    state.owner = outer_owner;
    // A paragraph a nested include contributed is spliced the same way.
    let dropped = match children.as_mut_slice() {
        [BlockNode::Paragraph(p)] if inline => p.attrs.take().and_then(|a| a.id),
        _ => None,
    };
    if let Some(id) = dropped {
        state.release_id(&id);
    }
    // Measured after expansion so a child that only passes through to nested
    // includes is levelled by the headings those actually contributed.
    let shift = if auto {
        auto_shift(&children, state.context_level)
    } else {
        stated
    };
    shift_blocks(&mut children, shift, state);
    // After the child's own includes, so a grandchild keeps its own identity.
    stamp_source_file(&mut children, &id);
    // THE FOOTNOTE BODIES TOO. They travel beside the blocks rather than in
    // them, so stamping only `children` left every merged definition with no
    // file identity - and the writer orders collected definitions by source
    // position, which is meaningless across files without one. Two children
    // each defining a note near their own start then tied, and the order fell
    // back to the label.
    for body in footnotes.values_mut() {
        stamp_source_file(body, &id);
    }
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

pub(crate) fn is_run_node(node: &InlineNode) -> bool {
    matches!(
        node,
        InlineNode::Text(_)
            | InlineNode::Mention(_)
            | InlineNode::Tag(_)
            | InlineNode::SmartPunctuation(_)
            | InlineNode::EscapedText(_)
    )
}

/// Source form of a run node. A directive's own syntax overlaps constructs the
/// core already parses (`#section` is TAG syntax, `@key:value` is MENTION
/// syntax), so recognition reassembles the run before matching (I9a).
fn run_node_text(node: &InlineNode) -> String {
    match node {
        InlineNode::Text(t) => t.value.clone(),
        InlineNode::Mention(m) => format!("@{}", m.user),
        InlineNode::Tag(t) => format!("#{}", t.name),
        // A QUOTED PATH is two more of those constructs: its delimiters arrive
        // as SmartPunctuation whose `value` is the quote the author typed
        // rather than the “ smart typography resolved it to, and an escaped
        // quote inside the path arrives as EscapedText without its backslash.
        // Reassembling from the author's spelling is what the grammar matches.
        InlineNode::SmartPunctuation(p) => p.value.clone(),
        InlineNode::EscapedText(t) => format!("\\{}", t.value),
        _ => String::new(),
    }
}

/// The byte length `run_node_text` gives a node, without building the string.
fn run_node_len(node: &InlineNode) -> usize {
    match node {
        InlineNode::Text(t) => t.value.len(),
        InlineNode::Mention(m) => 1 + m.user.len(),
        InlineNode::Tag(t) => 1 + t.name.len(),
        InlineNode::SmartPunctuation(p) => p.value.len(),
        InlineNode::EscapedText(t) => 1 + t.value.len(),
        _ => 0,
    }
}

/// A run's nodes addressed by offsets into its reassembled text.
///
/// Built ONCE per run. Slicing used to re-measure every node from the start
/// and clone each text node whole for every directive, so a paragraph that is
/// one long text node holding many directives cost the square of its length
/// (markup-carve/carve-rs#1618).
struct RunSlicer<'a> {
    run: &'a [InlineNode],
    ends: Vec<usize>,
}

impl<'a> RunSlicer<'a> {
    fn new(run: &'a [InlineNode]) -> Self {
        let mut end = 0usize;
        let ends = run
            .iter()
            .map(|node| {
                end += run_node_len(node);
                end
            })
            .collect();
        Self { run, ends }
    }

    /// The run nodes covering `[from, to)` of the reassembled text. Directive
    /// matches start with `{{` and end with `}}`, which the core always parses
    /// as text, so a boundary can only fall inside a text node; mention and tag
    /// nodes are either fully kept or fully consumed by a directive span.
    fn slice(&self, from: usize, to: usize) -> Vec<InlineNode> {
        let mut out = Vec::new();
        let first = self.ends.partition_point(|end| *end <= from);
        for (i, node) in self.run.iter().enumerate().skip(first) {
            let end = self.ends[i];
            let start = end - run_node_len(node);
            if start >= to {
                break;
            }
            let InlineNode::Text(value) = node else {
                out.push(node.clone());
                continue;
            };
            let lo = from.max(start) - start;
            let hi = to.min(end) - start;
            if lo == 0 && hi == value.value.len() {
                out.push(node.clone());
            } else if lo < hi {
                out.push(InlineNode::Text(crate::ast::Text {
                    value: value.value[lo..hi].to_string(),
                    pos: value.pos.clone(),
                }));
            }
        }
        out
    }
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
        let Some(replacement) = with_child(&d, state, true, |expanded, state| {
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
                return None;
            }
            // Merge BEFORE lifting the paragraph's inlines out: a footnote-label
            // rename rewrites the references inside `children`, so taking them
            // first would rename the definition while leaving the spliced
            // reference pointing at the label the parent kept.
            merge_footnotes(expanded.footnotes, &mut children, state, expanded.file);
            Some(match children.first_mut() {
                Some(BlockNode::Paragraph(p)) => std::mem::take(&mut p.children),
                _ => Vec::new(),
            })
        }) else {
            continue;
        };
        spans.push((start, end, replacement));
    }
    if spans.is_empty() {
        return run.to_vec();
    }
    let slicer = RunSlicer::new(run);
    let mut pieces: Vec<(InlineNode, bool)> = Vec::new();
    let mut at = 0usize;
    for (start, end, replacement) in spans {
        pieces.extend(slicer.slice(at, start).into_iter().map(|n| (n, true)));
        pieces.extend(replacement.into_iter().map(|n| (n, false)));
        at = end;
    }
    pieces.extend(slicer.slice(at, full.len()).into_iter().map(|n| (n, true)));
    coalesce_text(pieces)
}

/// PART 12 §1a: a serialized node's children hold no two adjacent `text` nodes,
/// and §1a's own POSITIONS paragraph says the coalescing happens in the parsed
/// tree rather than in the encoder. Splicing an inline include's content into
/// the host's run is the one place this pass leaves a run split, so it joins it
/// back here (CARVE-P12-002, markup-carve/carve-rs#1647).
///
/// The merged run spans the FILE THAT HOLDS IT, from the first host piece's
/// start to the last host piece's end. A run with no positioned host piece
/// publishes none: an included piece's coordinates are measured in another
/// file and would select the wrong text here.
fn coalesce_text(pieces: Vec<(InlineNode, bool)>) -> Vec<InlineNode> {
    let mut out: Vec<InlineNode> = Vec::with_capacity(pieces.len());
    let mut run: Vec<(InlineNode, bool)> = Vec::new();

    fn flush(run: &mut Vec<(InlineNode, bool)>, out: &mut Vec<InlineNode>) {
        if run.len() < 2 {
            out.extend(run.drain(..).map(|(node, _)| node));
            return;
        }
        let mut value = String::new();
        let mut first: Option<crate::Pos> = None;
        let mut last: Option<crate::Pos> = None;
        for (node, from_host) in run.drain(..) {
            let InlineNode::Text(text) = node else {
                continue;
            };
            value.push_str(&text.value);
            if !from_host {
                continue;
            }
            if let Some(pos) = text.pos {
                first.get_or_insert_with(|| pos.clone());
                last = Some(pos);
            }
        }
        let pos = match (first, last) {
            (Some(first), Some(last)) => Some(crate::Pos {
                end_line: last.end_line,
                end_column: last.end_column,
                end_offset: last.end_offset,
                ..first
            }),
            _ => None,
        };
        out.push(InlineNode::Text(crate::ast::Text { value, pos }));
    }

    for (node, from_host) in pieces {
        if matches!(node, InlineNode::Text(_)) {
            run.push((node, from_host));
            continue;
        }
        flush(&mut run, &mut out);
        out.push(node);
    }
    flush(&mut run, &mut out);
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
            InlineNode::Ruby(r) => {
                for pair in &mut r.pairs {
                    expand_inlines(&mut pair.base, state);
                    expand_inlines(&mut pair.annotation, state);
                }
            }
            InlineNode::Extension(e) => expand_inlines(&mut e.children, state),
            InlineNode::CriticInsert(c) => expand_inlines(&mut c.children, state),
            InlineNode::CriticDelete(c) => expand_inlines(&mut c.children, state),
            InlineNode::CriticSubstitute(c) => {
                expand_inlines(&mut c.old, state);
                expand_inlines(&mut c.new, state);
            }
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
                // I7: on any rejection this yields None and the original inline
                // nodes stay, rendering exactly as the core does with no
                // resolver - and `with_child` releases what the child claimed.
                // The paragraph is replaced by what it includes, so its id is
                // given back while the child claims; left literal, it keeps it.
                let held = block
                    .attrs
                    .as_ref()
                    .and_then(|a| a.id.clone())
                    .filter(|id| state.id_owner(id) == Some(state.owner));
                if let Some(id) = &held {
                    state.release_id(id);
                }
                let merged = with_child(&d, state, false, |expanded, state| {
                    let mut children = expanded.children;
                    merge_footnotes(expanded.footnotes, &mut children, state, expanded.file);
                    Some(children)
                });
                if let (None, Some(id)) = (&merged, &held) {
                    let owner = state.owner;
                    state.reserve_id(id, owner);
                }
                return merged;
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
            BlockNode::Directive(d) => expand_blocks(&mut d.children, state),
            BlockNode::Div(d) => expand_blocks(&mut d.children, state),
            BlockNode::Section(d) => expand_blocks(&mut d.children, state),
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
                match &mut *fig.target {
                    FigureTarget::BlockQuote(b) => expand_blocks(&mut b.children, state),
                    FigureTarget::Paragraph(p) => expand_inlines(&mut p.children, state),
                    FigureTarget::Table(t) => {
                        if let Some(caption) = &mut t.caption {
                            expand_inlines(caption, state);
                        }
                        for row in &mut t.rows {
                            for cell in &mut row.cells {
                                expand_inlines(&mut cell.children, state);
                                if let Some(blocks) = &mut cell.blocks {
                                    expand_blocks(blocks, state);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                expand_inlines(&mut fig.caption, state);
            }
            BlockNode::Heading(h) => {
                // An AUTO heading id is the slug of the heading's text, and the
                // parser derived it BEFORE this pass replaces that text. Note
                // whether the id still matches, so one an include invalidates
                // can be re-derived - `# {{ title.crv }}` kept `title-crv`
                // while its heading read "My Title" (I2). An EXPLICIT id does
                // not match its own text and is left alone.
                let auto_before = h
                    .attrs
                    .as_ref()
                    .and_then(|a| a.id.clone())
                    .filter(|id| *id == heading_slug(&h.children));
                expand_inlines(&mut h.children, state);
                if let Some(stale) = auto_before {
                    let derived = heading_slug(&h.children);
                    if derived != stale {
                        if let Some(attrs) = h.attrs.as_mut() {
                            attrs.id = Some(derived);
                        }
                    }
                }
                state.context_level = h.level as i32;
            }
            BlockNode::Table(t) => {
                if let Some(caption) = &mut t.caption {
                    expand_inlines(caption, state);
                }
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        expand_inlines(&mut cell.children, state);
                        if let Some(blocks) = &mut cell.blocks {
                            expand_blocks(blocks, state);
                        }
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
    expand_includes_with_extensions(doc, source, options, &options.extensions)
}

/// [`expand_includes`], parsing each child with `extensions` rather than the
/// set on `options`.
pub(crate) fn expand_includes_with_extensions(
    doc: Document,
    source: &str,
    options: &IncludeOptions<'_>,
    extensions: &[&dyn CarveExtension],
) -> IncludeResult {
    let mut doc = doc;
    let mut state = State {
        opts: options,
        extensions,
        warnings: Vec::new(),
        max_depth: options.max_depth.unwrap_or(DEFAULT_MAX_DEPTH),
        max_bytes: options
            .max_bytes
            .unwrap_or_else(|| MIN_BUDGET.max(source.len().saturating_mul(8))),
        used_bytes: 0,
        max_resolver_calls: options
            .max_resolver_calls
            .unwrap_or(DEFAULT_MAX_RESOLVER_CALLS),
        resolver_calls: 0,
        max_warnings: options.max_warnings.unwrap_or(DEFAULT_MAX_WARNINGS),
        suppressed_warnings: 0,
        seen_rules: std::collections::BTreeSet::new(),
        track_positions: doc_carries_positions(&doc),
        spent: None,
        stack: options.source_path.clone().into_iter().collect(),
        depth: 0,
        file: options.source_path.clone(),
        used_ids: HashMap::new(),
        inclusions: 0,
        pending_renames: Vec::new(),
        owner: 0,
        reservation_frames: Vec::new(),
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
        let mut claim = |attrs: &mut Option<Attrs>, _: Option<&[InlineNode]>| {
            if let Some(id) = attrs.as_ref().and_then(|a| a.id.as_deref()) {
                state.reserve_id(id, 0);
            }
        };
        for_each_id_site(&mut doc.children, &mut claim);
        for body in doc.footnote_defs.values_mut() {
            for_each_id_site(body, &mut claim);
        }
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
        finish_renames(&mut doc, &mut state);
    }
    IncludeResult {
        doc,
        warnings: state.warnings,
        suppressed_warnings: state.suppressed_warnings,
        dependencies: state.dependencies,
        charged_bytes: state.used_bytes,
    }
}

/// Whether the include pass runs for a given render target (spec I15).
///
/// Only the Carve target opts out, and the reason is not performance: that
/// target writes the document back as Carve SOURCE, and expanding first returns
/// a different document, with every child inlined and the directives gone. The
/// writer already preserves a directive verbatim (I12); expanding before it runs
/// takes that away by another route.
///
/// Lives here rather than in the CLI so the rule has ONE home, read by the CLI
/// and by the include-conformance suite alike. carve-js kept it inline in its
/// CLI and inlined every child on `render --carve` as a result.
pub fn expands_for_target(target: crate::RenderTarget) -> bool {
    !matches!(target, crate::RenderTarget::Carve)
}

// ---------------------------------------------------------------------------
// Filesystem resolver (spec I10)
// ---------------------------------------------------------------------------

/// Default per-target read cap for [`FileSystemResolver`]: 4 MiB.
#[cfg(feature = "fs")]
pub const DEFAULT_MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Filesystem resolver with canonical root-containment checks, for TRUSTED
/// hosts (a CLI, a static-site build). Not for untrusted input.
///
/// Every candidate is canonicalized (symlinks resolved) and only then checked
/// against the canonical root. This is deliberately NOT a lexical ban on `..`,
/// which is wrong on both sides: TOO STRICT (a document in `chapters/`
/// including `../shared/glossary.crv` is a normal book layout whose canonical
/// target is inside the root) and TOO WEAK (a symlink inside the root pointing
/// out of it, or an absolute path, escapes with no `..` present at all).
#[cfg(feature = "fs")]
pub struct FileSystemResolver {
    root_real: PathBuf,
    allow_absolute: bool,
    max_file_bytes: Option<u64>,
}

#[cfg(feature = "fs")]
impl FileSystemResolver {
    /// Canonicalizes `root` up front; fails if it does not exist, and refuses
    /// a spec that is not ABSOLUTE.
    ///
    /// A relative spec names no root: every canonicalizer resolves one against
    /// the process working directory, the value §19 forbids the root defaulting
    /// to. The test has to be on the CONFIGURED value - the canonicalized
    /// result is always absolute. A front end keeps the convenience by
    /// absolutizing its own flag before constructing this.
    pub fn new(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = root.as_ref();
        if !root.is_absolute() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "include containment root must be an absolute path, got {}",
                    root.display()
                ),
            ));
        }
        Ok(Self {
            root_real: std::fs::canonicalize(root)?,
            allow_absolute: false,
            max_file_bytes: Some(DEFAULT_MAX_FILE_BYTES),
        })
    }

    /// Largest target this resolver will read. `None` removes the cap.
    pub fn with_max_file_bytes(mut self, bytes: Option<u64>) -> Self {
        self.max_file_bytes = bytes;
        self
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

    /// Where `include_path` would land, before containment and before the
    /// target is looked for. `None` means this resolver will not look there at
    /// all, which today is an absolute path where those are not allowed.
    ///
    /// ONE ROOT PER EXPANSION (I10): relative paths resolve against the
    /// INCLUDING file, but containment is checked against the single top-level
    /// root. The root must NOT re-base per child, or a nested document could
    /// never reach a sibling directory of the project. The stack carries the
    /// canonical path of each ancestor, so a nested relative include resolves
    /// against its actual parent directory.
    fn candidate(&self, include_path: &str, ctx: &IncludeContext<'_>) -> Option<PathBuf> {
        let requested = Path::new(include_path);
        if requested.is_absolute() {
            if !self.allow_absolute {
                return None;
            }
            return Some(requested.to_path_buf());
        }
        let base = match ctx.stack.last() {
            Some(parent) => self
                .root_real
                .join(parent)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| self.root_real.clone()),
            None => self.root_real.clone(),
        };
        Some(base.join(requested))
    }
}

/// Whether `path` opens with a URI scheme, which names no place on this
/// filesystem for I11 to point at. The refusal itself is unchanged; only the
/// naming is withheld.
#[cfg(feature = "fs")]
fn is_uri(path: &str) -> bool {
    let Some(colon) = path.find(':') else {
        return false;
    };
    let scheme = &path[..colon];
    scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

#[cfg(feature = "fs")]
impl IncludeResolver for FileSystemResolver {
    fn resolve(
        &self,
        include_path: &str,
        ctx: &IncludeContext<'_>,
    ) -> Result<IncludeResolved, IncludeDenial> {
        let Some(candidate) = self.candidate(include_path, ctx) else {
            return Err(IncludeDenial::Denied);
        };
        // CONTAINMENT IS DECIDED FIRST, and lexically, so it does not depend on
        // the target existing: `mid/../../outside.crv` is outside the root
        // whether or not `mid` is there (markup-carve/carve#2021).
        if !self.contains(&lexical_real(&candidate)) {
            return Err(IncludeDenial::OutsideRoot);
        }
        // Checked again on the path actually read, in case a link changed
        // between the two canonicalizations.
        let real = std::fs::canonicalize(&candidate).map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => IncludeDenial::NotFound,
            _ => IncludeDenial::Denied,
        })?;
        if !self.contains(&real) {
            return Err(IncludeDenial::OutsideRoot);
        }
        // SIZE IS CHECKED BEFORE THE READ, and the expansion budget cannot
        // stand in for it: the budget charges a target only once its source is
        // in hand, so without a cap here one oversized file is read into
        // memory in full before expansion refuses it.
        if let Some(limit) = self.max_file_bytes {
            if std::fs::metadata(&real)
                .map_err(|_| IncludeDenial::Denied)?
                .len()
                > limit
            {
                return Err(IncludeDenial::Denied);
            }
        }
        // Read through the CANONICAL path: it holds no symlink components, so
        // the check that just passed describes the bytes actually read. A
        // residual TOCTOU window remains if a directory component is swapped
        // between the two syscalls, which is why this resolver is for trusted
        // trees only.
        let source = std::fs::read_to_string(&real).map_err(|_| IncludeDenial::Denied)?;
        Ok(IncludeResolved::with_id(
            source,
            real.to_string_lossy().into_owned(),
        ))
    }

    /// I11: a target that is simply not there is named by where it would be,
    /// the path a host watches. One that would land outside the root is refused
    /// like any other escape and keeps the directive's spelling.
    fn unresolved_id(&self, include_path: &str, ctx: &IncludeContext<'_>) -> Option<String> {
        if is_uri(include_path) {
            return None;
        }
        let would_be = lexical_real(&self.candidate(include_path, ctx)?);
        if !self.contains(&would_be) || would_be.exists() {
            return None;
        }
        Some(would_be.to_string_lossy().into_owned())
    }
}

/// `candidate` with its existing prefix canonicalized and the rest collapsed
/// lexically, so containment can be decided for a path that is not there.
#[cfg(feature = "fs")]
fn lexical_real(candidate: &Path) -> PathBuf {
    // The longest prefix that exists is canonicalized, which resolves the
    // symlinks actually on disk; what remains is pure arithmetic.
    let parts: Vec<std::ffi::OsString> = candidate
        .components()
        .map(|c| c.as_os_str().to_os_string())
        .collect();
    let mut split = parts.len();
    let canonical = loop {
        let prefix: PathBuf = parts[..split].iter().collect();
        if split == 0 {
            break PathBuf::new();
        }
        match std::fs::canonicalize(&prefix) {
            Ok(real) => break real,
            Err(_) => split -= 1,
        }
    };
    let mut out = canonical;
    for part in &parts[split..] {
        if part == ".." {
            out.pop();
        } else if part != "." {
            out.push(part);
        }
    }
    out
}
