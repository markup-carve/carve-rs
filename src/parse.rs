//! Carve block and inline parser.
//!
//! Block-level reads line by line; inline does a single linear scan
//! over each block's text. No backtracking.

pub(crate) mod delimited_table;
mod grapheme_extender_ranges;
mod inline_positions;
mod quote_state;
mod reference_resolution;
mod scoped_fence_closers;
mod verse_whitespace;

mod attrs;
mod block;
mod columns;
mod container;
mod cursor;
mod definition;
#[cfg(feature = "experimental-event-parser")]
mod event_pipeline;
mod footnote_prepass;
mod indent_container;
mod inline;
mod inline_abbreviations;
mod inline_crossrefs;
mod inline_links;
mod inline_resolution;
mod list;
mod own_lines;
mod paragraph;
mod positions;
mod prepass;
mod prepass_links;
mod quote;
pub use own_lines::decision_trace;
pub(in crate::parse) use own_lines::restore_authored_columns;
pub(crate) use own_lines::{
    authored_line_column, own_lines, walk_body, walk_moves_lines, EditsGuard,
};
mod table;
#[cfg(test)]
#[path = "parse/tests.rs"]
mod tests;
mod verse;
pub(crate) use attrs::*;
pub(crate) use block::*;
use columns::*;
pub(crate) use container::*;
use cursor::*;
use definition::*;
use footnote_prepass::*;
use indent_container::*;
pub(crate) use inline::*;
use inline_abbreviations::*;
pub(crate) use inline_crossrefs::*;
pub(crate) use inline_links::*;
pub(crate) use inline_resolution::*;
use list::*;
use paragraph::*;
pub(crate) use positions::*;
use prepass::*;
use prepass_links::*;
use quote::*;
use table::*;
use verse::*;

mod braced_closers;
mod continuation_marker;
mod substitution_scanner;
use braced_closers::{BracedClosers, NativeBracedView};
pub(crate) use inline_positions::InlineAnchor;
use inline_positions::InlinePositionMap;
use scoped_fence_closers::ScopedFenceClosers;
use verse_whitespace::expand_line_block_ws;

pub(crate) use reference_resolution::label_key;
use reference_resolution::{
    active_plain_link_destination, resolve_active_image, resolve_active_link,
    resolve_reference_links, with_active_link_defs, LinkDef,
};

use crate::ast::Pos;
use crate::ast::*;
use crate::extension::{
    AsciiHeadingIds, BlockMatch, HeadingIdOptions, InlineMatch, MatcherContext, Options,
};
use crate::include_walk::{block_pos_mut, inline_pos_mut};
use crate::sentinel_run::{occupied_private_use, pick_sentinel_run};
use crate::source_positions::CodepointLineStarts;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::num::NonZeroUsize;
use unicode_normalization::UnicodeNormalization;

/// The line a collected definition leaves behind, and the marker that says it
/// is ours rather than something the author wrote.
const PLACEHOLDER_DEFAULTS: [char; 2] = ['\u{E005}', '\u{E006}'];

/// Slot indices into [`PLACEHOLDER_DEFAULTS`] / [`PLACEHOLDERS`].
const P_DEFINITION: usize = 0;
const P_DOCUMENT_DEFINITION: usize = 1;

thread_local! {
    /// The suffix code points in force for the parse running on this thread.
    ///
    /// A thread-local for the same reason as `NESTING_DEPTH` above: the two
    /// markers are written by the definition collector and read by a dozen
    /// block-collection helpers that thread nothing else between them.
    ///
    /// Plain initializer for the same MSRV reason as `NESTING_DEPTH`.
    #[allow(clippy::missing_const_for_thread_local)]
    static PLACEHOLDERS: Cell<[char; 2]> = Cell::new(PLACEHOLDER_DEFAULTS);
}

/// Install `chars` for the duration of one parse, restoring the previous pair on
/// drop (panic unwind included), the discipline [`DepthGuard`] keeps.
struct PlaceholderGuard([char; 2]);

impl PlaceholderGuard {
    fn install(chars: [char; 2]) -> PlaceholderGuard {
        PlaceholderGuard(PLACEHOLDERS.with(|slot| slot.replace(chars)))
    }
}

impl Drop for PlaceholderGuard {
    fn drop(&mut self) {
        PLACEHOLDERS.with(|slot| slot.set(self.0));
    }
}

thread_local! {
    static UNTERMINATED_SOURCE_LINE: Cell<Option<usize>> = const { Cell::new(None) };
    static LAST_NONBLANK_SOURCE_LINE: Cell<Option<usize>> = const { Cell::new(None) };
    static LINT_EVENTS: RefCell<Option<Vec<LintEvent>>> = const { RefCell::new(None) };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LintEvent {
    Fence {
        pos: Pos,
        marker: String,
        end_line: usize,
    },
    Marker {
        line: usize,
        column: usize,
        host: &'static str,
        reason: &'static str,
        first_block: bool,
    },
}

pub(crate) fn collecting_lint_events<T>(f: impl FnOnce() -> T) -> (T, Vec<LintEvent>) {
    struct Guard(Option<Vec<LintEvent>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            LINT_EVENTS.with(|slot| slot.replace(self.0.take()));
        }
    }
    let guard = Guard(LINT_EVENTS.with(|slot| slot.replace(Some(Vec::new()))));
    let result = f();
    let events = LINT_EVENTS.with(|slot| slot.take()).unwrap_or_default();
    drop(guard);
    (result, events)
}

fn note_lint_event(event: LintEvent) {
    LINT_EVENTS.with(|slot| {
        if let Some(events) = slot.borrow_mut().as_mut() {
            events.push(event);
        }
    });
}

/// Retain the physical EOF through container collection and nested parses.
struct SourceEndingGuard(Option<usize>, Option<usize>);

impl SourceEndingGuard {
    fn install(source: &str) -> Self {
        let last = (!source.is_empty() && !source.ends_with('\n')).then(|| source.lines().count());
        let nonblank = source
            .lines()
            .enumerate()
            .filter(|(_, line)| !is_blank_line(line))
            .map(|(at, _)| at + 1)
            .last();
        Self(
            UNTERMINATED_SOURCE_LINE.with(|slot| slot.replace(last)),
            LAST_NONBLANK_SOURCE_LINE.with(|slot| slot.replace(nonblank)),
        )
    }

    fn fragment() -> Self {
        Self(
            UNTERMINATED_SOURCE_LINE.with(|slot| slot.replace(None)),
            LAST_NONBLANK_SOURCE_LINE.with(|slot| slot.replace(None)),
        )
    }
}

impl Drop for SourceEndingGuard {
    fn drop(&mut self) {
        UNTERMINATED_SOURCE_LINE.with(|slot| slot.set(self.0));
        LAST_NONBLANK_SOURCE_LINE.with(|slot| slot.set(self.1));
    }
}

fn needs_source_ending_map() -> bool {
    UNTERMINATED_SOURCE_LINE.with(|slot| slot.get().is_some())
}

/// Two private-use code points `source` does not contain.
///
/// The allocation itself is [`crate::sentinel_run`], shared with the site that
/// picks the Markdown target's escape carriers (markup-carve/carve-rs#1216) -
/// the properties that make it correct are stated there and are easy to lose in
/// a second copy: scan ONE CODE POINT AT A TIME, and answer from a SET of
/// occupied code points rather than from the document's text joined.
fn pick_definition_placeholders(source: &str) -> [char; 2] {
    // Only a collision with the preferred pair needs the occupied set.
    if source.is_ascii()
        || (!source.contains(PLACEHOLDER_DEFAULTS[0]) && !source.contains(PLACEHOLDER_DEFAULTS[1]))
    {
        return PLACEHOLDER_DEFAULTS;
    }
    pick_sentinel_run(
        &occupied_private_use(source),
        u32::from(PLACEHOLDER_DEFAULTS[0]),
    )
}

fn placeholder_char(which: usize) -> char {
    PLACEHOLDERS.with(|slot| slot.get()[which])
}

/// The marker a collected definition leaves behind, at its own column.
fn definition_placeholder() -> String {
    format!("%%{}", placeholder_char(P_DEFINITION))
}

/// The same marker for a definition written at the DOCUMENT's column zero.
fn document_definition_placeholder() -> String {
    format!("%%{}", placeholder_char(P_DOCUMENT_DEFINITION))
}

/// Which placeholder `line` is, if it is one.
///
/// Compares the suffix CODE POINT rather than building the marker string, so a
/// per-line test on a collected block allocates nothing.
fn placeholder_kind(line: &str) -> Option<usize> {
    let mut suffix = trim_ascii(line).strip_prefix("%%")?.chars();
    let ch = suffix.next()?;
    if suffix.next().is_some() {
        return None;
    }
    PLACEHOLDERS.with(|slot| slot.get().iter().position(|current| *current == ch))
}

fn is_definition_placeholder(line: &str) -> bool {
    placeholder_kind(line).is_some()
}

/// Only the marker a definition left at its container's column - NOT the one a
/// document-column definition left, which is an I5 interrupter instead.
fn is_collected_definition_placeholder(line: &str) -> bool {
    placeholder_kind(line) == Some(P_DEFINITION)
}

fn is_quoted_definition_placeholder(line: &str) -> bool {
    strip_blockquote_prefix(trim_ascii_start(line)).is_some_and(is_definition_placeholder)
}

fn list_marker_since_blank(lines: &[&str]) -> Vec<bool> {
    let mut seen = false;
    lines
        .iter()
        .map(|line| {
            let unquoted = without_blockquote_prefixes(line);
            if is_blank_line(unquoted) {
                seen = false;
            } else if detect_list_marker_full(line).is_some()
                || detect_list_marker_full(unquoted).is_some()
            {
                seen = true;
            }
            seen
        })
        .collect()
}

fn is_document_definition_placeholder(line: &str) -> bool {
    placeholder_kind(line) == Some(P_DOCUMENT_DEFINITION)
}

/// Maximum block + inline nesting depth. Pathological input (deeply nested
/// blockquotes, indented lists, bracketed inlines) recurses one stack frame
/// per level; without a cap a ~1000-deep document aborts the process with a
/// stack overflow (uncatchable -- a hard DoS for any embedder). Over the cap
/// the parser degrades gracefully (remaining block content becomes a flat
/// paragraph; inline content stays literal text) instead of recursing further.
///
/// The cap also bounds the depth of the AST the renderers walk recursively, so
/// it bounds the depth of the AST the renderers walk recursively.
///
/// The cap is 200, applied UNIFORMLY to blockquote, list, div, and admonition
/// nesting, matching carve-js (`MAX_NESTING_DEPTH = 200`) and carve-php so the
/// three implementations degrade at the same depth. Deeply nested input
/// degrades gracefully at the cap (remaining block content becomes a flat
/// paragraph; inline content stays literal) instead of recursing further, so
/// the AST depth is bounded by this constant. The recursive-descent parser and
/// the renderers use one native stack frame per level; in a release build 200
/// levels fit comfortably in a default 2 MiB thread stack (a debug build's
/// larger frames need more, which is why the worst-case-depth robustness tests
/// run on a generous worker stack). carve-php's analogous cap relies on PHP
/// growing its VM stack on the heap.
pub(crate) const MAX_NESTING_DEPTH: usize = 200;

/// The format a BARE frontmatter fence is in (PART 1, "a bare `---` defaults to
/// `yaml`").
///
/// Named rather than spelled twice: the parser publishes it as
/// `Frontmatter::format` and the canonical writer emits it on the opening fence
/// (PART 11 section 6b), and a second literal is a second place for the two to
/// drift apart.
pub(crate) const DEFAULT_FRONTMATTER_FORMAT: &str = "yaml";

fn trim_ascii_start(s: &str) -> &str {
    s.trim_start_matches([' ', '\t'])
}

fn trim_ascii_end(s: &str) -> &str {
    s.trim_end_matches([' ', '\t'])
}

fn trim_ascii(s: &str) -> &str {
    trim_ascii_end(trim_ascii_start(s))
}

fn is_blank_line(s: &str) -> bool {
    trim_ascii(s).is_empty()
}

thread_local! {
    // Plain initializer (not `const { … }`) to keep the crate's 1.75 MSRV;
    // the inline-const thread-local form clippy suggests requires Rust 1.79+,
    // so the lint is allowed here rather than followed.
    #[allow(clippy::missing_const_for_thread_local)]
    static NESTING_DEPTH: Cell<usize> = Cell::new(0);
}

/// RAII recursion-depth guard. `enter()` returns `None` when the cap is
/// already reached (the caller must degrade without recursing); otherwise it
/// increments the shared depth and returns a guard that decrements on drop
/// (including during panic unwind, so a normal parse always returns to 0).
struct DepthGuard;

impl DepthGuard {
    fn enter() -> Option<DepthGuard> {
        NESTING_DEPTH.with(|d| {
            if d.get() >= MAX_NESTING_DEPTH {
                None
            } else {
                d.set(d.get() + 1);
                Some(DepthGuard)
            }
        })
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        NESTING_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

thread_local! {
    // GROUPS DO NOT NEST (PART 9 §4c): a bare `::: figure` opener anywhere
    // inside an open group's body - at ANY depth, through divs, quotes and
    // list items - is a generic Tier-2 container, not an inner group. The
    // body parses through several re-entrant helpers on this thread, so the
    // state lives beside the depth counter they already share.
    //
    // Plain initializer for the same MSRV reason as `NESTING_DEPTH` above.
    #[allow(clippy::missing_const_for_thread_local)]
    static IN_FIGURE_GROUP: Cell<bool> = Cell::new(false);
}

thread_local! {
    // WHERE A FLOATING ATTRIBUTE RAN OUT OF BLOCKS (PART 9 §15 A4, ruled in
    // markup-carve/carve#1281). `None` while nobody is collecting, which is
    // every parse but the linter's - a `Some` is installed for the duration of
    // one call and taken back out at the end.
    //
    // A thread-local rather than an out-parameter because the drop happens
    // wherever a container's body finished, deep inside a recursion that returns
    // `Vec<BlockNode>` through a dozen call sites - the same reason the depth
    // counter and the figure-group flag above live here.
    //
    // Plain initializer for the same MSRV reason as `NESTING_DEPTH`.
    #[allow(clippy::missing_const_for_thread_local)]
    static UNATTACHED_BLOCK_ATTRS: RefCell<Option<Vec<Pos>>> = const { RefCell::new(None) };
}

/// Run `f`, collecting the source spans of every block attribute that reached
/// no block (PART 9 §15 A4).
///
/// The two ways to run out of following blocks - the end of the DOCUMENT and
/// the end of the CONTAINER holding the attribute - meet at one site, because a
/// container's body is parsed by its own [`parse_blocks`] call: a set still
/// pending when that call's loop ends is a set with nothing left in scope.
///
/// Positions are only recorded when the caller asked for them ([`Options`]
/// `positions`), since a span is what makes a diagnostic locatable and there is
/// nothing useful to report without one.
pub(crate) fn collecting_unattached_block_attrs<T>(f: impl FnOnce() -> T) -> (T, Vec<Pos>) {
    struct CollectorGuard(Option<Vec<Pos>>);
    impl Drop for CollectorGuard {
        fn drop(&mut self) {
            UNATTACHED_BLOCK_ATTRS.with(|slot| slot.replace(self.0.take()));
        }
    }
    let guard = CollectorGuard(UNATTACHED_BLOCK_ATTRS.with(|slot| slot.replace(Some(Vec::new()))));
    let out = f();
    let collected = UNATTACHED_BLOCK_ATTRS.with(|slot| slot.take());
    drop(guard);
    (out, collected.unwrap_or_default())
}

/// Record one dangling attribute run, if anybody is collecting.
fn note_unattached_block_attrs(pos: Option<Pos>) {
    if probing() {
        return;
    }
    let Some(pos) = pos else { return };
    UNATTACHED_BLOCK_ATTRS.with(|slot| {
        if let Some(spans) = slot.borrow_mut().as_mut() {
            spans.push(pos);
        }
    });
}

thread_local! {
    // Set while a PROBE is parsing a candidate source to answer a question about
    // it - "does this body end in an open paragraph", "does it end in a heading".
    // Those parses are thrown away, and their pending-attribute state is a fact
    // about a FRAGMENT rather than about the document: `:  d` / `   {.k}` /
    // `tail` probes the body `d` / `{.k}` WITHOUT the line the attribute might
    // still reach, so a recorder that counted probes reported an attribute the
    // finished parse had attached.
    //
    // Plain initializer for the same MSRV reason as `NESTING_DEPTH`.
    #[allow(clippy::missing_const_for_thread_local)]
    static PROBING: Cell<bool> = const { Cell::new(false) };
}

/// RAII guard marking a throwaway parse, restoring the previous value on drop
/// (panic unwind included), the discipline [`DepthGuard`] keeps.
struct ProbeGuard {
    previous: bool,
}

impl ProbeGuard {
    fn enter() -> ProbeGuard {
        ProbeGuard {
            previous: PROBING.with(|p| p.replace(true)),
        }
    }
}

impl Drop for ProbeGuard {
    fn drop(&mut self) {
        PROBING.with(|p| p.set(self.previous));
    }
}

fn probing() -> bool {
    PROBING.with(|p| p.get())
}

thread_local! {
    // Set while a CONTAINER LABEL's inline run is being scanned. An extension's
    // inline matcher belongs to the parse that registered it, and this scan runs
    // at RENDER time from an AST field, with no parse of its own to belong to -
    // so a matcher must not fire here. carve-js pins the same answer by scanning
    // a label through a session that carries no matchers at all.
    //
    // Plain initializer for the same MSRV reason as `NESTING_DEPTH`.
    #[allow(clippy::missing_const_for_thread_local)]
    static IN_CONTAINER_LABEL: Cell<bool> = const { Cell::new(false) };
}

/// RAII guard for [`IN_CONTAINER_LABEL`], with [`ProbeGuard`]'s discipline.
struct ContainerLabelGuard {
    previous: bool,
}

impl ContainerLabelGuard {
    fn enter() -> ContainerLabelGuard {
        ContainerLabelGuard {
            previous: IN_CONTAINER_LABEL.with(|f| f.replace(true)),
        }
    }
}

impl Drop for ContainerLabelGuard {
    fn drop(&mut self) {
        IN_CONTAINER_LABEL.with(|f| f.set(self.previous));
    }
}

/// The inline nodes of a CONTAINER LABEL - a div's, an admonition's, a
/// directive's or a fence's unconsumed `[label]`.
///
/// `CARVE-P9-041` used to ENUMERATE the hosts that have an inline run and named a
/// div label among them; `carve#2604` replaced the enumeration with a definition,
/// under which a container label is a delimited region parsed as `inline_content`
/// in its own right. So the label publishes that run and not the characters the
/// author typed (ruled on markup-carve/carve#2572).
///
/// THE LABEL STAYS A STRING IN THE AST, the way the oracle's own layout tree
/// holds it, so the scan happens where it is rendered and the interchange shape
/// does not move. What that costs is stated on the ticket: a reference, a
/// footnote reference and an abbreviation inside a label cannot resolve, because
/// resolution is a document pass over the tree and there is no run in the tree to
/// visit. Each renders as the text the author wrote, which is what every engine
/// does with an unresolved one anyway.
///
/// No positions: the label reaches this from an AST field, which carries no
/// offset of its own, and PART 12 section 4 forbids inventing one.
pub(crate) fn parse_container_label_inlines(label: &str, options: &Options<'_>) -> Vec<InlineNode> {
    let _label = ContainerLabelGuard::enter();
    let _depth = DepthScope::set(0);
    parse_inline_with_options(label, options)
}

/// Parse `source` to ANSWER A QUESTION about it, not to publish it.
///
/// What such a parse notices about attributes it could not place is a fact about
/// the fragment, so it is suppressed - see [`PROBING`].
fn probe_blocks(source: &str, options: &Options<'_>) -> Vec<BlockNode> {
    let _probing = ProbeGuard::enter();
    parse_blocks_with_options(source, options)
}

/// [`probe_blocks`] for a fragment that stands at the DOCUMENT level.
///
/// The level is not a detail the caller may leave at its default: an
/// abbreviation definition is only recognised there (see the `at_document_level`
/// arm of `line_starts_block`), so a probe run one level down reads `*[HTML]: x`
/// as an ordinary paragraph and answers every question about the line after it
/// the wrong way.
fn probe_blocks_at_document_level(source: &str, options: &Options<'_>) -> Vec<BlockNode> {
    probe_blocks_at_document_level_with_start(source, options).0
}

#[derive(Default)]
struct ProbeRootBoundary {
    start: Option<usize>,
    closed_initial_container_end: Option<usize>,
}

fn probe_blocks_at_document_level_with_start(
    source: &str,
    options: &Options<'_>,
) -> (Vec<BlockNode>, ProbeRootBoundary) {
    let _recording = definition_regions::RecordingPause::enter();
    let _probing = ProbeGuard::enter();
    let _source_ending = SourceEndingGuard::fragment();
    let mut root_start = ProbeRootBoundary::default();
    let blocks = drained(options, |pending| {
        let (blocks, start) = parse_blocks_with_options_at_level_into_reached_with_start(
            source,
            &[],
            None,
            options,
            ParseScope {
                at_document_level: true,
                in_item_body: false,
                in_quoted_body: false,
            },
            pending,
        );
        root_start = start;
        blocks
    });
    (blocks, root_start)
}

/// RAII guard that marks the current thread as parsing a figure group's body,
/// restoring the previous state on drop (panic unwind included), the same
/// discipline [`DepthGuard`] keeps for the depth counter.
struct FigureGroupGuard {
    previous: bool,
}

impl FigureGroupGuard {
    fn enter() -> FigureGroupGuard {
        let previous = IN_FIGURE_GROUP.with(Cell::get);
        IN_FIGURE_GROUP.with(|flag| flag.set(true));
        FigureGroupGuard { previous }
    }
}

impl Drop for FigureGroupGuard {
    fn drop(&mut self) {
        let previous = self.previous;
        IN_FIGURE_GROUP.with(|flag| flag.set(previous));
    }
}

/// Whether a container opener line is the BARE `::: figure` form - the fence,
/// its separator and the kind word, NOTHING else (PART 9 §4c,
/// `figure_group_open`). An opener carrying a quoted title or a label matches
/// `admonition_open` instead and stays a generic container.
fn is_bare_figure_open(open: &ContainerOpen) -> bool {
    !open.invalid_metadata
        && open.kind.as_deref() == Some("figure")
        && open.title.is_none()
        && open.label.is_none()
}

/// Parse Carve source with default options.
pub fn parse(source: &str) -> Document {
    #[cfg(feature = "experimental-event-parser")]
    if let Some((document, _)) = event_pipeline::parse_document(source) {
        return document;
    }
    parse_with_options(source, &Options::default())
}

/// Try the opt-in line-event parser for simple documents. The boolean reports
/// whether the event route was admitted; other inputs use the standard parser.
#[cfg(feature = "experimental-event-parser")]
pub fn parse_experimental_event_blocks(source: &str) -> (Document, bool) {
    if let Some(result) = event_pipeline::parse_document(source) {
        result
    } else {
        (parse_with_options(source, &Options::default()), false)
    }
}

/// Try the opt-in line-event parser with parser options. Unsupported option
/// combinations use the standard parser and return `false` for admission.
#[cfg(feature = "experimental-event-parser-options")]
pub fn parse_experimental_event_blocks_with_options(
    source: &str,
    options: &Options<'_>,
) -> (Document, bool) {
    if let Some(result) = event_pipeline::parse_document_with_options(source, options) {
        result
    } else {
        (parse_with_options(source, options), false)
    }
}

/// Parse Carve source using the supplied parser options.
pub fn parse_with_options(source: &str, options: &Options<'_>) -> Document {
    parse_with_options_mode(source, options, ParseMode::Html)
}

pub(crate) fn parse_with_render_index(
    source: &str,
    options: &Options<'_>,
) -> (Document, CrossrefIndex) {
    // The source-to-HTML facade renders immediately, so preserve the index that
    // parsing already built instead of walking the completed tree a second time.
    parse_with_options_mode_and_index(source, options, ParseMode::HtmlFacade)
}

/// The fmt parse WITHOUT positions, for comparing two renders' shapes.
///
/// `escaping_is_redundant` asks whether the minimal and conservative forms parse
/// to the same document. Two renders that differ only in escape bytes have
/// different offsets, so comparing position-bearing trees answers "no" for every
/// document carrying an escapable character, and the writer escalates to
/// conservative escaping - `See [it][ref].` came back `See [it][ref]\.`
/// (carve-rs#682, found the moment positions were enabled for the ordering
/// parse).
pub(crate) fn parse_for_carve_shape(source: &str) -> Document {
    parse_with_options_mode(source, &Options::default(), ParseMode::Carve)
}

pub(crate) fn parse_for_carve(source: &str) -> Document {
    // POSITIONS ON. The writer orders hoisted definitions by source position
    // (§7, PART 11 §6), and this parse is the only view of the document it
    // gets - without `pos` every definition reports usize::MAX and the order
    // falls back to "children, then footnotes by label", which is what put a
    // link definition ahead of the footnote it was written inside
    // (carve-rs#682).
    parse_with_options_mode(
        source,
        &Options::default().with_positions(true),
        ParseMode::Carve,
    )
}

mod definition_regions;
mod frontmatter;
mod layout;
mod source_map;
use definition_regions::verse_owned_lines;
use frontmatter::split_frontmatter;
pub(crate) use frontmatter::{
    find_frontmatter_closer, frontmatter_format_token, frontmatter_map, has_mapping_shape,
    opens_frontmatter,
};
pub(crate) use layout::{try_layout_html, try_layout_stream};
use source_map::{
    compose_mapped_source, first_mapped_line, map_pos_through_source, remap_source, LineBuffer,
    MappedSource, MarkerTail, SourceLine,
};
#[derive(Clone, Copy)]
enum ParseMode {
    Html,
    HtmlFacade,
    Carve,
}

/// Marks a lazily folded container line so nested parsing reads it as text.
/// Source normalization replaces authored NUL with U+FFFD. Nested block
/// parsing leaves the inserted marker intact. It pins the line to column 0,
/// and no block opener matches it.
pub(crate) const LAZY: &str = "\u{0000}L\u{0000}";
/// A quote's lazy follower at its owner's column can still supply a first block.
pub(crate) const LAZY_OWNER: &str = "\u{0000}L\u{0000}\u{0000}O\u{0000}";
/// A lazy term line with indentation past its host column still has literal spaces.
pub(crate) const LAZY_INDENT: &str = "\u{0000}L\u{0000}\u{0000}I\u{0000}";
/// Marks a separator line that ends a definition list and starts item paragraph text.
pub(crate) const ITEM_PARAGRAPH: &str = "\u{0000}P\u{0000}";

/// A body line with the LAZY frame removed.
///
/// EVERY SITE THAT TURNS A LINE INTO TEXT MUST CALL THIS, and the ones that
/// rebuild a source string for re-parsing must NOT: stripping there erases the
/// frame before it has done its job, which fails as a wrong answer rather than
/// as a visible sentinel. `tests/lazy_framing_never_leaks.rs` generates the
/// shapes that tell the two apart rather than listing them.
pub(crate) fn strip_lazy(line: &str) -> &str {
    line.strip_prefix(LAZY_OWNER)
        .or_else(|| line.strip_prefix(LAZY_INDENT))
        .or_else(|| line.strip_prefix(LAZY))
        .or_else(|| line.strip_prefix(ITEM_PARAGRAPH))
        .unwrap_or(line)
}

/// Join collected lines with a terminal newline so a trailing empty line
/// survives a later `str::lines()` pass.
fn joined_source<T: AsRef<str>>(lines: &[T]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let capacity = lines.iter().map(|line| line.as_ref().len() + 1).sum();
    let mut joined = String::with_capacity(capacity);
    for line in lines {
        joined.push_str(line.as_ref());
        joined.push('\n');
    }
    joined
}

/// Strip a leading BOM, normalize line endings, and replace authored NUL.
/// Parser and writer entry points share this normalization.
pub(crate) fn normalize_source(source: &str) -> std::borrow::Cow<'_, str> {
    if !(source.starts_with('\u{feff}') || source.contains('\r') || source.contains('\0')) {
        return std::borrow::Cow::Borrowed(source);
    }
    let trimmed = source.strip_prefix('\u{feff}').unwrap_or(source);
    std::borrow::Cow::Owned(
        trimmed
            .replace("\r\n", "\n")
            .replace('\r', "\n")
            .replace('\0', "\u{fffd}"),
    )
}

fn parse_with_options_mode(source: &str, options: &Options<'_>, mode: ParseMode) -> Document {
    parse_with_options_mode_and_index(source, options, mode).0
}

fn parse_with_options_mode_and_index(
    source: &str,
    options: &Options<'_>,
    mode: ParseMode,
) -> (Document, CrossrefIndex) {
    #[cfg(feature = "experimental-event-parser-options")]
    if event_root_allowed() {
        if let Some((document, index, _)) =
            event_pipeline::parse_document_with_custom_options(source, options, mode)
        {
            return (document, index.unwrap_or_default());
        }
    }
    // Kept for the offset table below: normalization rewrites the text the
    // parser sees, and PART 12 §4 positions index the ORIGINAL file.
    let original = source;
    let normalized = normalize_source(source);
    // Rule A: the readers only meet openers at a content column. The guard maps
    // positions in the walked text back to the authored columns.
    let owned = own_lines(normalized.as_ref());
    let _owned_edits = EditsGuard::install(owned.as_ref());
    let source = owned
        .as_ref()
        .map_or(normalized.as_ref(), |o| o.source.as_str());
    let _continuation_markers = continuation_marker::Guard::install(source);
    let _source_ending = SourceEndingGuard::install(source);
    // PICK THE DEFINITION PLACEHOLDERS FOR THIS DOCUMENT before anything reads
    // them. Every entry point funnels through here, and the guard restores the
    // previous pair on the way out, so a parse nested inside another one - a
    // throwaway probe, the writer's shape comparison - cannot leave a foreign
    // pair installed. See `PLACEHOLDER_DEFAULTS`.
    let _placeholders = PlaceholderGuard::install(pick_definition_placeholders(source));
    let (frontmatter, frontmatter_raw, body) = split_frontmatter(source, options.positions);
    let body_start_line = source
        [..(body.as_ptr() as usize).saturating_sub(source.as_ptr() as usize)]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    let (body, mut footnote_defs_src, mut footnote_def_pos, mut note_link_defs) =
        if body.contains("[^") {
            extract_footnote_defs(
                body,
                body_start_line,
                options.positions,
                false,
                None,
                options,
            )
        } else {
            let body_line_count = body.lines().count();
            (
                MappedSource {
                    source: body.to_owned(),
                    line_map: (body_start_line..body_start_line + body_line_count)
                        .map(Some)
                        .collect(),
                    col_map: if options.positions {
                        vec![Some(0); body_line_count]
                    } else {
                        Vec::new()
                    },
                    reached: Vec::new(),
                    opaque_blank_lines: Vec::new(),
                    discarded_outer_blank: false,
                    outer_plus_after_blank: false,
                    item_marker_at_start: false,
                    marker_tail: None,
                    in_quoted_body: false,
                    sublists_carry_authored_base: false,
                },
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
            )
        };
    extract_nested_footnote_defs(
        &mut footnote_defs_src,
        &mut footnote_def_pos,
        &mut note_link_defs,
        options,
    );
    let mut link_def_probe_budget = probe_budget_for(body.source.len());
    let (body_source, mut link_defs) =
        extract_link_defs_guarded(&body.source, options, &mut link_def_probe_budget);
    // These lines index the FOOTNOTE-REDUCED body: a multi-line footnote body
    // collapses to one placeholder line, so a definition written below it sits
    // at a lower reduced index than its authored line. Map each back to the
    // authored line through the footnote line map before it reaches
    // `append_link_reference_definitions`, which indexes the ORIGINAL source -
    // otherwise the definition takes an earlier line's span and two collected
    // definitions can share an offset, which flips their order on re-format
    // (carve-rs#1559 corpus 456). The nested defs merged below are already in
    // authored coordinates, so this runs before the merge, not after.
    for def in link_defs.values_mut() {
        if let Some(line) = def.line {
            def.line = body
                .line_map
                .get(line)
                .and_then(|mapped| *mapped)
                .map(|authored| authored - 1)
                .or(def.line);
        }
    }
    // A definition written inside a footnote body is document-level metadata,
    // and a definition at the top level WINS over one of the same label there -
    // measured on carve-js and carve-php, which both resolve `[t][r]` to the
    // outer target when both exist.
    for (label, def) in note_link_defs {
        link_defs.entry(label).or_insert(def);
    }
    let mut body = remap_source(body_source, &body);
    if !options.positions && body.source.contains("%%%") && body.source.contains("::") {
        body.col_map = vec![Some(0); body.source.lines().count()];
    }
    let ((mut footnote_defs, mut children), link_defs, needs_late_link_resolution) =
        with_active_link_defs(link_defs, || {
            let footnote_defs: BTreeMap<String, Vec<BlockNode>> = footnote_defs_src
                .into_iter()
                .map(|(label, source)| {
                    let children = if source.source.trim_end_matches([' ', '\t', '\n']) == "{empty}"
                    {
                        Vec::new()
                    } else {
                        parse_mapped_source(&source, options)
                    };
                    (label, children)
                })
                .collect();
            let children = parse_mapped_source_at_document_level(&body, options);
            (footnote_defs, children)
        });
    let original_line_starts = options
        .positions
        .then(|| CodepointLineStarts::original(original));
    if let Some(line_starts) = original_line_starts.as_ref() {
        // Offsets need the original text, which the parser only ever sees as
        // already-stripped lines, so they are derived here in one pass.
        // BUILT FROM THE ORIGINAL TEXT, not the normalized copy. Normalization
        // strips a leading BOM and collapses CRLF/CR to LF, so a table built
        // from the result is short by one codepoint per removed character and
        // every offset in the document lands before the text it names -
        // `<BOM># T` reported the space where the node said `T` (carve#876).
        //
        // Line NUMBERS are unaffected: the normalization preserves the line
        // count, so entry N still describes line N. Only where each line starts
        // in the file changes, which is exactly what this table holds.
        fill_offsets(&mut children, line_starts);
        for blocks in footnote_defs.values_mut() {
            fill_offsets(blocks, line_starts);
        }
        // A definition whose BODY places something already has an extent, and
        // that extent - not this line - is what reaches the wire. Keeping the
        // line beside it would put a fact in the document that its own
        // serialization does not carry, so an ingested copy would differ from
        // the parse (§6). Dropped here rather than never recorded because the
        // bodies are parsed after the scan that finds the definition lines.
        // Place both boundaries; a multi-line definition ends at the following
        // line start rather than on its opener line.
        for pos in footnote_def_pos.values_mut() {
            apply_offsets(pos, line_starts);
        }
        // A CONTAINER ENDS AT ITS LAST PLACED CHILD, which is the rule that
        // replaced `widen_over_hosted_definitions` (carve#1522, carve#1524).
        let source_lines: Vec<&str> = original.lines().collect();
        narrow_to_last_placed_child(&mut children, &source_lines);
        // A FOOTNOTE BODY IS A SEPARATE BLOCK LIST, walked separately by every
        // pass above for the same reason: it is not under `children`. Reaching
        // only `children` left a list inside a note body still ending after the
        // terminator of its last item, on one corpus document out of 1352 - the
        // shape this whole rule is about, in the one place a pass is easiest to
        // forget to apply.
        for blocks in footnote_defs.values_mut() {
            narrow_to_last_placed_child(blocks, &source_lines);
        }
        for (label, pos) in &mut footnote_def_pos {
            let last = footnote_defs
                .get(label)
                .and_then(|blocks| blocks.iter().rev().find_map(crate::ast_json::block_pos))
                .cloned();
            end_at_last_placed_child(pos, last);
        }
    }
    let mut doc = Document {
        frontmatter,
        frontmatter_raw,
        footnote_defs,
        footnote_def_pos,
        children,
        source_len: original.len(),
        // Measured here, so nothing about the parse path needs a second number.
        ingest_payload_len: 0,
    };
    let has_crossrefs = source.contains("</#");
    let (mut heading_index, _assigned_heading_ids) = heading_index(
        &doc.children,
        &doc.footnote_defs,
        options.heading_id_options(),
        has_crossrefs,
        needs_late_link_resolution,
    );
    if needs_late_link_resolution {
        resolve_reference_links(&mut doc, &link_defs, &heading_index);
    }
    if !matches!(mode, ParseMode::HtmlFacade) {
        // Definition nodes are part of the published AST and canonical writer,
        // but render no HTML. The source-to-HTML facade already owns the
        // resolved definition index and does not materialize invisible nodes.
        append_link_reference_definitions(
            &mut doc,
            &link_defs,
            source,
            original_line_starts.as_ref(),
        );
    }
    if matches!(mode, ParseMode::Html | ParseMode::HtmlFacade) {
        if source.contains("*[") {
            apply_abbreviations(&mut doc);
        }
        // Reference images and indented images can become figures only after
        // inline resolution. Promote them before the caption numbering pass so
        // every figure draws from the same sequence.
        if source.contains("![") {
            promote_block_images(&mut doc.children, false, true);
            for blocks in doc.footnote_defs.values_mut() {
                promote_block_images(blocks, false, true);
            }
        }
        let caption_titles = if source.contains('^') {
            number_crossref_captions(&mut doc)
        } else {
            BTreeMap::new()
        };
        if matches!(mode, ParseMode::HtmlFacade) && options.extensions.is_empty() && has_crossrefs {
            heading_index.extend_titles(caption_titles);
        }
        // PART 12 §5 again: a heading's GENERATED id is a resolution result and
        // is serialized. It is not recomputable from the heading - dedup assigns
        // the next free suffix in DOCUMENT ORDER, so `Notes-2` needs every
        // heading before it.
        //
        // Assigned HERE rather than in `ast_json::to_json`, because §6 asks that
        // `from_json(to_json(parse(x))) == parse(x)`: a field added at encode
        // time comes back on decode and the trees no longer match. carve-js
        // assigns it in its parse for the same reason (carve#750).
        // Keep the document-id seeder authoritative. Unlike the heading index,
        // it reserves explicit ids carried by inline nodes as well as blocks.
        stamp_generated_heading_ids(&mut doc, options.heading_id_options());
        if !matches!(mode, ParseMode::HtmlFacade) {
            let _ = crate::render::collect_footnotes(&mut doc, false);
        }
    } else {
        // Carve/fmt mode: promote image+caption paragraphs to figures too, so a
        // caption serializes as an unescaped `^ …` line -- portable and
        // round-tripping in every implementation. Without this the caption would
        // stay a paragraph `[Image, SoftBreak, "^ …"]` and the leading `^` would
        // be escaped to `\^`, which only carve-js's lenient parser reads back as
        // a caption (carve-rs / carve-php read it as literal text, losing the
        // figure). Reference-link resolution already ran above.
        if source.contains("![") {
            promote_block_images(&mut doc.children, true, true);
            for blocks in doc.footnote_defs.values_mut() {
                promote_block_images(blocks, true, true);
            }
        }
    }
    for ext in &options.extensions {
        doc = ext.after_parse(doc);
    }
    // Last, so it also covers runs an extension left behind: §1a is about the
    // tree that gets published, whoever produced it.
    if !matches!(mode, ParseMode::HtmlFacade) {
        // Adjacent text runs are a published-AST invariant. HTML writes their
        // bytes consecutively either way, so the private facade tree can avoid
        // a whole-document normalization pass.
        coalesce_text_runs(&mut doc);
    }
    // After every extension has had its say, so a heading an extension added is
    // a crossref target like any other.
    let crossref_index = if matches!(mode, ParseMode::HtmlFacade) && options.extensions.is_empty() {
        // The renderer consumes this exact index. Publishing `href` beside the
        // authored target matters to AST consumers, but would only make the
        // facade traverse and allocate strings that HTML resolves from here.
        heading_index
    } else if matches!(mode, ParseMode::HtmlFacade) {
        crossref_index_for_document(&doc, options.heading_id_options())
    } else {
        fill_crossref_hrefs(&mut doc, options.heading_id_options())
    };
    (doc, crossref_index)
}

/// Lift definitions written inside an already-lifted footnote body.
///
/// The first extraction pass cannot see these lines as independent definitions:
/// it consumes the complete outer body in one step. Repeating the same guarded
/// extraction over each newly found body gives nested notes the same
/// document-level ownership as notes written in any other container. Mapping is
/// composed back through the parent so positions continue to name the authored
/// document rather than the temporary body string.
fn extract_nested_footnote_defs(
    defs: &mut BTreeMap<String, MappedSource>,
    positions: &mut BTreeMap<String, Pos>,
    link_defs: &mut BTreeMap<String, LinkDef>,
    options: &Options<'_>,
) {
    let mut pending: Vec<String> = defs.keys().cloned().collect();
    let mut seen = BTreeSet::new();

    while let Some(label) = pending.pop() {
        if !seen.insert(label.clone()) {
            continue;
        }
        let Some(parent) = defs.remove(&label) else {
            continue;
        };
        if !parent.source.contains("[^") {
            defs.insert(label, parent);
            continue;
        }

        let (body, nested, nested_positions, nested_links) = extract_footnote_defs(
            &parent.source,
            1,
            options.positions,
            true,
            Some(&parent.line_map),
            options,
        );
        let body = compose_mapped_source(body, &parent);
        defs.insert(label, body);

        for (nested_label, nested_source) in nested {
            let nested_source = compose_mapped_source(nested_source, &parent);
            let nested_line = first_mapped_line(&nested_source);
            let existing_line = defs.get(&nested_label).and_then(first_mapped_line);
            if existing_line.is_some() && existing_line <= nested_line {
                continue;
            }
            defs.insert(nested_label.clone(), nested_source);
            seen.remove(&nested_label);
            pending.push(nested_label);
        }
        for (nested_label, mut pos) in nested_positions {
            map_pos_through_source(&mut pos, &parent);
            positions.entry(nested_label).or_insert(pos);
        }
        for (nested_label, mut def) in nested_links {
            if let Some(line) = def.line {
                def.line = parent
                    .line_map
                    .get(line)
                    .and_then(|mapped| *mapped)
                    .map(|mapped| mapped - 1);
            }
            link_defs.entry(nested_label).or_insert(def);
        }
    }
}

/// Publish each crossref's resolution BESIDE its authored target
/// (PART 12 section 3a).
///
/// This engine resolves `</#id>` at RENDER time, from an index built per
/// render, which is why the tree carried only what the author wrote. That is
/// half of what section 3a asks for: a consumer decoding the published tree
/// had to rebuild the heading table and re-run the match
/// before it could render a crossref, which is the recomputation section 5
/// exists to prevent.
///
/// The renderers keep using their index rather than this field. Both come from
/// the same builder, so they cannot disagree, and the render path stays able to
/// resolve a tree that arrived without hrefs at all.
fn fill_crossref_hrefs(doc: &mut Document, id_opts: HeadingIdOptions) -> CrossrefIndex {
    let index = crossref_index_for_document(doc, id_opts);
    fill_crossref_hrefs_from_index(doc, index)
}

fn fill_crossref_hrefs_from_index(doc: &mut Document, index: CrossrefIndex) -> CrossrefIndex {
    fn inlines(nodes: &mut [InlineNode], index: &CrossrefIndex) {
        for node in nodes {
            match node {
                InlineNode::CrossRef(c) => {
                    c.href = index.resolve(&c.target).map(|(id, _)| format!("#{id}"));
                }
                InlineNode::Emphasis(e) => inlines(&mut e.children, index),
                InlineNode::Span(sp) => inlines(&mut sp.children, index),
                InlineNode::Link(l) => inlines(&mut l.children, index),
                InlineNode::Footnote(f) => {
                    if let Some(inline) = &mut f.inline {
                        inlines(inline, index);
                    }
                }
                _ => {}
            }
        }
    }

    fn blocks(nodes: &mut [BlockNode], index: &CrossrefIndex) {
        for node in nodes {
            match node {
                BlockNode::Paragraph(p) => inlines(&mut p.children, index),
                BlockNode::Heading(h) => inlines(&mut h.children, index),
                BlockNode::BlockQuote(b) => {
                    blocks(&mut b.children, index);
                }
                BlockNode::Directive(d) => blocks(&mut d.children, index),
                BlockNode::Div(d) => blocks(&mut d.children, index),
                BlockNode::Figure(f) => inlines(&mut f.caption, index),
                BlockNode::FigureGroup(g) => {
                    blocks(&mut g.children, index);
                    if let Some(caption) = &mut g.caption {
                        inlines(caption, index);
                    }
                }
                BlockNode::List(l) => {
                    for item in &mut l.items {
                        blocks(&mut item.children, index);
                    }
                }
                BlockNode::DefinitionList(d) => {
                    for entry in &mut d.items {
                        for term in &mut entry.terms {
                            inlines(&mut term.children, index);
                        }
                        for description in &mut entry.definitions {
                            blocks(&mut description.children, index);
                        }
                    }
                }
                BlockNode::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            inlines(&mut cell.children, index);
                        }
                    }
                    if let Some(caption) = &mut t.caption {
                        inlines(caption, index);
                    }
                }
                BlockNode::LineBlock(l) => blocks(&mut l.children, index),
                _ => {}
            }
        }
    }

    blocks(&mut doc.children, &index);
    let labels: Vec<String> = doc.footnote_defs.keys().cloned().collect();
    for label in labels {
        if let Some(body) = doc.footnote_defs.get_mut(&label) {
            blocks(body, &index);
        }
    }
    index
}

/// A definition line with the enclosing item's content column removed, plus the
/// residual indentation that carve#1705's authored base absorbs.
///
/// The residual is absorbed WHENEVER the line reaches a content column, not only
/// where no deeper column is live. Withholding it there was the whole of
/// carve-rs#1505: under `- - x` the live columns are 2 and 4, and a definition
/// at 3 reaches 2 - the item at 4 opened on the marker line above and never on
/// this one, so it owns nothing here (markup-carve/carve#1896).
fn at_content_column<'a>(bare: &'a str, structural: &str, content_col: usize) -> &'a str {
    // A BLOCKQUOTE prefix does not disqualify the strip: columns are measured
    // inside the quote, so `> - a` / `>   [r]: /u` is the same shape as its
    // unquoted twin and the definition is the item's block either way
    // (carve#658). A LIST marker in the structural prefix does disqualify it -
    // there the marker already consumed the column, and stripping again would
    // eat the item's own content.
    let quoted_only = !structural.is_empty()
        && structural
            .chars()
            .all(|c| c == '>' || c == ' ' || c == '\t');
    if content_col > 0 && (structural.is_empty() || quoted_only) {
        let at = bare.strip_prefix(&" ".repeat(content_col)).unwrap_or(bare);
        let authored = at.trim_start_matches([' ', '\t']);
        if authored.starts_with('[')
            && (parse_link_def_line_with_value(authored).is_some()
                || parse_footnote_def_line(authored).is_some())
        {
            authored
        } else {
            at
        }
    } else {
        bare
    }
}

pub(crate) fn parse_blocks_with_options(source: &str, options: &Options<'_>) -> Vec<BlockNode> {
    let _recording = definition_regions::RecordingPause::enter();
    // Fragments have local line numbers and may omit collection separators.
    // Only a whole-document entry point can identify physical EOF.
    let _source_ending = SourceEndingGuard::fragment();
    parse_blocks_with_options_at_level(source, options, false)
}

/// The drained entry point: every container body this parse opens is resolved
/// before the blocks are handed back, so no caller ever sees a hollow node.
fn parse_blocks_with_options_at_level(
    source: &str,
    options: &Options<'_>,
    at_document_level: bool,
) -> Vec<BlockNode> {
    drained(options, |pending| {
        parse_blocks_with_options_at_level_into(source, options, at_document_level, false, pending)
    })
}

fn parse_blocks_with_options_at_level_into(
    source: &str,
    options: &Options<'_>,
    at_document_level: bool,
    in_item_body: bool,
    pending: &mut Vec<PendingBody>,
) -> Vec<BlockNode> {
    parse_blocks_with_options_at_level_into_reached(
        source,
        &[],
        None,
        options,
        ParseScope {
            at_document_level,
            in_item_body,
            in_quoted_body: false,
        },
        pending,
    )
}

#[derive(Clone, Copy)]
struct ParseScope {
    at_document_level: bool,
    in_item_body: bool,
    in_quoted_body: bool,
}

/// The same parse, told which of `source`'s lines reached the content column of
/// the container that collected them - see `LineCursor::reached`.
fn parse_blocks_with_options_at_level_into_reached(
    source: &str,
    reached: &[bool],
    marker_tail: Option<MarkerTail>,
    options: &Options<'_>,
    scope: ParseScope,
    pending: &mut Vec<PendingBody>,
) -> Vec<BlockNode> {
    parse_blocks_with_options_at_level_into_reached_with_start(
        source,
        reached,
        marker_tail,
        options,
        scope,
        pending,
    )
    .0
}

fn parse_blocks_with_options_at_level_into_reached_with_start(
    source: &str,
    reached: &[bool],
    marker_tail: Option<MarkerTail>,
    options: &Options<'_>,
    scope: ParseScope,
    pending: &mut Vec<PendingBody>,
) -> (Vec<BlockNode>, ProbeRootBoundary) {
    parse_blocks_with_options_at_level_into_reached_recording(
        source,
        reached,
        marker_tail,
        options,
        scope,
        pending,
        None,
    )
}

fn parse_blocks_with_options_at_level_into_reached_recording(
    source: &str,
    reached: &[bool],
    marker_tail: Option<MarkerTail>,
    options: &Options<'_>,
    scope: ParseScope,
    pending: &mut Vec<PendingBody>,
    extents: Option<&mut Vec<(usize, usize)>>,
) -> (Vec<BlockNode>, ProbeRootBoundary) {
    let mut lines: Vec<&str> = source.lines().collect();
    // `lines()` already drops a single trailing newline; nothing more to do.
    let _ = &mut lines;

    // Physical EOF needs line origins even when positions are not requested.
    let want_lines = options.source_lines
        || options.positions
        || needs_source_ending_map()
        || needs_quote_marker_line_map(source);
    let line_map: Vec<Option<usize>> = if want_lines {
        (1..=lines.len()).map(Some).collect()
    } else {
        Vec::new()
    };
    // Nothing has been stripped from a top-level line, so every column here is
    // a column in the document.
    let col_map: Vec<Option<isize>> = if options.positions {
        vec![Some(0); lines.len()]
    } else {
        Vec::new()
    };
    let mut cursor = boxed_cursor(
        &lines,
        want_lines.then_some(line_map.as_slice()),
        options.positions.then_some(col_map.as_slice()),
    );
    cursor.at_document_level = scope.at_document_level;
    cursor.in_item_body = scope.in_item_body;
    // Quote X2 also applies in the unpositioned fast path. Losing this bit
    // makes a separator under an open quoted term look like a document line.
    cursor.in_quoted_body = scope.in_quoted_body;
    cursor.reached = Some(reached);
    cursor.marker_tail = marker_tail;
    if extents.is_some() {
        cursor.block_extents = Some(Vec::new());
    }
    let blocks = parse_blocks(&mut cursor, options, pending);
    if let Some(extents) = extents {
        extents.extend(cursor.block_extents.take().unwrap_or_default());
    }
    (
        blocks,
        ProbeRootBoundary {
            start: cursor.last_block_start,
            closed_initial_container_end: cursor.closed_initial_container_end,
        },
    )
}

/// The cursor lives on the heap: this frame sits on the stack once per quote
/// level, and a `LineCursor` held here made it the tallest one.
#[inline(never)]
fn boxed_cursor<'a>(
    lines: &'a [&'a str],
    line_map: Option<&'a [Option<usize>]>,
    col_map: Option<&'a [Option<isize>]>,
) -> Box<LineCursor<'a>> {
    Box::new(LineCursor::new_with_cols(lines, line_map, col_map))
}

fn parse_mapped_source(source: &MappedSource, options: &Options<'_>) -> Vec<BlockNode> {
    parse_mapped_source_at_level(source, options, false)
}

fn parse_quoted_mapped_source(source: &MappedSource, options: &Options<'_>) -> Vec<BlockNode> {
    drained(options, |pending| {
        parse_mapped_source_at_level_into(source, options, false, false, true, pending)
    })
}

fn parse_mapped_source_at_document_level(
    source: &MappedSource,
    options: &Options<'_>,
) -> Vec<BlockNode> {
    parse_mapped_source_at_level(source, options, true)
}

/// The drained entry point - see [`parse_blocks_with_options_at_level`].
fn parse_mapped_source_at_level(
    source: &MappedSource,
    options: &Options<'_>,
    at_document_level: bool,
) -> Vec<BlockNode> {
    drained(options, |pending| {
        parse_mapped_source_at_level_into(source, options, at_document_level, false, false, pending)
    })
}

fn parse_mapped_source_at_level_into(
    source: &MappedSource,
    options: &Options<'_>,
    at_document_level: bool,
    in_item_body: bool,
    in_quoted_body: bool,
    pending: &mut Vec<PendingBody>,
) -> Vec<BlockNode> {
    parse_mapped_source_at_level_into_recording(
        source,
        options,
        at_document_level,
        in_item_body,
        in_quoted_body,
        pending,
        None,
    )
}

fn parse_mapped_source_at_level_into_recording(
    source: &MappedSource,
    options: &Options<'_>,
    at_document_level: bool,
    in_item_body: bool,
    in_quoted_body: bool,
    pending: &mut Vec<PendingBody>,
    extents: Option<&mut Vec<(usize, usize)>>,
) -> Vec<BlockNode> {
    if !options.source_lines
        && !options.positions
        && !needs_source_ending_map()
        && !source.col_map.iter().any(Option::is_some)
        && !source.source.contains('+')
    {
        return parse_unpositioned_mapped_source(
            source,
            options,
            at_document_level,
            in_item_body,
            in_quoted_body,
            pending,
            extents,
        );
    }
    parse_positioned_mapped_source(
        source,
        options,
        at_document_level,
        in_item_body,
        in_quoted_body,
        pending,
        extents,
    )
}

#[inline(never)]
fn parse_unpositioned_mapped_source(
    source: &MappedSource,
    options: &Options<'_>,
    at_document_level: bool,
    in_item_body: bool,
    in_quoted_body: bool,
    pending: &mut Vec<PendingBody>,
    extents: Option<&mut Vec<(usize, usize)>>,
) -> Vec<BlockNode> {
    #[cfg(feature = "experimental-event-parser-options")]
    let mut extents = extents;
    // The ladder parses its tail eagerly, before the parent's pending bodies,
    // and extension callbacks observe that order. Take it ahead of the event
    // reader, which would defer the tail onto the worklist.
    #[cfg(feature = "experimental-event-parser-options")]
    if !at_document_level && !options.extensions.is_empty() {
        if let Some(blocks) = parse_eof_closed_colon_ladder(source, options) {
            if let Some(extents) = extents {
                extents.extend(blocks.iter().map(|_| (0, source.source.lines().count())));
            }
            return blocks;
        }
    }
    #[cfg(feature = "experimental-event-parser-options")]
    if !at_document_level && nested_event_allowed() && !needs_positioned_mapped_body(source) {
        if let Some(blocks) = event_pipeline::parse_nested_mapped_blocks(
            source,
            options,
            ParseScope {
                at_document_level,
                in_item_body,
                in_quoted_body,
            },
            extents.as_deref_mut(),
            pending,
            false,
        ) {
            return blocks;
        }
    }
    // The ladder builds its own containers inside-out and hands back finished
    // nodes, so it contributes nothing to `pending` - and must not, because it
    // WRAPS the blocks it parsed, which would move any index recorded in them.
    if let Some(blocks) = parse_eof_closed_colon_ladder(source, options) {
        if let Some(extents) = extents {
            extents.extend(blocks.iter().map(|_| (0, source.source.lines().count())));
        }
        return blocks;
    }
    if needs_positioned_mapped_body(source) {
        return parse_positioned_mapped_source(
            source,
            options,
            at_document_level,
            in_item_body,
            in_quoted_body,
            pending,
            extents,
        );
    }
    parse_blocks_with_options_at_level_into_reached_recording(
        &source.source,
        &source.reached,
        source.marker_tail,
        options,
        ParseScope {
            at_document_level,
            in_item_body,
            in_quoted_body: in_quoted_body || source.in_quoted_body,
        },
        pending,
        extents,
    )
    .0
}

fn needs_positioned_mapped_body(source: &MappedSource) -> bool {
    source.sublists_carry_authored_base
        && source.source.contains(":::")
        && source.source.lines().any(|line| {
            line.starts_with([' ', '\t'])
                && strip_blockquote_prefix(trim_ascii_start(line)).is_some()
        })
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
thread_local! {
    static DISABLE_NESTED_EVENT: Cell<bool> = const { Cell::new(false) };
    static DISABLE_EVENT_ROOT: Cell<bool> = const { Cell::new(false) };
}

#[cfg(feature = "experimental-event-parser-options")]
fn event_root_allowed() -> bool {
    #[cfg(test)]
    {
        !DISABLE_EVENT_ROOT.with(Cell::get)
    }
    #[cfg(not(test))]
    {
        true
    }
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
struct StandardEventRootGuard(bool);

#[cfg(all(test, feature = "experimental-event-parser-options"))]
impl StandardEventRootGuard {
    fn install() -> Self {
        Self(DISABLE_EVENT_ROOT.with(|disabled| disabled.replace(true)))
    }
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
impl Drop for StandardEventRootGuard {
    fn drop(&mut self) {
        DISABLE_EVENT_ROOT.with(|disabled| disabled.set(self.0));
    }
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
fn nested_event_allowed() -> bool {
    !DISABLE_NESTED_EVENT.with(Cell::get)
}

#[cfg(all(not(test), feature = "experimental-event-parser-options"))]
fn nested_event_allowed() -> bool {
    true
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
struct StandardNestedReaderGuard(bool);

#[cfg(all(test, feature = "experimental-event-parser-options"))]
impl StandardNestedReaderGuard {
    fn install() -> Self {
        Self(DISABLE_NESTED_EVENT.with(|disabled| disabled.replace(true)))
    }
}

#[cfg(all(test, feature = "experimental-event-parser-options"))]
impl Drop for StandardNestedReaderGuard {
    fn drop(&mut self) {
        DISABLE_NESTED_EVENT.with(|disabled| disabled.set(self.0));
    }
}

#[inline(never)]
fn parse_positioned_mapped_source(
    source: &MappedSource,
    options: &Options<'_>,
    at_document_level: bool,
    in_item_body: bool,
    in_quoted_body: bool,
    pending: &mut Vec<PendingBody>,
    extents: Option<&mut Vec<(usize, usize)>>,
) -> Vec<BlockNode> {
    #[cfg(feature = "experimental-event-parser-options")]
    let mut extents = extents;
    #[cfg(feature = "experimental-event-parser-options")]
    if !at_document_level && nested_event_allowed() {
        let mut event_extents = Vec::new();
        if let Some(blocks) = event_pipeline::parse_nested_mapped_blocks(
            source,
            options,
            ParseScope {
                at_document_level,
                in_item_body,
                in_quoted_body,
            },
            extents.is_some().then_some(&mut event_extents),
            pending,
            true,
        ) {
            if let Some(extents) = extents.as_deref_mut() {
                extents.extend(event_extents);
            }
            return blocks;
        }
    }
    parse_positioned_mapped_source_standard(
        source,
        options,
        at_document_level,
        in_item_body,
        in_quoted_body,
        pending,
        extents,
    )
}

#[inline(never)]
fn parse_positioned_mapped_source_standard(
    source: &MappedSource,
    options: &Options<'_>,
    at_document_level: bool,
    in_item_body: bool,
    in_quoted_body: bool,
    pending: &mut Vec<PendingBody>,
    extents: Option<&mut Vec<(usize, usize)>>,
) -> Vec<BlockNode> {
    let lines: Vec<&str> = source.source.lines().collect();
    // The mapped source carries the widths its container already stripped, so a
    // nested block's column is measured against the document, not against the
    // rewritten text the parser sees.
    let mut cursor = Box::new(LineCursor::new_with_cols(
        &lines,
        Some(&source.line_map),
        source
            .col_map
            .iter()
            .any(Option::is_some)
            .then_some(source.col_map.as_slice()),
    ));
    cursor.at_document_level = at_document_level;
    cursor.in_item_body = in_item_body;
    cursor.in_quoted_body = in_quoted_body || source.in_quoted_body;
    cursor.reached = Some(source.reached.as_slice());
    cursor.sublists_carry_authored_base = source.sublists_carry_authored_base;
    cursor.marker_tail = source.marker_tail;
    if extents.is_some() {
        cursor.block_extents = Some(Vec::new());
    }
    let blocks = parse_blocks(&mut cursor, options, pending);
    if let Some(extents) = extents {
        extents.extend(cursor.block_extents.take().unwrap_or_default());
    }
    blocks
}

#[inline(never)]
fn parse_eof_closed_colon_ladder(
    source: &MappedSource,
    options: &Options<'_>,
) -> Option<Vec<BlockNode>> {
    // This fast path can only match when the document starts with a flush-left
    // colon-container opener. Avoid allocating a line index and scanning every
    // line of ordinary documents merely to reject the specialization.
    if !source.source.starts_with(":::") {
        return None;
    }
    let lines: Vec<&str> = source.source.lines().collect();
    if lines.is_empty()
        || lines
            .iter()
            .any(|line| exact_colon_fence_len(line).is_some())
        || lines.iter().any(|line| {
            line.starts_with([' ', '\t'])
                && (strip_blockquote_prefix(trim_ascii_start(line)).is_some()
                    || is_definition_list_start(trim_ascii_start(line)))
        })
    {
        return None;
    }

    let mut opens = Vec::new();
    for line in &lines {
        if line.starts_with([' ', '\t']) {
            break;
        }
        let Some(open) = detect_container_open(line) else {
            break;
        };
        opens.push(open);
    }
    if opens.is_empty() {
        return None;
    }

    let current_depth = NESTING_DEPTH.with(Cell::get);
    let available = MAX_NESTING_DEPTH.saturating_sub(current_depth);
    let take = opens.len().min(available);

    // PART 9 §4c along this fast path too: the outermost bare `::: figure`
    // opener not already inside a group is a composite figure; every bare
    // opener under it stays a generic container (groups do not nest). Walked
    // outer-in, seeded from the thread state, so a ladder inside an open
    // group's body demotes exactly as the ordinary path would.
    let mut in_group = IN_FIGURE_GROUP.with(Cell::get);
    let mut opens_a_group: Vec<bool> = Vec::with_capacity(take);
    for open in opens.iter().take(take) {
        let bare = is_bare_figure_open(open) && !in_group;
        opens_a_group.push(bare);
        in_group = in_group || bare;
    }

    let tail = &lines[take..];
    let mut children = if tail.iter().all(|line| is_blank_line(line)) {
        Vec::new()
    } else {
        let _guard = in_group.then(FigureGroupGuard::enter);
        if opens.len() > available {
            flattened_paragraphs(tail, None, options)
        } else {
            let tail_source = joined_source(tail);
            parse_blocks_with_options(&tail_source, options)
        }
    };

    for (open, opens_group) in opens.into_iter().take(take).zip(opens_a_group).rev() {
        if opens_group {
            // An EOF-closed group never wrote its closer, so it has no line
            // for a caption to hang on (§4c).
            children = vec![BlockNode::FigureGroup(FigureGroup {
                attrs: open.attrs,
                children,
                caption: None,
                pos: None,
            })];
            continue;
        }
        children = vec![if let Some(kind) = open.kind {
            // CARVE-P12-057: a named container whose kind names GENERATED
            // CONTENT is a `directive`. The list is CLOSED, so `::: endnotes`
            // and `::: contents` take the admonition branch below.
            if crate::ast::is_generated_content_kind(&kind) {
                BlockNode::Directive(Directive {
                    attrs: open.attrs,
                    kind,
                    title: open
                        .title
                        .map(|title| parse_inline_with_options(&title, options)),
                    label: container_open_label(open.label, options),
                    children,
                    pos: None,
                })
            } else {
                BlockNode::Admonition(Admonition {
                    attrs: open.attrs,
                    kind,
                    title: open
                        .title
                        .map(|title| parse_inline_with_options(&title, options)),
                    label: container_open_label(open.label, options),
                    children,
                    pos: None,
                })
            }
        } else {
            BlockNode::Div(Div {
                attrs: open.attrs,
                label: container_open_label(open.label, options),
                children,
                pos: None,
            })
        }];
    }

    Some(children)
}

/// Inline-parse flattened over-cap text with the depth budget handed back.
///
/// The block and inline parsers share `NESTING_DEPTH`, so AT the cap the inline
/// pass refuses too and returns the run verbatim - escapes included, so a
/// canonical `\\:\\: x` published its backslashes where every other engine
/// publishes `:: x`, and `fmt` stopped round-tripping the corpus document that
/// reaches the cap. Flattening is the LAST step at that depth: nothing recurses
/// after it, so the inline pass can have the budget back (carve-rs#530).
fn parse_flattened_inline(text: &str, options: &Options<'_>) -> Vec<InlineNode> {
    let _depth = DepthScope::set(0);
    parse_inline_with_options(text, options)
}

/// `parse_flattened_inline` with the per-line anchors that place what it
/// returns. Same budget handling, for the same reason.
///
/// A flattened run is not REASSEMBLED in PART 12 §4's sense: its lines are
/// contiguous and verbatim, so every node in it has an honest span. Only the
/// anchors were missing (carve-rs#716).
fn parse_flattened_inline_with_anchors(
    text: &str,
    options: &Options<'_>,
    anchors: Vec<Option<(usize, isize)>>,
) -> Vec<InlineNode> {
    let _depth = DepthScope::set(0);
    parse_inline_lines_with_anchor(text, options, anchors)
}

/// The source line and stripped-column maps for a run of lines, parallel to the
/// lines themselves. `None` where the caller has none to give.
///
/// This is what a `LineCursor` carries per line; a flattened over-cap run has
/// already been lifted out of one, so its two maps travel together instead.
type LineMaps<'a> = Option<(&'a [Option<usize>], &'a [Option<isize>])>;

/// Span of the flattened lines `[start, end)`, in the ORIGINAL source.
///
/// The line-column pair per body line is what a `LineCursor` would carry; here
/// the run has already been lifted out of one, so the maps are passed directly.
/// The arithmetic is `span_of`'s, deliberately: a flattened paragraph is an
/// ordinary paragraph that happened to be built past the cap, and a second
/// spelling of the same span is how the two drift apart.
fn flattened_span(lines: &[&str], maps: LineMaps<'_>, start: usize, end: usize) -> Option<Pos> {
    let (line_map, col_map) = maps?;
    let last = end.saturating_sub(1).max(start);
    let start_line = *line_map.get(start)?.as_ref()?;
    let end_line = line_map.get(last).copied().flatten().unwrap_or(start_line);
    let stripped = *col_map.get(start)?.as_ref()?;
    let end_stripped = *col_map.get(last)?.as_ref()?;
    let first = strip_lazy(lines.get(start)?);
    let indent = first.chars().count() - trim_ascii_start(first).chars().count();
    let width = lines
        .get(last)
        .map(|l| strip_lazy(l).chars().count())
        .unwrap_or(0);
    Some(Pos {
        start_line,
        end_line,
        start_column: document_column(stripped, indent),
        end_column: document_column(end_stripped, width),
        start_offset: 0,
        end_offset: 0,
        file: None,
    })
}

/// Split flattened over-cap lines into paragraphs at every blank line (§25).
///
/// `maps` are the source line and stripped-column maps, parallel to `lines`.
/// `None` means the caller has none to give - the EOF-closed ladder builds its
/// tail from a plain slice - and the paragraphs then carry no span, which §4
/// permits only because there is genuinely nothing to report. Every caller that
/// HAS the maps passes them: a run past the cap is contiguous verbatim source,
/// so it has an honest span and omitting it is a gap, not an exemption
/// (carve-rs#716).
fn flattened_paragraphs(
    lines: &[&str],
    maps: LineMaps<'_>,
    options: &Options<'_>,
) -> Vec<BlockNode> {
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < lines.len() {
        while idx < lines.len() && is_blank_line(lines[idx]) {
            idx += 1;
        }
        if idx >= lines.len() {
            break;
        }
        let start = idx;
        while idx < lines.len() && !is_blank_line(lines[idx]) {
            idx += 1;
        }
        let text = lines[start..idx]
            .iter()
            .map(|line| strip_lazy(line))
            .collect::<Vec<_>>()
            .join("\n");
        // The lines are joined VERBATIM here, unlike `parse_paragraph` which
        // strips each line's indentation first, so a line's anchor is the
        // column its container took and nothing more.
        let children = match (options.positions, maps) {
            (true, Some((line_map, col_map))) => {
                let anchors = (start..idx)
                    .map(|i| {
                        Some((
                            line_map.get(i).copied().flatten()?,
                            col_map.get(i).copied().flatten()?,
                        ))
                    })
                    .collect();
                parse_flattened_inline_with_anchors(&text, options, anchors)
            }
            _ => parse_flattened_inline(&text, options),
        };
        out.push(BlockNode::Paragraph(Paragraph {
            attrs: None,
            children,
            // The `positions` gate is REDUNDANT today and kept deliberately.
            // `parse_mapped_source` withholds the column map from the cursor
            // unless positions were asked for, so every entry reaching here is
            // `None` in that case and `flattened_span` returns `None` on its
            // own. Removing the gate is therefore a GREEN mutation - no test
            // can catch it, because the invariant it guards is established two
            // layers up. It stays because the opt-in contract is worth stating
            // where the field is set, rather than inferred from a distant call
            // (markup-carve/carve#755: a check that cannot fail is recorded as
            // such, not deleted and not presented as proof).
            pos: options
                .positions
                .then(|| flattened_span(lines, maps, start, idx))
                .flatten(),
            ..Default::default()
        }));
    }
    out
}

fn parse_colon_view(
    view: ColonView,
    options: &Options<'_>,
    pending: &mut Vec<PendingBody>,
) -> Vec<BlockNode> {
    let source = &view.source.inner.lines[view.start..view.end];
    let materialized = std::cell::OnceCell::new();
    let lines = CursorLines::Colon {
        source,
        view: &view,
        base: view.start,
        materialized: &materialized,
    };
    #[cfg(test)]
    if !COLON_CACHE_ENABLED.with(Cell::get)
        && NESTING_DEPTH.with(|depth| depth.get() < MAX_NESTING_DEPTH)
    {
        let want_lines = options.source_lines
            || options.positions
            || needs_source_ending_map()
            || continuation_marker::active();
        let line_map: Vec<_> = source.iter().map(|line| line.source_line()).collect();
        let col_map: Vec<_> = source.iter().map(|line| line.stripped).collect();
        let mut cursor = LineCursor::from_lines(
            lines,
            want_lines.then_some(line_map.as_slice()),
            options.positions.then_some(col_map.as_slice()),
        );
        cursor.colon_view = Some(&view);
        return parse_blocks(&mut cursor, options, pending);
    }
    // A colon body read without source lines cannot ask a `+` for its decision,
    // so the plain path is only taken when there is none to ask.
    if !options.source_lines
        && !options.positions
        && !needs_source_ending_map()
        && !continuation_marker::active()
        && NESTING_DEPTH.with(|depth| depth.get() < MAX_NESTING_DEPTH)
    {
        let mut cursor = LineCursor::from_lines(lines, None, None);
        cursor.colon_view = Some(&view);
        return parse_blocks(&mut cursor, options, pending);
    }
    if NESTING_DEPTH.with(|depth| depth.get() < MAX_NESTING_DEPTH) {
        let mut cursor =
            LineCursor::from_lines(lines, Some(&[]), options.positions.then_some([].as_slice()));
        cursor.colon_view = Some(&view);
        cursor.colon_geometry = Some(source);
        return parse_blocks(&mut cursor, options, pending);
    }
    let line_map: Vec<_> = source.iter().map(|line| line.source_line()).collect();
    let col_map: Vec<_> = source.iter().map(|line| line.stripped).collect();
    if lines.iter().all(|line| is_blank_line(line)) {
        return Vec::new();
    }
    flattened_paragraphs(&lines, Some((&line_map, &col_map)), options)
}

/// For each fence char, `vec[len]` is one past the index of the LAST line that
/// is closer-shaped with a run of at least `len` - suffix-maxed so a single
/// lookup answers "at least this wide", which is what a code fence closer needs
/// (unlike a comment fence, which matches its width exactly).
fn build_code_closer_last_index(lines: &[&str]) -> HashMap<u8, Vec<usize>> {
    #[cfg(test)]
    CODE_INDEX_LINES.with(|count| count.set(count.get() + lines.len()));
    let mut per_char: HashMap<u8, Vec<usize>> = HashMap::new();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let bytes = trimmed.as_bytes();
        let Some(&fence_char) = bytes.first() else {
            continue;
        };
        if fence_char != b'`' && fence_char != b'~' {
            continue;
        }
        let run = bytes.iter().take_while(|&&b| b == fence_char).count();
        // The same tail test `is_fence_close` makes, and it has to be: `false`
        // from this index is FINAL, so a tail this prefilter rejects can never
        // be re-examined by the exact scan. A trailing tab is dropped
        // whitespace at a closer (carve#1295), so admitting it here is what
        // lets the scan below ever see the line.
        if !bytes[run..].iter().all(|&b| b == b' ' || b == b'\t') {
            continue;
        }
        let by_len = per_char.entry(fence_char).or_default();
        if by_len.len() <= run {
            by_len.resize(run + 1, 0);
        }
        by_len[run] = by_len[run].max(idx + 1);
    }
    for by_len in per_char.values_mut() {
        for len in (0..by_len.len().saturating_sub(1)).rev() {
            by_len[len] = by_len[len].max(by_len[len + 1]);
        }
    }
    per_char
}

/// Query the index built by `build_code_closer_last_index`. `false` is final;
/// `true` only means an exact scan is worth running.
fn code_closer_exists_after(
    index: &HashMap<u8, Vec<usize>>,
    start: usize,
    fence_char: u8,
    fence_len: usize,
) -> bool {
    index
        .get(&fence_char)
        .and_then(|by_len| by_len.get(fence_len).copied())
        .is_some_and(|last| last > start + 1)
}

fn build_comment_closer_last_index(lines: &[&str]) -> HashMap<usize, usize> {
    #[cfg(test)]
    COMMENT_INDEX_LINES.with(|count| count.set(count.get() + lines.len()));
    let mut last_index = HashMap::new();
    for (idx, line) in lines.iter().enumerate() {
        // Any column: the consumption sites read an indented fence, so the
        // index that tells them whether a closer exists has to see one too.
        if let Some(open) = detect_comment_fence_line_any_column(line) {
            last_index.insert(open.fence_len, idx);
        }
    }
    last_index
}

/// What `take_comment_block` found on the cursor.
enum CommentBlock {
    /// Not a comment line; the cursor has not moved.
    NotAComment,
    /// A comment line was consumed. `None` is the placeholder a collected
    /// definition leaves behind, which produces no node.
    Consumed(Option<Box<BlockNode>>),
}

/// Take a `%%` line comment or a `%%%` comment fence off the cursor.
///
/// SHARED BY BOTH BLOCK PARSERS on purpose. `parse_blocks` had this and
/// `parse_block` did not, so the same `%% c` was a block comment at top level
/// and a paragraph wrapping an inline comment when a `+` continuation marker
/// attached it - one construct, two shapes, decided by which entry point ran
/// (carve-rs#678). The rendered HTML agrees either way, since a comment renders
/// nothing, which is why only the tree showed it.
#[inline(never)]
fn take_comment_block(cur: &mut LineCursor, options: &Options<'_>) -> CommentBlock {
    let Some(line) = cur.peek() else {
        return CommentBlock::NotAComment;
    };
    if let Some(open) = detect_comment_fence_line_any_column(line) {
        // No matching closer: degrade to the ordinary `%%` line comment path
        // below instead of swallowing to EOF.
        if cur.has_comment_closer_after(cur.pos + 1, open.fence_len) {
            let span_start = cur.pos;
            let mut content = Vec::new();
            if !open.tail.is_empty() {
                content.push(open.tail);
            }
            cur.consume();
            while let Some(line) = cur.peek() {
                cur.consume();
                if is_comment_fence_close_any_column(line, open.fence_len) {
                    break;
                }
                content.push(line.to_string());
            }

            // A comment is a LEAF, so its span begins at the `%` markup, not in
            // the leading indentation - the latitude the leaf keeps was withdrawn
            // by markup-carve/carve#1928 (carve-rs#1549).
            let pos = span_of(cur, span_start, cur.pos, options);
            return CommentBlock::Consumed(Some(Box::new(BlockNode::Comment(Comment {
                block: true,
                delimited: false,
                content: content
                    .iter()
                    .map(|l| strip_lazy(l))
                    .collect::<Vec<_>>()
                    .join("\n"),
                pos,
            }))));
        }
    }
    if trim_ascii_start(line).starts_with("%%") {
        // The line a collected definition left behind is consumed here and
        // produces nothing. It did its work earlier - it was non-blank through
        // collection, so the item did not loosen - and an author never typed
        // it, so it is not a comment node. See DEFINITION_PLACEHOLDER.
        if is_definition_placeholder(line) {
            cur.consume();

            return CommentBlock::Consumed(None);
        }
        let content = trim_ascii_start(line)
            .strip_prefix("%%")
            .unwrap_or_default();
        let content = content
            .strip_prefix(' ')
            .unwrap_or(content)
            .trim_end_matches([' ', '\t'])
            .to_string();
        let span_start = cur.pos;
        cur.consume();

        // A comment leaf begins at its `%` markup (carve#1928, carve-rs#1549).
        let pos = span_of(cur, span_start, cur.pos, options);
        return CommentBlock::Consumed(Some(Box::new(BlockNode::Comment(Comment {
            block: false,
            delimited: false,
            content,
            pos,
        }))));
    }

    CommentBlock::NotAComment
}

/// The over-cap degrade, in a frame of its OWN.
///
/// PART 9 §25 caps nesting at `MAX_NESTING_DEPTH`, and this is what the cap
/// produces: everything still in the cursor, flattened into one paragraph. It
/// runs at most once per parse, at the deepest point.
///
/// It is a separate `#[inline(never)]` function because inlining it back into
/// `parse_blocks` costs stack on EVERY level, not only the one that reaches the
/// cap: LLVM reserves the slots for the `BlockNode` this branch builds in the
/// recursive hub's frame, so a cold path that runs once was charged 200 times
/// over. The cap is what bounds depth; it should not be what inflates the cost
/// of each level (markup-carve/carve-wasm#44).
#[inline(never)]
#[cold]
fn over_cap_paragraph(cur: &mut LineCursor, options: &Options<'_>) -> Vec<BlockNode> {
    let span_start = cur.pos;
    let mut rest: Vec<&str> = Vec::new();
    while let Some(line) = cur.consume() {
        rest.push(line);
    }
    let text = rest
        .iter()
        .map(|l| strip_lazy(l))
        .collect::<Vec<_>>()
        .join("\n");
    // Check line-wise: `is_blank_line` only trims spaces/tabs, so a joined
    // multi-line all-blank tail (which contains newlines) must be tested
    // per line, not on the joined string.
    if rest.iter().all(|line| is_blank_line(line)) {
        return Vec::new();
    }
    // The SECOND over-cap producer, and a real one. The colon-fence
    // document that named carve-rs#716 never reaches this branch - it
    // degrades through `parse_colon_view` instead - but a deep quote
    // ladder and a deep list ladder both arrive here with positions on, and
    // published nothing either. The lines are contiguous and still in the
    // cursor, so the span is the ordinary one and the anchors are the
    // ordinary ones; `rest` holds the lines VERBATIM, so a line's anchor is
    // its own stripped column.
    let children = if options.positions {
        let anchors = rest
            .iter()
            .enumerate()
            .map(|(idx, line)| inline_anchor_for_line(cur, span_start + idx, line))
            .collect();
        parse_flattened_inline_with_anchors(&text, options, anchors)
    } else {
        parse_flattened_inline(&text, options)
    };
    vec![BlockNode::Paragraph(Paragraph {
        attrs: None,
        children,
        pos: span_of(cur, span_start, cur.pos, options),
        ..Default::default()
    })]
}

fn delimited_payload_lines(
    cur: &LineCursor<'_>,
    start: usize,
    code: &CodeBlock,
    options: &Options<'_>,
) -> Vec<delimited_table::PayloadLine> {
    if !options.positions
        || code.content.is_empty()
        || !matches!(code.lang.as_deref(), Some("csv" | "tsv"))
    {
        return Vec::new();
    }
    let mut payload = Vec::new();
    let mut offset = 0;
    for (index, text) in code.content.split_terminator('\n').enumerate() {
        let at = start + index + 1;
        let Some(original) = cur.lines.line(at) else {
            return Vec::new();
        };
        let Some(line) = cur.source_line(at) else {
            return Vec::new();
        };
        let Some(stripped) = stripped_col(cur.source_col(at), original, text) else {
            return Vec::new();
        };
        payload.push(delimited_table::PayloadLine {
            start: offset,
            text: text.to_string(),
            line,
            column: document_column(stripped, 0),
            file_offset: None,
        });
        offset += text.len() + 1;
    }
    payload
}

/// A `csv` / `tsv` fence that carries `table` becomes a table, once its attributes are on.
pub(in crate::parse) fn promote_delimited_code(
    node: &mut BlockNode,
    cur: &LineCursor,
    start_line: usize,
    options: &Options<'_>,
) {
    match &*node {
        BlockNode::CodeBlock(code) => {
            let payload = delimited_payload_lines(cur, start_line, code, options);
            if let Some(table) = delimited_table::from_code(code, &payload) {
                *node = BlockNode::Table(table);
            }
        }
        BlockNode::Figure(figure) => {
            if let FigureTarget::CodeBlock(code) = &*figure.target {
                let mut code = code.clone();
                if let Some(attrs) = &figure.attrs {
                    let mut block = BlockNode::CodeBlock(code.clone());
                    apply_attrs_to_block(&mut block, attrs.clone());
                    if let BlockNode::CodeBlock(with_attrs) = block {
                        code = with_attrs;
                    }
                }
                let payload = delimited_payload_lines(cur, start_line, &code, options);
                if let Some(mut table) = delimited_table::from_code(&code, &payload) {
                    table.caption = Some(figure.caption.clone());
                    table.pos = figure.pos.clone();
                    *node = BlockNode::Table(table);
                }
            }
        }
        _ => {}
    }
}

#[inline(never)]
fn append_parsed_block(
    out: &mut Vec<BlockNode>,
    node: Box<BlockNode>,
    pending_attrs: &mut Option<Attrs>,
    pending_attrs_pos: &mut Option<Pos>,
    start_line: usize,
    cur: &LineCursor,
    options: &Options<'_>,
) {
    let mut node = *node;
    let renders_nothing = matches!(node, BlockNode::AbbreviationDef(_) | BlockNode::Comment(_));
    if !renders_nothing {
        if let Some(attrs) = pending_attrs.take() {
            apply_attrs_to_block(&mut node, attrs);
            *pending_attrs_pos = None;
        }
    }
    promote_delimited_code(&mut node, cur, start_line, options);
    resolve_code_title(&mut node);
    if options.source_lines {
        if let Some(line) = cur.source_line(start_line) {
            stamp_source_line(&mut node, line);
        }
    }
    out.push(node);
}

/// A colon container whose body has been COLLECTED but not yet parsed.
///
/// `collect_shared_colon_body` materializes a body into a [`LineBuffer`]
/// before anything parses it, so a level never has to suspend a half-finished
/// parse in order to descend into one. That is what makes the descent a
/// worklist rather than a recursion: [`parse_blocks`] emits the container node
/// with EMPTY children, records the body here, and runs to the end of its own
/// input; [`resolve_pending_bodies`] parses the bodies afterwards, on the
/// heap (markup-carve/carve-rs#1165).
struct PendingBody {
    /// Where the hollow node sits in the level's own output vector. Stable
    /// because the level loop only ever PUSHES - nothing shifts under it.
    index: usize,
    body: Body,
}

/// Which hollow node a [`PendingBody`] fills, and with what.
enum Body {
    /// A colon container's body. The blocks ARE the node's children.
    Colon(ContainerBody),
    /// ONE CHUNK of a list item's content. An item can have several - its own
    /// body, then a sub-list collected beside it, then another - so a chunk is
    /// APPENDED to the item rather than assigned to it, and the worklist is
    /// what puts them back in source order (see [`resolve_pending_bodies`]).
    Item(ItemBody),
}

/// A list item's content chunk, collected but not yet parsed.
///
/// The same split [`open_container`] makes for a colon body: every collector in
/// `parse_list` materializes the chunk into a [`MappedSource`] BEFORE anything
/// parses it, so the item can be built hollow and the parse can happen off the
/// stack. The recursive form parsed the chunk between those two halves and paid
/// a `parse_list` frame - 2744 bytes of it - for every nesting level
/// (markup-carve/carve-rs#1165).
struct ItemBody {
    /// Which item of the list at [`PendingBody::index`].
    item: usize,
    chunk: Chunk,
    /// `NESTING_DEPTH` as it stood where the chunk was collected, so the cap
    /// arithmetic is the arithmetic the recursive descent did.
    depth: usize,
    /// `IN_FIGURE_GROUP` for the chunk, for the same reason.
    in_group: bool,
    /// A block-attribute set held from an EARLIER chunk, for this one's first
    /// VISIBLE block. The recursive form applied it to the blocks in hand; the
    /// worklist applies it the moment they exist, which is the same block.
    attrs: Option<Box<Attrs>>,
    /// A collected definition written on the marker line is the item's first
    /// (invisible) block, and the prepass leaves an empty paragraph where its
    /// line was. The item must not carry it.
    drop_empty_paragraphs: bool,
    /// The collector refused a continuation marker in this marker-line body.
    /// That refusal does not create the L1 paragraph gap the accepted blocks
    /// alone would otherwise suggest.
    suppress_tree_looseness: bool,
    /// The list took its own span from its ITEMS rather than from the cursor
    /// (`span_across_items`), so widening the last item moves the list's end
    /// with it. `parse_list` cannot know which it used until the loop is over,
    /// so it sets this on its own entries afterwards.
    widen_list: bool,
}

/// What a chunk hands the worklist.
enum Chunk {
    /// Collected text. Nothing has looked at its blocks, so nothing has to
    /// exist yet - the ordinary case, and the one that keeps `parse_list` off
    /// the stack.
    Source(MappedSource),
    /// Blocks parsed where the chunk was collected, because the branch that
    /// built it decides the LIST's tightness from what the chunk opens with and
    /// cannot go on parsing without knowing. They come back with their own
    /// containers and lists hollow, so that parse did not descend either: the
    /// bodies it left are carried here and land in the same worklist.
    Parsed {
        blocks: Vec<BlockNode>,
        carried: Vec<PendingBody>,
    },
}

/// A range in a collected body, shared by its nested colon containers.
struct ColonView {
    source: std::rc::Rc<ColonSource>,
    start: usize,
    end: usize,
    marker_hosted: bool,
}

struct ColonSource {
    inner: LineBuffer,
    fence_closers: std::cell::OnceCell<ScopedFenceClosers>,
    // Indexed by opener line. A boundary names the closer line, or the
    // source end when unclosed. Over-cap scans leave the index empty.
    boundaries: Vec<Option<(usize, bool)>>,
}

impl ColonSource {
    fn fence_closers(&self) -> &ScopedFenceClosers {
        self.fence_closers.get_or_init(|| {
            #[cfg(test)]
            COLON_CLOSER_LINES.with(|count| count.set(count.get() + self.inner.lines.len()));
            ScopedFenceClosers::new(self.inner.lines.iter().map(|line| line.text.as_str()))
        })
    }
}

/// Collected lines and the parser state at the container opener.
struct ContainerBody {
    inner: ColonView,
    /// `NESTING_DEPTH` as it stood in the level that opened this container, so
    /// the body's cap arithmetic is the arithmetic the recursive descent did.
    depth: usize,
    /// `IN_FIGURE_GROUP` for the body. The recursive form parsed it inside the
    /// opener's [`FigureGroupGuard`], so a group's body sees `true` and every
    /// other body sees whatever its opener saw (PART 9 section 4c: groups do
    /// not nest).
    in_group: bool,
}

/// Restore `NESTING_DEPTH` to an ARBITRARY value on drop, which the RAII
/// [`DepthGuard`] cannot do: the worklist parses a body whose opening level's
/// guard has already dropped, so it has to REINSTATE that depth rather than
/// increment the current one.
struct DepthScope(usize);

impl DepthScope {
    fn set(depth: usize) -> DepthScope {
        DepthScope(NESTING_DEPTH.with(|d| d.replace(depth)))
    }
}

impl Drop for DepthScope {
    fn drop(&mut self) {
        let previous = self.0;
        NESTING_DEPTH.with(|d| d.set(previous));
    }
}

/// [`FigureGroupGuard`] for a value that is not always `true` - the worklist
/// reinstates what the opener saw rather than entering a group.
struct FigureGroupScope(bool);

impl FigureGroupScope {
    fn set(in_group: bool) -> FigureGroupScope {
        FigureGroupScope(IN_FIGURE_GROUP.with(|flag| flag.replace(in_group)))
    }
}

impl Drop for FigureGroupScope {
    fn drop(&mut self) {
        let previous = self.0;
        IN_FIGURE_GROUP.with(|flag| flag.set(previous));
    }
}

/// One body the worklist has parsed, and where its blocks belong.
struct ResolvedBody {
    /// The arena slot holding the node that takes them.
    slot: usize,
    /// The node's index within that slot.
    index: usize,
    /// `None` for a container - the body IS the node's children - and
    /// `Some((item, widen_list))` for one chunk of that item, which is appended
    /// instead. See [`append_item_children`] for the second field.
    item: Option<(usize, bool)>,
    /// The slot the blocks themselves landed in.
    child_slot: usize,
    /// A blank separated accepted paragraph-shaped blocks inside this item
    /// chunk. Computed from parsed block extents, not fence-shaped source text.
    loose_inside_chunk: bool,
    /// Recheck the gap after nested list bodies have filled their hollow nodes.
    layout: Option<ItemLayout>,
}

struct ItemLayout {
    source: String,
    opaque_blank_lines: Vec<usize>,
    discarded_outer_blank: bool,
    outer_plus_after_blank: bool,
    extents: Vec<(usize, usize)>,
}

fn trailing_verbatim_pos(mut block: &BlockNode) -> Option<&Pos> {
    loop {
        block = match block {
            BlockNode::CodeBlock(_)
            | BlockNode::RawBlock(_)
            | BlockNode::Comment(_)
            | BlockNode::LineBlock(_) => return crate::ast_json::block_pos(block),
            BlockNode::BlockQuote(node) => node.children.last()?,
            BlockNode::Div(node) => node.children.last()?,
            BlockNode::Admonition(node) => node.children.last()?,
            BlockNode::Directive(node) => node.children.last()?,
            BlockNode::FigureGroup(node) => node.children.last()?,
            BlockNode::ExtensionCarrier(node) => node.children.last()?,
            BlockNode::List(node) => node.items.last()?.children.last()?,
            BlockNode::DefinitionList(node) => {
                node.items.last()?.definitions.last()?.children.last()?
            }
            BlockNode::Figure(node) => match &*node.target {
                FigureTarget::BlockQuote(quote) => quote.children.last()?,
                _ => return None,
            },
            _ => return None,
        };
    }
}

/// Fill in the children of a node [`open_container`] emitted hollow.
fn fill_container_children(node: &mut BlockNode, children: Vec<BlockNode>) {
    if let Some(pos) = block_pos_mut(node) {
        if let Some(last) = children.last().and_then(trailing_verbatim_pos) {
            if (last.end_line, last.end_column) > (pos.end_line, pos.end_column) {
                pos.end_line = last.end_line;
                pos.end_column = last.end_column;
                pos.end_offset = last.end_offset;
            }
        }
    }
    match node {
        BlockNode::Admonition(n) => n.children = children,
        BlockNode::Directive(n) => n.children = children,
        BlockNode::Div(n) => n.children = children,
        BlockNode::FigureGroup(n) => n.children = children,
        // `open_container` builds nothing else, and the index came from it.
        _ => debug_assert!(
            false,
            "a pending body pointed at something that is not a container"
        ),
    }
}

/// Append one chunk to the item of a list node a level emitted hollow.
///
/// APPEND, not assign, and that is what makes the ordering work without a
/// sequence number: chunks of one item reach [`resolve_pending_bodies`] in
/// source order, a LIFO worklist pops them in reverse, and the stitching pass
/// walks the links backwards - so the reversal happens twice and the chunks go
/// back in the order they were written.
fn append_item_children(
    node: &mut BlockNode,
    item: usize,
    children: Vec<BlockNode>,
    widen_list: bool,
    loose_inside_chunk: bool,
) {
    let BlockNode::List(list) = node else {
        debug_assert!(
            false,
            "a pending item chunk pointed at something that is not a list"
        );
        return;
    };
    if loose_inside_chunk {
        list.tight = false;
    }
    let last = list.items.len().saturating_sub(1);
    // `parse_list` recorded the index against the items it had built.
    let Some(target) = list.items.get_mut(item) else {
        debug_assert!(false, "a pending chunk pointed past the list's items");
        return;
    };
    target.children.extend(children);
    // AND WIDEN IT AGAIN, because the blocks it is widened over only exist
    // now. `parse_list` runs the same pass before it returns, over the lead
    // paragraph an item is sometimes built with; this repeats it over every
    // chunk as the chunk lands. Both take the LATEST end among all the
    // children present, so the last one to run is the one that decides, and it
    // sees the whole item (carve#565).
    widen_item_over_children(target);
    if !widen_list || item != last {
        return;
    }
    let Some(ref end) = list.items[item].pos else {
        return;
    };
    if let Some(pos) = list.pos.as_mut() {
        pos.end_line = end.end_line;
        pos.end_column = end.end_column;
        pos.end_offset = end.end_offset;
    }
}

/// Parse one list-item chunk, under the state the collector stood in.
fn parse_item_body_into(
    body: ItemBody,
    options: &Options<'_>,
    nested: &mut Vec<PendingBody>,
) -> (Vec<BlockNode>, bool, Option<ItemLayout>) {
    let ItemBody {
        chunk,
        depth,
        in_group,
        attrs,
        drop_empty_paragraphs,
        suppress_tree_looseness,
        ..
    } = body;
    let mut loose_inside_chunk = false;
    let mut layout = None;
    let mut children = match chunk {
        Chunk::Source(source) => {
            let _depth = DepthScope::set(depth);
            let _group = FigureGroupScope::set(in_group);
            let mut extents = Vec::new();
            let children = parse_mapped_source_at_level_into_recording(
                &source,
                options,
                false,
                true,
                false,
                nested,
                Some(&mut extents),
            );
            if !suppress_tree_looseness {
                if drop_empty_paragraphs {
                    loose_inside_chunk = item_chunk_tree_looseness(
                        &source.source,
                        &source.opaque_blank_lines,
                        source.discarded_outer_blank,
                        &children,
                        &extents,
                    );
                } else {
                    layout = Some(ItemLayout {
                        source: source.source,
                        opaque_blank_lines: source.opaque_blank_lines,
                        discarded_outer_blank: source.discarded_outer_blank,
                        outer_plus_after_blank: source.outer_plus_after_blank,
                        extents,
                    });
                }
            }
            children
        }
        Chunk::Parsed { blocks, carried } => {
            nested.extend(carried);
            blocks
        }
    };
    if drop_empty_paragraphs {
        drop_empty_item_paragraphs(&mut children, nested);
    }
    apply_held_item_attrs(&mut children, attrs.map(|value| *value));
    (children, loose_inside_chunk, layout)
}

/// A blank can loosen this item only when the next accepted child is
/// paragraph-shaped. The parser's block extents keep payload blanks inside
/// their fence, so a second source-level fence classifier is unnecessary.
fn item_chunk_tree_looseness(
    source: &str,
    opaque_blank_lines: &[usize],
    discarded_outer_blank: bool,
    children: &[BlockNode],
    extents: &[(usize, usize)],
) -> bool {
    if children.len() != extents.len() {
        // The nesting-cap fallback returns flattened paragraphs before the
        // ordinary block loop can record their individual extents.
        return false;
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut previous_end = 0;
    let mut previous_start = 0;
    let mut previous_child = None;
    for (child, &(start, end)) in children.iter().zip(extents) {
        let mut opaque_trailing = 0usize;
        let mut visible_trailing = 0usize;
        for line in (previous_start..start)
            .rev()
            .take_while(|&line| lines.get(line).is_some_and(|text| is_blank_line(text)))
        {
            if opaque_blank_lines.binary_search(&line).is_ok() {
                opaque_trailing += 1;
            } else {
                visible_trailing += 1;
            }
        }
        let trailing_blank = visible_trailing > 0
            || (matches!(child, BlockNode::Paragraph(node) if node.at_content_column)
                && previous_child
                    .is_some_and(|prior| opaque_trailing > trailing_payload_blank_count(prior)))
            || (visible_trailing + opaque_trailing > 0
                && lines
                    .get(start)
                    .is_some_and(|line| trim_ascii_start(line).starts_with('+')));
        let search_start = if lines
            .get(previous_end..start)
            .is_some_and(|span| span.iter().any(|line| is_definition_placeholder(line)))
        {
            previous_start
        } else {
            previous_end
        };
        let blank_before_invisible = (search_start..start)
            .rev()
            .find(|&line| {
                !lines.get(line).is_some_and(|text| {
                    is_definition_placeholder(text) || is_line_comment_any_column(text)
                })
            })
            .is_some_and(|line| {
                lines.get(line).is_some_and(|text| is_blank_line(text))
                    && opaque_blank_lines.binary_search(&line).is_err()
            });
        if block_is_paragraph_shaped(child)
            && (trailing_blank || blank_before_invisible || discarded_outer_blank)
            && (previous_end > 0 || start > 1)
        {
            return true;
        }
        previous_start = start;
        previous_end = end;
        previous_child = Some(child);
    }
    false
}

fn trailing_payload_blank_count(mut block: &BlockNode) -> usize {
    loop {
        block = match block {
            BlockNode::CodeBlock(node) => return payload_blank_count(&node.content),
            BlockNode::RawBlock(node) => return payload_blank_count(&node.content),
            BlockNode::Comment(_) | BlockNode::LineBlock(_) => return usize::MAX,
            BlockNode::BlockQuote(node) => match node.children.last() {
                Some(child) => child,
                None => return 0,
            },
            BlockNode::Div(node) => match node.children.last() {
                Some(child) => child,
                None => return 0,
            },
            BlockNode::List(node) => {
                match node.items.last().and_then(|item| item.children.last()) {
                    Some(child) => child,
                    None => return 0,
                }
            }
            _ => return 0,
        };
    }
}

fn payload_blank_count(content: &str) -> usize {
    let trailing = content
        .bytes()
        .rev()
        .take_while(|&byte| byte == b'\n')
        .count();
    trailing.saturating_sub(usize::from(trailing < content.len()))
}

fn mark_nested_discarded_gap(children: &mut [BlockNode]) -> bool {
    for block in children.iter_mut().rev() {
        let BlockNode::List(list) = block else {
            continue;
        };
        let Some(item) = list.items.last_mut() else {
            continue;
        };
        if item.children.len() > 1 && item.children.last().is_some_and(block_is_paragraph_shaped) {
            list.tight = false;
            return true;
        }
        if mark_nested_discarded_gap(&mut item.children) {
            return true;
        }
    }
    false
}

/// The empty paragraph a collected definition leaves behind, dropped - and the
/// chunk's own pending bodies renumbered, because dropping a block shifts every
/// index recorded after it.
fn drop_empty_item_paragraphs(children: &mut Vec<BlockNode>, nested: &mut [PendingBody]) {
    fn is_empty_paragraph(block: &BlockNode) -> bool {
        matches!(block, BlockNode::Paragraph(p) if p.children.is_empty())
    }
    if !children.iter().any(is_empty_paragraph) {
        return;
    }
    let mut moved_to = Vec::with_capacity(children.len());
    let mut kept = 0usize;
    for block in children.iter() {
        moved_to.push(kept);
        if !is_empty_paragraph(block) {
            kept += 1;
        }
    }
    children.retain(|block| !is_empty_paragraph(block));
    for body in nested.iter_mut() {
        // An empty paragraph holds no body, so the slot a dropped block would
        // have mapped to is never read.
        body.index = moved_to[body.index];
    }
}

/// A held attribute set reaches the chunk's first VISIBLE block - the same
/// block the recursive form picked, chosen by node KIND, which a hollow node
/// still reports correctly.
fn apply_held_item_attrs(children: &mut [BlockNode], attrs: Option<Attrs>) {
    let Some(attrs) = attrs else {
        return;
    };
    if let Some(target) = children
        .iter_mut()
        .find(|block| !matches!(block, BlockNode::Comment(_)))
    {
        apply_attrs_to_block(target, attrs);
    }
}

/// Collect one list-item chunk for the worklist, and hand the caller the empty
/// vector that stands in for the blocks until it runs.
///
/// Every `parse_list` site reads the same: build the chunk's source, ask for
/// its blocks, put them in the item. What changed is that the blocks arrive
/// later - `parse_list` never descends into one, so a nesting level costs a
/// heap entry rather than the 2744-byte frame the three extraction attempts on
/// markup-carve/carve-rs#1165 could not shrink.
fn item_body(
    deferred: &mut Vec<ItemBody>,
    item: usize,
    source: MappedSource,
    attrs: Option<Attrs>,
    drop_empty_paragraphs: bool,
    options: &Options<'_>,
) -> Vec<BlockNode> {
    item_body_with_layout_suppression(
        deferred,
        item,
        source,
        attrs,
        drop_empty_paragraphs,
        false,
        options,
    )
}

fn item_body_with_layout_suppression(
    deferred: &mut Vec<ItemBody>,
    item: usize,
    source: MappedSource,
    attrs: Option<Attrs>,
    drop_empty_paragraphs: bool,
    suppress_tree_looseness: bool,
    _options: &Options<'_>,
) -> Vec<BlockNode> {
    deferred.push(ItemBody {
        item,
        chunk: Chunk::Source(source),
        depth: NESTING_DEPTH.with(Cell::get),
        in_group: IN_FIGURE_GROUP.with(Cell::get),
        attrs: attrs.map(Box::new),
        drop_empty_paragraphs,
        suppress_tree_looseness,
        widen_list: false,
    });
    Vec::new()
}

/// Parse one chunk of the item the loop is standing on, WITH ITS BLOCKS IN HAND.
///
/// Two branches have to read the chunk's blocks before they can carry on
/// parsing: whether the first visible one is a paragraph decides the list's
/// tightness, and whether they all render nothing decides whether the blank
/// line above them survives. Neither can be deferred, because both steer the
/// parsing that follows.
///
/// Neither has to be, either. Every question they ask is a question about a
/// node's KIND, which a node with hollow children answers exactly, so the chunk
/// parses through the level-into entry: its own containers and lists go onto
/// the worklist rather than down the stack, and what comes back is a list of
/// correctly-typed blocks plus the bodies still waiting on them.
fn parse_item_chunk(
    source: &MappedSource,
    options: &Options<'_>,
    carried: &mut Vec<PendingBody>,
) -> (Vec<BlockNode>, bool) {
    let mut extents = Vec::new();
    if !source
        .source
        .lines()
        .any(|line| line.starts_with([' ', '\t']))
    {
        let children = parse_mapped_source_at_level_into_recording(
            source,
            options,
            false,
            true,
            false,
            carried,
            Some(&mut extents),
        );
        let loose = item_chunk_tree_looseness(
            &source.source,
            &source.opaque_blank_lines,
            source.discarded_outer_blank,
            &children,
            &extents,
        );
        return (children, loose);
    }
    let children = parse_mapped_source_at_level_into_recording(
        source,
        options,
        false,
        true,
        false,
        carried,
        Some(&mut extents),
    );
    let loose = item_chunk_tree_looseness(
        &source.source,
        &source.opaque_blank_lines,
        source.discarded_outer_blank,
        &children,
        &extents,
    );
    (children, loose)
}

fn try_extension_block(cur: &mut LineCursor, options: &Options<'_>) -> Option<BlockNode> {
    if options.extensions.is_empty() {
        return None;
    }
    let ctx = MatcherContext::new(options);
    for ext in &options.extensions {
        if let Some(BlockMatch {
            node,
            lines_consumed,
        }) = ext.match_block(&cur.lines, cur.pos, &ctx)
        {
            if lines_consumed == 0 || cur.pos + lines_consumed > cur.lines.len() {
                continue;
            }
            cur.pos += lines_consumed;
            return Some(node);
        }
    }
    None
}

/// A footnote body's continuation floor is a COLUMN count.
///
/// `footnote_body_floor` is compared against `indent_columns` at both of its
/// call sites, so it has to speak the same unit. It was computed with
/// `leading_ws`, a CHARACTER count, and a tab in the definition line's own
/// indentation made the two sides disagree.
///
/// These assertions are on the function rather than on the engine's output, and
/// deliberately so: no input reaches the site today (see the function's own
/// note), so a test written against rendered HTML or a published AST would be a
/// check that cannot fail - the class catalogued in markup-carve/carve#755. The
/// ticket asked for exactly this: fix the unit, and prove it with a case where
/// the two spellings differ on a tabbed input.
pub(crate) fn lint_opens_code_fence(line: &str) -> bool {
    detect_fence_open(line).is_some()
}
/// Byte ranges of a table row's cells within `row`, split with the parser's own
/// pipe rules (escapes and verbatim runs do not split).
pub(crate) fn lint_table_cell_ranges(row: &str) -> Vec<(usize, usize)> {
    let (_, body) = split_row_attrs(row);
    let content = strip_row_closing_pipe(body.strip_prefix('|').unwrap_or(body));
    let prefix = content.as_ptr() as usize - row.as_ptr() as usize;
    let offsets: Vec<_> = content
        .char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(content.len()))
        .collect();
    split_table_cells_ranged(content)
        .cells
        .into_iter()
        .map(|cell| (prefix + offsets[cell.start], prefix + offsets[cell.end]))
        .collect()
}

pub(crate) fn lint_reversed_cell_markers(line: &str) -> Vec<(usize, usize)> {
    if !is_table_start(line) {
        return Vec::new();
    }
    let (_, body) = split_row_attrs(line);
    let content = strip_row_closing_pipe(body.strip_prefix('|').unwrap_or(body));
    let prefix = content.as_ptr() as usize - line.as_ptr() as usize;
    let offsets: Vec<_> = content
        .char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(content.len()))
        .collect();
    split_table_cells_ranged(content)
        .cells
        .into_iter()
        .filter_map(|cell| {
            let start = offsets[cell.start];
            let end = offsets[cell.end];
            let raw = &content[start..end];
            let (_, close) = read_attrs_at(raw.as_bytes(), 0, raw.rfind('}'))?;
            raw.as_bytes()
                .get(close)
                .filter(|&&b| b"<>~^v?".contains(&b))
                .map(|_| (prefix + start, close + 1))
        })
        .collect()
}

/// Which part of a cell's marker run the padding rule points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnpaddedCellRun {
    Kind,
    Align,
    Attrs,
}

/// Cells whose marker run the parser reads but T11 rejects for want of a
/// space, as `(start, end, part)` byte ranges into `line`. A kind marker
/// followed by a rejected alignment glyph (`=^`, `=<<`) is literal for that
/// reason instead, and a reversed `{...}<` run belongs to its own advice.
pub(crate) fn lint_unpadded_cell_runs(line: &str) -> Vec<(usize, usize, UnpaddedCellRun)> {
    if !is_table_start(line) {
        return Vec::new();
    }
    let (_, body) = split_row_attrs(line);
    let content = strip_row_closing_pipe(body.strip_prefix('|').unwrap_or(body));
    let prefix = content.as_ptr() as usize - line.as_ptr() as usize;
    let offsets: Vec<_> = content
        .char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(content.len()))
        .collect();
    let glyph = |b: Option<&u8>| b.is_some_and(|b| b"<>~^v?".contains(b));
    split_table_cells_ranged(content)
        .cells
        .into_iter()
        .filter_map(|cell| {
            let at = offsets[cell.start];
            let raw = &content[at..offsets[cell.end]];
            let run = scan_cell_marker_run(raw);
            let bytes = raw.as_bytes();
            if run.run_len == 0 || bytes.get(run.run_len) == Some(&b' ') {
                return None;
            }
            if run.header && run.align_run.is_none() && glyph(bytes.get(1)) {
                return None;
            }
            if bytes.first() == Some(&b'{') && glyph(bytes.get(run.run_len)) {
                return None;
            }
            let at = prefix + at;
            let (start, end, part) = if let Some((s, e)) = run.align_run {
                (s, e, UnpaddedCellRun::Align)
            } else if run.header {
                (0, 1, UnpaddedCellRun::Kind)
            } else {
                let (s, e) = run.attr_span?;
                (s, e, UnpaddedCellRun::Attrs)
            };
            Some((at + start, at + end, part))
        })
        .collect()
}

#[cfg(test)]
thread_local! {
    static INNERMOST_CONTENT_SCANS: Cell<usize> = const { Cell::new(0) };
    static INNERMOST_MARKER_STEPS: Cell<usize> = const { Cell::new(0) };
    static REUSE_MARKER_TAIL: Cell<bool> = const { Cell::new(true) };
}

pub(crate) fn carrier_next_is_list_marker(line: &str) -> bool {
    detect_list_marker_full(trim_ascii_start(line)).is_some()
}
