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
mod container;
mod definition;
mod footnote_prepass;
mod inline;
mod inline_links;
mod inline_resolution;
mod list;
mod paragraph;
mod prepass;
mod quote;
mod rebase;
mod table;
mod verse;
pub(crate) use attrs::*;
pub(crate) use block::*;
pub(crate) use container::*;
use definition::*;
use footnote_prepass::*;
pub(crate) use inline::*;
pub(crate) use inline_links::*;
pub(crate) use inline_resolution::*;
use list::*;
use paragraph::*;
use prepass::*;
use quote::*;
use rebase::*;
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

#[cfg(test)]
mod footnote_comment_fence_work {
    use super::FOOTNOTE_FENCE_CLOSER_LOOKUPS;

    #[test]
    fn repeated_note_comments_use_bounded_index_lookups() {
        for count in [32, 64, 128] {
            let mut source = String::new();
            for index in 0..count {
                source.push_str(&format!(
                    "[^n{index}]: note\n%% comment\n```\ncode\n```\n\n"
                ));
            }
            FOOTNOTE_FENCE_CLOSER_LOOKUPS.with(|work| work.set(0));
            let html = crate::to_html(&source);
            assert_eq!(html.matches("<pre><code>").count(), count);
            let lookups = FOOTNOTE_FENCE_CLOSER_LOOKUPS.with(|work| work.get());
            assert!(lookups >= count as u64, "{count}: {lookups} lookups");
            assert!(lookups <= count as u64 * 2, "{count}: {lookups} lookups");
        }
    }
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
    static LINT_EVENTS: RefCell<Option<Vec<LintEvent>>> = const { RefCell::new(None) };
}

#[derive(Clone)]
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
struct SourceEndingGuard(Option<usize>);

impl SourceEndingGuard {
    fn install(source: &str) -> Self {
        let last = (!source.is_empty() && !source.ends_with('\n')).then(|| source.lines().count());
        Self(UNTERMINATED_SOURCE_LINE.with(|slot| slot.replace(last)))
    }

    fn fragment() -> Self {
        Self(UNTERMINATED_SOURCE_LINE.with(|slot| slot.replace(None)))
    }
}

impl Drop for SourceEndingGuard {
    fn drop(&mut self) {
        UNTERMINATED_SOURCE_LINE.with(|slot| slot.set(self.0));
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

pub fn parse(source: &str) -> Document {
    parse_with_options(source, &Options::default())
}

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

/// A body line with the LAZY frame removed.
///
/// EVERY SITE THAT TURNS A LINE INTO TEXT MUST CALL THIS, and the ones that
/// rebuild a source string for re-parsing must NOT: stripping there erases the
/// frame before it has done its job, which fails as a wrong answer rather than
/// as a visible sentinel. `tests/lazy_framing_never_leaks.rs` generates the
/// shapes that tell the two apart rather than listing them.
pub(crate) fn strip_lazy(line: &str) -> &str {
    line.strip_prefix(LAZY).unwrap_or(line)
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
    // Kept for the offset table below: normalization rewrites the text the
    // parser sees, and PART 12 §4 positions index the ORIGINAL file.
    let original = source;
    let normalized = normalize_source(source);
    let source = normalized.as_ref();
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
                    authored_base_at_start: false,
                    item_marker_at_start: false,
                    marker_colon_rebased: false,
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
        // A resolved reference image lands as a one-image paragraph (the
        // syntactic block-image check ran before resolution); promote it to a
        // block image like a standalone direct image, matching carve-php.
        if source.contains("![") {
            promote_block_images(&mut doc.children, false, true);
            for blocks in doc.footnote_defs.values_mut() {
                promote_block_images(blocks, false, true);
            }
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
        let mut body = compose_mapped_source(body, &parent);
        rebase_body_blocks(&mut body, true, options, false);
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

/// The content column of the innermost list item a line-based prepass is inside.
///
/// Both definition prepasses need it for the same reason: a definition on an
/// item's CONTINUATION line carries no marker, so `strip_container_prefixes`
/// leaves the item's indentation in front of the `[` and the line stops looking
/// like a definition. Stripping exactly this many columns - never more - is what
/// separates a definition AT the content column (collected) from one below it
/// (paragraph text that registers nothing, PART 9 §24 C3 as carve#624 states it).
///
/// A line-based approximation, as the note in `extract_link_defs` says: tab-vs-
/// space marker alignment is char-counted, the post-blank `baseIndent + 2` rule
/// is not modeled, and lists inside blockquotes are not fully modeled.
#[derive(Default)]
struct ContentColumns {
    /// One frame per open BLOCKQUOTE level, `frames[0]` being document level.
    /// Each frame holds the list content columns live inside that container.
    ///
    /// A flat stack could not answer this: cases 1 and 2 of carve-rs#593 need an
    /// item's columns DISCARDED when a quote opens under it, cases 3 and 4 need
    /// them KEPT across a quote nested at the item's own column - and the indent
    /// a flat stack compares against is in a different coordinate system
    /// depending on how deep the quote nesting is. Scoping by container answers
    /// "which columns are live" from structure instead of from an indent
    /// measured in whichever coordinate the caller happened to strip to.
    frames: Vec<ColumnFrame>,
    prev_blank: bool,
}

/// One container's live content columns, plus the definition list open inside
/// it.
///
/// A DEFINITION BODY IS A CONTAINER WITH A CONTENT COLUMN, like a list item
/// (PART 9 §24 C3, markup-carve/carve#1350). The columns alone could not say so:
/// a `:  ` line opens a body whose content column is three past the marker, and
/// a definition written there is that body's block - so it registers, and it
/// interrupts. Without the open list recorded, a bare `:  x` line anywhere would
/// claim a column that no `dd` was ever opened at.
#[derive(Default)]
struct ColumnFrame {
    cols: Vec<usize>,
    list_columns: Vec<(usize, usize)>,
    first_list_col: Option<(usize, usize)>,
    first_list_start: Option<usize>,
    /// The indent of the innermost OPEN definition list, set by its `:: ` term.
    def_list: Option<usize>,
}

impl ColumnFrame {
    fn pop_column(&mut self) {
        let popped = self.cols.pop();
        if let Some(column) = popped {
            if self
                .list_columns
                .last()
                .is_some_and(|(_, floor)| *floor == column)
            {
                self.list_columns.pop();
            }
        }
        if popped == self.first_list_col.map(|(_, column)| column) {
            self.first_list_col = None;
            self.first_list_start = None;
        }
    }
}

impl ContentColumns {
    fn new() -> Self {
        Self {
            frames: vec![ColumnFrame::default()],
            prev_blank: true,
        }
    }

    /// Feed the next RAW line (quote markers included). `opaque` suppresses
    /// tracking inside a fence or a line block, where `- verse` is content
    /// rather than a marker and a `>` is text rather than a container.
    ///
    /// THE COLUMN IS REACHED BY COMPOSING THE STRIPS, NOT BY WALKING THE
    /// PREFIX (PART 1 S4). A `>` written at an item's content column is that
    /// item's container prefix exactly as a flush-left one is, so the walk
    /// peels ONE level at a time and records each level's markers in that
    /// level's own frame. Counting only the flush-left `>` runs left every
    /// list opened inside an indented quote invisible: `- > - - x` recorded
    /// the outer item and nothing else, so a definition at the inner item's
    /// content column read as lazy text - while the same body with one
    /// container peeled off (`> - - x`) registered (carve-rs#1096).
    fn observe(&mut self, raw_line: &str, opaque: bool) -> usize {
        self.observe_indexed(raw_line, opaque, None)
    }

    fn observe_indexed(
        &mut self,
        raw_line: &str,
        opaque: bool,
        line_index: Option<usize>,
    ) -> usize {
        let bare = without_blockquote_prefixes(raw_line);
        let was_prev_blank = self.prev_blank;
        self.prev_blank = trim_ascii(bare).is_empty();
        if opaque {
            return self.current().cols.last().copied().unwrap_or(0);
        }
        // A blank line opens no container, so it is fed to a frame as it stands
        // rather than walked. Whether it CLOSES one depends on how it is
        // written, and the two spellings part here.
        //
        // A BARE blank ends every open quote, and every column opened inside one
        // dies with it (PART 0, A NEW MARKER DOES NOT REACH A DEAD CONTAINER'S
        // COLUMN; carve#1892). The parser already reads it that way - `> - x`
        // over a blank over `>   [r]: /u` renders as two separate quotes - but
        // the frames outlived the quote that held them, so the second `>` line
        // reached the dead item's column, and its definition registered
        // document-wide while the page printed it as ordinary text. I5 permits
        // neither reading to do both. Level 0 is document level and survives:
        // a list item IS transparent across a blank.
        //
        // A QUOTE-MARKED blank (`>`) closes nothing - it is the quote's own
        // continuation - so its frames stand.
        if trim_ascii(bare).is_empty() {
            if trim_ascii(raw_line).is_empty() {
                self.frames.truncate(1);
            }
            let level = self.frames.len() - 1;
            self.observe_segment(level, bare, was_prev_blank, line_index);
            return self.current().cols.last().copied().unwrap_or(0);
        }
        if strip_blockquote_prefix(raw_line).is_some() && !self.frames[0].cols.is_empty() {
            self.frames.truncate(1);
            self.frames[0] = ColumnFrame::default();
        }
        let mut rest = raw_line;
        let mut level = 0usize;
        loop {
            while let Some(inner) = strip_blockquote_prefix(rest) {
                rest = inner;
                level += 1;
            }
            while self.frames.len() <= level {
                self.frames.push(ColumnFrame::default());
            }
            let prior_columns = self.frames[level].cols.len();
            self.observe_segment(level, rest, was_prev_blank, line_index);
            if self.frames[level].cols.len() < prior_columns {
                self.frames.truncate(level + 1);
            }
            let Some(offset) = self.next_quote_offset(level, rest) else {
                break;
            };
            rest = &rest[offset..];
        }
        self.frames.truncate(level + 1);
        self.current().cols.last().copied().unwrap_or(0)
    }

    /// Where this level hands off to the NEXT container level, if it does: the
    /// byte offset of a `>` that is this level's own content column rather than
    /// text sitting in it.
    ///
    /// Two spellings reach the same column and both are container prefixes.
    /// `- > x` writes the quote straight after the marker, where the item's
    /// content begins. `  > x` writes it at the column that marker established,
    /// which is the spelling every CONTINUATION line uses - and the one a
    /// flush-left walk can never see.
    fn next_quote_offset(&self, level: usize, line: &str) -> Option<usize> {
        let mut inside = line;
        let mut consumed_marker = false;
        while let Some(marker) = detect_list_marker_full(inside) {
            if marker.content.as_ptr() as usize <= inside.as_ptr() as usize {
                break;
            }
            inside = marker.content;
            consumed_marker = true;
        }
        if consumed_marker {
            // The item's content begins exactly here, so a `>` at this position
            // is written AT the content column.
            return inside
                .starts_with('>')
                .then(|| inside.as_ptr() as usize - line.as_ptr() as usize);
        }
        // A description's marker line (`: > - para`) hands off the same way.
        if let Some((indent, width)) =
            detect_prepass_def_body(line).filter(|(at, _)| self.frames[level].def_list == Some(*at))
        {
            let offset = indent + width;
            return line[offset..].starts_with('>').then_some(offset);
        }
        // EXACTLY a live content column, never past one: an indented `>` that
        // reaches no open item is ordinary text, and the block parser reads it
        // that way too (carve-rs#1082).
        let content_col = self.reached_by_at(level, leading_ws(line));
        let bytes = line.as_bytes();
        (content_col > 0
            && bytes.len() > content_col
            && bytes[..content_col].iter().all(|b| *b == b' ')
            && bytes[content_col] == b'>')
            .then_some(content_col)
    }

    /// Record ONE container level's own markers. `line` is the raw line with
    /// every prefix up to and including this level's quote markers removed, so
    /// its columns are measured inside that container.
    fn observe_segment(
        &mut self,
        level: usize,
        line: &str,
        was_prev_blank: bool,
        line_index: Option<usize>,
    ) {
        // Ownership is measured in visual columns. A leading tab reaches
        // column four and must not close a definition body at column three.
        let indent = indent_columns(line);
        let raw_trimmed = trim_ascii(line);
        if line_index.is_some()
            && !raw_trimmed.is_empty()
            && self.frames[level]
                .first_list_col
                .is_some_and(|(base, floor)| {
                    indent < base || (indent < floor && !is_list_marker(raw_trimmed))
                })
        {
            self.frames[level].first_list_col = None;
            self.frames[level].first_list_start = None;
        }
        let starts_block = is_heading_marker_line(raw_trimmed)
            || standalone_attrs_block_len(&[raw_trimmed]).is_some()
            || raw_trimmed.starts_with('>')
            || detect_fence_open(raw_trimmed).is_some()
            || detect_thematic_break(raw_trimmed)
            // THE TABLE, which §24 C3 names in the same block-opener set as the
            // rest of this list and which was the one member missing. A row at
            // the document column ends the list above it, so the columns that
            // list opened are dead - left live, `* }` / `|b|` / `  [f]: t`
            // collected a definition at a column nothing reaches any more, and
            // the author's line vanished from the output (carve-rs#2096).
            || is_table_start(raw_trimmed);
        if let Some(term_indent) = detect_prepass_def_term(line) {
            // A term opens the list (or re-opens it at a new indent). Its own
            // line has no content column: `:: t` holds the TERM, and only the
            // `:  ` line below it opens a body.
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > term_indent) {
                frame.pop_column();
            }
            frame.def_list = Some(term_indent);
        } else if let Some((body_indent, marker_width)) =
            detect_prepass_def_body(line).filter(|(at, _)| self.frames[level].def_list == Some(*at))
        {
            // `:  ` is three columns wide, so the body's content begins three
            // past the marker - the same arithmetic the collector does when it
            // slices the continuation lines it takes.
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > body_indent) {
                frame.pop_column();
            }
            frame.cols.push(body_indent + marker_width);
        } else if let Some((marker_indent, marker_width)) = detect_prepass_list_marker(line) {
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > marker_indent) {
                frame.pop_column();
            }
            // One line can open SEVERAL items: `- - a` opens an outer item
            // whose content is another item, so BOTH content columns are
            // live under it (2 and 4). Recording only the outer one left a
            // definition at the INNER column looking like text, so it
            // registered nothing here while carve-js and carve-php read it
            // as that item's block (carve#655).
            let mut offset = marker_width;
            frame.cols.push(offset);
            if level > 0 {
                frame.list_columns.push((marker_indent, offset));
            }
            if frame.first_list_col.is_none() {
                frame.first_list_start = line_index;
            }
            frame.first_list_col.get_or_insert((marker_indent, offset));
            while let Some((nested_indent, nested_width)) =
                detect_prepass_list_marker(&line[offset..])
            {
                if nested_indent != 0 {
                    break;
                }
                let marker_column = offset;
                offset += nested_width;
                self.frames[level].cols.push(offset);
                if level > 0 {
                    self.frames[level]
                        .list_columns
                        .push((marker_column, offset));
                }
            }
        } else if !raw_trimmed.is_empty() && (was_prev_blank || starts_block) {
            let frame = &mut self.frames[level];
            while frame.cols.last().is_some_and(|col| *col > indent) {
                frame.pop_column();
            }
            // A line that dedents to or past the term's own column has left the
            // definition list, so the next `:  ` line down there opens no body
            // until another term does.
            if frame.def_list.is_some_and(|at| indent <= at) {
                frame.def_list = None;
            }
        }
    }

    fn current(&self) -> &ColumnFrame {
        self.frames
            .last()
            .expect("the document frame is never popped")
    }

    /// The content column of the open item a line at `indent` actually reaches:
    /// the deepest one at or below it, or 0 when it reaches none.
    ///
    /// Not always the innermost. Under `- - a` a definition written at column 2
    /// belongs to the outer item, and one written between the two live columns
    /// belongs to the outer of them - the deeper item never opened on that
    /// line, so it cannot own it (markup-carve/carve#1896).
    ///
    /// Only the INNERMOST container's columns are consulted: a column measured
    /// outside the quote a line sits in is not a column that line can reach.
    fn reached_by(&self, indent: usize) -> usize {
        self.reached_by_at(self.frames.len() - 1, indent)
    }

    /// `reached_by`, asked of a NAMED container level rather than the innermost
    /// one. The composed walk needs it: a `>` written at an outer item's content
    /// column is measured against THAT item's columns, and the innermost frame's
    /// columns are in a different coordinate system entirely.
    fn reached_by_at(&self, level: usize, indent: usize) -> usize {
        self.frames
            .get(level)
            .and_then(|frame| {
                frame
                    .cols
                    .iter()
                    .copied()
                    .filter(|col| *col <= indent)
                    .max()
            })
            .unwrap_or(0)
    }
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
    let mut cursor = LineCursor::new_with_cols(
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
    let blocks = parse_blocks(&mut cursor, options, pending);
    (
        blocks,
        ProbeRootBoundary {
            start: cursor.last_block_start,
            closed_initial_container_end: cursor.closed_initial_container_end,
        },
    )
}

fn needs_quote_marker_line_map(source: &str) -> bool {
    if !source.contains('+') {
        return false;
    }
    let mut previous_quote = false;
    for line in source.lines() {
        if previous_quote && is_plus_marker(trim_ascii_start(line)) {
            return true;
        }
        previous_quote = strip_blockquote_prefix(trim_ascii_start(line)).is_some()
            || detect_list_marker_full(line)
                .is_some_and(|marker| strip_blockquote_prefix(marker.content).is_some());
    }
    false
}

/// Where the block parser starts DOCUMENT-LEVEL blocks in `lines`, with no
/// block attribute pending, plus the end of a closed container opening on line 0.
///
/// Container and item bodies are collected but never parsed: a top-level block
/// boundary is decided before any body is, so these are the parser's own.
struct DocumentBlockStarts {
    starts: Vec<usize>,
    closed_initial_container_end: Option<usize>,
}

fn document_block_starts<'a>(lines: &'a [&'a str], options: &Options<'_>) -> DocumentBlockStarts {
    let _recording = definition_regions::RecordingPause::enter();
    let _probing = ProbeGuard::enter();
    let _source_ending = SourceEndingGuard::fragment();
    let want_lines = options.source_lines || options.positions || needs_source_ending_map();
    let line_map: Vec<Option<usize>> = if want_lines {
        (1..=lines.len()).map(Some).collect()
    } else {
        Vec::new()
    };
    let col_map: Vec<Option<isize>> = if options.positions {
        vec![Some(0); lines.len()]
    } else {
        Vec::new()
    };
    let mut cursor = LineCursor::new_with_cols(
        lines,
        want_lines.then_some(line_map.as_slice()),
        options.positions.then_some(col_map.as_slice()),
    );
    cursor.at_document_level = true;
    cursor.reached = Some(&[]);
    cursor.block_starts = Some(Vec::new());
    parse_blocks(&mut cursor, options, &mut Vec::new());
    DocumentBlockStarts {
        starts: cursor.block_starts.take().unwrap_or_default(),
        closed_initial_container_end: cursor.closed_initial_container_end,
    }
}

#[derive(Clone, Copy)]
enum CursorLines<'a> {
    Slice(&'a [&'a str]),
    Colon {
        source: &'a [SourceLine],
        view: &'a ColonView,
        base: usize,
        materialized: &'a std::cell::OnceCell<Vec<&'a str>>,
    },
}

impl<'a> CursorLines<'a> {
    fn len(self) -> usize {
        match self {
            Self::Slice(lines) => lines.len(),
            Self::Colon { source, .. } => source.len(),
        }
    }

    fn at(self, index: usize) -> &'a str {
        self.line(index).expect("cursor line index")
    }

    fn range(self, range: std::ops::Range<usize>) -> impl DoubleEndedIterator<Item = &'a str> {
        range.map(move |index| self.at(index))
    }

    fn is_empty(self) -> bool {
        self.len() == 0
    }

    fn window<'b>(
        self,
        range: std::ops::Range<usize>,
        materialized: &'b std::cell::OnceCell<Vec<&'b str>>,
    ) -> CursorLines<'b>
    where
        'a: 'b,
    {
        match self {
            Self::Slice(lines) => CursorLines::Slice(&lines[range]),
            Self::Colon {
                source, view, base, ..
            } => CursorLines::Colon {
                source: &source[range.clone()],
                view,
                base: base + range.start,
                materialized,
            },
        }
    }

    fn colon_end(self, start: usize) -> Option<usize> {
        if !reuse_colon_views() {
            return None;
        }
        let Self::Colon {
            source, view, base, ..
        } = self
        else {
            return None;
        };
        let &(end, closed) = view.source.boundaries.get(base + start)?.as_ref()?;
        (end <= base + source.len() && (!closed || end < base + source.len()))
            .then_some(end - base + usize::from(closed))
    }

    fn code_after(self, start: usize, character: u8, width: usize) -> Option<bool> {
        if !reuse_colon_views() {
            return None;
        }
        match self {
            Self::Slice(_) => None,
            Self::Colon {
                source, view, base, ..
            } => Some(view.source.fence_closers().code_in(
                base + start + 1..base + source.len(),
                character,
                width,
            )),
        }
    }

    fn comment_after(self, start: usize, width: usize) -> Option<bool> {
        if !reuse_colon_views() {
            return None;
        }
        match self {
            Self::Slice(_) => None,
            Self::Colon {
                source, view, base, ..
            } => Some(
                view.source
                    .fence_closers()
                    .comment_in(base + start..base + source.len(), width),
            ),
        }
    }

    fn line(self, index: usize) -> Option<&'a str> {
        match self {
            Self::Slice(lines) => lines.get(index).copied(),
            Self::Colon { source, .. } => source.get(index).map(|line| line.text.as_str()),
        }
    }
}

impl<'a> std::ops::Deref for CursorLines<'a> {
    type Target = [&'a str];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Slice(lines) => lines,
            Self::Colon {
                source,
                materialized,
                ..
            } => materialized.get_or_init(|| {
                #[cfg(test)]
                COLON_REFERENCE_ENTRIES.with(|count| count.set(count.get() + source.len()));
                source.iter().map(|line| line.text.as_str()).collect()
            }),
        }
    }
}

struct LineCursor<'a> {
    colon_view: Option<&'a ColonView>,
    colon_geometry: Option<&'a [SourceLine]>,
    marker_tail: Option<MarkerTail>,
    lines: CursorLines<'a>,
    line_map: Option<&'a [Option<usize>]>,
    /// Columns already stripped from the front of each line by an enclosing
    /// container, so a nested strip accumulates rather than resetting. `None`
    /// for a line whose stripped width is not known - a block starting there
    /// gets no position rather than a wrong one.
    col_map: Option<&'a [Option<isize>]>,
    pos: usize,
    last_block_start: Option<usize>,
    /// Every line this cursor started a block on with no block attributes
    /// pending, recorded only for [`document_block_starts`].
    block_starts: Option<Vec<usize>>,
    closed_initial_container_end: Option<usize>,
    at_document_level: bool,
    in_quoted_body: bool,
    /// True for the cursor reading a LIST ITEM's own body, and that body only.
    in_item_body: bool,
    /// Whether the INVISIBLE arms of `interrupts_paragraph` are live for the
    /// question being asked. True everywhere except a container's BELOW-COLUMN
    /// band, where §10 I5 makes a block-attribute line lazy paragraph text of
    /// that container rather than a floating attribute (§10 I5 DEFINITION
    /// OWNERSHIP IS COLUMN-SCOPED, markup-carve/carve#1809).
    ///
    /// PASSED, NOT INHERITED, for the reason `in_item_body` gives: it describes
    /// one question about one line, not the region. Only
    /// `interrupts_paragraph_in_band` clears it, and it restores it.
    invisible_arms: bool,
    /// Did each line of the source this cursor reads REACH the content column
    /// of the container that collected it? `None` at the document level and
    /// wherever the answer was not carried down.
    ///
    /// THE COLLECTOR'S DEDENT DESTROYS THE COLUMN, so a nested re-parse cannot
    /// recompute it: a line one column past a container's column and a line
    /// below it both arrive with a single residual column. `MappedSource`
    /// records the answer per line and this is where the nested parse reads it
    /// (markup-carve/carve-rs#1518, the same fact markup-carve/carve#1896 added
    /// `MappedSource::reached` for one layer up).
    reached: Option<&'a [bool]>,
    /// This cursor reads a FOOTNOTE or DEFINITION body, where an over-indented
    /// sublist carries its own authored base: a list marker past the body's
    /// content column anchors its span at the marker instead of at the placing
    /// indent (see `MappedSource::sublists_carry_authored_base`,
    /// markup-carve/carve#1980). Never inherited by a nested container body,
    /// which collects into its own `MappedSource`.
    sublists_carry_authored_base: bool,
    comment_closer_last_index: Option<HashMap<usize, usize>>,
    future_quoted_comments: RefCell<Option<FutureQuotedComments>>,
    literal_comments: Option<ContainerCommentClosers>,
    literal_code: LiteralCodeLookup,
    /// Last consumed comment closer and the column of its opener.
    closed_comment_location: Option<(usize, usize)>,
    /// The last collected item body closed its marker-line colon container.
    marker_colon_closed: bool,
    code_closer_last_index: Option<HashMap<u8, Vec<usize>>>,
    list_fence_closer_last_index: Option<HashMap<u8, Vec<usize>>>,
    list_code_closers: Option<Box<ListCodeClosers>>,
}

impl<'a> LineCursor<'a> {
    fn new_with_cols(
        lines: &'a [&'a str],
        line_map: Option<&'a [Option<usize>]>,
        col_map: Option<&'a [Option<isize>]>,
    ) -> Self {
        Self::from_lines(CursorLines::Slice(lines), line_map, col_map)
    }

    fn from_lines(
        lines: CursorLines<'a>,
        line_map: Option<&'a [Option<usize>]>,
        col_map: Option<&'a [Option<isize>]>,
    ) -> Self {
        LineCursor {
            colon_view: None,
            colon_geometry: None,
            marker_tail: None,
            lines,
            line_map,
            col_map,
            pos: 0,
            last_block_start: None,
            block_starts: None,
            closed_initial_container_end: None,
            at_document_level: false,
            in_quoted_body: false,
            in_item_body: false,
            invisible_arms: true,
            reached: None,
            sublists_carry_authored_base: false,
            comment_closer_last_index: None,
            future_quoted_comments: RefCell::new(None),
            literal_comments: None,
            literal_code: LiteralCodeLookup::default(),
            closed_comment_location: None,
            marker_colon_closed: false,
            code_closer_last_index: None,
            list_fence_closer_last_index: None,
            list_code_closers: None,
        }
    }

    fn peek(&self) -> Option<&'a str> {
        self.lines.line(self.pos)
    }
    fn consume(&mut self) -> Option<&'a str> {
        let line = self.peek();
        if line.is_some() {
            self.pos += 1;
        }
        line
    }
    fn eof(&self) -> bool {
        self.pos >= self.lines.len()
    }
    fn source_line(&self, pos: usize) -> Option<usize> {
        if let Some(lines) = self.colon_geometry {
            return lines.get(pos).and_then(|line| line.source_line());
        }
        self.line_map
            .and_then(|map| map.get(pos).copied().flatten())
    }

    fn continuation_decision(&self, pos: usize) -> Option<continuation_marker::Decision> {
        self.source_line(pos)
            .and_then(continuation_marker::decision)
    }

    /// Columns stripped from the front of the line at `pos`, when known.
    fn source_col(&self, pos: usize) -> Option<isize> {
        if let Some(lines) = self.colon_geometry.filter(|_| self.col_map.is_some()) {
            return lines.get(pos).and_then(|line| line.stripped);
        }
        self.col_map.and_then(|map| map.get(pos).copied().flatten())
    }

    /// Did the line at `pos` reach the content column of the container that
    /// collected it?
    ///
    /// The carried answer wins when there is one. Without it the local column
    /// arithmetic is the best available reading, which is what every frame did
    /// before the flag was threaded - and what made a line that reached the
    /// OUTER item indistinguishable from one that reached nothing.
    fn line_reached(&self, pos: usize, indent: usize, strip_cols: usize) -> bool {
        match self.reached.and_then(|flags| flags.get(pos).copied()) {
            Some(reached) => reached,
            None => indent >= strip_cols,
        }
    }

    /// `line_reached` WITHOUT the local fallback: `None` at the outermost frame,
    /// where no enclosing collection recorded an answer.
    ///
    /// The distinction matters where the fallback and the absence of a carried
    /// flag mean opposite things. A degraded fence ends a frame for a line an
    /// ancestor already claimed, and at the outermost frame there is no ancestor
    /// to have claimed one.
    fn carried_reach(&self, pos: usize) -> Option<bool> {
        self.reached.and_then(|flags| flags.get(pos).copied())
    }

    /// Is there a comment-fence closer of exactly `fence_len` at or after `start`?
    ///
    /// A closer must match the opener width EXACTLY, so ANY later line carrying a
    /// fence of that width IS a valid closer: the question is exactly "last index
    /// for this width >= start". One pass builds the width -> last index map and
    /// every lookup after that is O(1).
    ///
    /// There used to be a per-width negative cache in front of this map. It could
    /// never change an outcome: the map already answers in O(1), and its own hit
    /// condition (a second opener of the same width after a proven-no-closer
    /// point) is unreachable, because a second line of the same width IS a closer
    /// for the first.
    /// Whether any line after `start` could close a code fence of this char and
    /// width. Deliberately OVER-approximate: it ignores indentation, which the
    /// real closer test does not. A `false` is therefore final and a `true` only
    /// means "worth scanning", so the caller keeps its exact scan behind this.
    ///
    /// Without it the exact scan runs from every opener to the end of the
    /// document, which is quadratic in the number of unterminated openers - the
    /// same shape `comment_closer_last_index` was added to remove for `%%%`.
    /// Code fences never got the equivalent.
    fn has_code_closer_after(&mut self, start: usize, fence_char: u8, fence_len: usize) -> bool {
        if let Some(possible) = self.lines.code_after(start, fence_char, fence_len) {
            return possible;
        }
        if self.code_closer_last_index.is_none() {
            self.code_closer_last_index = Some(build_code_closer_last_index(&self.lines));
        }
        self.code_closer_last_index
            .as_ref()
            .is_some_and(|index| code_closer_exists_after(index, start, fence_char, fence_len))
    }

    fn has_list_fence_closer_after(&mut self, start: usize, open: FenceOpen) -> bool {
        if self.list_fence_closer_last_index.is_none() {
            let mut lines = HashMap::<u8, Vec<usize>>::new();
            for index in 0..self.lines.len() {
                let line = strip_lazy(self.lines.at(index));
                if indent_columns(line) != 0 {
                    continue;
                }
                let bytes = line.as_bytes();
                let Some(&fence_char @ (b'`' | b'~')) = bytes.first() else {
                    continue;
                };
                let run = bytes.iter().take_while(|&&byte| byte == fence_char).count();
                if !bytes[run..]
                    .iter()
                    .all(|&byte| byte == b' ' || byte == b'\t')
                {
                    continue;
                }
                let by_len = lines.entry(fence_char).or_default();
                if by_len.len() <= run {
                    by_len.resize(run + 1, 0);
                }
                by_len[run] = index + 1;
            }
            for by_len in lines.values_mut() {
                for len in (0..by_len.len().saturating_sub(1)).rev() {
                    by_len[len] = by_len[len].max(by_len[len + 1]);
                }
            }
            self.list_fence_closer_last_index = Some(lines);
        }
        self.list_fence_closer_last_index
            .as_ref()
            .and_then(|lines| lines.get(&open.fence_char))
            .and_then(|by_len| by_len.get(open.fence_len))
            .is_some_and(|&last| last > start + 1)
    }

    fn has_comment_closer_after(&mut self, start: usize, fence_len: usize) -> bool {
        if let Some(possible) = self.lines.comment_after(start, fence_len) {
            return possible;
        }
        if self.comment_closer_last_index.is_none() {
            self.comment_closer_last_index = Some(build_comment_closer_last_index(&self.lines));
        }
        self.comment_closer_last_index
            .as_ref()
            .and_then(|last_index| last_index.get(&fence_len).copied())
            .is_some_and(|last| last >= start)
    }
}

/// Codepoints a container took from the front of a line, when that is knowable.
fn stripped_col(outer: Option<isize>, original: &str, stripped: &str) -> Option<isize> {
    let outer = outer?;
    if original.ends_with(stripped) {
        return Some(outer + original.chars().count() as isize - stripped.chars().count() as isize);
    }
    // THE SAME SUFFIX TEST, against the line with its trailing whitespace
    // dropped. It stays anchored at the end - of the trimmed body - so it names
    // exactly one offset, and the prefix it implies is measured rather than
    // guessed. That is what keeps it as safe as the rule above it: a trailing
    // trim is invisible to a constant that describes the front of the line.
    let body = trim_ascii_end(original);
    if body.ends_with(stripped) {
        return Some(outer + body.chars().count() as isize - stripped.chars().count() as isize);
    }
    let synthetic = stripped.chars().take_while(|c| *c == ' ').count();
    let tail = &stripped[synthetic..];
    if synthetic == 0 || !original.ends_with(tail) {
        return None;
    }
    let consumed = original.chars().count().checked_sub(tail.chars().count())?;
    if !original
        .chars()
        .take(consumed)
        .all(|c| c == ' ' || c == '\t')
    {
        return None;
    }
    Some(outer + consumed as isize - synthetic as isize)
}

/// WHERE THE SIGNED QUANTITY STOPS: a 1-based document column.
///
/// `stripped` is the constant the column map carries for a line and `within` is
/// the codepoint offset inside the line the parser saw, so their sum is the
/// 0-based document column. Every `Pos` column in this file is built here, so
/// there is exactly one place the two units meet and exactly one place the
/// clamp lives.
///
/// The clamp is UNREACHABLE for a well-formed map, and is a guard rather than a
/// fix: a negative constant only ever arises from a synthetic space prefix, and
/// nothing is placed at a column inside that prefix - it is indentation, and
/// `within` has already skipped it. Saturating here means a future producer
/// that does get it wrong publishes column 1 rather than wrapping to
/// `usize::MAX` and reporting a span past the end of the document.
fn document_column(stripped: isize, within: usize) -> usize {
    (stripped + within as isize).max(0) as usize + 1
}

/// Seed a list item's body with its marker line, carrying the column the marker
/// itself occupied so a block opened on that line lands where the author wrote
/// it rather than at column 1.
fn item_marker_source(cur: &LineCursor<'_>, content: &str, at: usize) -> MappedSource {
    let stripped = cur
        .lines
        .line(at)
        .and_then(|line| stripped_col(cur.source_col(at), line, content));
    let mut source = MappedSource::new_line_at(content.to_string(), cur.source_line(at), stripped);
    source.item_marker_at_start = true;
    source.in_quoted_body = cur.in_quoted_body;
    source
}

/// Span of a list item's lead paragraph. It starts where the marker's CONTENT
/// starts - the paragraph is the text, not the bullet - and ends at the last
/// line folded into it by lazy continuation.
fn item_paragraph_span(
    cur: &LineCursor<'_>,
    start_at: usize,
    end_at: usize,
    content: &str,
    options: &Options<'_>,
) -> Option<Pos> {
    if !options.positions {
        return None;
    }
    let start_line = cur.source_line(start_at)?;
    let line = cur.lines.line(start_at)?;
    let start_stripped = stripped_col(cur.source_col(start_at), line, content)?;
    let end_line = cur.source_line(end_at).unwrap_or(start_line);
    let end_width = cur
        .lines
        .line(end_at)
        .map(|l| strip_lazy(l).chars().count())
        .unwrap_or(0);
    Some(Pos {
        start_line,
        end_line,
        start_column: document_column(start_stripped, 0),
        end_column: document_column(cur.source_col(end_at).unwrap_or(0), end_width),
        start_offset: 0,
        end_offset: 0,
        file: None,
    })
}

/// Anchor for the line at `pos`: its source line, and the codepoints in front
/// of `inline_line` in the document.
///
/// THE LAZY FRAME IS NOT SOURCE TEXT, so neither side is measured with it on.
/// A framed line is `LAZY` plus the line's content, and the inline text a
/// caller builds holds the content alone, so an anchor taken against the framed
/// line read the frame's three codepoints as the line's own indentation: a code
/// span opened there began at the INDENTATION rather than at its backtick run
/// (markup-carve/carve-rs#1832). Two framing sites had compensated for that by
/// discounting the frame from the column they recorded, which left the same
/// line reading one way here and another in `span_of`; the frame is discounted
/// in the readers now, so every site records the column it actually took.
fn inline_anchor_for_line(
    cur: &LineCursor<'_>,
    pos: usize,
    inline_line: &str,
) -> Option<(usize, isize)> {
    Some((
        cur.source_line(pos)?,
        stripped_col(
            cur.source_col(pos),
            strip_lazy(cur.lines.line(pos)?),
            strip_lazy(inline_line),
        )?,
    ))
}

fn parse_inline_lines_with_anchor(
    text: &str,
    options: &Options<'_>,
    lines: Vec<Option<(usize, isize)>>,
) -> Vec<InlineNode> {
    parse_inline_with_anchor(text, options, InlineAnchor::lines(&lines))
}

/// The same, for a text whose segments are NOT separated by newlines.
///
/// `breaks` are the byte offsets at which the next entry of `lines` takes over.
/// A table cell rebuilt across a `+` continuation is the only text shaped this
/// way: its fragments come from different source lines and are joined by a
/// manufactured space, so nothing in the text itself marks where one ends.
fn parse_inline_segments_with_anchor(
    text: &str,
    options: &Options<'_>,
    lines: Vec<Option<(usize, isize)>>,
    breaks: Vec<usize>,
) -> Vec<InlineNode> {
    parse_inline_with_anchor(
        text,
        options,
        InlineAnchor {
            lines: &lines,
            breaks: &breaks,
            columns: None,
        },
    )
}

/// Build a span for the lines `[start, end)` of `cur`, in the ORIGINAL source.
///
/// Returns `None` when the source line or the stripped column width is unknown
/// for the first line - a position that cannot be trusted is worse than no
/// position, because a consumer cannot tell the difference.
///
/// Offsets are left at zero here and filled by `fill_offsets` once the whole
/// document is parsed: an offset needs the original text, which the parser sees
/// only as already-stripped lines.
fn span_of(cur: &LineCursor<'_>, start: usize, end: usize, options: &Options<'_>) -> Option<Pos> {
    if !options.positions {
        return None;
    }
    let start_line = cur.source_line(start)?;
    let stripped = cur.source_col(start)?;
    let last = end.saturating_sub(1).max(start);
    let end_line = cur.source_line(last).unwrap_or(start_line);
    // The parser sees the line with its container prefix removed, so the
    // document column is what the container took plus the indent that remains.
    //
    // INDENTATION IS SPACE AND TAB, and nothing else is. `trim_start` here was
    // the Unicode whitespace property, so a line opening with a no-break space
    // or an en quad had that character counted as INDENT - the block's span
    // began one column past its own first child, which PART 12 §4's containment
    // rule forbids (carve#913, and the finding that pass reports). Those
    // characters are CONTENT under PART 1's `indent` terminal (carve#890), so
    // they belong inside the span, not in front of it.
    let indent = cur
        .lines
        .line(start)
        .map(strip_lazy)
        .map(|l| l.chars().count() - trim_ascii_start(l).chars().count())
        .unwrap_or(0);
    let width = cur
        .lines
        .line(last)
        .map(|l| strip_lazy(l).chars().count())
        .unwrap_or(0);
    // The LAST line may have had a different amount taken off it than the
    // first: a lazily continued paragraph starts inside a blockquote or list
    // item and ends flush left, so reusing the opening line's count runs the
    // end column past the end of the document.
    let end_stripped = if last == start {
        stripped
    } else {
        cur.source_col(last)?
    };
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

/// Fill node and sidecar offsets in codepoints (PART 12 §4).
fn fill_offsets(blocks: &mut [BlockNode], line_starts: &CodepointLineStarts) {
    for block in blocks {
        if let Some(pos) = block_pos_mut(block) {
            apply_offsets(pos, line_starts);
        }
        // Recurse into the containers that hold blocks and inline content.
        match block {
            BlockNode::Heading(h) => apply_inline_offsets(&mut h.children, line_starts),
            BlockNode::Paragraph(p) => {
                apply_inline_offsets(&mut p.children, line_starts);
                let first = p
                    .children
                    .iter()
                    .find(|node| {
                        !matches!(node, InlineNode::SoftBreak(_) | InlineNode::HardBreak(_))
                    })
                    .and_then(owned_inline_pos);
                let last = p.children.last().and_then(owned_inline_pos);
                // THE TWO ENDS ARE TAKEN SEPARATELY, because they are two
                // rules. Requiring BOTH before touching either meant an
                // unplaced last child also gave up the start the first child
                // supplies, and in a footnote body - where the block layer's
                // extent and the inline offsets do not agree, a question still
                // open across the engines - that put the stanza's first `text`
                // OUTSIDE its own paragraph. Which is the containment defect
                // this pair of rules exists to prevent, arrived at from the
                // third side.
                if let Some(pos) = &mut p.pos {
                    if let Some(first) = first {
                        pos.start_line = first.start_line;
                        pos.start_column = first.start_column;
                        pos.start_offset = first.start_offset;
                    }
                    if let Some(last) = last {
                        pos.end_line = last.end_line;
                        pos.end_column = last.end_column;
                        pos.end_offset = last.end_offset;
                    }
                }
            }
            BlockNode::BlockQuote(b) => {
                fill_offsets(&mut b.children, line_starts);
            }
            BlockNode::Directive(d) => {
                if let Some(title) = &mut d.title {
                    apply_inline_offsets(title, line_starts);
                }
                fill_offsets(&mut d.children, line_starts);
            }
            BlockNode::Div(d) => fill_offsets(&mut d.children, line_starts),
            BlockNode::Admonition(a) => {
                if let Some(title) = &mut a.title {
                    apply_inline_offsets(title, line_starts);
                }
                fill_offsets(&mut a.children, line_starts);
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    // The ITEM's own span too, not only the blocks inside it:
                    // a span whose offsets are never filled stays 0..0, which
                    // reads as present and selects nothing.
                    if let Some(pos) = item.pos.as_mut() {
                        apply_offsets(pos, line_starts);
                    }
                    fill_offsets(&mut item.children, line_starts);
                    if let (Some(pos), Some(last)) = (
                        item.pos.as_mut(),
                        item.children
                            .iter()
                            .rev()
                            .find_map(crate::ast_json::block_pos)
                            .cloned(),
                    ) {
                        pos.end_line = last.end_line;
                        pos.end_column = last.end_column;
                        pos.end_offset = last.end_offset;
                    }
                }
            }
            BlockNode::Table(t) => apply_table_offsets(t, line_starts),
            BlockNode::LineBlock(l) => fill_offsets(&mut l.children, line_starts),
            BlockNode::DefinitionList(d) => {
                for item in &mut d.items {
                    for term in &mut item.terms {
                        // The TERM's own span, not only its inline content. This
                        // walk reached the children and skipped the node, so a
                        // `<dt>` carried line and column with offsets of 0..0 -
                        // present, and selecting nothing. Same for `<dd>` below.
                        if let Some(pos) = term.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        apply_inline_offsets(&mut term.children, line_starts);
                    }
                    for def in &mut item.definitions {
                        if let Some(pos) = def.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        fill_offsets(&mut def.children, line_starts);
                    }
                }
            }
            BlockNode::Figure(f) => {
                // The CAPTION first. It was the one part of a figure this walk
                // never reached, so a caption's inline spans kept line and
                // column but offsets of 0..0 - which reads as present and
                // selects nothing, the exact shape section 4 forbids.
                apply_inline_offsets(&mut f.caption, line_starts);
                match &mut *f.target {
                    FigureTarget::BlockQuote(q) => {
                        // The quote's OWN span, which this arm skipped while
                        // filling everything inside it: it kept line and column
                        // and offsets of 0..0, so the quote selected nothing and
                        // every block within it fell outside its own parent
                        // (carve#565). Same defect the code-block arm below
                        // records, one target along.
                        if let Some(pos) = q.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        fill_offsets(&mut q.children, line_starts);
                    }
                    FigureTarget::Paragraph(p) => {
                        if let Some(pos) = p.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                        apply_inline_offsets(&mut p.children, line_starts);
                    }
                    FigureTarget::Table(t) => apply_table_offsets(t, line_starts),
                    FigureTarget::Image(i) => {
                        if let Some(pos) = i.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                    }
                    // A code block was reached by the catch-all below and so
                    // kept offsets of 0..0 - present, and selecting nothing.
                    // The arms are exhaustive now: a target added later fails to
                    // compile rather than silently reporting an empty span.
                    FigureTarget::CodeBlock(c) => {
                        if let Some(pos) = c.pos.as_mut() {
                            apply_offsets(pos, line_starts);
                        }
                    }
                }
            }
            BlockNode::FigureGroup(g) => {
                if let Some(caption) = &mut g.caption {
                    apply_inline_offsets(caption, line_starts);
                }
                fill_offsets(&mut g.children, line_starts);
            }
            _ => {}
        }
    }
}

/// Widen a container over the DEFINITION it hosted.
fn start_list_at_markup_after_quote(pos: &mut Pos, lines: &[&str]) {
    let Some(line) = lines.get(pos.start_line.saturating_sub(1)) else {
        return;
    };
    let prefix: String = line
        .chars()
        .take(pos.start_column.saturating_sub(1))
        .collect();
    if !prefix.contains('>') {
        return;
    }
    let tail: String = line
        .chars()
        .skip(pos.start_column.saturating_sub(1))
        .collect();
    if detect_list_marker_full(&tail).is_none() {
        return;
    }
    let placing = tail.chars().count() - trim_ascii_start(&tail).chars().count();
    pos.start_column += placing;
    pos.start_offset += placing;
}

fn narrow_to_last_placed_child(blocks: &mut [BlockNode], lines: &[&str]) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::BlockQuote(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::Directive(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::Div(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::Admonition(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::FigureGroup(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::LineBlock(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::ExtensionCarrier(n) => narrow_to_last_placed_child(&mut n.children, lines),
            BlockNode::Figure(n) => {
                if let FigureTarget::BlockQuote(q) = &mut *n.target {
                    narrow_to_last_placed_child(&mut q.children, lines);
                    // A FENCED quote owns its `:::` closer, so it ends after it
                    // rather than at its last child (CARVE-P12-014). Narrowing
                    // here made the same construct answer one way standalone
                    // and another as a figure target (carve-rs#2165).
                    if !q.fenced {
                        let last = q
                            .children
                            .iter()
                            .rev()
                            .find_map(crate::ast_json::block_pos)
                            .cloned();
                        if let Some(pos) = q.pos.as_mut() {
                            narrow_container_end(pos, last, lines, true);
                        }
                    }
                }
            }
            BlockNode::DefinitionList(n) => {
                for item in &mut n.items {
                    // A TERM ENDS AT ITS LAST PLACED CHILD like every other
                    // container without a closer. Its span covers the `:: `
                    // marker and every line the term folded, so it was derived
                    // from those LINES - and a folded line ending in a space
                    // put that space inside the term and inside no child of it,
                    // because PART 2 drops a trailing run before the inline
                    // content is parsed. Its sibling `definition_description`
                    // already narrowed to its last block at parse time; the
                    // term was the odd one out in its own function.
                    for term in &mut item.terms {
                        let last = term.children.iter().rev().find_map(owned_inline_pos);
                        if let Some(pos) = term.pos.as_mut() {
                            end_at_last_placed_child(pos, last);
                        }
                    }
                    for def in &mut item.definitions {
                        narrow_to_last_placed_child(&mut def.children, lines);
                        // END AT THE LAST PLACED CHILD, like every other
                        // container. A footnote definition authored in the body
                        // and hoisted out by PART 9 SS7 is a SIBLING, not a
                        // child (markup-carve/carve#1522), so the description's
                        // span must not reach over it (carve-rs#1555). The list
                        // that hosts it derives its own end from this, so both
                        // narrow together.
                        let last = def
                            .children
                            .iter()
                            .rev()
                            .find_map(crate::ast_json::block_pos)
                            .cloned();
                        if let Some(pos) = def.pos.as_mut() {
                            // A description whose only content hoisted to the
                            // root has no placed child; its span is then its own
                            // MARKER LINE and stops there, not the continuation
                            // lines that carried the hoisted definitions
                            // (carve#1522, carve#1963). Its `:` marker is not one
                            // of the list openers `narrow_container_end` knows, so
                            // the marker-line fallback is spelled here.
                            if !end_at_last_placed_child(pos, last) {
                                end_at_marker_line(pos, lines);
                            }
                        }
                    }
                }
            }
            BlockNode::List(n) => {
                if let Some(pos) = n.pos.as_mut() {
                    start_list_at_markup_after_quote(pos, lines);
                }
                for item in &mut n.items {
                    narrow_to_last_placed_child(&mut item.children, lines);
                    let last = item
                        .children
                        .iter()
                        .rev()
                        .find_map(crate::ast_json::block_pos)
                        .cloned();
                    if let Some(pos) = item.pos.as_mut() {
                        start_list_at_markup_after_quote(pos, lines);
                        narrow_container_end(pos, last, lines, false);
                    }
                }
            }
            _ => {}
        }
        match block {
            // A QUOTE HAS TWO SPELLINGS AND ONLY ONE OF THEM ENDS AT ITS LAST
            // CHILD. `docs/ast-json-contract.md` reads §4's end rule as a
            // statement about MARKUP - a container ends at the markup that
            // closes it, and "ends at its last placed child" is the case for a
            // container whose closer is IMPLICIT. The `>` prefix form has none,
            // so its content's end is its extent; the `::: >` form has one, and
            // every other colon fence spans it. Narrowed by kind alone, the
            // fenced quote was shrunk off its own closer, so it read 1 to 2
            // where a div, an admonition and a line block over the same three
            // lines all read 1 to 3 - and `carve lint`, which reads the span to
            // find the closer, then reported a closed container as unclosed
            // (carve-rs#2099, and carve-php#2636 for the same reading). An
            // emptied fenced quote is unaffected: it has no placed child, so the
            // narrowing never reached it.
            BlockNode::BlockQuote(n) if !n.fenced => {
                let last = n
                    .children
                    .iter()
                    .rev()
                    .find_map(crate::ast_json::block_pos)
                    .cloned();
                if let Some(pos) = n.pos.as_mut() {
                    narrow_container_end(pos, last, lines, true);
                }
            }
            BlockNode::List(n) => {
                let last = n.items.iter().rev().find_map(|item| item.pos.clone());
                if let Some(pos) = n.pos.as_mut() {
                    narrow_container_end(pos, last, lines, false);
                }
            }
            // A HEADING ENDS AT ITS LAST PLACED CHILD. Its span came from the
            // line it consumed, at that line's full width, while its children
            // were parsed from the same line passed through `trim_ascii_end` -
            // so a heading written with a trailing space claimed a codepoint
            // that PART 2 had already dropped and no child could hold.
            //
            // With NO placed child the heading keeps the line it was written
            // on: `#` alone still names a construct the author typed, and §4's
            // "omit rather than invent" is about a node that cannot be placed
            // at all, not one whose content is empty.
            BlockNode::Heading(n) => {
                let last = n.children.iter().rev().find_map(owned_inline_pos);
                if let Some(pos) = n.pos.as_mut() {
                    end_at_last_placed_child(pos, last);
                }
            }
            BlockNode::DefinitionList(n) => {
                let last = n.items.iter().rev().find_map(|item| {
                    item.definitions
                        .iter()
                        .rev()
                        .find_map(|def| def.pos.clone())
                        .or_else(|| item.terms.iter().rev().find_map(|term| term.pos.clone()))
                });
                if let Some(pos) = n.pos.as_mut() {
                    narrow_container_end(pos, last, lines, false);
                }
            }
            _ => {}
        }
    }
}

/// One container's end: its last placed child's, or its own markup where it has
/// no placed child at all.
pub(crate) fn end_at_last_placed_child(pos: &mut Pos, last: Option<Pos>) -> bool {
    let Some(last) = last else {
        return false;
    };
    pos.end_line = last.end_line;
    pos.end_column = last.end_column;
    pos.end_offset = last.end_offset;
    true
}

/// End a span at its own start line's full width - the reading a
/// `definition_description` takes when its only content hoisted to the root, so
/// it has no placed child but its authored marker line is still its own extent
/// (carve#1963). Matches the width the reference derives from that line: the
/// span runs from its start column to the end of the line, trailing run and all,
/// the same bytes `lineRange` covers in carve-js.
fn end_at_marker_line(pos: &mut Pos, lines: &[&str]) {
    let Some(line) = lines.get(pos.start_line.saturating_sub(1)) else {
        return;
    };
    let len = line.chars().count();
    let start = pos.start_column.saturating_sub(1);
    if len < start {
        return;
    }
    let width = len - start;
    pos.end_line = pos.start_line;
    pos.end_column = pos.start_column + width;
    pos.end_offset = pos.start_offset + width;
}

fn narrow_container_end(pos: &mut Pos, last: Option<Pos>, lines: &[&str], quote: bool) {
    if end_at_last_placed_child(pos, last) {
        return;
    }
    let Some(line) = lines.get(pos.start_line.saturating_sub(1)) else {
        return;
    };
    let width = emptied_container_markup(line, pos.start_column, quote);
    if width == 0 {
        return;
    }
    pos.end_line = pos.start_line;
    pos.end_column = pos.start_column + width;
    pos.end_offset = pos.start_offset + width;
}

/// The width in CODEPOINTS of the markup that opened an emptied container, read
/// from its own start column, or 0 where the line does not spell one - in which
/// case the span is left as it was rather than guessed at.
fn emptied_container_markup(line: &str, start_column: usize, quote: bool) -> usize {
    let tail: Vec<char> = line.chars().skip(start_column.saturating_sub(1)).collect();
    let mut i = 0;
    while i < tail.len() && (tail[i] == ' ' || tail[i] == '\t') {
        i += 1;
    }
    if quote {
        if i < tail.len() && tail[i] == '>' {
            i += 1;
        } else {
            return 0;
        }
    } else if i < tail.len() && matches!(tail[i], '-' | '+' | '*' | '.') {
        i += 1;
    } else {
        let mut j = i;
        while j < tail.len() && tail[j].is_ascii_alphanumeric() {
            j += 1;
        }
        if j > i && j < tail.len() && (tail[j] == '.' || tail[j] == ')') {
            i = j + 1;
        } else {
            return 0;
        }
    }
    while i < tail.len() && (tail[i] == ' ' || tail[i] == '\t') {
        i += 1;
    }
    i
}

/// Turn the line/column pair already on a span into codepoint offsets.
fn apply_offsets(pos: &mut Pos, line_starts: &CodepointLineStarts) {
    if let Some(start) = line_starts.get(pos.start_line.saturating_sub(1)) {
        pos.start_offset = start + pos.start_column.saturating_sub(1);
    }
    if let Some(end) = line_starts.get(pos.end_line.saturating_sub(1)) {
        pos.end_offset = end + pos.end_column.saturating_sub(1);
    }
    if line_starts.has_leading_bom() {
        if pos.start_line == 1 {
            pos.start_column += 1;
        }
        if pos.end_line == 1 {
            pos.end_column += 1;
        }
    }
}

/// Fill offsets for a table: the rows' and cells' OWN spans, and the inline
/// content of the caption and of every cell.
///
/// Both halves in one place, because they were added a week apart and the
/// second nearly dropped the first: a span whose offsets are never filled stays
/// 0..0, which reads as present and selects nothing.
fn apply_table_offsets(table: &mut Table, line_starts: &CodepointLineStarts) {
    if let Some(caption) = &mut table.caption {
        apply_inline_offsets(caption, line_starts);
    }
    for row in &mut table.rows {
        if let Some(pos) = row.pos.as_mut() {
            apply_offsets(pos, line_starts);
        }
        for cell in &mut row.cells {
            if let Some(pos) = cell.pos.as_mut() {
                apply_offsets(pos, line_starts);
            }
            apply_inline_offsets(&mut cell.children, line_starts);
        }
    }
}

fn apply_inline_offsets(nodes: &mut [InlineNode], line_starts: &CodepointLineStarts) {
    for node in nodes {
        if let Some(pos) = inline_pos_mut(node) {
            apply_offsets(pos, line_starts);
        }
        match node {
            InlineNode::Emphasis(e) => apply_inline_offsets(&mut e.children, line_starts),
            InlineNode::Link(l) => apply_inline_offsets(&mut l.children, line_starts),
            InlineNode::Span(s) => apply_inline_offsets(&mut s.children, line_starts),
            InlineNode::Extension(e) => apply_inline_offsets(&mut e.children, line_starts),
            InlineNode::CitationGroup(g) => {
                for item in &mut g.items {
                    if let Some(pos) = &mut item.pos {
                        apply_offsets(pos, line_starts);
                    }
                    if let Some(prefix) = &mut item.prefix {
                        apply_inline_offsets(prefix, line_starts);
                    }
                    if let Some(locator) = &mut item.locator {
                        apply_inline_offsets(locator, line_starts);
                    }
                    if let Some(suffix) = &mut item.suffix {
                        apply_inline_offsets(suffix, line_starts);
                    }
                }
            }
            InlineNode::Footnote(f) => {
                if let Some(inline) = &mut f.inline {
                    apply_inline_offsets(inline, line_starts);
                }
            }
            InlineNode::CriticInsert(c) => apply_inline_offsets(&mut c.children, line_starts),
            InlineNode::CriticDelete(c) => apply_inline_offsets(&mut c.children, line_starts),
            InlineNode::CriticSubstitute(c) => {
                apply_inline_offsets(&mut c.old, line_starts);
                apply_inline_offsets(&mut c.new, line_starts);
            }
            _ => {}
        }
    }
}

fn owned_inline_pos(node: &InlineNode) -> Option<Pos> {
    node.pos().cloned()
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
        );
    }
    parse_positioned_mapped_source(
        source,
        options,
        at_document_level,
        in_item_body,
        in_quoted_body,
        pending,
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
) -> Vec<BlockNode> {
    // The ladder builds its own containers inside-out and hands back finished
    // nodes, so it contributes nothing to `pending` - and must not, because it
    // WRAPS the blocks it parsed, which would move any index recorded in them.
    if let Some(blocks) = parse_eof_closed_colon_ladder(source, options) {
        return blocks;
    }
    if source.sublists_carry_authored_base
        && source.source.contains(":::")
        && source.source.lines().any(|line| {
            line.starts_with([' ', '\t'])
                && strip_blockquote_prefix(trim_ascii_start(line)).is_some()
        })
    {
        return parse_positioned_mapped_source(
            source,
            options,
            at_document_level,
            in_item_body,
            in_quoted_body,
            pending,
        );
    }
    parse_blocks_with_options_at_level_into_reached(
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
    )
}

#[inline(never)]
fn parse_positioned_mapped_source(
    source: &MappedSource,
    options: &Options<'_>,
    at_document_level: bool,
    in_item_body: bool,
    in_quoted_body: bool,
    pending: &mut Vec<PendingBody>,
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
    parse_blocks(&mut cursor, options, pending)
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
                && strip_blockquote_prefix(trim_ascii_start(line)).is_some()
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
        let want_lines = options.source_lines || options.positions || needs_source_ending_map();
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
    if !options.source_lines
        && !options.positions
        && !needs_source_ending_map()
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
            .or_else(|| content.strip_prefix('\t'))
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
        });
        offset += text.len() + 1;
    }
    payload
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
    match &node {
        BlockNode::CodeBlock(code) => {
            let payload = delimited_payload_lines(cur, start_line, code, options);
            if let Some(table) = delimited_table::from_code(code, &payload) {
                node = BlockNode::Table(table);
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
                    node = BlockNode::Table(table);
                }
            }
        }
        _ => {}
    }
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
    attrs: Option<Attrs>,
    /// A collected definition written on the marker line is the item's first
    /// (invisible) block, and the prepass leaves an empty paragraph where its
    /// line was. The item must not carry it.
    drop_empty_paragraphs: bool,
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
) {
    let BlockNode::List(list) = node else {
        debug_assert!(
            false,
            "a pending item chunk pointed at something that is not a list"
        );
        return;
    };
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
) -> Vec<BlockNode> {
    let ItemBody {
        chunk,
        depth,
        in_group,
        attrs,
        drop_empty_paragraphs,
        ..
    } = body;
    let mut children = match chunk {
        Chunk::Source(source) => {
            let _depth = DepthScope::set(depth);
            let _group = FigureGroupScope::set(in_group);
            parse_mapped_source_at_level_into(&source, options, false, true, false, nested)
        }
        Chunk::Parsed { blocks, carried } => {
            nested.extend(carried);
            blocks
        }
    };
    if drop_empty_paragraphs {
        drop_empty_item_paragraphs(&mut children, nested);
    }
    apply_held_item_attrs(&mut children, attrs);
    children
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
    mut source: MappedSource,
    attrs: Option<Attrs>,
    drop_empty_paragraphs: bool,
    options: &Options<'_>,
) -> Vec<BlockNode> {
    rebase_overindented_blocks(&mut source, false, options);
    deferred.push(ItemBody {
        item,
        chunk: Chunk::Source(source),
        depth: NESTING_DEPTH.with(Cell::get),
        in_group: IN_FIGURE_GROUP.with(Cell::get),
        attrs,
        drop_empty_paragraphs,
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
) -> Vec<BlockNode> {
    if !source
        .source
        .lines()
        .any(|line| line.starts_with([' ', '\t']))
    {
        return parse_mapped_source_at_level_into(source, options, false, true, false, carried);
    }
    let mut rebased = source.clone();
    rebase_overindented_blocks(&mut rebased, false, options);
    parse_mapped_source_at_level_into(&rebased, options, false, true, false, carried)
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

#[cfg(test)]
mod container_comment_dedent_steps {
    //! COUNTED guard on the container-comment dedent walk
    //! (markup-carve/carve-rs#1047).
    //!
    //! Counted rather than timed, for the reason the module below it records at
    //! length: a call count is a property of the algorithm and reproduces
    //! byte-identically across runs and loads, while this repo's timing tests
    //! had to be serialized against each other to stop them flaking.
    //!
    //! The subject is the shape the first version of the fix had. One container
    //! can hold many comment openers, and answering "does this one close inside
    //! its container" by walking forward from each meant walking the whole
    //! container once per opener - O(m^3) work for an O(m^2) document, which is
    //! exactly the class `comment_fence_close_index` exists to avoid for
    //! column-0 openers.

    use super::CONTAINER_DEDENT_STEPS;

    /// Dedent-walk steps over one full parse-and-render of `src`, on its own
    /// thread so the thread-local tally cannot pick up another test's.
    fn steps_for(src: String) -> u64 {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                CONTAINER_DEDENT_STEPS.with(|c| c.set(0));
                let _ = crate::parse(&src);
                CONTAINER_DEDENT_STEPS.with(|c| c.get())
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// `m` comment openers of distinct widths at one item's content column,
    /// above `m * m` filler lines, with every matching closer only past the
    /// dedent. Every opener therefore passes the document-wide "is there a
    /// closer of this width later" test and none of them closes inside the
    /// item, which is the worst case for the container bound.
    ///
    /// The work is the line count: a walk that visits the container once has to
    /// grow with it, and one that visits it per opener cannot stay flat.
    fn openers_over_one_container(m: usize) -> (String, u64) {
        let mut out = String::from("- x\n");
        for i in 0..m {
            out.push_str("  ");
            out.push_str(&"%".repeat(3 + i));
            out.push_str(" o\n");
        }
        for _ in 0..m * m {
            out.push_str("  filler line here\n");
        }
        for i in 0..m {
            out.push_str(&"%".repeat(3 + i));
            out.push('\n');
        }
        out.push_str("\n[r]: /u\n\n[r][]\n");
        let lines = out.lines().count() as u64;
        (out, lines)
    }

    /// The walk must cost steps in proportion to the DOCUMENT, not to openers
    /// times container length.
    ///
    /// The lower bound keeps the counter live. Per-line cost must stay flat as
    /// the opener count grows, catching per-opener rescanning.
    #[test]
    fn many_openers_over_one_container_walk_it_once_apiece() {
        let (small_src, small_lines) = openers_over_one_container(60);
        let (large_src, large_lines) = openers_over_one_container(120);
        let small_steps = steps_for(small_src);
        let large_steps = steps_for(large_src);

        assert!(
            small_steps > 0 && large_steps > 0,
            "the dedent-step counter is dead: {small_steps} and {large_steps} steps",
        );
        assert!(
            large_steps <= 8 * large_lines,
            "{large_steps} steps over {large_lines} lines ({:.1} each): \
             the container is being re-walked per opener",
            large_steps as f64 / large_lines as f64,
        );
        assert!(
            large_steps * small_lines * 10 <= small_steps * large_lines * 11,
            "the per-line dedent cost climbs with size ({:.1} at {small_lines} lines, \
             {:.1} at {large_lines}): the container is being re-walked per opener",
            small_steps as f64 / small_lines as f64,
            large_steps as f64 / large_lines as f64,
        );
    }

    /// The same worst case one prefix over: `m` quoted openers of distinct
    /// widths in ONE quote, above `m * m` quoted filler lines, with every
    /// matching closer only past the blank that ends the quote.
    ///
    /// The quote bound (markup-carve/carve#1341) is a second forward walk, and a
    /// new walk with no counted guard is a walk nobody would notice going
    /// quadratic. Every opener passes the "is there a closer of this width and
    /// depth later" test and none of them closes inside its quote, which is the
    /// shape that makes a per-opener walk visible.
    fn quoted_openers_over_one_quote(m: usize) -> (String, u64) {
        let mut out = String::new();
        for i in 0..m {
            out.push_str("> ");
            out.push_str(&"%".repeat(3 + i));
            out.push_str(" o\n");
        }
        for _ in 0..m * m {
            out.push_str("> filler line here\n");
        }
        out.push('\n');
        for i in 0..m {
            out.push_str("> ");
            out.push_str(&"%".repeat(3 + i));
            out.push('\n');
        }
        out.push_str("\n[r]: /u\n\n[r][]\n");
        let lines = out.lines().count() as u64;
        (out, lines)
    }

    /// The three claims `many_openers_over_one_container_walk_it_once_apiece`
    /// makes, for the blank-line bound: a floor so a dead counter cannot pass,
    /// a ceiling, and the flat per-line shape that a per-opener walk cannot hold.
    #[test]
    fn many_quoted_openers_over_one_quote_walk_it_once_apiece() {
        let (small_src, small_lines) = quoted_openers_over_one_quote(60);
        let (large_src, large_lines) = quoted_openers_over_one_quote(120);
        let small_steps = steps_for(small_src);
        let large_steps = steps_for(large_src);

        assert!(
            small_steps > 0 && large_steps > 0,
            "the blank-walk counter is dead: {small_steps} and {large_steps} steps",
        );
        assert!(
            large_steps <= 8 * large_lines,
            "{large_steps} steps over {large_lines} lines ({:.1} each): \
             the quote is being re-walked per opener",
            large_steps as f64 / large_lines as f64,
        );
        assert!(
            large_steps * small_lines * 10 <= small_steps * large_lines * 11,
            "the per-line blank-walk cost climbs with size ({:.1} at {small_lines} lines, \
             {:.1} at {large_lines}): the quote is being re-walked per opener",
            small_steps as f64 / small_lines as f64,
            large_steps as f64 / large_lines as f64,
        );
    }
}

#[cfg(test)]
mod quote_prefix_calls {
    //! COUNTED guards on the blockquote prefix (markup-carve/carve-rs#731).
    //!
    //! They live here rather than in `tests/` because the counter they read is
    //! `#[cfg(test)]`, so a release build carries none of it. `cargo test` runs
    //! this binary, which is what CI runs.
    //!
    //! Counted, not timed, deliberately. This repo already records why a clock
    //! cannot express these bounds: `tests/perf_regressions.rs` had to
    //! serialize its 34 timing tests against each other because they flaked
    //! competing for cores, and carve-js's `writer-deep-list-perf.test.ts`
    //! carries "No ratio guard here on purpose... would also fail on the
    //! healthy build". A call count is a property of the algorithm, not of the
    //! machine: every figure quoted below reproduces byte-identically across
    //! runs and loads.

    use super::{
        detect_fence_open, fence_closer_suffix_maxima, is_fence_close, leading_ws,
        strip_container_prefixes_keep_indent, strip_prepass_blockquote_prefix, FenceOpen,
        QUOTE_PREFIX_CALLS,
    };
    #[test]
    fn repeated_unclosed_quoted_fences_have_linear_work() {
        for definitions in [false, true] {
            let mut measurements = Vec::new();
            for n in [128, 256, 512, 1024] {
                let tail = if definitions { "\n[r]: /target\n" } else { "" };
                let source = format!("> ::: |\n> verse\n{}> :::\n{tail}", "> ```x\n".repeat(n));
                let calls = calls_for(source);
                println!("quoted_fences definitions={definitions} n={n} prefix_calls={calls}");
                measurements.push(calls);
            }
            assert!(measurements[3] <= measurements[0] * 12,
                "eight times the input must use at most twelve times the prefix work: {measurements:?}");
        }
    }

    #[test]
    fn prepass_closer_index_preserves_columns_and_run_widths() {
        let lines = [
            "payload",
            "> ```",
            "  ~~~~",
            " > `````",
            "> - ```",
            "- ~~~",
            "``` info",
            "> > ~~~~~",
            "```` tail",
            "  ```\t",
        ];
        let closers = fence_closer_suffix_maxima(&lines);
        for index in 0..lines.len() {
            for quoted in [false, true] {
                for content_col in 0..=6 {
                    for fence_char in *b"`~" {
                        for fence_len in 3..=6 {
                            let open = FenceOpen {
                                fence_char,
                                fence_len,
                                content_col,
                                quoted,
                                ..detect_fence_open("```").unwrap()
                            };
                            let suffix = if fence_char == b'`' {
                                closers.maxima.0[index + 1]
                            } else {
                                closers.maxima.1[index + 1]
                            };
                            let expected = suffix >= fence_len
                                && lines[index + 1..].iter().any(|&line| {
                                    let line = if quoted {
                                        strip_prepass_blockquote_prefix(line).unwrap_or(line)
                                    } else {
                                        line
                                    };
                                    let kept = strip_container_prefixes_keep_indent(line);
                                    let candidate = if leading_ws(&kept) >= content_col {
                                        &kept[content_col..]
                                    } else {
                                        &kept
                                    };
                                    is_fence_close(candidate, open)
                                });
                            assert_eq!(closers.closes_after(index, open), expected,
                                "index={index}, quoted={quoted}, column={content_col}, character={fence_char}, width={fence_len}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn lazy_quote_fence_admission_has_linear_prefix_work() {
        for shape in 0..4 {
            let measurements: Vec<_> = [128, 256, 512, 1024]
                .into_iter()
                .map(|n| {
                    let source = match shape {
                        0 => format!("> p\n{}> ```\n> [r]: /target\n", "lazy\n".repeat(n)),
                        1 => "> p\nlazy\n> ```\n> [r]: /target\n\n".repeat(n),
                        2 => format!(
                            "> ```a\n{}> ```\nlazy\n> ~~~\n> [r]: /target\n",
                            "> ```x\n".repeat(n)
                        ),
                        _ => format!("{}> ```\n", "> ```a\nlazy\n".repeat(n)),
                    };
                    calls_for(source)
                })
                .collect();
            assert!(
                measurements[3] <= measurements[0] * 12,
                "lazy quote admission repeats prefix work: {measurements:?}"
            );
        }
    }

    /// Strips counted over one full parse-and-render of `src`.
    ///
    /// On its own thread for two reasons: the counter is thread-local, so a
    /// fresh thread cannot pick up another test's tally, and a 200-deep quote
    /// recurses past the default 2 MiB test stack in a debug build.
    fn calls_for(src: String) -> u64 {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                QUOTE_PREFIX_CALLS.with(|c| c.set(0));
                let _ = crate::parse(&src);
                QUOTE_PREFIX_CALLS.with(|c| c.get())
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// One measurement: the work the document contains, and the strips it cost.
    struct Measured {
        work: u64,
        calls: u64,
    }

    impl Measured {
        fn per_unit(&self) -> f64 {
            self.calls as f64 / self.work as f64
        }
    }

    /// Assert that a document's strip count stays PROPORTIONAL to the work it
    /// contains, across a doubling of that work.
    ///
    /// Three separate claims, each of which has been shown to fail on its own:
    ///
    /// 1. A floor. Every quote marker has to be stripped at least once to be
    ///    recognized, so a count below the work is not a faster parser - it is a
    ///    counter that stopped counting. Without this the rest would pass on a
    ///    dead counter reporting zero.
    /// 2. A ceiling, generous enough to survive honest constant-factor drift.
    /// 3. The shape, which is the load-bearing half: a cost proportional to the
    ///    work holds its per-unit rate flat or falling as the document grows,
    ///    while anything superlinear in the work must make it climb. This one
    ///    is a statement about the curve rather than its constant, so it still
    ///    fires when a ceiling has been raised past the point of usefulness.
    ///
    /// The comparison is cross-multiplied integers so no float rounding enters
    /// the verdict; a tenth is allowed for drift. Floats appear in the failure
    /// messages only.
    fn assert_proportional(what: &str, small: Measured, large: Measured, ceiling: u64) {
        assert!(
            small.calls >= small.work && large.calls >= large.work,
            "{what}: the strip counter is not counting - {} strips for {} units of work, \
             {} for {}",
            small.calls,
            small.work,
            large.calls,
            large.work,
        );
        assert!(
            large.calls <= ceiling * large.work,
            "{what}: {} strips for {} units of work ({:.1} each, ceiling {ceiling}) - \
             the quote markers are being re-walked",
            large.calls,
            large.work,
            large.per_unit(),
        );
        assert!(
            large.calls * small.work * 10 <= small.calls * large.work * 11,
            "{what}: the per-unit strip cost climbs with size ({:.1} at the smaller size, \
             {:.1} at the larger): the quote markers are being re-walked",
            small.per_unit(),
            large.per_unit(),
        );
    }

    /// A blockquote depth ladder: line `i` carries `i` quote markers, so the
    /// document holds `depth * (depth + 1) / 2` markers in total - which is the
    /// work, and is exactly what carve-js spends on it.
    fn ladder(depth: usize) -> (String, u64) {
        ladder_of(depth, "a")
    }

    /// The same ladder with a chosen line body, so a guard can say what the
    /// quoted text is made of.
    fn ladder_of(depth: usize, body: &str) -> (String, u64) {
        let mut out = String::new();
        for i in 1..=depth {
            for _ in 0..i {
                out.push_str("> ");
            }
            out.push_str(body);
            out.push('\n');
        }
        (out, (depth * (depth + 1) / 2) as u64)
    }

    /// One deep quoted paragraph followed by `depth` lazy continuation lines,
    /// each of which has to ask whether that paragraph is still open. The work
    /// is one pass over every line at every level.
    fn lazy_tail(depth: usize) -> (String, u64) {
        let mut out = "> ".repeat(depth);
        out.push_str("a\n");
        for _ in 0..depth {
            out.push_str("lazy\n");
        }
        (out, ((depth + 1) * depth) as u64)
    }

    /// A ladder must not cost strips per nesting level per line.
    ///
    /// Before the fix the quote's paragraph-open state was decided eagerly on
    /// every quoted line, and deciding it walked the line down to its innermost
    /// content - a walk each enclosing level repeated over the same markers:
    ///
    /// | depth | markers | strips, eager | strips, deferred |
    /// | ----- | ------- | ------------- | ---------------- |
    /// | 25    | 325     | 6,495         | 3,245            |
    /// | 50    | 1,275   | 35,495        | 12,120           |
    /// | 100   | 5,050   | 223,495       | 46,745           |
    /// | 200   | 20,100  | 1,556,994     | 183,494          |
    ///
    /// carve-rs still spends about 9 strips per marker where carve-js spends 1.
    /// That residue is the prefix re-scan every implementation shares
    /// (markup-carve/carve#752), it is flat in depth, and it is not this
    /// guard's subject.
    #[test]
    fn a_depth_ladder_costs_strips_in_proportion_to_its_markers() {
        let (small_src, small_work) = ladder(100);
        let (large_src, large_work) = ladder(200);
        assert_proportional(
            "depth ladder",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            16,
        );
    }

    /// Ordinary quoted PROSE holding a colon must stay on the deferred path.
    ///
    /// The ladder above is made of `a`, so it says nothing about the pre-test
    /// that decides when §12's absorption flag has to be resolved
    /// (markup-carve/carve-rs#738). That pre-test has to be conservative, and
    /// the conservative reading is a COLON RUN: widening it to any `:` puts the
    /// cubic walk back for every `Note:`, `12:30` and `https://` in a quoted
    /// document - 1,556,994 strips at depth 200 rather than 183,494 - while the
    /// colon-free ladder above stays green and sees none of it.
    ///
    /// A line that really does carry `:::` is not covered, and cannot be:
    /// deciding the absorption on such a line is what #738 asks for, and that
    /// answer is only reachable by walking to the innermost content.
    #[test]
    fn quoted_prose_holding_a_colon_still_defers() {
        let (small_src, small_work) = ladder_of(100, "Note: at 12:30, see https://example.com");
        let (large_src, large_work) = ladder_of(200, "Note: at 12:30, see https://example.com");
        assert_proportional(
            "a depth ladder of colon-bearing prose",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            16,
        );
    }

    /// A depth ladder ending in one lazy line must not replay the body at every
    /// level: with no fence in it there is nothing to replay for.
    #[test]
    fn a_depth_ladder_ending_in_a_lazy_line_costs_strips_in_proportion_to_its_markers() {
        let with_lazy = |depth: usize| {
            let (mut src, work) = ladder(depth);
            src.push_str("lazy\n");
            (src, work)
        };
        let (small_src, small_work) = with_lazy(100);
        let (large_src, large_work) = with_lazy(200);
        assert_proportional(
            "depth ladder ending in a lazy line",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            16,
        );
    }

    /// A run of lazy continuation lines must resolve the open paragraph once,
    /// not once per line.
    ///
    /// The ladder above never reads the paragraph-open state at all, so it
    /// cannot see this: deferring the walk without caching its answer passes
    /// the ladder guard and the whole corpus byte for byte, while costing
    /// 4,143,610 strips here against 103,910 - and climbing, 28 to 53 to 103
    /// per unit across depths 50, 100 and 200, where the cached answer falls,
    /// 2.86 to 2.68 to 2.58.
    #[test]
    fn lazy_continuation_under_a_deep_quote_resolves_the_paragraph_once() {
        let (small_src, small_work) = lazy_tail(100);
        let (large_src, large_work) = lazy_tail(200);
        assert_proportional(
            "lazy continuation under a deep quote",
            Measured {
                work: small_work,
                calls: calls_for(small_src),
            },
            Measured {
                work: large_work,
                calls: calls_for(large_src),
            },
            8,
        );
    }
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
#[cfg(test)]
mod footnote_body_floor_unit {
    use super::{footnote_body_floor, indent_columns, leading_ws};

    #[test]
    fn a_tab_makes_the_two_spellings_disagree() {
        // THE PROOF. One tab indents the definition line. As characters it is
        // 1, so the old spelling asked continuations to reach column 3; as
        // columns it is 4, so the floor is 6. A continuation at column 4 or 5
        // would have been accepted under the old spelling and must not be.
        let def = "\t[^a]: note";
        assert_eq!(leading_ws(def), 1, "the character count");
        assert_eq!(indent_columns(def), 4, "the column count");
        assert_ne!(
            leading_ws(def) + 2,
            footnote_body_floor(def),
            "the two spellings must differ on a tabbed definition line, or this \
             test is pinning nothing"
        );
        assert_eq!(footnote_body_floor(def), 6);
    }

    #[test]
    fn a_mixed_run_disagrees_too() {
        // A space then a tab: the tab advances to the next 4-stop from column
        // 1, so the indentation is still 4 columns while being 2 characters.
        // Included because a fix that special-cased a LEADING tab would pass
        // the case above and fail here.
        let def = " \t[^a]: note";
        assert_eq!(leading_ws(def), 2);
        assert_eq!(indent_columns(def), 4);
        assert_eq!(footnote_body_floor(def), 6);
    }

    #[test]
    fn control_space_only_indentation_is_unchanged() {
        // CONTROL, and the reason the bug was latent: for space-only
        // indentation the two counts are equal, so every reachable input today
        // gets the same answer either way. This is what must NOT move.
        for def in ["[^a]: note", "  [^a]: note", "      [^a]: note"] {
            assert_eq!(leading_ws(def), indent_columns(def), "{def}");
            assert_eq!(footnote_body_floor(def), leading_ws(def) + 2, "{def}");
        }
    }

    #[test]
    fn control_the_floor_is_the_indentation_plus_two() {
        // CONTROL for the other half of the expression. §16 asks for ">= 2"
        // relative to the definition line; a mutation changing the addend
        // passes every unit assertion above that only compares the two
        // spellings, so the constant is pinned on its own.
        assert_eq!(footnote_body_floor("[^a]: x"), 2);
        assert_eq!(footnote_body_floor("    [^a]: x"), 6);
    }
}

#[cfg(test)]
mod promote_block_images_is_iterative {
    use super::promote_block_images;
    use crate::ast::{BlockNode, Div};

    /// Nest `depth` divs, innermost first, and return the outer level.
    fn nested_divs(depth: usize) -> Vec<BlockNode> {
        let mut children: Vec<BlockNode> = Vec::new();
        for _ in 0..depth {
            children = vec![BlockNode::Div(Div {
                attrs: None,
                label: None,
                children,
                pos: None,
            })];
        }
        children
    }

    /// Nothing pinned that this pass descends into a BLOCK QUOTE.
    ///
    /// Found by mutation while turning the pass into a worklist: drop the
    /// `BlockNode::BlockQuote` arm and all 155 corpus tests still pass, as do
    /// `reference_image_fmt`, `unresolved_reference_shape`,
    /// `caption_inside_a_list_item`, `non_html_parity` and
    /// `position_spans_match_source`. The behavior does change - the quote
    /// publishes `<p><img ...></p>` instead of a bare block image - so the
    /// descent set was live and simply unpinned. Every other arm has a corpus
    /// document behind it; this one did not.
    #[test]
    fn a_resolved_reference_image_promotes_inside_a_block_quote() {
        let html = crate::to_html("> ![a][i]\n\n[i]: p.png\n");
        assert!(
            !html.contains("<p>"),
            "the sole image in the quote must promote to a block image, got: {html}"
        );
        assert!(
            html.contains("<img src=\"p.png\" alt=\"a\">"),
            "got: {html}"
        );
    }

    /// THE PROOF that the pass costs no native stack per level.
    ///
    /// The thread's stack is 256 KiB and the tree is 4000 levels deep. The
    /// recursive spelling this pass had until markup-carve/carve-wasm#44 spent
    /// about 1.4 KiB of stack per level, so it needed roughly 5.5 MiB here and
    /// aborted the process; the worklist spelling needs a constant frame and a
    /// heap vector. Revert `promote_block_images` to recursion and this test
    /// takes the whole run down with it, which is what makes it load-bearing.
    ///
    /// 4000 levels is far past `MAX_NESTING_DEPTH`, and deliberately so. The
    /// pass runs only over parser-produced trees, so 200 is the real ceiling
    /// and 200 recursive levels would still fit a test thread - the depth here
    /// is chosen so the difference between the two spellings cannot be read as
    /// measurement noise.
    ///
    /// The tree is leaked rather than dropped. Teardown is compiler-generated
    /// recursive drop glue and would overflow on its own, which would prove
    /// nothing about this pass.
    #[test]
    fn a_four_thousand_level_tree_survives_a_small_stack() {
        let worker = std::thread::Builder::new()
            .stack_size(256 * 1024)
            .spawn(|| {
                let mut blocks = nested_divs(4000);
                promote_block_images(&mut blocks, false, true);
                promote_block_images(&mut blocks, true, false);
                std::mem::forget(blocks);
            })
            .expect("spawn");
        worker.join().expect("the pass must not overflow the stack");
    }
}

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

#[cfg(test)]
mod collector_scope_tests {
    use super::*;

    #[test]
    fn an_unwinding_nested_collector_restores_its_parent() {
        let (_, outer) = collecting_unattached_block_attrs(|| {
            note_unattached_block_attrs(Some(Pos {
                start_line: 1,
                ..Pos::default()
            }));
            let result = std::panic::catch_unwind(|| {
                collecting_unattached_block_attrs(|| {
                    note_unattached_block_attrs(Some(Pos {
                        start_line: 2,
                        ..Pos::default()
                    }));
                    panic!("extension failed");
                });
            });
            assert!(result.is_err());
            note_unattached_block_attrs(Some(Pos {
                start_line: 3,
                ..Pos::default()
            }));
        });
        assert_eq!(
            outer.iter().map(|pos| pos.start_line).collect::<Vec<_>>(),
            vec![1, 3]
        );
        assert!(UNATTACHED_BLOCK_ATTRS.with(|slot| slot.borrow().is_none()));
    }
}

#[cfg(test)]
mod comment_fence_work {
    use super::COMMENT_FENCE_BYTES;

    fn measure(n: usize) -> (u64, u64) {
        let mut source = String::new();
        for width in 3..n + 3 {
            source.push_str(&"%".repeat(width));
            source.push_str(" x\n");
        }
        COMMENT_FENCE_BYTES.with(|count| count.set(0));
        assert_eq!(crate::to_html(&source), "");
        let scanned = COMMENT_FENCE_BYTES.with(|count| count.get());
        (source.len() as u64, scanned)
    }

    #[test]
    fn unterminated_comment_fence_openers_scan_proportional_bytes() {
        // Widths grow with the opener count, so input bytes grow quadratically.
        // A suffix rescan per opener adds another factor; count both opener
        // and closer scans to detect it without depending on machine load.
        let (small_bytes, small_scanned) = measure(64);
        let (large_bytes, large_scanned) = measure(128);
        assert!(small_scanned >= small_bytes && large_scanned >= large_bytes,
            "the counter must observe the input: {small_scanned}/{small_bytes}, {large_scanned}/{large_bytes}");
        assert!(
            large_scanned <= 16 * large_bytes,
            "comment fence scans visited {large_scanned} bytes for {large_bytes} input bytes"
        );
        assert!(large_scanned * small_bytes * 10 <= small_scanned * large_bytes * 11,
            "comment fence work per byte grew: {small_scanned}/{small_bytes}, {large_scanned}/{large_bytes}");
    }
}

#[cfg(test)]
thread_local! {
    static INNERMOST_CONTENT_SCANS: Cell<usize> = const { Cell::new(0) };
    static INNERMOST_MARKER_STEPS: Cell<usize> = const { Cell::new(0) };
    static REUSE_MARKER_TAIL: Cell<bool> = const { Cell::new(true) };
}

#[cfg(test)]
mod nested_list_content_scans {
    #[test]
    fn carried_boundaries_match_fresh_scans() {
        use super::*;
        for marker in ["- ", "1. ", "-{.x} ", "- [x] ", "\t- "] {
            for tail in [
                "end\n",
                "α *bold*\n   # heading\n",
                "``` =html\n<pre>x</pre>\n```\n",
            ] {
                let source = format!("{}{tail}", marker.repeat(24));
                for positions in [false, true] {
                    let options = Options {
                        positions,
                        source_lines: positions,
                        ..Options::default()
                    };
                    REUSE_MARKER_TAIL.with(|flag| flag.set(false));
                    let expected = parse_with_options(&source, &options);
                    REUSE_MARKER_TAIL.with(|flag| flag.set(true));
                    assert_eq!(
                        parse_with_options(&source, &options),
                        expected,
                        "{marker:?} {tail:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn descendants_reuse_the_marker_tail_boundary() {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                for depth in [32, 64, 128, 192] {
                    super::INNERMOST_CONTENT_SCANS.with(|count| count.set(0));
                    super::INNERMOST_MARKER_STEPS.with(|count| count.set(0));
                    let source = format!("{}end\n", "- ".repeat(depth));
                    let html = crate::to_html(&source);
                    assert_eq!(html.matches("<ul>").count(), depth);
                    assert_eq!(super::INNERMOST_CONTENT_SCANS.with(|count| count.get()), 0);
                    assert!(super::INNERMOST_MARKER_STEPS.with(|count| count.get()) <= depth * 2);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}

#[cfg(test)]
mod reference_definition_original_positions {
    use super::*;

    #[test]
    fn definition_spans_select_original_codepoints() {
        for newline in ["\n", "\r\n", "\r"] {
            for prefix in ["", "\u{feff}", "😀 prose", "\u{feff}😀 prose"] {
                let definition = "[a]: /first";
                let source = if prefix.is_empty() || prefix == "\u{feff}" {
                    format!("{prefix}{definition}")
                } else {
                    format!("{prefix}{newline}{newline}{definition}")
                };
                let options = Options {
                    positions: true,
                    ..Options::default()
                };
                let doc = parse_with_options(&source, &options);
                let node = doc
                    .children
                    .iter()
                    .find_map(|node| match node {
                        BlockNode::LinkReferenceDefinition(def) => Some(def),
                        _ => None,
                    })
                    .expect("definition");
                let pos = node.pos.as_ref().expect("position");
                let selected: String = source
                    .chars()
                    .skip(pos.start_offset)
                    .take(pos.end_offset - pos.start_offset)
                    .collect();
                assert_eq!(selected, definition, "{source:?}");
                assert_eq!(pos.start_column, if prefix == "\u{feff}" { 2 } else { 1 });
                assert_eq!(
                    pos.end_column - pos.start_column,
                    definition.chars().count()
                );
            }
        }
    }
}

#[cfg(test)]
mod reference_definition_provenance {
    use super::*;

    #[test]
    fn hoisted_definitions_keep_the_authored_line() {
        for (source, expected) in [
            ("---\ntitle: x\n---\n\n[r]: /u", "[r]: /u"),
            ("> [r]: /u", "> [r]: /u"),
            ("- [r]: /u", "- [r]: /u"),
            ("[^a]: note\n  [r]: /u\n\nsee[^a] [t][r]", "  [r]: /u"),
            (
                "[^a]: note\n  [^b]: nested\n    [r]: /u\n\nsee[^a]",
                "    [r]: /u",
            ),
            ("😀\0 prose\n\n[r]: /u", "[r]: /u"),
        ] {
            for newline in ["\n", "\r\n", "\r"] {
                let source = format!("\u{feff}{}", source.replace('\n', newline));
                let doc = parse_with_options(&source, &Options::default().with_positions(true));
                let pos = doc
                    .children
                    .iter()
                    .find_map(|node| match node {
                        BlockNode::LinkReferenceDefinition(def) => def.pos.as_ref(),
                        _ => None,
                    })
                    .expect("definition position");
                let selected: String = source
                    .chars()
                    .skip(pos.start_offset)
                    .take(pos.end_offset - pos.start_offset)
                    .collect();
                assert_eq!(selected, expected, "{source:?}");
            }
        }
    }

    #[test]
    fn footnote_definition_columns_include_the_bom() {
        let options = Options::default().with_positions(true);
        let doc = parse_with_options("\u{feff}[^n]: {empty}", &options);
        let pos = doc.footnote_def_pos.get("n").expect("footnote position");
        assert_eq!(pos.start_column, 2);
        assert_eq!(pos.start_offset, 1);
    }
}

#[cfg(test)]
mod container_open_readings {
    //! COUNTED guards on a RECOVERED container's nesting cost
    //! (markup-carve/carve#2693, implemented here by carve-rs#2260).
    //!
    //! They live here rather than in `tests/` for the two reasons
    //! `quote_prefix_calls` gives: the counters are `#[cfg(test)]`, and a count
    //! is a property of the algorithm rather than of the machine. The second
    //! reason binds harder here than usual, because the shape these guards name
    //! does not appear in `tests/perf_regressions.rs` at all and that file is
    //! OFF the pull-request path - so a timed spelling of this would have let a
    //! regression merge before anything measured it.
    //!
    //! Every colon opener the timing suite builds - `::: note`, `::: d`,
    //! `::: index`, and the widening `::: d` ladder - reads as VALID metadata.
    //! Nothing there puts an invalid opener on the nesting path, which is the
    //! blind spot carve-php merged a cubic regression through.

    use super::{CONTAINER_OPEN_READINGS, CONTAINER_RECOVERY_ACCEPTS, CONTAINER_RECOVERY_CHARS};

    struct Counted {
        readings: u64,
        recovery_chars: u64,
        recoveries: u64,
        nested: usize,
    }

    /// Counts over one full parse-and-render of `src`, on its own thread: the
    /// counters are thread-local, and a deep ladder recurses past the default
    /// test stack in a debug build.
    fn count(src: String) -> Counted {
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                CONTAINER_OPEN_READINGS.with(|c| c.set(0));
                CONTAINER_RECOVERY_CHARS.with(|c| c.set(0));
                CONTAINER_RECOVERY_ACCEPTS.with(|c| c.set(0));
                let html = crate::to_html(&src);
                Counted {
                    readings: CONTAINER_OPEN_READINGS.with(|c| c.get()),
                    recovery_chars: CONTAINER_RECOVERY_CHARS.with(|c| c.get()),
                    recoveries: CONTAINER_RECOVERY_ACCEPTS.with(|c| c.get()),
                    nested: html.matches("<aside").count() + html.matches("<div").count(),
                }
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// `depth` nested colon containers and their closers, every line the same
    /// length at every depth.
    ///
    /// Widths alternate 3/4 rather than widening outward, so a per-byte reading
    /// measures the nesting and not the fence growing with the ladder. The
    /// metadata decides validity and nothing else: `note Bare title!` is an
    /// unquoted title, which §3 refuses and §12 recovers, and
    /// `noteaaaaaaaaaaaa` is a plain kind of the same byte length.
    fn ladder(depth: usize, metadata: &str) -> String {
        let widths: Vec<usize> = (0..depth).map(|i| 3 + i % 2).collect();
        let mut out = String::new();
        for width in &widths {
            out.push_str(&":".repeat(*width));
            out.push(' ');
            out.push_str(metadata);
            out.push('\n');
        }
        out.push_str("body\n");
        for width in widths.iter().rev() {
            out.push_str(&":".repeat(*width));
            out.push('\n');
        }
        out
    }

    fn recovered(depth: usize) -> String {
        ladder(depth, "note Bare title!")
    }

    fn valid(depth: usize) -> String {
        ladder(depth, "noteaaaaaaaaaaaa")
    }

    /// The ladder has to reach the depth it claims, in both spellings, or every
    /// count below is a count of something else.
    #[test]
    fn both_ladders_nest_to_their_depth() {
        for depth in [100usize, 200] {
            assert_eq!(recovered(depth).len(), valid(depth).len());
            assert_eq!(count(recovered(depth)).nested, depth);
            assert_eq!(count(valid(depth)).nested, depth);
        }
    }

    /// RECOVERY MUST COST WHAT A VALID OPENER COSTS, to the reading.
    ///
    /// This is the assertion that pins the ruling rather than the parser: the
    /// two ladders nest alike and differ only in whether §3 accepts the
    /// metadata, so any work the recovery path adds per level shows up here as
    /// an inequality. It is what carve-php#2817 would have failed - its
    /// recovery branch called two full-run helpers once per level, which no
    /// valid opener paid for.
    #[test]
    fn a_recovered_opener_costs_no_more_readings_than_a_valid_one() {
        for depth in [100usize, 200] {
            let rec = count(recovered(depth));
            let val = count(valid(depth));
            assert!(
                rec.readings > 0 && val.readings > 0,
                "the opener-reading counter is dead at depth {depth}",
            );
            assert_eq!(
                rec.readings, val.readings,
                "depth {depth}: a recovered container cost {} opener readings against {} for a \
                 valid one - the recovery path is doing work per level",
                rec.readings, val.readings,
            );
        }
    }

    /// Only the recovered ladder RECOVERS a container, so the guard above is
    /// measuring the construct it is named for and not two spellings of the
    /// same parse.
    ///
    /// The valid ladder still ENTERS the recovery branch - its `body` line is
    /// no opener by either reading - so the claim is on what the branch
    /// accepts, not on whether it ran.
    #[test]
    fn only_the_recovered_ladder_recovers_a_container() {
        let rec = count(recovered(200));
        let val = count(valid(200));
        assert!(
            rec.recoveries >= 200,
            "the recovered ladder recovered {} containers at depth 200",
            rec.recoveries,
        );
        assert_eq!(
            val.recoveries, 0,
            "the valid ladder recovered {} containers - its openers are supposed to parse",
            val.recoveries,
        );
    }

    /// A recovered ladder, every opener carrying a metadata tail of `tail`
    /// characters after its kind.
    ///
    /// The tail is what separates a BOUNDED PREFIX from a full-line walk. The
    /// ladder's own lines are the same length at every depth, so varying the
    /// tail varies only how far past the kind there is to read - and a gate
    /// that answers from the prefix cannot see it at all.
    fn tailed(depth: usize, tail: usize) -> String {
        ladder(depth, &format!("note {}", "x".repeat(tail)))
    }

    /// A RECOVERY READING MUST ANSWER FROM A BOUNDED PREFIX.
    ///
    /// The three scans the branch makes are a colon run, the space run after
    /// it, and the kind. Each is a prefix, so the characters per reading hold
    /// flat however much metadata follows. A gate that walked the whole run to
    /// answer a bounded question - carve-php#2817's two helpers, which read a
    /// full indentation run to compare three columns - makes this climb with
    /// the run it walks.
    ///
    /// Quadrupling the tail is the axis rather than deepening the ladder,
    /// because carve-rs nests containers FLUSH LEFT: a body line that starts
    /// with a space opens nothing, so an opener on the nesting path has no
    /// indentation run to re-walk and depth alone cannot express this claim.
    #[test]
    fn a_recovery_reading_reads_a_bounded_prefix() {
        let small = count(tailed(100, 32));
        let large = count(tailed(100, 128));
        let small_per = small.recovery_chars as f64 / small.readings as f64;
        let large_per = large.recovery_chars as f64 / large.readings as f64;
        assert!(
            small.recovery_chars > 0 && large.recovery_chars > 0,
            "the recovery-character counter is dead: {} and {}",
            small.recovery_chars,
            large.recovery_chars,
        );
        assert!(
            large.recovery_chars * small.readings * 10
                <= small.recovery_chars * large.readings * 11,
            "the recovery characters per reading grew with the metadata tail \
             ({small_per:.2} at 32 characters, {large_per:.2} at 128): a recovery gate is \
             walking the run instead of reading its prefix",
        );
    }

    /// Shared colon boundaries keep opener readings proportional to input lines.
    #[test]
    fn opener_readings_stay_proportional_to_the_nesting_walk() {
        let work = |depth: u64| depth;
        let small = count(recovered(100));
        let large = count(recovered(200));
        let small_work = work(100);
        let large_work = work(200);
        assert!(
            small.readings >= 100 && large.readings >= 200,
            "the opener-reading counter is not counting: {} readings at depth 100, {} at 200",
            small.readings,
            large.readings,
        );
        assert!(
            large.readings <= 5 * large_work,
            "{} opener readings for {large_work} units of nesting work ({:.2} each) - \
             an opener is being read more than five times per level",
            large.readings,
            large.readings as f64 / large_work as f64,
        );
        assert!(
            large.readings * small_work * 10 <= small.readings * large_work * 11,
            "the per-unit opener-reading cost climbs with depth ({:.3} at depth 100, {:.3} at \
             200): a recovered container is being re-read per level",
            small.readings as f64 / small_work as f64,
            large.readings as f64 / large_work as f64,
        );
    }
}

#[cfg(test)]
mod ascii_heading_id_tests {
    use super::{
        de_typography, sanitize_id_source, slugify_parse, AsciiHeadingIds, HeadingIdOptions,
    };

    #[test]
    fn ascii_heading_ids_keep_separator_number_and_case_rules_in_every_mode() {
        let ascii: String = (0..=127).map(char::from).collect();
        assert_eq!(sanitize_id_source(&ascii), ascii);
        assert_eq!(de_typography(&ascii), ascii);
        for ascii in [
            AsciiHeadingIds::Off,
            AsciiHeadingIds::Fold,
            AsciiHeadingIds::Strict,
        ] {
            for lowercase in [false, true] {
                let options = HeadingIdOptions { ascii, lowercase };
                for (source, expected) in [
                    ("  ASCII Title... ", "ASCII-Title"),
                    ("A\0B\tC\nD", "A-B-C-D"),
                    ("123 & punctuation", "s-123-punctuation"),
                    ("", "s"),
                    ("---", "s"),
                    ("A Z", "A-Z"),
                ] {
                    let expected = if lowercase {
                        expected.to_ascii_lowercase()
                    } else {
                        expected.into()
                    };
                    assert_eq!(
                        slugify_parse(source, options),
                        expected,
                        "{source:?}: {options:?}"
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod wrapped_attrs_work_tests {
    use super::*;

    #[test]
    fn unclosed_attribute_lines_do_not_repeatedly_copy_the_paragraph_tail() {
        for prefix in ["", "> ", "- "] {
            for suffix in ["", "}\n", "\n}\n", "\n{#valid\n.class}\ntext\n"] {
                let source = format!("{prefix}{{abcd\n").repeat(8000) + suffix;
                ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(0));
                let doc = parse(&source);
                assert!(!doc.children.is_empty());
                let work = ATTRS_LOOKAHEAD_BYTES.with(Cell::get);
                assert!(
                    work <= source.len() * 3,
                    "copied {work} bytes for {} source bytes",
                    source.len()
                );
            }
        }
    }

    #[test]
    fn container_collectors_stop_before_the_next_invalid_fragment() {
        for source in [
            "> {abcd\n".repeat(8000) + "> }\n",
            String::from("- x\n") + &"  {abcd\n".repeat(8000) + "  }\n",
        ] {
            ATTRS_LOOKAHEAD_BYTES.with(|work| work.set(0));
            let doc = parse(&source);
            assert!(!doc.children.is_empty());
            let work = ATTRS_LOOKAHEAD_BYTES.with(Cell::get);
            assert!(
                work <= source.len() * 8,
                "read {work} bytes for {} source bytes",
                source.len()
            );
        }
    }

    #[test]
    fn a_valid_three_line_block_attaches_after_a_rejected_opener_in_each_container() {
        for source in [
            "{abcd\n{#valid\n  .class\nrole=note}\ntext\n",
            "> {abcd\n> {#valid\n>   .class\n> role=note}\n> text\n",
            "- {abcd\n  {#valid\n    .class\n  role=note}\n  text\n",
        ] {
            let html = crate::to_html(source);
            assert!(
                html.contains("id=\"valid\" class=\"class\" role=\"note\""),
                "{html}"
            );
            assert!(html.contains("{abcd"), "{html}");
        }
    }

    #[test]
    fn a_later_wrapped_attribute_block_keeps_its_closer() {
        let doc = parse("{abcd\n\n{#valid\n.class}\ntext\n");
        assert_eq!(
            plain_inlines_parse(match &doc.children[0] {
                BlockNode::Paragraph(p) => &p.children,
                _ => panic!("unclosed opener must remain a paragraph"),
            }),
            "{abcd"
        );
        let BlockNode::Paragraph(p) = &doc.children[1] else {
            panic!("expected attributed paragraph")
        };
        let attrs = p.attrs.as_ref().expect("wrapped block survives");
        assert_eq!(attrs.id.as_deref(), Some("valid"));
        assert_eq!(attrs.classes, ["class"]);
    }
}

#[cfg(test)]
mod shared_colon_work_tests {
    use super::*;

    #[test]
    fn rejected_fence_carry_scans_only_new_chunks() {
        for positions in [false, true] {
            for retained in [false, true] {
                for count in [64, 256, 1024] {
                    let source = format!(
                        "- a\n    ~~~\n{}",
                        format!(
                            "  ````\n  code\n  ````\n{}after\n",
                            if retained { "  " } else { "" }
                        )
                        .repeat(count)
                    );
                    source_map::MAPPED_SOURCE_SCAN_BYTES.with(|work| work.set(0));
                    CODE_INDEX_LINES.with(|work| work.set(0));
                    let html = crate::to_html_with_options(
                        &source,
                        &Options {
                            positions,
                            ..Options::default()
                        },
                    );
                    assert_eq!(
                        html.matches("<pre>").count(),
                        if retained { count } else { 1 }
                    );
                    assert_eq!(html.matches("after").count(), count);
                    assert!(
                        source_map::MAPPED_SOURCE_SCAN_BYTES.with(Cell::get) <= source.len() * 64,
                        "count={count}, positions={positions}, scan={}, bytes={}",
                        source_map::MAPPED_SOURCE_SCAN_BYTES.with(Cell::get),
                        source.len()
                    );
                    assert!(CODE_INDEX_LINES.with(Cell::get) <= source.lines().count() * 64);
                }
            }
        }
    }

    #[test]
    fn quoted_anonymous_body_comments_share_their_index() {
        for level in 0..=2 {
            for count in [64, 512, 4096] {
                let prefix = "> ".repeat(level);
                let mut body = LineBuffer::default();
                for _ in 0..count {
                    for line in ["- :::", "%%%", "comment", "%%%", "  :::"] {
                        body.push_at(format!("{prefix}{line}"), None, None);
                    }
                }
                let mut comments = QuotedBodyComments::default();
                COMMENT_INDEX_LINES.with(|work| work.set(0));
                for from in (0..body.lines.len()).step_by(5) {
                    assert!(quoted_colon_body_follows(
                        &body.lines,
                        from,
                        level,
                        2,
                        &mut comments,
                    ));
                }
                assert!(COMMENT_INDEX_LINES.with(Cell::get) <= body.lines.len());
            }
        }
    }

    #[test]
    fn quoted_body_comment_indices_stop_at_their_quote_scope() {
        let mut body = LineBuffer::default();
        for _ in 0..4096 {
            body.push_at("> prelude".to_owned(), None, None);
        }
        let mut groups = Vec::new();
        for level in 2..=32 {
            let prefix = "> ".repeat(level);
            groups.push((level, body.lines.len()));
            body.push_at(format!("{prefix}%%%"), None, None);
            body.push_at(format!("{prefix}%%%"), None, None);
            body.push_at("departure".to_owned(), None, None);
        }
        COMMENT_INDEX_LINES.with(|work| work.set(0));
        for (level, from) in groups {
            assert!(QuotedBodyComments::default().closes_after(&body.lines, from, level, 3));
        }
        assert!(COMMENT_INDEX_LINES.with(Cell::get) <= 32 * 2);
    }

    #[test]
    fn quoted_body_comment_index_reenters_and_can_rewind() {
        let mut body = LineBuffer::default();
        for line in ["> %%%", "> %%%", "departure", "> %%%%", "> %%%%"] {
            body.push_at(line.to_owned(), None, None);
        }
        let mut comments = QuotedBodyComments::default();
        assert!(comments.closes_after(&body.lines, 0, 1, 3));
        assert!(comments.closes_after(&body.lines, 3, 1, 4));
        assert!(!comments.closes_after(&body.lines, 3, 1, 3));
        assert!(comments.closes_after(&body.lines, 0, 1, 3));
    }

    #[test]
    fn literal_code_index_reenters_a_later_quote_scope() {
        let mut body = LineBuffer::default();
        for line in [
            ">   ``` info",
            ">   ```",
            "departure",
            ">   ~~~ info",
            ">   ~~~",
        ] {
            body.push_at(line.to_owned(), None, None);
        }
        let mut lookup = LiteralCodeLookup::default();
        assert!(literal_code_closes(
            &body.lines,
            0,
            1,
            2,
            2,
            detect_fence_open("``` info").unwrap(),
            &mut lookup
        ));
        assert!(literal_code_closes(
            &body.lines,
            3,
            1,
            2,
            2,
            detect_fence_open("~~~ info").unwrap(),
            &mut lookup
        ));
        assert!(!literal_code_closes(
            &body.lines,
            3,
            1,
            2,
            2,
            detect_fence_open("``` info").unwrap(),
            &mut lookup
        ));
    }

    #[test]
    fn literal_code_indices_do_not_scan_unrelated_quote_scopes() {
        let mut body = LineBuffer::default();
        for _ in 0..4096 {
            body.push_at("> prelude".to_owned(), None, None);
        }
        let mut groups = Vec::new();
        for level in 2..=32 {
            let prefix = "> ".repeat(level);
            groups.push((level, body.lines.len()));
            body.push_at(format!("{prefix}  ``` info"), None, None);
            body.push_at(format!("{prefix}  ```"), None, None);
            body.push_at("departure".to_owned(), None, None);
        }
        let mut lookup = LiteralCodeLookup::default();
        let open = detect_fence_open("``` info").unwrap();
        LITERAL_CODE_LINES.with(|work| work.set(0));
        for (level, from) in groups {
            assert!(literal_code_closes(
                &body.lines,
                from,
                level,
                2,
                2,
                open,
                &mut lookup
            ));
        }
        assert!(LITERAL_CODE_LINES.with(Cell::get) <= 32 * 3);
    }

    #[test]
    fn literal_code_columns_store_one_record_per_run() {
        let mut column = LiteralCodeColumn::default();
        for index in 0..4096 {
            column.push(index * 2, 1_000_000 - index);
        }
        assert_eq!(column.positions.len(), 4096);
        assert!(column.maxima.len() <= column.positions.len() * 4);
        assert_eq!(column.next(0, 999_999), Some(2));
        assert_eq!(column.next(2, 999_999), None);
        assert_eq!(column.next(8190, 1), None);
    }

    #[test]
    fn literal_code_scope_misses_share_the_departure_bound() {
        for count in [64, 512, 4096] {
            let mut body = LineBuffer::default();
            for _ in 0..count {
                body.push_at("  ``` info".to_string(), None, None);
            }
            body.push_at("- sibling".to_string(), None, None);
            body.push_at("  ```".to_string(), None, None);
            let mut lookup = LiteralCodeLookup::default();
            let open = detect_fence_open("``` info").unwrap();
            LITERAL_CODE_LINES.with(|work| work.set(0));
            for from in 0..count {
                assert!(!literal_code_closes(
                    &body.lines,
                    from,
                    0,
                    2,
                    2,
                    open,
                    &mut lookup
                ));
            }
            assert!(LITERAL_CODE_LINES.with(Cell::get) <= body.lines.len() * 2);
        }
    }

    #[test]
    fn literal_code_misses_refute_varied_unclosed_runs() {
        for level in 0..=2 {
            for count in [64, 512, 4096] {
                let prefix = "> ".repeat(level);
                let mut body = LineBuffer::default();
                for index in 0..count {
                    body.push_at(
                        format!("{prefix}    {} info", "`".repeat(3 + index % 8)),
                        None,
                        None,
                    );
                }
                body.push_at(format!("{prefix}   ``````````"), None, None);
                let mut misses = LiteralCodeLookup::default();
                LITERAL_CODE_LINES.with(|work| work.set(0));
                for index in 0..count {
                    let open =
                        detect_fence_open(&format!("{} info", "`".repeat(3 + index % 8))).unwrap();
                    assert!(!literal_code_closes(
                        &body.lines,
                        index,
                        level,
                        4,
                        4,
                        open,
                        &mut misses
                    ));
                }
                assert!(LITERAL_CODE_LINES.with(Cell::get) <= body.lines.len());
            }
        }
    }

    #[test]
    fn literal_code_descending_bare_runs_share_the_index() {
        for count in [64, 512] {
            let mut body = LineBuffer::default();
            for index in 0..count {
                body.push_at(format!("    {}", "`".repeat(count - index + 3)), None, None);
            }
            let mut misses = LiteralCodeLookup::default();
            LITERAL_CODE_LINES.with(|work| work.set(0));
            for index in 0..count {
                let open = detect_fence_open(&"`".repeat(count - index + 3)).unwrap();
                assert!(!literal_code_closes(
                    &body.lines,
                    index,
                    0,
                    4,
                    4,
                    open,
                    &mut misses
                ));
            }
            assert!(LITERAL_CODE_LINES.with(Cell::get) <= body.lines.len());
        }
    }

    #[test]
    fn literal_code_cached_index_answers_smaller_widths() {
        let mut body = LineBuffer::default();
        for line in [
            "    ```` info",
            "    ``` info",
            "    ```",
            "- sibling",
            "    ```",
            "    ```",
        ] {
            body.push_at(line.to_owned(), None, None);
        }
        let mut misses = LiteralCodeLookup::default();
        assert!(!literal_code_closes(
            &body.lines,
            0,
            0,
            4,
            4,
            detect_fence_open("```` info").unwrap(),
            &mut misses
        ));
        assert!(literal_code_closes(
            &body.lines,
            1,
            0,
            4,
            4,
            detect_fence_open("``` info").unwrap(),
            &mut misses
        ));
        assert!(literal_code_closes(
            &body.lines,
            4,
            0,
            4,
            4,
            detect_fence_open("```").unwrap(),
            &mut misses
        ));
    }

    #[test]
    fn literal_code_varied_columns_and_growing_buffers_are_indexed_once() {
        for count in [64, 512] {
            let mut body = LineBuffer::default();
            for index in 0..count {
                body.push_at(format!("{}``` info", " ".repeat(index + 3)), None, None);
            }
            body.push_at(" ```".to_owned(), None, None);
            let mut lookup = LiteralCodeLookup::default();
            let open = detect_fence_open("``` info").unwrap();
            LITERAL_CODE_LINES.with(|work| work.set(0));
            for index in 0..count {
                assert!(!literal_code_closes(
                    &body.lines,
                    index,
                    0,
                    2,
                    index + 3,
                    open,
                    &mut lookup
                ));
            }
            assert!(LITERAL_CODE_LINES.with(Cell::get) <= body.lines.len());

            let mut growing = LineBuffer::default();
            let mut lookup = LiteralCodeLookup::default();
            LITERAL_CODE_LINES.with(|work| work.set(0));
            for index in 0..count {
                growing.push_at("    ``` info".to_owned(), None, None);
                assert!(!literal_code_closes(
                    &growing.lines,
                    index,
                    0,
                    4,
                    4,
                    open,
                    &mut lookup
                ));
            }
            assert_eq!(LITERAL_CODE_LINES.with(Cell::get), 0);
        }
    }

    #[test]
    fn literal_code_closes_at_any_owned_payload_column() {
        let open = detect_fence_open("``` info").unwrap();
        for (closer, expected) in [
            ("  ```", true),
            ("    ```", true),
            ("   ```", true),
            (" ```", false),
        ] {
            let mut body = LineBuffer::default();
            for line in ["    ``` info", closer] {
                body.push_at(line.to_owned(), None, None);
            }
            assert_eq!(
                literal_code_closes(
                    &body.lines,
                    0,
                    0,
                    2,
                    4,
                    open,
                    &mut LiteralCodeLookup::default()
                ),
                expected
            );
        }
    }

    #[test]
    fn repeated_attachment_probes_share_code_and_comment_indices() {
        for count in [64, 512, 4096] {
            let unit = [
                "::: |", "```", "code", "```", "%%%", "comment", "%%%", ":::", "+",
            ];
            let lines: Vec<_> = (0..count).flat_map(|_| unit).collect();
            let mut code = None;
            let mut comments = None;
            CODE_INDEX_LINES.with(|work| work.set(0));
            COMMENT_INDEX_LINES.with(|work| work.set(0));
            for start in (0..lines.len()).step_by(unit.len()) {
                assert_eq!(
                    attached_block_end(
                        CursorLines::Slice(&lines),
                        start,
                        &mut comments,
                        &mut code,
                        &mut |line, _| line == "+"
                    ),
                    start + unit.len() - 1
                );
            }
            assert_eq!(CODE_INDEX_LINES.with(Cell::get), lines.len());
            assert_eq!(COMMENT_INDEX_LINES.with(Cell::get), lines.len());
        }
    }

    #[test]
    fn repeated_verse_comment_probes_share_their_index() {
        for count in [64, 512, 4096] {
            let source = "::: |\n%%%\nx\n%%%\n:::\n".repeat(count);
            let lines = source.split('\n').count();
            COMMENT_INDEX_LINES.with(|work| work.set(0));
            assert_eq!(colon_fences_left_open(&source), 0);
            assert!(COMMENT_INDEX_LINES.with(Cell::get) <= lines);
            COMMENT_INDEX_LINES.with(|work| work.set(0));
            assert!(!continuation_source_loosens(&source, false));
            assert!(COMMENT_INDEX_LINES.with(Cell::get) <= lines);
            let refs: Vec<_> = source.lines().collect();
            COMMENT_INDEX_LINES.with(|work| work.set(0));
            assert_eq!(
                attached_block_end(
                    CursorLines::Slice(&refs),
                    0,
                    &mut None,
                    &mut None,
                    &mut |_, _| false
                ),
                refs.len()
            );
            assert!(COMMENT_INDEX_LINES.with(Cell::get) <= lines);
        }
    }

    #[test]
    fn nested_bodies_copy_the_payload_once_with_and_without_positions() {
        for positions in [false, true] {
            for depth in [20, 80, 150] {
                let mut source: String = (0..depth)
                    .map(|i| ["::: box\n", "::: >\n", "::: \\\n"][i % 3])
                    .collect();
                source.push_str(&"payload ".repeat(8000));
                source.push('\n');
                source.push_str(&":::\n".repeat(depth));
                COLON_COPIED_BYTES.with(|count| count.set(0));
                let doc =
                    parse_with_options(&source, &Options::default().with_positions(positions));
                let copied = COLON_COPIED_BYTES.with(Cell::get);
                assert!(
                    copied <= source.len() * 2,
                    "{depth} levels copied {copied} bytes for {} source bytes",
                    source.len()
                );
                assert!(crate::render_html(&doc)
                    .unwrap()
                    .contains("payload payload"));
            }
        }
    }
    #[test]
    fn nested_colon_line_references_are_materialized_only_where_needed() {
        for positions in [false, true] {
            for depth in [24, 96, 192] {
                for prefix in [
                    "::: box\ntext\n\n",
                    "::: box\n- item\n\n",
                    "::: box\n```text\ncode\n```\n\n",
                    "::: box\n%%%\ncomment\n%%%\n\n",
                    "::: box\n::: |\nverse\n:::\n\n",
                    "::: box\n> text\n> ```text\n> code\n> ```\n\n",
                    "::: box\n- item\n+\n# attached\n\n",
                    "::: box\n- item\n+\n```text\ncode\n```\n\n",
                    "::: box\n- item\n+\n%%%\ncomment\n%%%\n\n",
                    "::: box\nterm\n:  +\n# attached\n\n",
                ] {
                    for closed in [true, false] {
                        for payload in [
                            "payload\n".repeat(500),
                            format!("``` =html\n<pre>\n{}</pre>\n```\n", "payload\n".repeat(500)),
                        ] {
                            let source = format!(
                                "{}{}{}",
                                prefix.repeat(depth),
                                payload,
                                if closed {
                                    ":::\n".repeat(depth)
                                } else {
                                    String::new()
                                }
                            );
                            COLON_REFERENCE_ENTRIES.with(|count| count.set(0));
                            COLON_CLOSER_LINES.with(|count| count.set(0));
                            let document = parse_with_options(
                                &source,
                                &Options::default().with_positions(positions),
                            );
                            let entries = COLON_REFERENCE_ENTRIES.with(Cell::get);
                            assert!(
                                COLON_CLOSER_LINES.with(Cell::get) <= source.lines().count() * 2
                            );
                            assert!(
                                entries <= source.lines().count() * 2,
                                "{depth} levels, prefix {prefix:?}, closed {closed}: \
                                 materialized {entries} references"
                            );
                            assert!(crate::render_html(&document).unwrap().contains("payload"));
                            if depth == 24 {
                                COLON_CACHE_ENABLED.with(|flag| flag.set(false));
                                let reference = parse_with_options(
                                    &source,
                                    &Options::default().with_positions(positions),
                                );
                                COLON_CACHE_ENABLED.with(|flag| flag.set(true));
                                assert_eq!(
                                    crate::ast_json::to_json(&document),
                                    crate::ast_json::to_json(&reference)
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn cached_boundaries_match_fresh_collection_on_adversarial_bodies() {
        let mut cases = vec![
            "::: outer\n::: inner\npayload\n".to_owned(),
            "::: outer\n::: inner\n```text\n:::\n```\n:::\n:::\n".to_owned(),
            "::: outer\n::: inner\n%%%\n:::\n%%%\n:::\n:::\n".to_owned(),
            "::: outer\n::: |\n::: inner\nverse\n:::\n:::\n:::\n".to_owned(),
            "::: outer\n::: >\n::: \\\nfirst\nsecond\n:::\n:::\n:::\n".to_owned(),
            "::: outer\n::: figure\n![map](map.svg)\n:::\n^ caption\n:::\n".to_owned(),
            "::: outer\n:::: inner\n:::\npayload\n::::\n:::\n".to_owned(),
            "::: outer\n- item\n+\n  ```text\n  x\n  ```\n:::\n".to_owned(),
            "::: box\n- item\n+\n::: note\n  ```\n:::\n  ```\n:::\n:::\n".to_owned(),
        ];
        cases.push(format!(
            "{}payload\n{}",
            "::: box\n".repeat(205),
            ":::\n".repeat(205)
        ));
        cases.push("::: outer\n- item\n  ::: inner\n  payload\n  :::\n:::\n".to_owned());
        for (positions, source_lines) in
            [(false, false), (true, false), (false, true), (true, true)]
        {
            for source in &cases {
                let options = Options::default()
                    .with_positions(positions)
                    .with_source_lines(source_lines);
                COLON_CACHE_ENABLED.with(|flag| flag.set(false));
                let reference = parse_with_options(source, &options);
                COLON_CACHE_ENABLED.with(|flag| flag.set(true));
                let cached = parse_with_options(source, &options);
                assert_eq!(
                    crate::ast_json::to_json(&reference),
                    crate::ast_json::to_json(&cached),
                    "{source}"
                );
                assert_eq!(
                    crate::render_html(&reference).unwrap(),
                    crate::render_html(&cached).unwrap(),
                    "{source}"
                );
            }
        }
    }
}

#[cfg(test)]
mod nested_list_indent_work {
    //! COUNTED guard on the nested-list indentation walk (carve-rs#2301).
    //!
    //! Counts, not durations: a count is a property of the algorithm and
    //! reproduces on any machine under any load, while a duration describes the
    //! machine that took it.

    use super::{BODY_LINES_COLLECTED, INDENT_BYTES_WALKED, MAX_NESTING_DEPTH};

    /// A `depth`-level ladder of `- x`, then `siblings` items at the deepest level.
    fn ladder(depth: usize, siblings: usize) -> String {
        let mut out = String::new();
        for level in 0..depth {
            out.push_str(&"  ".repeat(level));
            out.push_str("- x\n");
        }
        for _ in 0..siblings {
            out.push_str(&"  ".repeat(depth));
            out.push_str("- y\n");
        }
        out
    }

    /// `(indent bytes walked, body lines collected)` over one parse of `src`, on
    /// its own deep-stacked thread so the tallies cannot pick up another test's.
    fn work_for(src: String) -> (u64, u64) {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                INDENT_BYTES_WALKED.with(|c| c.set(0));
                BODY_LINES_COLLECTED.with(|c| c.set(0));
                let _ = crate::parse(&src);
                (
                    INDENT_BYTES_WALKED.with(|c| c.get()),
                    BODY_LINES_COLLECTED.with(|c| c.get()),
                )
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// Work charged to ONE added sibling line at `depth`, isolating the ladder.
    fn per_sibling(depth: usize, siblings: usize) -> (f64, f64) {
        let (bare_bytes, bare_lines) = work_for(ladder(depth, 0));
        let (full_bytes, full_lines) = work_for(ladder(depth, siblings));
        (
            (full_bytes as f64 - bare_bytes as f64) / siblings as f64,
            (full_lines as f64 - bare_lines as f64) / siblings as f64,
        )
    }

    /// The indent walking a sibling line costs is bounded by the square of its
    /// depth, with a ceiling the pre-fix parser could not meet: it charged
    /// about 11.1 bytes per depth squared, and this allows 6.
    #[test]
    fn a_deep_sibling_line_walks_a_bounded_multiple_of_its_depth_squared() {
        for depth in [50usize, 100, 200] {
            let (bytes, _) = per_sibling(depth, 100);
            let ratio = bytes / (depth * depth) as f64;
            assert!(
                ratio <= 6.0,
                "depth {depth}: {bytes:.0} indent bytes per sibling line is {ratio:.2} per \
                 depth squared, over the ceiling of 6"
            );
        }
    }

    /// The re-collection count per line is the enclosing-item count and no
    /// more, so nothing above it multiplies the work again.
    #[test]
    fn a_sibling_line_is_collected_once_per_enclosing_item() {
        for depth in [50usize, 100, 200] {
            let (_, collections) = per_sibling(depth, 100);
            let cap = depth.min(MAX_NESTING_DEPTH) as f64;
            assert!(
                collections <= cap,
                "depth {depth}: {collections:.1} collections per sibling line, over {cap}"
            );
        }
    }

    /// At a FIXED depth the cost is linear in the line count: the work charged
    /// to one sibling line does not depend on how many siblings there are.
    #[test]
    fn at_a_fixed_depth_the_cost_per_line_is_flat() {
        let (hundred, _) = per_sibling(150, 100);
        let (three_hundred, _) = per_sibling(150, 300);
        let ratio = three_hundred / hundred;
        assert!(
            (0.95..=1.05).contains(&ratio),
            "per-line indent work moved by {ratio:.3}x between 100 and 300 sibling lines \
             ({hundred:.0} against {three_hundred:.0} bytes)"
        );
    }

    #[test]
    #[ignore = "a measurement, not a guard"]
    fn measure() {
        for depth in [25usize, 50, 100, 150, 199, 200, 201, 250] {
            let (bytes, collections) = per_sibling(depth, 100);
            println!(
                "depth {depth}: {bytes:.0} indent bytes ({:.2} per depth squared), \
                 {collections:.1} collections per sibling line",
                bytes / (depth * depth) as f64,
            );
        }
    }
}

#[cfg(test)]
mod nested_list_ladder_work {
    //! COUNTED guard on a list ladder deeper than the nesting cap.
    //!
    //! This replaces `perf_regressions::deeply_nested_list_parse_is_bounded`,
    //! which asserted a 10-second wall clock. An absolute duration describes
    //! the machine that chose it: the same test passed on an idle box and
    //! failed at load 42 (12.5 s), so it could not tell a regression from a
    //! busy runner, and its own doc comment recorded the cap being widened from
    //! 2 s to 10 s for exactly that reason. The intent is kept and the
    //! stopwatch is dropped, following `nested_list_indent_work` (#2324).
    //!
    //! The intent was that the nesting cap keeps a deep ladder's
    //! collect-and-reparse from blowing up per level. That is a relationship
    //! and it is stated as one here: past the cap, one more level costs one
    //! more collection per enclosing item and no more, which is the cap, and
    //! the indent bytes that level's own line costs stay a bounded multiple of
    //! its length times the cap. An uncapped per-level rescan fails both.
    //!
    //! ONE parse per depth, all relationships from the same three tallies: a
    //! 300-level ladder is the expensive part, and asserting each relationship
    //! in its own test would parse it three times over.

    use super::{BODY_LINES_COLLECTED, INDENT_BYTES_WALKED, MAX_NESTING_DEPTH};

    /// A `depth`-level ladder of `- x`, each level one column deeper.
    fn ladder(depth: usize) -> String {
        let mut out = String::new();
        for level in 0..depth {
            out.push_str(&"  ".repeat(level));
            out.push_str("- x\n");
        }
        out
    }

    /// `(indent bytes walked, body lines collected)` over one parse, on its own
    /// deep-stacked thread so the tallies cannot pick up another test's.
    fn work_for(depth: usize) -> (f64, f64) {
        let src = ladder(depth);
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(move || {
                INDENT_BYTES_WALKED.with(|c| c.set(0));
                BODY_LINES_COLLECTED.with(|c| c.set(0));
                let doc = crate::parse(&src);
                assert!(
                    matches!(doc.children.first(), Some(crate::BlockNode::List(_))),
                    "the ladder must actually parse into a list, or the tallies below \
                     are measuring a parse that did nothing"
                );
                (
                    INDENT_BYTES_WALKED.with(|c| c.get()) as f64,
                    BODY_LINES_COLLECTED.with(|c| c.get()) as f64,
                )
            })
            .expect("spawn the counting thread")
            .join()
            .expect("the counting thread parses without panicking")
    }

    /// Indent bytes a level's own line may cost: its length grows with its
    /// depth and it is re-read once per enclosing item, which the cap bounds.
    ///
    /// CALIBRATED AGAINST THE REGRESSION IT GUARDS, not against headroom alone.
    /// Undoing #2324's reuse (the `_at` helpers re-measuring the indent they
    /// were handed) takes the two pairs below from 2.25 and 2.58 to 6.16 and
    /// 7.06, so this sits between them: 55% over the measurement and 35% under
    /// the regression. A looser ceiling of 9 was tried first and could not see
    /// that regression at all, which is the thing a guard has to do.
    const BYTES_PER_LEVEL_LENGTH_PER_ENCLOSING_ITEM: f64 = 4.0;

    #[test]
    fn a_ladder_past_the_cap_pays_the_cap_per_added_level_and_no_more() {
        let cap = MAX_NESTING_DEPTH as f64;
        let at_the_cap = work_for(MAX_NESTING_DEPTH);
        let past = work_for(MAX_NESTING_DEPTH + 50);
        let further = work_for(MAX_NESTING_DEPTH + 100);

        for (name, shallow, deep, levels) in [
            ("at the cap to 50 past it", at_the_cap, past, 50.0),
            ("50 past the cap to 100 past it", past, further, 50.0),
        ] {
            // ONE MORE COLLECTION PER ENCLOSING ITEM. The recursion stops at the
            // cap, so a level added past it is re-collected by the items above
            // it and by nothing more. Below the cap this count is the depth
            // itself and grows; past it, it is flat at the cap. An uncapped
            // per-level rescan keeps growing here.
            let per_level_collections = (deep.1 - shallow.1) / levels;
            assert!(
                per_level_collections <= cap,
                "{name}: {per_level_collections:.1} collections per added level, over the \
                 cap of {cap}"
            );
            // AND ITS INDENT IS WALKED A BOUNDED NUMBER OF TIMES. The line's own
            // length is what grows with depth; the number of walks over it is
            // what a regression multiplies (#2324).
            let per_level_bytes = (deep.0 - shallow.0) / levels;
            let line_length = levels.mul_add(0.5, MAX_NESTING_DEPTH as f64) * 2.0;
            let ratio = per_level_bytes / (line_length * cap);
            assert!(
                ratio <= BYTES_PER_LEVEL_LENGTH_PER_ENCLOSING_ITEM,
                "{name}: {per_level_bytes:.0} indent bytes per added level is {ratio:.2} per \
                 line byte per enclosing item, over the ceiling of \
                 {BYTES_PER_LEVEL_LENGTH_PER_ENCLOSING_ITEM}"
            );
        }

        // AND THE TOTAL IS LINEAR IN THE LADDER'S LINES, with the cap as the
        // constant. The quadratic an uncapped rescan produces passes the
        // marginal checks above only while the sample is narrow; this one it
        // cannot pass at all.
        let lines = (MAX_NESTING_DEPTH + 100) as f64;
        assert!(
            further.1 <= lines * cap,
            "a {lines}-level ladder collected {:.0} body lines, over the linear bound of {:.0}",
            further.1,
            lines * cap
        );
    }

    #[test]
    #[ignore = "a measurement, not a guard"]
    fn measure() {
        let cap = MAX_NESTING_DEPTH as f64;
        let mut previous: Option<(usize, (f64, f64))> = None;
        for depth in [50usize, 100, 150, 200, 250, 300] {
            let work = work_for(depth);
            if let Some((before, earlier)) = previous {
                let levels = (depth - before) as f64;
                let per_level_bytes = (work.0 - earlier.0) / levels;
                let line_length = (before + depth) as f64;
                println!(
                    "{before:4} to {depth:4}: {per_level_bytes:10.0} indent bytes per level \
                     ({:.2} per line byte per enclosing item), {:.1} collections per level",
                    per_level_bytes / (line_length * cap),
                    (work.1 - earlier.1) / levels,
                );
            }
            previous = Some((depth, work));
        }
    }
}

#[cfg(test)]
mod single_line_open_paragraph_probes {
    use super::*;

    #[test]
    fn deep_single_line_lists_do_not_reparse_their_prefix_chains() {
        for depth in [8, 16, 32] {
            for terminal in [
                "a",
                "*a*",
                "`a`",
                "é",
                "[a](url)",
                "# h",
                "",
                "%% c",
                "{.x}",
                "::: note",
                "[a]: url",
                "[^a]: body",
                "   ",
            ] {
                PARAGRAPH_BODY_PROBES.with(|count| count.set(0));
                let source = format!("{}{terminal}\n", "- > ".repeat(depth));
                let html = crate::to_html(&source);
                assert_eq!(html.matches("<ul>").count(), depth);
                assert_eq!(html.matches("<blockquote>").count(), depth);
                assert_eq!(
                    PARAGRAPH_BODY_PROBES.with(Cell::get),
                    0,
                    "{depth}: {terminal}"
                );
            }
        }
    }

    #[test]
    fn prefix_chains_match_block_verdicts() {
        let options = Options::default();
        for terminal in [
            "a",
            "a *b*",
            "a `b`",
            "a {+b+}",
            "a [b](url)",
            "*a*",
            "`a`",
            "é",
            "^ caption",
            "<tag>",
            "[a](url)",
            "[^a]",
            "[^a]: body",
            "# h",
            "---",
            "```",
            "::: note",
            "%% c",
            "| a |",
            "{.x}",
            "",
            "[a]: url",
            "   ",
            "%%% note",
            ":: term",
        ] {
            for prefix in ["", "- ", "> ", "- > - > ", "> - > - "] {
                let body = format!("{prefix}{terminal}");
                let blocks = probe_blocks(&body, &options);
                let expected = block_ends_with_open_paragraph(
                    blocks.last(),
                    colon_fences_left_open(&body),
                    false,
                );
                assert_eq!(
                    body_ends_with_open_paragraph(&format!("{prefix}{terminal}"), &options),
                    expected,
                    "{prefix}{terminal}"
                );
            }
        }
    }
}

#[cfg(test)]
mod hosted_inline_fence_lookup_work {
    use super::*;
    #[test]
    fn indexed_closers_match_scans_at_each_authorized_column() {
        for level in [0, 1, 2] {
            let prefix = "> ".repeat(level);
            let mut body = LineBuffer::default();
            for text in [
                "``` info",
                "  ```",
                "    ````",
                "~~~",
                "    ~~~~~",
                "``` bad",
                " L tail",
                "``````",
            ] {
                body.push_at(format!("{prefix}{text}"), None, None);
            }
            for from in 0..body.lines.len() {
                for column in [0, 2, 4] {
                    for alternate in [None, Some(0), Some(2)] {
                        for marker in ["```", "````", "~~~~"] {
                            let open = detect_fence_open(marker).unwrap();
                            let scanned = closer_ahead(&body.lines, from, level, |text| {
                                let seen = indent_columns(text);
                                (seen == column || alternate == Some(seen))
                                    && is_fence_close(trim_ascii_start(text), open)
                            });
                            assert_eq!(
                                indexed_code_closes(
                                    &body.lines,
                                    from,
                                    level,
                                    column,
                                    alternate,
                                    open,
                                    &mut LiteralCodeLookup::default()
                                ),
                                scanned
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn repeated_unclosed_openers_share_one_index() {
        for count in [256, 4096] {
            let mut body = LineBuffer::default();
            for _ in 0..count {
                body.push_at("    ``` language".to_owned(), None, None);
            }
            let mut lookup = LiteralCodeLookup::default();
            let open = detect_fence_open("``` language").unwrap();
            LITERAL_CODE_LINES.with(|work| work.set(0));
            for from in 0..count {
                assert!(!indexed_code_closes(
                    &body.lines,
                    from,
                    0,
                    4,
                    None,
                    open,
                    &mut lookup
                ));
            }
            assert!(LITERAL_CODE_LINES.with(Cell::get) > 0);
            assert!(LITERAL_CODE_LINES.with(Cell::get) <= count);
        }
    }
}

#[cfg(test)]
mod frame_probe_tests {
    use super::*;

    #[test]
    fn definition_free_probe_reduction_retains_existing_normalization() {
        let options = Options::default();
        for source in [
            "",
            "p",
            "p\n- :: t\n  ```\n  x",
            "\u{feff}p",
            "p\r\nq",
            "p\r",
            "p\n",
            "p\n\n",
            " p \t\n q",
            "p [text]",
            "- [x] task",
            "[r]: /u\np",
            "p\rinside",
            "[r]: <b>{=html}",
            "[r]:\t/u",
            "> - [r]: /u",
        ] {
            assert_eq!(
                reduce_frame_probe_links(source.to_owned(), &options),
                extract_link_defs(source, &options).0,
                "{source:?}"
            );
        }
    }
}

#[cfg(test)]
mod definition_gate_tests {
    use super::*;

    #[test]
    fn global_gate_preserves_source_maps_and_guard_budget() {
        let extension = Extension;
        for positions in [false, true] {
            for extended in [false, true] {
                let mut options = Options::default().with_positions(positions);
                if extended {
                    options.extensions.push(&extension);
                }
                for source in [
                    "",
                    "p",
                    "p\n",
                    "p\n\n",
                    "p\r\nq",
                    "\u{feff}p",
                    " p \t\n q",
                    "p [text]: literal",
                    "[r]: /u",
                    "[r]:\t/u\n\n[r][]",
                    "[r]:/u\n\n[r][]",
                    "[r]:\n  /u\n\n[r][]",
                    "\u{feff}[r]: /u\n\n[r][]",
                    "[r]: /u \"title\" {.c}",
                    "[r]: /u junk",
                    "[@r]: /u",
                    "[]: /u",
                    "- [r]: /u",
                    "> - [r]: /u",
                    "  > - : [r]: /u",
                    ":: t\n: [r]: /u",
                    ":: t\n: - [r]: /u",
                    "- a\n  > [r]: /u",
                    "- a\n  [r]: /u",
                    "- a\n   [r]: /u",
                    "- a\n\t[r]: /u",
                    "> - a\n>   [r]: /u",
                    "- - x\n   [r]: /u",
                    "1. a\n   [r]: /u",
                    "-{.c} [r]: /u",
                    ". [r]: /u",
                    "- [x]  - [r]: /u",
                    "- [x] [r]: /u",
                    "- [x] p [r]: /u",
                    "p\n- :: t\n  ```\n  x",
                    "para\n  - :: t\n    ```",
                    "```\n[r]: /u\n```",
                    "> ```\n> [r]: /u\n> ```",
                    "%%%\n[r]: /u\n%%%",
                    "::: |\n[r]: /u\n:::",
                    "- x\n  %%% o\n  filler\n%%%\n\n[r][]\n",
                ] {
                    let mut fast_budget = 1_000_000;
                    let mut full_budget = fast_budget;
                    let fast =
                        extract_link_defs_with_guard(source, &options, Some(&mut fast_budget));
                    let full =
                        extract_link_defs_with_guard_full(source, &options, Some(&mut full_budget));
                    assert!(
                        full.1.is_empty() || source_may_contain_link_definition(source),
                        "{source:?}"
                    );
                    assert_eq!(fast.0, full.0, "{source:?}");
                    let projection = |map: &BTreeMap<String, LinkDef>| {
                        map.iter()
                            .map(|(label, def)| {
                                (
                                    label.clone(),
                                    def.raw_label.clone(),
                                    def.href.clone(),
                                    def.title.clone(),
                                    def.line,
                                    format!("{:?}", def.attrs),
                                )
                            })
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(projection(&fast.1), projection(&full.1), "{source:?}");
                    assert_eq!(fast_budget, full_budget, "{source:?}");
                }
            }
        }
    }

    #[test]
    fn definition_free_gate_preserves_source_without_scanning_container_bounds() {
        let source = "- x\n  %%% o\n  filler\n%%%\n\n[r][]\n";
        std::thread::spawn(move || {
            let options = Options::default();
            CONTAINER_DEDENT_STEPS.with(|counter| counter.set(0));
            let fast = extract_link_defs(source, &options);
            assert_eq!(CONTAINER_DEDENT_STEPS.with(Cell::get), 0);
            assert!(fast.1.is_empty());
            let full = extract_link_defs_with_guard_full(source, &options, None);
            assert!(CONTAINER_DEDENT_STEPS.with(Cell::get) > 0);
            assert_eq!(fast.0, full.0);
            assert!(full.1.is_empty());
        })
        .join()
        .unwrap();
    }

    struct CountingExtension(Cell<usize>);
    impl crate::extension::CarveExtension for CountingExtension {
        fn name(&self) -> &'static str {
            "definition-gate-callback-control"
        }
        fn match_block(
            &self,
            _lines: &[&str],
            _start: usize,
            _context: &crate::extension::MatcherContext<'_>,
        ) -> Option<crate::extension::BlockMatch> {
            self.0.set(self.0.get() + 1);
            None
        }
    }

    #[test]
    fn definition_gate_retains_extension_matcher_calls() {
        for (source, has_matcher_work) in [
            ("p [text]: literal\n\n::: |\nx\n:::\n", true),
            ("p\n\n::: |\nx\n:::\n", false),
        ] {
            for positions in [false, true] {
                let extension = CountingExtension(Cell::new(0));
                let mut options = Options::default().with_positions(positions);
                options.extensions.push(&extension);
                let full = extract_link_defs_with_guard_full(source, &options, None);
                let full_calls = extension.0.replace(0);
                assert_eq!(full_calls > 0, has_matcher_work);
                let probe = reduce_frame_probe_links(source.to_owned(), &options);
                assert_eq!(probe, full.0);
                assert_eq!(extension.0.replace(0), full_calls);
                let fast = extract_link_defs(source, &options);
                assert_eq!(fast.0, full.0);
                assert_eq!(extension.0.get(), full_calls);
            }
        }
    }

    struct Extension;
    impl crate::extension::CarveExtension for Extension {
        fn name(&self) -> &'static str {
            "global-definition-gate-control"
        }
    }

    #[test]
    fn canonical_prefix_gate_preserves_probe_reduction() {
        let options = Options::default();
        for source in [
            "p [text]: literal",
            "p [text]: literal\n- :: t\n  ```\n  x",
            "[r]: /u",
            " [r]: /u",
            "- [r]: /u",
            "> - [r]: /u",
            "  > - : [r]: /u",
            ">  >   - [r]: /u",
            ":: t\n: [r]: /u",
            ":: t\n: - [r]: /u",
            "- a\n  > [r]: /u",
            "- a\n  > - [r]: /u",
            "[r]: /u \"title\" {.c}",
            "[r]: /u junk",
            "[@r]: /u",
            "[]: /u",
            "\u{feff}[r]: /u",
            "p\r\n[r]: /u",
            "p\r",
            "",
            "p\n",
            "p\n\n",
            "[r]: /u\n",
            "\t[r]: /u",
            "- [x] [r]: /u",
            "- [x] p [r]: /u",
        ] {
            assert_eq!(
                reduce_frame_probe_links(source.to_owned(), &options),
                extract_link_defs(source, &options).0,
                "{source:?}"
            );
        }
        assert!(!source_may_contain_link_definition("p [text]: literal"));
        assert!(source_may_contain_link_definition("  > - : [r]: /u"));
    }
}

#[cfg(test)]
mod quoted_prepass_work_tests {
    use super::*;

    #[test]
    fn literal_angle_text_keeps_root_columns_and_needs_no_quote_memo() {
        let lines = [
            ":: t",
            "",
            "- a",
            "- - b",
            "",
            "    [r]: /x",
            "",
            "x > [y][r]",
        ];
        let maxima = fence_closer_suffix_maxima(&lines);
        let ownership = term_folded_lines(&lines, &maxima, &Options::default());
        assert!(ownership.tracks_quote_context);
        let mut columns = ContentColumns::new();
        let mut previous_non_blank = "";
        for (index, line) in lines.iter().enumerate() {
            assert!(!prepass_quote_line_is_folded(&ownership, index, line));
            columns.observe(line, false);
            let prefix = strip_container_prefixes_at(
                line,
                &columns,
                opens_definition_entry(previous_non_blank),
            );
            remember_prepass_paragraph_line(&ownership, index, line, &prefix, &columns);
            assert!(
                ownership.paragraph_quotes.borrow().is_empty(),
                "line {index}"
            );
            if !is_blank_line(line) {
                previous_non_blank = line;
            }
            if index == 3 {
                assert_eq!(columns.current().cols, [2, 4]);
            }
        }
    }
}

#[cfg(test)]
mod marker_note_comment_bounds {
    use super::*;

    #[test]
    fn note_comment_queries_reuse_the_content_boundary() {
        for depth in 0..=2 {
            for count in [128, 512, 2048] {
                let prefix = "> ".repeat(depth);
                let owned = vec![format!("{prefix}    %%%"); count];
                let lines: Vec<_> = owned.iter().map(String::as_str).collect();
                let mut index = ContainerCommentClosers::build(&lines);
                CONTAINER_DEDENT_STEPS.with(|counter| counter.set(0));
                for at in 0..count - 1 {
                    assert!(index.closes_in_note(&lines, at, depth, 4, 3));
                }
                assert!(CONTAINER_DEDENT_STEPS.with(Cell::get) <= count as u64);
            }
        }
    }

    #[test]
    fn a_note_comment_cannot_close_after_its_content_scope() {
        for depth in 0..=2 {
            let prefix = "> ".repeat(depth);
            for middle in [format!("{prefix}- sibling"), "outside".to_string()] {
                let owned = [
                    format!("{prefix}    %%%"),
                    middle,
                    format!("{prefix}    %%%"),
                ];
                let lines: Vec<_> = owned.iter().map(String::as_str).collect();
                let mut index = ContainerCommentClosers::build(&lines);
                assert!(!index.closes_in_note(&lines, 0, depth, 4, 3));
            }
            let owned = [
                format!("{prefix}    %%%"),
                format!("{prefix}    body"),
                format!("{prefix}    %%%"),
            ];
            let lines: Vec<_> = owned.iter().map(String::as_str).collect();
            assert!(ContainerCommentClosers::build(&lines).closes_in_note(&lines, 0, depth, 4, 3));
        }
    }

    #[test]
    fn unmatched_comment_widths_do_not_scan_note_suffixes() {
        let owned: Vec<_> = (3..259)
            .map(|width| format!("    {}", "%".repeat(width)))
            .collect();
        let lines: Vec<_> = owned.iter().map(String::as_str).collect();
        let mut index = ContainerCommentClosers::build(&lines);
        CONTAINER_DEDENT_STEPS.with(|counter| counter.set(0));
        for (at, width) in (3..259).enumerate() {
            assert!(!index.closes_in_note(&lines, at, 0, 4, width));
        }
        assert_eq!(CONTAINER_DEDENT_STEPS.with(Cell::get), 0);
    }
}

#[cfg(test)]
mod named_colon_rebase_work {
    use super::*;

    #[test]
    fn named_colon_payload_rebasing_is_bounded_by_the_line_count() {
        for size in [64, 128, 256] {
            let mut source = "- ::: d\n".to_string();
            for column in 1..size {
                source.push_str(&format!("  {}::: x\n", " ".repeat(column)));
            }
            NAMED_COLON_REBASE_LINES.with(|count| count.set(0));
            let _doc = parse(&source);
            let work = NAMED_COLON_REBASE_LINES.with(Cell::get);
            assert!(
                work <= size * 6,
                "{size} lines required {work} payload rewrites"
            );
        }
    }
}

pub(crate) fn carrier_next_is_list_marker(line: &str) -> bool {
    detect_list_marker_full(trim_ascii_start(line)).is_some()
}

struct BodyCodeColumns {
    runs: HashMap<(usize, u8), Vec<(usize, usize)>>,
}

impl BodyCodeColumns {
    fn new(lines: &[Cow<'_, str>]) -> Self {
        let mut runs: HashMap<(usize, u8), Vec<(usize, usize)>> = HashMap::new();
        for (index, line) in lines.iter().enumerate() {
            let line = strip_lazy(line);
            let bytes = trim_ascii_start(line).as_bytes();
            let Some(&character @ (b'`' | b'~')) = bytes.first() else {
                continue;
            };
            let width = bytes.iter().take_while(|&&byte| byte == character).count();
            if bytes[width..]
                .iter()
                .all(|byte| matches!(byte, b' ' | b'\t'))
            {
                runs.entry((indent_columns(line), character))
                    .or_default()
                    .push((index, width));
            }
        }
        for column in runs.values_mut() {
            for index in (0..column.len().saturating_sub(1)).rev() {
                column[index].1 = column[index].1.max(column[index + 1].1);
            }
        }
        Self { runs }
    }

    fn closes_after(&self, at: usize, column: usize, open: FenceOpen) -> bool {
        let Some(runs) = self.runs.get(&(column, open.fence_char)) else {
            return false;
        };
        let after = runs.partition_point(|&(index, _)| index <= at);
        runs.get(after)
            .is_some_and(|&(_, width)| width >= open.fence_len)
    }
}

#[cfg(test)]
mod term_marker_probe_work_tests {
    use super::*;

    fn probed_bytes(source: &str) -> usize {
        let lines: Vec<&str> = source.lines().collect();
        let maxima = fence_closer_suffix_maxima(&lines);
        TERM_MARKER_PROBE_BYTES.with(|bytes| bytes.set(0));
        drop(term_folded_lines(&lines, &maxima, &Options::default()));
        TERM_MARKER_PROBE_BYTES.with(std::cell::Cell::get)
    }

    // carve-rs#2690: every marker probe here used to parse the whole document
    // above it, until the probe budget ran out.
    #[test]
    fn a_marker_probe_parses_the_block_holding_its_marker() {
        let unit = "A paragraph that only adds bytes above the next marker, see[^n].\n\n\
                    - ::: |\n  - ```\n- :: term\n    ```\n";
        for separator in ["\n", "\n\n", "\n\n\n"] {
            let source = vec![unit; 200].join(separator);
            let probed = probed_bytes(&source);
            assert!(
                probed > 0,
                "no marker probe ran with separator {separator:?}"
            );
            assert!(
                probed * 16 < source.len(),
                "marker probes parsed {probed} bytes of a {}-byte document with separator \
                 {separator:?}",
                source.len()
            );
        }
    }
}

#[cfg(test)]
mod ladder_body_reparses {
    //! COUNTED guard on a list ladder whose items hold quotes or containers.
    //!
    //! Each item asked whether its body ends in an open paragraph by re-parsing
    //! the body, and a sub-list inside that body asked the same of its own body
    //! inside the probe, so every level parsed what was below it twice: 30
    //! levels took 0.5 s and 40 levels 44 s. The answer is now probed only when
    //! a later line needs it.
    use super::*;

    /// `depth` levels alternating a list item with a quote (`> q` and a blank
    /// quote line) or with a `::: note` container.
    fn ladder(shape: &str, depth: usize) -> String {
        let mut out = String::new();
        let mut indent = String::new();
        let mut closers = Vec::new();
        for i in 0..depth {
            if i % 2 == 1 && shape == "quote" {
                out += &format!("{indent}> q{i}\n{indent}>\n");
                indent += "> ";
            } else if i % 2 == 1 {
                let fence = ":".repeat(3 + (depth - i).div_ceil(2));
                out += &format!("{indent}{fence} note\n");
                closers.push(format!("{indent}{fence}"));
            } else {
                out += &format!("{indent}- item {i}\n");
                indent += "  ";
            }
        }
        while let Some(closer) = closers.pop() {
            out += &closer;
            out.push('\n');
        }
        out
    }

    fn parses(source: &str) -> usize {
        BLOCK_SEQUENCE_PARSES.with(|count| count.set(0));
        let html = crate::to_html(source);
        assert!(html.contains("item"), "{html}");
        BLOCK_SEQUENCE_PARSES.with(Cell::get)
    }

    /// Doubling the depth may cost a few times the parses, since each level
    /// re-reads what is below it; doubling them per level is what this catches.
    #[test]
    fn doubling_a_ladder_keeps_its_body_parses_polynomial() {
        for shape in ["quote", "container"] {
            for tail in ["", "lazy tail\n"] {
                let short = parses(&(ladder(shape, 16) + tail));
                let long = parses(&(ladder(shape, 32) + tail));
                assert!(
                    long <= 5 * short,
                    "{shape} {tail:?}: {short} body parses at 16 levels, {long} at 32"
                );
            }
        }
    }
}
