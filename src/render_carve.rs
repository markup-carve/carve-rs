mod escape_window;

use crate::ast::*;
use crate::ast_json::block_pos;
use crate::render::MAX_RENDER_DEPTH;
use crate::render_text::{trim_end_non_nbsp, trim_non_nbsp};
mod session;
use session::{CellScope, RenderSession};
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};

/// A definition the author wrote ON a definition list's description line.
///
/// Collecting it empties the `dd` (spec markup-carve/carve#801), and an empty
/// description has no source spelling - the production requires content after
/// the marker - so the writer emitted a bare `:` line, which re-parses as a
/// continuation of the term above it. That is `to_html(fmt(x)) == to_html(x)`
/// failing, PART 11 section 1 (markup-carve/carve#805).
///
/// Nothing new is needed in the language. The description keeps the span of its
/// own marker line and the hoisted definition keeps the span it was written at
/// (PART 12 section 4); the two name the SAME line, so the description writes
/// the definition back on it and the document-level pass skips what a
/// description already claimed.
#[derive(Debug, Clone)]
enum DefinitionAtLine {
    Link(Box<LinkReferenceDefinition>),
    Footnote(String, Vec<BlockNode>),
}

struct CarveContext {
    block_depth: usize,
    inline_depth: usize,
    list_depth: usize,
    /// Depth of line-block nesting, so the inline writer drops the explicit
    /// backslash: inside a `::: |` fence every newline already IS a hard break.
    line_block_depth: usize,
    rendered_verbatim_tail: bool,
    colon_fence_depth: usize,
    /// Inside a table cell, where a leading `^` cannot open a caption: a
    /// caption marker is a BLOCK line, and a cell's content is not one.
    table_cell_depth: usize,
    /// Inside a table cell that is not the last in its row.
    cell_not_last: bool,
    /// Inside an inline note's content, where PART 9 §16 disables note
    /// recognition at every depth - so a `^[` written there opens nothing and
    /// needs no escape.
    note_content_depth: usize,
    after_caption_host: bool,
    paragraph_starts_after_caption_host: bool,
    /// Inside a definition term, where a comment opening a line keeps its
    /// separator instead of joining a `%`-leading content into a fence.
    in_term: bool,
    escape_mode: EscapeMode,
    /// The unit the character being written now belongs to (PART 11 §2b).
    ///
    /// A per-PASS ordinal rather than a node address: `render_block` and
    /// `render_inline` hand out the next one on entry and restore the previous
    /// on the way out, so a run of prose is charged to its text node and the
    /// strings a block writes itself are charged to the block. Two of the block
    /// arms render a node built on the spot, whose ADDRESS is a stack temporary
    /// that a later pass need not reuse; an ordinal is the same in every pass
    /// because the walk is the same in every pass.
    escape_unit: usize,
    /// Definitions written on a description line, keyed by that line.
    definitions_by_line: HashMap<usize, DefinitionAtLine>,
    /// The lines a description has already written back.
    ///
    /// PER PASS, because `render_carve_once` renders the document up to three
    /// times and picks between the forms (PART 11 section 4). A set that
    /// survived one pass would tell the next that every definition is already
    /// placed - the description emits a bare `:` again and the document-level
    /// arm emits nothing, deleting the definition outright.
    written_in_place: HashSet<usize>,
    /// Every braced span written so far: its delimiter and its `pos` offset,
    /// which the HTML importer uses as a mark.
    braced_spans: Vec<(char, Option<usize>)>,
    /// The emphasis kinds open around the node being written.
    open_kinds: Vec<char>,
    attribute_bracket_depth: usize,
    attribute_markers: HashMap<char, usize>,
    /// The brackets of the inline run being written, as (text node address,
    /// bracket ordinal in that node), and whether a run has claimed them.
    brackets: BracketScope,
    /// Clones the writer made of nodes a claimed scope already keyed, mapped to
    /// the node each was cloned from.
    bracket_aliases: HashMap<usize, usize>,
    /// The last text node written ended on a bare `]` that closes a pair, and
    /// nothing has been written since.
    paired_closer_carry: std::cell::Cell<bool>,
}

/// PART 11 §5's view of the text brackets in one inline run.
#[derive(Default)]
struct BracketScope {
    claimed: bool,
    /// The run about to claim a scope is content written between `[` and `]`.
    bracketed: bool,
    /// Unpaired, inside content a construct writes between `[` and `]`.
    lone: HashSet<(usize, usize)>,
    /// A `]` the bracket scan pairs with an earlier `[`.
    paired_closers: HashSet<(usize, usize)>,
    /// The `[` of a pair whose two brackets sit under DIFFERENT formatting
    /// nodes. The run between them would isolate the delimiters it crosses, so
    /// the OPENER carries the escape (markup-carve/carve-php#2756).
    crossing_openers: HashSet<(usize, usize)>,
    fixed: HashSet<(usize, usize)>,
    incomplete_raw: bool,
    /// A `]` whose own `[` is escaped and which therefore falls through to an
    /// OUTER opener across the same boundary. It takes an escape too, or the
    /// writer's next pass escapes that outer opener instead and the formatter
    /// stops agreeing with itself (carve-rs#2209).
    crossing_closers: HashSet<(usize, usize)>,
    /// In bracketed content, each paired `]` mapped to its `[`.
    closer_openers: HashMap<(usize, usize), (usize, usize)>,
    /// Every node the scan read.
    keyed: HashSet<usize>,
    /// Pending literal brackets; escaped openers are removed lazily.
    literal_openers: Vec<((usize, usize), usize)>,
    /// Group openers so a reference visits each host once.
    literal_hosts: HashMap<usize, HashSet<(usize, usize)>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EscapeMode {
    Minimal,
    Conservative,
}

/// Render a tree as canonical Carve source.
pub fn render_carve(doc: &Document) -> Result<String, crate::RenderCarveError> {
    let session = &RenderSession::new();
    // Reject excessive depth before recursive preparation reads the tree.
    crate::render_depth::refuse_if_too_deep(doc, "carve")?;
    crate::render_loss::record_ruby_in_document(doc);
    let one_run = text_as_one_run(doc);
    let doc = one_run.as_ref().unwrap_or(doc);
    let source_watch = crate::render_carve_error::SourceSpellWatch::new();
    let watch = crate::render_depth::RenderDepthWatch::new();
    let output = protect_leading_bom(render_carve_unguarded(session, doc));
    if let Some(error) = source_watch.error() {
        return Err(error);
    }
    watch.into_result(output).map_err(Into::into)
}

/// The tree with every stretch of adjacent text nodes merged into one, ruby
/// flattened first as the writer writes it, or `None` when nothing merges.
///
/// PART 11 §2: adjacent text is one run, so where a tree splits text cannot
/// decide which character carries an escape (`x (r` beside `) y` is
/// `x \(r) y`, as the single node is).
fn text_as_one_run(doc: &Document) -> Option<Document> {
    use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};

    fn splits(nodes: &[InlineNode]) -> bool {
        nodes.iter().any(|node| matches!(node, InlineNode::Ruby(_)))
            || nodes
                .windows(2)
                .any(|pair| matches!(pair, [InlineNode::Text(_), InlineNode::Text(_)]))
            || nodes.iter().any(inline_splits)
    }
    fn inline_splits(node: &InlineNode) -> bool {
        match node {
            InlineNode::Emphasis(e) => splits(&e.children),
            InlineNode::Link(l) => splits(&l.children),
            InlineNode::Span(s) => splits(&s.children),
            InlineNode::CriticInsert(c) => splits(&c.children),
            InlineNode::CriticDelete(c) => splits(&c.children),
            InlineNode::CriticSubstitute(c) => splits(&c.old) || splits(&c.new),
            InlineNode::Extension(e) => splits(&e.children),
            InlineNode::Footnote(f) => f.inline.as_deref().is_some_and(splits),
            InlineNode::CitationGroup(g) => g.items.iter().any(|item| {
                [&item.prefix, &item.locator, &item.suffix]
                    .into_iter()
                    .flatten()
                    .any(|part| splits(part))
            }),
            _ => false,
        }
    }
    fn blocks_split(blocks: &[BlockNode]) -> bool {
        blocks.iter().any(block_splits)
    }
    fn cells_split(rows: &[TableRow]) -> bool {
        rows.iter()
            .flat_map(|row| &row.cells)
            .any(|cell| splits(&cell.children) || cell.blocks.as_deref().is_some_and(blocks_split))
    }
    fn block_splits(block: &BlockNode) -> bool {
        match block {
            BlockNode::Heading(h) => splits(&h.children),
            BlockNode::Paragraph(p) => splits(&p.children),
            BlockNode::LineBlock(b) => blocks_split(&b.children),
            BlockNode::BlockQuote(b) => blocks_split(&b.children),
            BlockNode::Admonition(a) => {
                a.title.as_deref().is_some_and(splits) || blocks_split(&a.children)
            }
            BlockNode::Directive(d) => {
                d.title.as_deref().is_some_and(splits) || blocks_split(&d.children)
            }
            BlockNode::Div(d) => blocks_split(&d.children),
            BlockNode::Section(d) => blocks_split(&d.children),
            BlockNode::List(l) => l.items.iter().any(|item| blocks_split(&item.children)),
            BlockNode::DefinitionList(d) => d.items.iter().any(|item| {
                item.terms.iter().any(|term| splits(&term.children))
                    || item
                        .definitions
                        .iter()
                        .any(|def| blocks_split(&def.children))
            }),
            BlockNode::Table(t) => t.caption.as_deref().is_some_and(splits) || cells_split(&t.rows),
            BlockNode::FigureGroup(g) => {
                blocks_split(&g.children) || g.caption.as_deref().is_some_and(splits)
            }
            BlockNode::Figure(f) => {
                splits(&f.caption)
                    || match &*f.target {
                        FigureTarget::BlockQuote(b) => blocks_split(&b.children),
                        FigureTarget::Paragraph(p) => splits(&p.children),
                        FigureTarget::Table(t) => cells_split(&t.rows),
                        FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => false,
                    }
            }
            BlockNode::BlockExtension(e) => block_splits(&e.fallback),
            BlockNode::ExtensionCarrier(e) => blocks_split(&e.children),
            _ => false,
        }
    }
    struct Merge;
    impl SubtreeVisitor for Merge {
        fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
            for block in blocks {
                visit_block_children(block, self);
            }
        }
        fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
            while inlines
                .iter()
                .any(|node| matches!(node, InlineNode::Ruby(_)))
            {
                *inlines = std::mem::take(inlines)
                    .into_iter()
                    .flat_map(|node| match node {
                        InlineNode::Ruby(ruby) if ruby.attrs.is_some() => {
                            vec![InlineNode::Span(Span {
                                attrs: ruby.attrs.clone(),
                                children: ruby.flattened(),
                                injected: false,
                                pos: ruby.pos,
                            })]
                        }
                        InlineNode::Ruby(ruby) => ruby.flattened(),
                        other => vec![other],
                    })
                    .collect();
            }
            let mut merged: Vec<InlineNode> = Vec::with_capacity(inlines.len());
            for node in std::mem::take(inlines) {
                match (merged.last_mut(), node) {
                    (Some(InlineNode::Text(previous)), InlineNode::Text(text)) => {
                        previous.value.push_str(&text.value);
                        previous.pos = None;
                    }
                    (_, node) => merged.push(node),
                }
            }
            *inlines = merged;
            for inline in inlines {
                visit_inline_children(inline, self);
            }
        }
    }
    if !blocks_split(&doc.children) && !doc.footnote_defs.values().any(|b| blocks_split(b)) {
        return None;
    }
    let mut copy = doc.clone();
    let mut merge = Merge;
    merge.blocks(&mut copy.children);
    for blocks in copy.footnote_defs.values_mut() {
        merge.blocks(blocks);
    }
    Some(copy)
}

/// A U+FEFF that would land at the head of the OUTPUT is written one column in.
///
/// `normalize_source` strips a single leading byte order mark before the parser
/// sees it, so a document whose first content character is one cannot be
/// written back flush left: the re-parse eats it and the document comes back
/// empty. The character is content - PART 2 keeps it, and corpus
/// `268-trailing-whitespace-on-a-content-line-is-dropped-8` is a paragraph
/// holding exactly one - so the writer has to put it somewhere a re-parse can
/// still read it.
///
/// One leading SPACE does that and nothing else: it is INDENTATION on re-parse,
/// which a paragraph drops, so the tree round-trips unchanged. It does not
/// violate PART 11 §7 either, which forbids a line whose ONLY content is
/// whitespace - this line has content, and the space is in front of it.
///
/// Idempotent by construction: the second pass sees the same tree and writes
/// the same leading space.
fn protect_leading_bom(out: String) -> String {
    if out.starts_with('\u{feff}') {
        return format!(" {out}");
    }
    out
}

/// The ids a fresh parse would assign, for headings that carry an unslotted id.
///
/// Computed with `assigned_heading_ids` - the pass the renderer itself uses -
/// with unslotted heading ids ignored, without cloning the document.
pub(crate) fn redundant_heading_ids(doc: &Document) -> std::collections::BTreeSet<String> {
    crate::document_ids::redundant_heading_ids(doc)
}

fn render_carve_unguarded(session: &RenderSession, doc: &Document) -> String {
    // One render with the default sentinels. If the document turns out to
    // contain one of them itself, the counts disagree and the whole render is
    // repeated with a character it does not contain (see SENTINEL_DEFAULTS). Only a
    // document that actually holds a private-use sentinel pays for the second
    // pass, and nothing else changes: the retry runs the same code.
    let first = render_carve_once(session, doc);
    let current = session.sentinels.with(|s| s.get());
    let inserted = session.inserted.with(|c| c.get());
    let seen = session.seen.with(|c| c.get());
    if (0..SENTINEL_COUNT).all(|i| seen[i] <= inserted[i]) {
        return first;
    }
    // Choose against the staged text: `first` has been through restore, so an
    // authored occurrence is no longer visible in it.
    let staged = session.staged.with(|c| c.borrow().clone());
    let mut next = current;
    for i in 0..SENTINEL_COUNT {
        if seen[i] > inserted[i] {
            next[i] = free_sentinel(&staged, &next);
        }
    }
    session.sentinels.with(|s| s.set(next));
    render_carve_once(session, doc)
}

/// One full render, with the insertion counters reset for it.
fn render_carve_once(session: &RenderSession, doc: &Document) -> String {
    let redundant = redundant_heading_ids(doc);
    session
        .redundant_ids
        .with(|cell| *cell.borrow_mut() = redundant);
    session.inserted.with(|c| c.set([0; SENTINEL_COUNT]));
    session.seen.with(|c| c.set([0; SENTINEL_COUNT]));
    session.staged.with(|c| c.borrow_mut().clear());
    let minimal = render_with_escapes(session, doc, EscapeMode::Minimal);
    let conservative = render_with_escapes(session, doc, EscapeMode::Conservative);
    if minimal == conservative {
        return minimal;
    }
    // ONE parse of the conservative form, shared by the redundancy check and the
    // narrowing below. Parsing it in each made every narrowed document pay a
    // second full parse of its own output for an answer it already had.
    let conservative_tree = comparable_tree(&conservative);
    if escaping_is_redundant(&minimal, conservative_tree.as_ref()) {
        return minimal;
    }
    // The minimal form of the WHOLE document does not hold, which used to end
    // the decision here with the conservative form of the whole document. PART
    // 11 §2b says how far that fallback actually reaches: the smallest unit
    // whose minimal form fails, and §2's own test everywhere else.
    narrow_escalation(session, doc, conservative, conservative_tree)
}

/// The conservative form of the units that need it, and the minimal form of
/// every other unit (PART 11 §2b).
fn narrow_escalation(
    session: &RenderSession,
    doc: &Document,
    conservative: String,
    conservative_tree: Option<Document>,
) -> String {
    // `None` answers "cannot tell", exactly as it does for the minimal form:
    // with no tree to hold the narrowing against, there is nothing to narrow
    // toward.
    let Some(conservative_tree) = conservative_tree else {
        return conservative;
    };
    // How many units the document has is a property of the WALK, so it is
    // counted by walking: the pass below is the same conservative render, and
    // its agreeing with `conservative` is the first half of the control.
    let discovered = render_with_escapes(session, doc, EscapeMode::Conservative);
    let total = session.pass.unit_counter.with(|c| c.get());
    if discovered != conservative || total == 0 {
        return conservative;
    }

    let all: Vec<usize> = (1..=total).collect();
    session
        .escalated_units
        .with(|cell| *cell.borrow_mut() = Some(all.iter().copied().collect()));
    session
        .asked_units
        .with(|cell| *cell.borrow_mut() = Some(HashSet::new()));
    // The control render also records where every unit sits, for the
    // windowed probes below. A document whose break spelling needs the
    // frontmatter fallback renders differently from its windows, so it keeps
    // whole-document probes.
    let windows_fit = doc.frontmatter_raw.is_some()
        || !doc.frontmatter.is_empty()
        || !crate::parse::opens_frontmatter(&conservative);
    #[cfg(test)]
    let windows_fit = windows_fit && !tests::WHOLE_DOCUMENT_PROBES.with(std::cell::Cell::get);
    let (control, layout) = if windows_fit {
        let (control, layout) = escape_window::record(session, || {
            render_with_escapes(session, doc, EscapeMode::Conservative)
        });
        (control, Some(layout))
    } else {
        (
            render_with_escapes(session, doc, EscapeMode::Conservative),
            None,
        )
    };
    let asked = session
        .asked_units
        .with(|cell| cell.borrow_mut().take())
        .unwrap_or_default();
    if control != conservative {
        session
            .escalated_units
            .with(|cell| *cell.borrow_mut() = None);
        return conservative;
    }
    let units: Vec<usize> = all
        .into_iter()
        .filter(|unit| asked.contains(unit))
        .collect();
    let probe = Probe {
        doc,
        tree: &conservative_tree,
        window_limit: conservative.len() / 2,
        layout,
        charged: std::cell::Cell::new(0),
        search_start: std::cell::Cell::new(0),
        parse_allowance: ESCAPE_SEARCH_PARSE_FACTOR * conservative.len(),
        extra_probes: std::cell::Cell::new(0),
        extra_cap: std::cell::Cell::new(0),
    };
    // No guard for an EMPTY `units`: `relax_units` returns on an empty group,
    // and a check here would be one no corpus document can reach -- the control
    // render asks about a unit for every byte the two forms differ in, and they
    // differ or this is not running.
    let search = |local: bool| -> String {
        session
            .escalated_units
            .with(|cell| *cell.borrow_mut() = Some((1..=total).collect()));
        let mut best = control.clone();
        // Eight times the depth of the halving, which is what narrowing four
        // independent failing units costs. See `budget` on `relax_units`.
        let mut budget = 8 * (usize::BITS - units.len().leading_zeros()) as usize + 8;
        probe.begin_search(budget);
        relax_units(session, &probe, local, &units, &mut best, &mut budget, None);
        probe.settle(session, local, best)
    };
    let mut best = search(true);
    if comparable_tree(&best).as_ref() != Some(&conservative_tree) {
        best = search(false);
    }
    // PART 11 §2 TAKES THE DECISION PER OPENER OCCURRENCE, and a unit is still
    // ONE KNOB: a unit that fails is written conservatively IN FULL, so every
    // candidate character beside the one that needed it is escaped for nothing
    // -- `\{\.note\}` where §2 wants `\{.note}`. §2b bounds how far the fallback
    // reaches; this is what is left inside the bound (markup-carve/carve#1533).
    narrow_occurrences(session, &probe, &mut best);
    session
        .escalated_units
        .with(|cell| *cell.borrow_mut() = None);
    best
}

/// The narrowing searches' oracle.
///
/// With `local`, a probe renders and re-parses only the blocks around the
/// relaxed units and compares that window before and after, so a probe costs
/// the window instead of the document. The caller re-verifies the finished
/// state against the whole document and repeats the search with `local` off
/// when it does not hold, which is the search as it was before windows.
struct Probe<'a> {
    doc: &'a Document,
    tree: &'a Document,
    /// A window longer than this saves nothing over the whole document.
    window_limit: usize,
    layout: Option<escape_window::Layout>,
    /// Bytes of source the probes have rendered for re-parsing, cached or not,
    /// so the charge is a property of the search rather than of a cache.
    charged: std::cell::Cell<usize>,
    search_start: std::cell::Cell<usize>,
    parse_allowance: usize,
    /// Probes a search has made past its count, and how many it may make.
    extra_probes: std::cell::Cell<usize>,
    extra_cap: std::cell::Cell<usize>,
}

/// How many documents' worth of source one narrowing search may re-parse
/// beyond its probe count. Windowed probes are cheap, so this lets a large
/// document with many independent failing units finish the search, while the
/// total stays linear in the document.
const ESCAPE_SEARCH_PARSE_FACTOR: usize = 16;

/// How many times its probe count a search may probe at most. A probe still
/// walks structures sized by the document, so the count stays logarithmic.
const ESCAPE_SEARCH_PROBE_FACTOR: usize = 4;

/// A probe's answer, with the whole-document candidate it rendered, if any.
enum Verdict {
    Holds,
    Fails(Option<String>),
}

impl Probe<'_> {
    fn begin_search(&self, count: usize) {
        self.search_start.set(self.charged.get());
        self.extra_probes.set(0);
        self.extra_cap.set((ESCAPE_SEARCH_PROBE_FACTOR - 1) * count);
    }

    /// Whether a search may not probe again: its count is spent, and so is
    /// either its parse allowance or its cap on extra probes.
    fn exhausted(&self, budget: usize) -> bool {
        budget == 0
            && (self.charged.get() - self.search_start.get() >= self.parse_allowance
                || self.extra_probes.get() >= self.extra_cap.get())
    }

    /// Spend one probe of `budget`, counting it as extra once that is empty.
    fn spend(&self, budget: &mut usize) {
        if *budget == 0 {
            self.extra_probes.set(self.extra_probes.get() + 1);
        } else {
            *budget -= 1;
        }
    }

    fn charge(&self, bytes: usize) {
        self.charged.set(self.charged.get() + bytes);
    }

    fn render_window(&self, session: &RenderSession, window: &escape_window::Window) -> String {
        escape_window::render_pruned(session, window, || {
            render_with_escapes_once(session, self.doc, EscapeMode::Conservative)
        })
    }

    /// Apply a relaxation and keep it when the tree still holds.
    fn keeps(
        &self,
        session: &RenderSession,
        local: bool,
        units: impl IntoIterator<Item = usize>,
        (apply, undo): (impl FnOnce(), impl FnOnce()),
        best: &mut String,
        rejected: Option<&str>,
    ) -> Verdict {
        let window = self
            .layout
            .as_ref()
            .filter(|_| local)
            .and_then(|layout| layout.window_for(units));
        let before = window.as_ref().and_then(|window| {
            let before = self.render_window(session, window);
            (before.len() <= self.window_limit)
                .then(|| comparable_tree(&before))
                .flatten()
                .map(|tree| (window, tree, before.len()))
        });
        apply();
        if let Some((window, before, before_len)) = before {
            #[cfg(test)]
            tests::WINDOW_PROBES.with(|n| n.set(n.get() + 1));
            let after = self.render_window(session, window);
            self.charge(before_len + after.len());
            if comparable_tree(&after).as_ref() == Some(&before) {
                return Verdict::Holds;
            }
            undo();
            return Verdict::Fails(None);
        }
        let candidate = render_with_escapes(session, self.doc, EscapeMode::Conservative);
        self.charge(candidate.len());
        if candidate_holds(&candidate, best, rejected, self.tree) {
            *best = candidate;
            return Verdict::Holds;
        }
        undo();
        Verdict::Fails(Some(candidate))
    }

    /// The whole document in the state a search finished in. A local search's
    /// `best` is only the last whole-document probe, so the state is rendered.
    fn settle(&self, session: &RenderSession, local: bool, best: String) -> String {
        if !local {
            return best;
        }
        render_with_escapes(session, self.doc, EscapeMode::Conservative)
    }
}

/// Narrow escaping one occurrence at a time after the unit-level pass.
/// Candidate sites come from the writer's own log. If logging changes the
/// control render, the unit-level result is kept. The search is bounded because
/// every load-bearing occurrence can require another full render and parse.
/// Where the budget binds, remaining occurrences stay escaped as §2 requires.
fn narrow_occurrences(session: &RenderSession, probe: &Probe, best: &mut String) {
    let unit_scoped = best.clone();
    session
        .relaxed_occurrences
        .with(|cell| *cell.borrow_mut() = Some(HashSet::new()));
    session
        .occurrence_log
        .with(|cell| *cell.borrow_mut() = Some(Vec::new()));
    let control = render_with_escapes(session, probe.doc, EscapeMode::Conservative);
    let occurrences = session
        .occurrence_log
        .with(|cell| cell.borrow_mut().take())
        .unwrap_or_default();
    if control != unit_scoped || occurrences.is_empty() {
        session
            .relaxed_occurrences
            .with(|cell| *cell.borrow_mut() = None);
        return;
    }

    // OFFERED FROM THE END OF THE DOCUMENT BACKWARDS, which is what makes the
    // escape that survives the OPENER's. §2 asks whether omitting the escapes
    // on an occurrence would let the construct FORM, and a construct forms at
    // its opener -- so with the opener still escaped every later candidate on
    // the same line is free, while relaxing the opener first leaves the escape
    // on a closer that was never load bearing (`{.note \}` where §2 wants
    // `\{.note}`). Both spellings re-parse to the same tree, so only the order
    // separates them.
    let order: Vec<Occurrence> = occurrences.into_iter().rev().collect();
    let search = |local: bool| -> String {
        session
            .relaxed_occurrences
            .with(|cell| *cell.borrow_mut() = Some(HashSet::new()));
        let mut best = unit_scoped.clone();
        let mut budget = 8 * (usize::BITS - order.len().leading_zeros()) as usize + 8;
        probe.begin_search(budget);
        relax_occurrences(session, probe, local, &order, &mut best, &mut budget, None);
        // AND THEN ONE SWEEP OF WHAT IS LEFT, because the halving is not a
        // FIXPOINT. Relaxing occurrences is not monotone: an occurrence
        // rejected while a neighbour was still escaped can be free once that
        // neighbour is relaxed, and the halving never revisits a group it has
        // descended past. Corpus 160 is the case -- the closing `:::` line
        // cannot go bare while the OPENING one is escaped, because then it is
        // the only fence marker on the page, and it can once the opener is
        // bare. The sweep spends the same budget, so where the budget is
        // already gone it costs nothing, which is the pathological document.
        for key in &order {
            if probe.exhausted(budget) {
                break;
            }
            if session
                .relaxed_occurrences
                .with(|cell| cell.borrow().as_ref().is_some_and(|set| set.contains(key)))
            {
                continue;
            }
            relax_occurrences(
                session,
                probe,
                local,
                std::slice::from_ref(key),
                &mut best,
                &mut budget,
                None,
            );
        }
        probe.settle(session, local, best)
    };
    let mut narrowed = search(true);
    if comparable_tree(&narrowed).as_ref() != Some(probe.tree) {
        narrowed = search(false);
    }
    *best = narrowed;
    session
        .relaxed_occurrences
        .with(|cell| *cell.borrow_mut() = None);
}

/// Hand `group` its bare form where the document still holds, halving the group
/// on failure.
fn relax_occurrences(
    session: &RenderSession,
    probe: &Probe,
    local: bool,
    group: &[Occurrence],
    best: &mut String,
    budget: &mut usize,
    rejected: Option<&str>,
) {
    if group.is_empty() || probe.exhausted(*budget) {
        return;
    }
    probe.spend(budget);
    let verdict = probe.keeps(
        session,
        local,
        group.iter().map(|&(unit, _, _)| unit),
        (
            || set_relaxed(session, group, true),
            || set_relaxed(session, group, false),
        ),
        best,
        rejected,
    );
    let Verdict::Fails(candidate) = verdict else {
        return;
    };
    if group.len() == 1 {
        return;
    }
    let half = group.len() / 2;
    let rejected = candidate.as_deref();
    relax_occurrences(
        session,
        probe,
        local,
        &group[..half],
        best,
        budget,
        rejected,
    );
    relax_occurrences(
        session,
        probe,
        local,
        &group[half..],
        best,
        budget,
        rejected,
    );
}

/// Whether `candidate` re-parses to `conservative_tree`.
///
/// The verdict is a function of the bytes alone, so a candidate identical to
/// `best` (verified) or to `rejected` (the enclosing group's failed candidate)
/// is answered without a parse. The halving produces such repeats constantly:
/// when one half of a failed group moves no bytes, the other half renders the
/// group's candidate again.
fn candidate_holds(
    candidate: &str,
    best: &str,
    rejected: Option<&str>,
    conservative_tree: &Document,
) -> bool {
    #[cfg(test)]
    tests::PROBES.with(|n| n.set(n.get() + 1));
    if candidate == best {
        return true;
    }
    if rejected == Some(candidate) {
        return false;
    }
    #[cfg(test)]
    tests::PROBE_PARSES.with(|n| n.set(n.get() + 1));
    comparable_tree(candidate).as_ref() == Some(conservative_tree)
}

fn set_relaxed(session: &RenderSession, group: &[Occurrence], relaxed: bool) {
    session.relaxed_occurrences.with(|cell| {
        if let Some(set) = cell.borrow_mut().as_mut() {
            for key in group {
                if relaxed {
                    set.insert(*key);
                } else {
                    set.remove(key);
                }
            }
        }
    });
}
/// Hand `units` their minimal form where the document still holds, halving the
/// group on failure.
///
/// `budget` BOUNDS THE SEARCH, because its cost is proportional to how many
/// units FAIL. A group holding no failing unit is relaxed in one probe, so a
/// document with a handful of them costs about log(n) probes -- but one where
/// nearly every unit fails drives the recursion to its leaves and pays a probe
/// per unit.
///
/// Such a document gains almost nothing from narrowing: it IS the conservative
/// form, arrived at because every block needed it. So the search stops when the
/// budget runs out and returns the state it has reached, which is verified like
/// every other -- the escalation is wider than §2b's minimum there, never
/// narrower, and no document's output can be wrong for it.
fn relax_units(
    session: &RenderSession,
    probe: &Probe,
    local: bool,
    units: &[usize],
    best: &mut String,
    budget: &mut usize,
    rejected: Option<&str>,
) {
    if units.is_empty() || probe.exhausted(*budget) {
        return;
    }
    probe.spend(budget);
    let verdict = probe.keeps(
        session,
        local,
        units.iter().copied(),
        (
            || set_escalated(session, units, false),
            || set_escalated(session, units, true),
        ),
        best,
        rejected,
    );
    let Verdict::Fails(candidate) = verdict else {
        return;
    };
    if units.len() == 1 {
        return;
    }
    let half = units.len() / 2;
    let rejected = candidate.as_deref();
    relax_units(
        session,
        probe,
        local,
        &units[..half],
        best,
        budget,
        rejected,
    );
    relax_units(
        session,
        probe,
        local,
        &units[half..],
        best,
        budget,
        rejected,
    );
}

fn set_escalated(session: &RenderSession, units: &[usize], escalated: bool) {
    session.escalated_units.with(|cell| {
        if let Some(set) = cell.borrow_mut().as_mut() {
            for unit in units {
                if escalated {
                    set.insert(*unit);
                } else {
                    set.remove(unit);
                }
            }
        }
    });
}

/// The comparable tree of `source`, or `None` when it does not parse.
///
/// The same normalization `escaping_is_redundant` compares through, so the
/// narrowing cannot answer differently from the decision that sent it here.
fn comparable_tree(source: &str) -> Option<Document> {
    #[cfg(test)]
    tests::PARSED_BYTES.with(|n| n.set(n.get() + source.len()));
    std::panic::catch_unwind(|| comparable_document(crate::parse::parse_for_carve_shape(source)))
        .ok()
}

/// The lines every EMPTIED marker-line container sits on, anywhere in the tree.
///
/// Empty is the only case that matters: a description holding content writes
/// that content and needs nothing from here. Collecting the set first keeps the
/// map below empty, and avoids cloning, for documents without such a description.
fn emptied_marker_lines(blocks: &[BlockNode], into: &mut HashSet<usize>) {
    emptied_marker_lines_at(blocks, 0, into);
}

/// `list_depth` is how many lists enclose `blocks`. It gates the emptied-item
/// arm below and nothing else: at the TOP level the canonical form of an
/// emptied item is `- +`, pinned by corpus fixtures 16-reference-link-4 and
/// 117-footnote-definition-inside-a-container-is-collected-2, and it round-trips
/// there because nothing follows at a shallower column for the marker to
/// capture. carve-js and carve-php draw the line in the same place.
fn emptied_marker_lines_at(blocks: &[BlockNode], list_depth: usize, into: &mut HashSet<usize>) {
    for block in blocks {
        match block {
            BlockNode::DefinitionList(list) => {
                for item in &list.items {
                    for def in &item.definitions {
                        if def.children.is_empty() {
                            if let Some(pos) = &def.pos {
                                into.insert(pos.start_line);
                            }
                        } else {
                            emptied_marker_lines_at(&def.children, list_depth, into);
                        }
                    }
                }
            }
            BlockNode::BlockQuote(quote) => {
                emptied_marker_lines_at(&quote.children, list_depth, into)
            }
            BlockNode::Admonition(admonition) => {
                emptied_marker_lines_at(&admonition.children, list_depth, into);
            }
            BlockNode::Directive(div) => emptied_marker_lines_at(&div.children, list_depth, into),
            BlockNode::Div(div) => emptied_marker_lines_at(&div.children, list_depth, into),
            BlockNode::Section(div) => emptied_marker_lines_at(&div.children, list_depth, into),
            // The two other walks over this tree (`normalize_escapes_block` and
            // `redundant_heading_ids`) both descend into a figure's block-quote
            // target, so this one does too. No input reaches it today - a `dd`
            // inside a block quote is not emptied here, because the definition
            // in it is not collected - but the asymmetry would be a trap the
            // moment that changes.
            BlockNode::Figure(figure) => {
                if let FigureTarget::BlockQuote(quote) = &*figure.target {
                    emptied_marker_lines_at(&quote.children, list_depth, into);
                }
            }
            BlockNode::FigureGroup(group) => {
                emptied_marker_lines_at(&group.children, list_depth, into)
            }
            BlockNode::List(list) => {
                for item in &list.items {
                    // A definition can be the only authored content on an
                    // item's marker line. Collection hoists it to the document,
                    // leaving an empty item whose own source span still names
                    // that line. Put the definition back there; spelling the
                    // item with `+` would attach the following outer content to
                    // this inner item on the next parse (carve-rs#1144).
                    if list_depth > 0 && item.children.is_empty() {
                        if let Some(pos) = &item.pos {
                            into.insert(pos.start_line);
                        }
                    }
                    // A definition the author wrote BETWEEN two of an item's
                    // blocks is the same case one level over: collecting it
                    // empties the line, and here that emptied line is what
                    // SPLIT one paragraph into two (corpus 228). Dropping it
                    // rejoins them, which is a different document. Nothing is
                    // left to carry the line, so the GAP between the two
                    // neighbours names it.
                    for pair in item.children.windows(2) {
                        let (Some(from), Some(to)) = (block_pos(&pair[0]), block_pos(&pair[1]))
                        else {
                            continue;
                        };
                        for line in (from.end_line + 1)..to.start_line {
                            into.insert(line);
                        }
                    }
                    emptied_marker_lines_at(&item.children, list_depth + 1, into);
                }
            }
            BlockNode::ExtensionCarrier(extension) => {
                emptied_marker_lines_at(&extension.children, list_depth, into);
            }
            _ => {}
        }
    }
}

/// Hoisted definitions that sit on one of those lines, keyed by the line.
///
/// "Those lines" is both cases: an emptied description's own line, and a line
/// inside an item's gap. A definition on either belongs back on it.
fn definitions_by_description_line(doc: &Document) -> HashMap<usize, DefinitionAtLine> {
    let mut lines = HashSet::new();
    emptied_marker_lines(&doc.children, &mut lines);
    let mut out = HashMap::new();
    if lines.is_empty() {
        return out;
    }
    for child in &doc.children {
        if let BlockNode::LinkReferenceDefinition(def) = child {
            if let Some(pos) = &def.pos {
                if lines.contains(&pos.start_line) {
                    // First writer wins for a line, which cannot normally
                    // collide: two definitions on one line is not a shape the
                    // parser produces.
                    out.entry(pos.start_line)
                        .or_insert_with(|| DefinitionAtLine::Link(Box::new(def.clone())));
                }
            }
        }
    }
    // A footnote definition is not in `children` - it hangs off the document in
    // its own map - so its line is the line its body starts on, which is the
    // definition line by production. That is the line
    // `footnote_defs_in_source_order` orders by, too.
    for (label, blocks) in &doc.footnote_defs {
        let Some(line) = blocks.first().and_then(block_pos).map(|pos| pos.start_line) else {
            continue;
        };
        if lines.contains(&line) {
            out.entry(line)
                .or_insert_with(|| DefinitionAtLine::Footnote(label.clone(), blocks.clone()));
        }
    }
    out
}

/// Render, and fall back to a break spelling that cannot be read as frontmatter
/// when the finished bytes would be.
fn render_with_escapes(session: &RenderSession, doc: &Document, escape_mode: EscapeMode) -> String {
    let authored = render_with_escapes_once(session, doc, escape_mode);
    if doc.frontmatter_raw.is_some()
        || !doc.frontmatter.is_empty()
        || !crate::parse::opens_frontmatter(&authored)
    {
        return authored;
    }
    let _marker = CellScope::replace(&session.hyphen_breaks_are_unsafe, true);
    let fallback = render_with_escapes_once(session, doc, escape_mode);
    if crate::parse::opens_frontmatter(&fallback) {
        authored
    } else {
        fallback
    }
}

fn render_with_escapes_once(
    session: &RenderSession,
    doc: &Document,
    escape_mode: EscapeMode,
) -> String {
    session.begin_pass();
    let mut ctx = CarveContext {
        block_depth: 0,
        inline_depth: 0,
        list_depth: 0,
        line_block_depth: 0,
        rendered_verbatim_tail: false,
        colon_fence_depth: 0,
        table_cell_depth: 0,
        cell_not_last: false,
        note_content_depth: 0,
        after_caption_host: false,
        paragraph_starts_after_caption_host: false,
        in_term: false,
        escape_mode,
        escape_unit: 0,
        definitions_by_line: definitions_by_description_line(doc),
        written_in_place: HashSet::new(),
        braced_spans: Vec::new(),
        open_kinds: Vec::new(),
        attribute_bracket_depth: 0,
        attribute_markers: HashMap::new(),
        brackets: BracketScope::default(),
        paired_closer_carry: std::cell::Cell::new(false),
        bracket_aliases: HashMap::new(),
    };
    let mut parts = Vec::new();
    // THE BLOCK AS WRITTEN when the tree has it. The key/value map cannot hold
    // a JSON or TOML block at all and loses a YAML block's key order, so
    // rebuilding from it dropped the first two and sorted the third
    // (markup-carve/carve-rs#1666). The map is the fallback for a document
    // built without one.
    if let Some(raw) = &doc.frontmatter_raw {
        parts.push(render_raw_frontmatter(raw));
    } else if !doc.frontmatter.is_empty() {
        parts.push(render_frontmatter(session, &doc.frontmatter));
    }
    // §7 puts hoisted definitions after the body, ordered among themselves by
    // source position, and PART 11 §6 binds the writer to the order the tree
    // holds: "fmt does not reorder ... those are the author's choices and the
    // AST records them".
    //
    // Rendering `children` and then the footnote map wrote every link definition
    // ahead of every footnote, and the footnotes themselves in LABEL order,
    // because the map is a BTreeMap - so `[^b]` written first came out after
    // `[^a]` (carve-rs#682). The ordering is the encoder's own
    // `ordered_document_entries`, reused rather than reimplemented, so the
    // written source and the published tree cannot disagree.
    let footnote_defs = crate::ast_json::footnote_defs_in_source_order(doc);
    let mut rendered = Vec::new();
    // The document level joins its own entries rather than going through
    // `render_blocks`, so the adjacent-sibling-list separator is written here
    // too. See the note beside `lists_would_merge`; without it a top-level pair
    // -- which is where authors actually write one -- still merged (carve#1088).
    let mut previous_list: Option<&List> = None;
    let mut separated_from_previous = false;
    for (index, entry) in crate::ast_json::ordered_document_entries(doc, &footnote_defs)
        .into_iter()
        .enumerate()
    {
        let escape_window::Visit::Render(recorded) =
            escape_window::visit(session, escape_window::ROOT, index)
        else {
            continue;
        };
        let text = match entry {
            crate::ast_json::DocEntry::Block(child) => {
                ctx.paragraph_starts_after_caption_host = ctx.after_caption_host;
                let text = render_block(session, child, &mut ctx);
                ctx.after_caption_host = hosts_caption(child);
                if let BlockNode::List(list) = child {
                    separated_from_previous =
                        previous_list.is_some_and(|previous| lists_would_merge(previous, list));
                    previous_list = Some(list);
                } else if !writes_nothing(&text) {
                    previous_list = None;
                    separated_from_previous = false;
                }
                text
            }
            crate::ast_json::DocEntry::FootnoteDef(label, blocks, _) => {
                ctx.after_caption_host = false;
                // Unless a definition list already wrote it where the author put
                // it (markup-carve/carve#805).
                let text = if blocks
                    .first()
                    .and_then(block_pos)
                    .is_some_and(|pos| ctx.written_in_place.contains(&pos.start_line))
                {
                    String::new()
                } else {
                    render_footnote_def_source(session, label, blocks, &mut ctx)
                };
                // A HOISTED DEFINITION IS A NON-LIST ENTRY and clears the pair
                // state exactly as a non-list block does. Without this the
                // boundary owed to the two lists ABOVE it was written in front
                // of the DEFINITION - the state was still raised when the
                // definition arrived, and this loop applies it where the entry
                // is pushed rather than inside the list arm.
                if !writes_nothing(&text) {
                    previous_list = None;
                    separated_from_previous = false;
                }
                text
            }
        };
        escape_window::leave(session, recorded);
        if !writes_nothing(&text) {
            rendered.push(if separated_from_previous && !rendered.is_empty() {
                hard_list_boundary(session, &text)
            } else {
                text
            });
        }
    }
    // THE FALLBACK SPELLING IS DECIDED IN `render_with_escapes`, on the finished
    // bytes, not here. It used to be decided here, from the FIRST RENDERED
    // BLOCK, and that could only see the shape where the break itself opens the
    // document - so a hoisted definition promoting a `---yaml`-shaped PARAGRAPH
    // to byte 0 walked straight past it (carve-rs#819).
    //
    // What stays here is the ORDER: §7 puts hoisted definitions after the body,
    // which is the decision that does the promoting.
    if !rendered.is_empty() {
        parts.push(rendered.join("\n\n"));
    }
    normalize(session, &parts.join("\n\n"))
}

/// `conservative_tree` is the caller's single parse of the conservative form;
/// `None` means it did not parse, which answers the question conservatively -
/// as does a minimal form that will not parse either.
fn escaping_is_redundant(minimal: &str, conservative_tree: Option<&Document>) -> bool {
    let Some(conservative_tree) = conservative_tree else {
        return false;
    };
    comparable_tree(minimal).is_some_and(|minimal_tree| &minimal_tree == conservative_tree)
}

fn comparable_document(mut doc: Document) -> Document {
    doc.source_len = 0;
    for block in &mut doc.children {
        normalize_escapes_block(block);
    }
    // Footnote definitions are NOT in `children` -- they hang off the document in
    // their own map. Leaving them un-normalized meant any escape inside one made
    // the two renders differ, so W4 escalated the WHOLE document to conservative:
    // `a.` alone formatted as `a.`, but the same paragraph beside a `[^f]: b.`
    // definition came back `a\.` (carve#352, corpus 22-footnotes).
    for blocks in doc.footnote_defs.values_mut() {
        for block in blocks.iter_mut() {
            normalize_escapes_block(block);
        }
    }
    doc
}

/// Collapse adjacent text and escaped-text nodes into one text node.
///
/// An escape is exactly what this comparison is deciding, so the two renders
/// must not be told apart BY it. Escaping a character both retypes the node and
/// SPLITS the run it sat in - `blue.` is one text node, `blue\.` is a text node
/// plus an escaped-text node - so without this every candidate character would
/// report a difference and escalate the whole document to conservative
/// escaping.
///
/// What survives the merge is the question worth asking: same characters, same
/// order, same surrounding structure - does dropping the escapes change
/// anything ELSE? PART 11 section 1 states this as the invariant's own
/// definition of equality.
fn normalize_escapes_inlines(nodes: &mut Vec<InlineNode>) {
    let mut merged: Vec<InlineNode> = Vec::with_capacity(nodes.len());
    for node in nodes.drain(..) {
        let text = match node {
            InlineNode::Text(t) => Some(t.value),
            InlineNode::EscapedText(t) => Some(t.value),
            other => {
                let mut other = other;
                normalize_escapes_nested(&mut other);
                merged.push(other);
                None
            }
        };
        if let Some(t) = text {
            if let Some(InlineNode::Text(previous)) = merged.last_mut() {
                previous.value.push_str(&t);
            } else {
                merged.push(InlineNode::text(t));
            }
        }
    }
    *nodes = merged;
}

/// Recurse into an inline node that carries inline children of its own.
fn normalize_escapes_nested(node: &mut InlineNode) {
    match node {
        InlineNode::Comment(_) => {}
        InlineNode::Emphasis(e) => normalize_escapes_inlines(&mut e.children),
        InlineNode::Link(l) => normalize_escapes_inlines(&mut l.children),
        InlineNode::Span(s) => normalize_escapes_inlines(&mut s.children),
        InlineNode::Ruby(r) => {
            for pair in &mut r.pairs {
                normalize_escapes_inlines(&mut pair.base);
                normalize_escapes_inlines(&mut pair.annotation);
            }
        }
        // An inline extension carries inline children too, and omitting it meant
        // an escape inside one made the two renders differ and escalated the
        // WHOLE document: `Press :kbd[Ctrl+C] to copy.` came back
        // `Press :kbd[Ctrl\+C] to copy\.` (carve#352, corpus 45-inline-extensions).
        InlineNode::Extension(e) => normalize_escapes_inlines(&mut e.children),
        // Editorial insert and delete carry inline children too. Omitting them
        // escalated any document containing an escape inside one: `{++a++}{.a}`
        // came back `{+\+a\++}{.a}`, over-escaping content the HTML target shows
        // as a literal `+a+` (carve#352, corpus 126).
        InlineNode::CriticInsert(i) => normalize_escapes_inlines(&mut i.children),
        InlineNode::CriticDelete(d) => normalize_escapes_inlines(&mut d.children),
        InlineNode::CriticSubstitute(s) => {
            normalize_escapes_inlines(&mut s.old);
            normalize_escapes_inlines(&mut s.new);
        }
        InlineNode::Footnote(f) => {
            if let Some(inline) = &mut f.inline {
                normalize_escapes_inlines(inline);
            }
        }
        // Listed rather than caught by `_`, so a new inline node that carries
        // children fails to compile here instead of being silently skipped. That
        // catch-all is how the extension gap (carve-rs#310) and the editorial gap
        // above both survived: adding a node type with children was enough to
        // introduce an over-escaping bug, with nothing to notice it.
        InlineNode::Text(_)
        | InlineNode::EscapedText(_)
        | InlineNode::SmartPunctuation(_)
        | InlineNode::Code(_)
        | InlineNode::Image(_)
        | InlineNode::Math(_)
        | InlineNode::RawInline(_)
        | InlineNode::LiteralInline(_)
        | InlineNode::Symbol(_)
        | InlineNode::AutoLink(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::Mention(_)
        | InlineNode::Tag(_)
        | InlineNode::CitationGroup(_)
        | InlineNode::Abbreviation(_)
        | InlineNode::NonBreakingSpace(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticComment(_) => {}
    }
}

fn normalize_escapes_block(block: &mut BlockNode) {
    match block {
        // No inline children: the label, destination and title are plain strings.
        BlockNode::LinkReferenceDefinition(_) => {}
        BlockNode::CitationDefinition(d) => normalize_escapes_inlines(&mut d.children),
        BlockNode::Heading(h) => normalize_escapes_inlines(&mut h.children),
        BlockNode::Paragraph(p) => normalize_escapes_inlines(&mut p.children),
        BlockNode::List(l) => {
            for item in &mut l.items {
                for child in &mut item.children {
                    normalize_escapes_block(child);
                }
            }
        }
        BlockNode::BlockQuote(b) => {
            for child in &mut b.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::Table(t) => {
            if let Some(cap) = &mut t.caption {
                normalize_escapes_inlines(cap);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    normalize_escapes_inlines(&mut cell.children);
                    if let Some(blocks) = &mut cell.blocks {
                        for child in blocks {
                            normalize_escapes_block(child);
                        }
                    }
                }
            }
        }
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                normalize_escapes_inlines(title);
            }
            for child in &mut a.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::LineBlock(lb) => {
            for child in &mut lb.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::Directive(d) => {
            if let Some(title) = &mut d.title {
                normalize_escapes_inlines(title);
            }
            for child in &mut d.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::Div(d) => {
            for child in &mut d.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::Section(d) => {
            for child in &mut d.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::DefinitionList(dl) => {
            for item in &mut dl.items {
                for term in &mut item.terms {
                    normalize_escapes_inlines(term);
                }
                for def in &mut item.definitions {
                    for child in def.iter_mut() {
                        normalize_escapes_block(child);
                    }
                }
            }
        }
        BlockNode::Figure(f) => {
            normalize_escapes_inlines(&mut f.caption);
            normalize_escapes_figure_target(f);
        }
        BlockNode::FigureGroup(g) => {
            if let Some(caption) = &mut g.caption {
                normalize_escapes_inlines(caption);
            }
            for child in &mut g.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::BlockExtension(e) => normalize_escapes_block(&mut e.fallback),
        BlockNode::ExtensionCarrier(e) => {
            for child in &mut e.children {
                normalize_escapes_block(child);
            }
        }
        BlockNode::CodeBlock(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

fn normalize_escapes_figure_target(f: &mut crate::ast::Figure) {
    match &mut *f.target {
        FigureTarget::BlockQuote(b) => {
            for child in &mut b.children {
                normalize_escapes_block(child);
            }
        }
        FigureTarget::Table(t) => {
            if let Some(cap) = &mut t.caption {
                normalize_escapes_inlines(cap);
            }
            for row in &mut t.rows {
                for cell in &mut row.cells {
                    normalize_escapes_inlines(&mut cell.children);
                    if let Some(blocks) = &mut cell.blocks {
                        for child in blocks {
                            normalize_escapes_block(child);
                        }
                    }
                }
            }
        }
        FigureTarget::Paragraph(p) => normalize_escapes_inlines(&mut p.children),
        FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
    }
}

/// Keep a hard boundary when list markers overlap or sibling lookahead would
/// change the first marker's dialect (PART 9 section 11 N1/N3).
fn lists_would_merge(a: &List, b: &List) -> bool {
    if a.ordered != b.ordered || is_task_list(a) != is_task_list(b) {
        return false;
    }
    if a.ordered {
        if !b.items.is_empty() {
            let alpha_roman = matches!(
                (a.ol_type, b.ol_type),
                (
                    Some(OrderedListType::LowerAlpha),
                    Some(OrderedListType::LowerRoman)
                ) | (
                    Some(OrderedListType::UpperAlpha),
                    Some(OrderedListType::UpperRoman)
                )
            );
            let roman_alpha = matches!(
                (a.ol_type, b.ol_type),
                (
                    Some(OrderedListType::LowerRoman),
                    Some(OrderedListType::LowerAlpha)
                ) | (
                    Some(OrderedListType::UpperRoman),
                    Some(OrderedListType::UpperAlpha)
                )
            );
            let next_start = b.start.unwrap_or(1);
            if alpha_roman && matches!(next_start, 1 | 5 | 10 | 50 | 100 | 500 | 1000) {
                return true;
            }
            if roman_alpha && matches!(next_start, 3 | 4 | 9 | 12 | 13 | 22 | 24) {
                return true;
            }
            let roman_value = match a.start.unwrap_or(1) {
                3 => Some(100),
                4 => Some(500),
                12 => Some(50),
                13 => Some(1000),
                22 => Some(5),
                24 => Some(10),
                _ => None,
            };
            if a.items.len() == 1 && alpha_roman && roman_value.is_some_and(|v| next_start == v + 1)
            {
                return true;
            }
            if a.items.len() == 1 && roman_alpha && a.start.unwrap_or(1) == 1 && next_start == 10 {
                return true;
            }
        }
        return a.delim.unwrap_or('.') == b.delim.unwrap_or('.') && a.ol_type == b.ol_type;
    }
    a.bullet_char.unwrap_or('-') == b.bullet_char.unwrap_or('-')
}

/// PART 11 §6g. An item with no recorded state takes the default for its box.
fn task_marker(item: &ListItem) -> String {
    let state = item
        .task_state
        .unwrap_or(if item.checked == Some(true) { 'x' } else { ' ' });
    format!("[{state}]")
}

fn is_task_list(list: &List) -> bool {
    list.items.iter().any(|item| item.checked.is_some())
}

/// `text` with §11 N1a's boundary in front of it: two extra blank lines, which
/// join with the ordinary one-blank block separator to make the run of three.
///
/// WRITTEN AS THE VERBATIM-BLANK SENTINEL, not as literal newlines.
/// `collapse_blank_lines` squeezes every run of three or more newlines to two --
/// correct for a decorative run, which the rule says to normalize away, and
/// fatal for this one, which the rule says to keep. The squeeze cannot tell them
/// apart from the text; only the writer knows, so the writer marks them and
/// `restore_verbatim` turns each marker line back into the blank it stands for.
fn hard_list_boundary(session: &RenderSession, text: &str) -> String {
    let blank = verbatim_blank(session);
    note_inserted(session, S_BLANK);
    note_inserted(session, S_BLANK);
    format!("{blank}\n{blank}\n{text}")
}

/// `text` with the same boundary in front of it, for a TIGHT ITEM's join.
///
/// THREE MARKER LINES, NOT TWO. `render_blocks` joins its parts with `\n\n`, so
/// the ordinary block separator already contributes one of §11 N1a's three blank
/// lines and the boundary supplies the other two. A tight item joins its
/// children with a SINGLE newline - a blank there would loosen the item on
/// re-parse - so nothing is contributed and all three are the boundary's.
///
/// §10i fixes the length at three whatever run the author wrote: the markers are
/// not newlines, so `collapse_blank_lines` squeezes a decorative run past them
/// and leaves this one alone.
fn hard_list_boundary_in_a_tight_item(session: &RenderSession, text: &str) -> String {
    let blank = verbatim_blank(session);
    note_inserted(session, S_BLANK);
    note_inserted(session, S_BLANK);
    note_inserted(session, S_BLANK);
    format!("{blank}\n{blank}\n{blank}\n{text}")
}

/// Whether this block leaves a PARAGRAPH OPEN on its last line, so a line
/// written below it at the same column is read as its continuation rather than
/// as a block of its own.
///
/// The other half of the `folds_into_the_paragraph_above` question: not "does
/// this block fold INTO an open paragraph" but "does it leave one open BELOW
/// it". The first three members are the same three, for the same reason - their
/// canonical source IS a bare inline run on its own line. A definition list
/// joins them because its last description ends in one too.
///
/// EACH MEMBER IS LOAD-BEARING, not carried along for symmetry: in an item
/// holding a sub-list, a table, one of these four blocks and a second sub-list,
/// that second sub-list is lost without the blank line. A heading, fence, table,
/// break, div, admonition and a sub-list with a different marker close at their
/// last line and owe the block under them nothing.
fn leaves_a_paragraph_open(block: &BlockNode) -> bool {
    matches!(
        block,
        BlockNode::Paragraph(_)
            | BlockNode::BlockImage(_)
            | BlockNode::Figure(_)
            | BlockNode::DefinitionList(_)
    )
}

/// The block a line written BELOW `node` is read against, for the kinds that
/// host one. `None` for every other kind, and for an EMPTY host - which is the
/// answer that keeps an emptied last item from costing a marker.
///
/// A definition list hands the question to its last description's last block,
/// the same way a quote and a list hand it to their last child. Answering it by
/// KIND instead - `DefinitionList` in `leaves_a_paragraph_open` - over-claims: a
/// description ending in a heading, table or fence leaves nothing open
/// (markup-carve/carve#1970).
fn the_last_block_inside(node: &BlockNode) -> Option<&BlockNode> {
    match node {
        BlockNode::BlockQuote(quote) => quote.children.last(),
        BlockNode::List(list) => list.items.last().and_then(|item| item.children.last()),
        BlockNode::DefinitionList(list) => list
            .items
            .last()
            .and_then(|item| item.definitions.last())
            .and_then(|body| body.children.last()),
        _ => None,
    }
}

/// Does an open paragraph inside `previous` reach DOWN to the next line written
/// at the same column?
///
/// `leaves_a_paragraph_open` answers this for a block that ends in a bare inline
/// run of its OWN. Two kinds end in one that is not theirs, and both were missed
/// because the fold test asked only whether the sibling above was itself a
/// paragraph: a SUB-LIST hands the question to its last item, whose marker
/// column IS the hosting item's content column, and a BLOCKQUOTE hands it to its
/// last child the same way.
///
/// Neither answers `true` on its own account, which is why this recurses instead
/// of naming the container kinds. An emptied last item, an empty quote, and a
/// quote or item ending in a heading, table, fence or break all leave nothing
/// open, and those are written at the content column exactly as before - a
/// marker there would cost the document a construct it did not have.
///
/// Ported from carve-js, which settled this shape in carve-js#1682 and
/// markup-carve/carve#1970; this engine kept the half-question until
/// carve-rs#1595.
fn an_open_paragraph_reaches_down(previous: &BlockNode) -> bool {
    match the_last_block_inside(previous) {
        Some(tail) => an_open_paragraph_reaches_down(tail),
        None => leaves_a_paragraph_open(previous),
    }
}

/// Whether a block FOLDS INTO an open paragraph written above it at the same
/// column - the other half of `leaves_a_paragraph_open`'s question. The three
/// kinds whose canonical source IS a bare inline run on its own line.
fn folds_into_an_open_paragraph(block: &BlockNode, rendered: &str) -> bool {
    matches!(
        block,
        BlockNode::Paragraph(_) | BlockNode::BlockImage(_) | BlockNode::Figure(_)
    ) && !opens_with_an_attribute_line(rendered)
}

/// Whether a block's written form OPENS with a block-attributes line (PART 2) -
/// `{` to `}` alone on the line. Such a line opens a block of its own, so
/// nothing above it can reach down past it.
fn opens_with_an_attribute_line(rendered: &str) -> bool {
    rendered
        .split('\n')
        .next()
        .is_some_and(|line| line.starts_with('{') && line.ends_with('}'))
}

/// Whether a sub-list written at the item's content column needs a blank line
/// above it to open at all.
fn needs_a_blank_line_above(
    previous: Option<&BlockNode>,
    previous_at_marker_column: bool,
    a_sub_list_already_opened: bool,
) -> bool {
    if previous_at_marker_column {
        return true;
    }
    match previous {
        None => false,
        Some(BlockNode::BlockQuote(_)) => true,
        Some(block) => a_sub_list_already_opened && leaves_a_paragraph_open(block),
    }
}

/// Whether a block's rendered text puts NOTHING into the written source.
fn writes_nothing(text: &str) -> bool {
    trim_non_nbsp(text).is_empty()
}

/// A caption line's written form, or `None` where the run reaches the page as
/// nothing.
fn caption_row(
    session: &RenderSession,
    caption: &[InlineNode],
    ctx: &mut CarveContext,
) -> Option<String> {
    let written = render_inlines(session, caption, ctx);
    if writes_nothing(&written) {
        return None;
    }
    Some(format!("^ {written}"))
}

fn render_blocks(session: &RenderSession, blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    if ctx.block_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
        return String::new();
    }
    ctx.block_depth += 1;
    let previous_host = ctx.after_caption_host;
    let previous_paragraph_start = ctx.paragraph_starts_after_caption_host;
    ctx.after_caption_host = false;
    let mut rendered = Vec::new();
    // TWO ADJACENT SIBLING LISTS NEED SOMETHING BETWEEN THEM. Written at the
    // same column with matching markers they merge on re-parse, so
    // `parse(fmt(x)) == parse(x)` is false for a document the parser reads as
    // two lists (carve#1088). carve#286 spent the marker axis -- emit the marker
    // as authored -- which separates them only while the markers DIFFER; when
    // both are `1.` at column 0 there is nothing left to preserve.
    //
    // THE SEPARATOR IS §11 N1a's HARD BOUNDARY: three blank lines. That is the
    // language's own way of saying "these are two lists", so the writer says it
    // instead of encoding the same fact as layout.
    //
    // It REPLACES a cumulative one-space offset. That offset existed only
    // because no separator was spelled, and it cost real correctness: the
    // second list came back indented by a space the author never wrote, a third
    // had to step to two, and at two spaces a bullet's content column NESTS the
    // later list inside the earlier one. Three blank lines separate any number
    // of sibling lists at the column they were written.
    let mut previous_list: Option<&List> = None;
    let mut separated_from_previous = false;
    let list = blocks.as_ptr() as usize;
    for (index, block) in blocks.iter().enumerate() {
        let escape_window::Visit::Render(recorded) = escape_window::visit(session, list, index)
        else {
            continue;
        };
        ctx.paragraph_starts_after_caption_host = ctx.after_caption_host;
        let text = render_block(session, block, ctx);
        escape_window::leave(session, recorded);
        ctx.after_caption_host = hosts_caption(block);
        if let BlockNode::List(list) = block {
            separated_from_previous =
                previous_list.is_some_and(|previous| lists_would_merge(previous, list));
            previous_list = Some(list);
        } else if !writes_nothing(&text) {
            previous_list = None;
            separated_from_previous = false;
        }
        if !writes_nothing(&text) {
            rendered.push(if separated_from_previous && !rendered.is_empty() {
                hard_list_boundary(session, &text)
            } else {
                text
            });
        }
    }
    let out = rendered.join("\n\n");
    ctx.after_caption_host = previous_host;
    ctx.paragraph_starts_after_caption_host = previous_paragraph_start;
    ctx.block_depth -= 1;
    out
}

fn hosts_caption(block: &BlockNode) -> bool {
    match block {
        BlockNode::Table(_)
        | BlockNode::CodeBlock(_)
        | BlockNode::BlockQuote(_)
        | BlockNode::BlockImage(_) => true,
        // The group's closer hosts the caption slot (§4c). With the slot
        // already filled, a following `^ ` paragraph re-parses as a paragraph
        // either way and §4 asks for the minimal form - so only an
        // UNCAPTIONED group makes the escape necessary (corpus
        // 318-composite-figures-6 is the detached shape that needs it).
        BlockNode::FigureGroup(group) => group.caption.is_none(),
        BlockNode::Paragraph(paragraph) if paragraph.children.len() == 1 => {
            match &paragraph.children[0] {
                InlineNode::Image(image) => !image.src.is_empty(),
                InlineNode::Math(math) => math.display,
                _ => false,
            }
        }
        _ => false,
    }
}

fn with_reset_colon_fence_depth<T>(
    ctx: &mut CarveContext,
    f: impl FnOnce(&mut CarveContext) -> T,
) -> T {
    let saved = ctx.colon_fence_depth;
    ctx.colon_fence_depth = 0;
    let out = f(ctx);
    ctx.colon_fence_depth = saved;
    out
}

fn render_inside_colon_container(
    session: &RenderSession,
    blocks: &[BlockNode],
    ctx: &mut CarveContext,
) -> String {
    ctx.colon_fence_depth += 1;
    let body = render_blocks(session, blocks, ctx);
    ctx.colon_fence_depth -= 1;
    body
}

/// Render a list item's children. A loose item separates every block with a
/// blank line. A tight item joins its blocks with a single newline so the
/// re-parse stays tight - EXCEPT it keeps the blank line adjacent to a nested
/// list child, whose own loose/tight rendering (and the continuation-indent
/// logic below) needs it. Without the tight join, a tight item with more than
/// one child (e.g. text after a fenced block, corpus 162) would be loosened by
/// the blank lines, breaking to_html(fmt(x)) == to_html(x); without the
/// nested-list exception, a tight item whose child is a nested list (corpus
/// 142) would stop being idempotent.
/// The definition the author wrote on a line strictly between two blocks.
///
/// The description case can ask its own node for the line; here the node is
/// gone, so the neighbours' spans name it. Marked written the same way, so the
/// document-level pass skips it and the label is not defined twice.
fn definition_in_gap(
    session: &RenderSession,
    before: &BlockNode,
    after: &BlockNode,
    ctx: &mut CarveContext,
) -> Option<String> {
    let from = block_pos(before)?.end_line;
    let to = block_pos(after)?.start_line;
    let (line, definition) = ((from + 1)..to).find_map(|line| {
        ctx.definitions_by_line
            .get(&line)
            .filter(|_| !ctx.written_in_place.contains(&line))
            .cloned()
            .map(|definition| (line, definition))
    })?;
    // MARKED AFTER RENDERING, not before. `render_block` returns an empty
    // string for a definition already marked written, so marking it first made
    // the gap render nothing and the document-level pass skip it too - the
    // definition disappeared from the document entirely.
    let written = match definition {
        DefinitionAtLine::Link(def) => {
            render_block(session, &BlockNode::LinkReferenceDefinition(*def), ctx)
        }
        DefinitionAtLine::Footnote(label, blocks) => {
            render_footnote_def_source(session, &label, &blocks, ctx)
        }
    };
    if written.is_empty() {
        return None;
    }
    ctx.written_in_place.insert(line);
    Some(written)
}

/// Write the hoisted definition whose authored source line is `line`.
///
/// Rendering happens before the line is claimed because the document-level
/// definition arm suppresses definitions that have already been written in
/// place. This is shared by every marker-line container that collection can
/// empty.
fn definition_at_line(
    session: &RenderSession,
    line: usize,
    ctx: &mut CarveContext,
) -> Option<String> {
    if ctx.written_in_place.contains(&line) {
        return None;
    }
    let definition = ctx.definitions_by_line.get(&line)?.clone();
    let written = match definition {
        DefinitionAtLine::Link(def) => {
            render_block(session, &BlockNode::LinkReferenceDefinition(*def), ctx)
        }
        DefinitionAtLine::Footnote(label, blocks) => {
            render_footnote_def_source(session, &label, &blocks, ctx)
        }
    };
    if written.is_empty() {
        return None;
    }
    ctx.written_in_place.insert(line);
    Some(written)
}

/// Sentinel marking a line to be written at the ITEM's marker column.
///
/// The list writer prefixes an item's continuation lines with its content
/// column. A `+` continuation marker and the block it attaches are the two
/// things that must NOT get that prefix (§17 L3), and they are produced deep
/// inside the item body where the prefix is not yet known - so they are tagged
/// here and the prefix loop honours the tag.
///
/// It is a PICKED sentinel (`SENTINEL_DEFAULTS`), not a fixed code point. The
/// tag is undone BY POSITION - a line that starts with it - so a continuation
/// line the AUTHOR opened with the same character answered that test, and the
/// writer ate the character AND wrote the line at the marker column, moving the
/// block out of the item (markup-carve/carve-rs#1226). carve-js reached the
/// same place, and moved the same marker into its own picked run, in
/// markup-carve/carve-js#1289.
fn marker_column(session: &RenderSession) -> char {
    sentinel(session, S_MARKER_COLUMN)
}

fn at_marker_column(session: &RenderSession, text: &str) -> String {
    let marker = marker_column(session);
    text.split('\n')
        .map(|line| {
            note_inserted(session, S_MARKER_COLUMN);
            format!("{marker}{line}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn adjacent_blocks_merge(left: &BlockNode, right: &BlockNode) -> bool {
    match (left, right) {
        (BlockNode::BlockQuote(_), BlockNode::BlockQuote(_))
        | (BlockNode::Table(_), BlockNode::Table(_))
        | (BlockNode::LineBlock(_), BlockNode::LineBlock(_))
        | (BlockNode::DefinitionList(_), BlockNode::DefinitionList(_)) => true,
        (BlockNode::List(left), BlockNode::List(right)) => {
            // Compared through the defaults, like the two marker checks above:
            // a parse records neither `.` nor `-` (carve#2828), so an ingested
            // tree that spells one out describes the same marker and has to
            // merge the same way.
            left.ordered == right.ordered
                && left.delim.unwrap_or('.') == right.delim.unwrap_or('.')
                && left.bullet_char.unwrap_or('-') == right.bullet_char.unwrap_or('-')
                && left.ol_type == right.ol_type
        }
        _ => false,
    }
}

fn render_item_blocks(
    session: &RenderSession,
    blocks: &[BlockNode],
    tight: bool,
    ctx: &mut CarveContext,
) -> String {
    if !tight {
        return render_blocks(session, blocks, ctx);
    }
    if ctx.block_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
        return String::new();
    }
    ctx.block_depth += 1;
    let mut out = String::new();
    let mut prev: Option<&BlockNode> = None;
    let mut prev_at_marker_column = false;
    // Whether a sub-list has already opened at this item's content column - the
    // condition under which a later bullet written there joins it instead of
    // opening below the paragraph above it. See `needs_a_blank_line_above`.
    let mut a_sub_list_already_opened = false;
    let list = blocks.as_ptr() as usize;
    for (index, block) in blocks.iter().enumerate() {
        let escape_window::Visit::Render(recorded) = escape_window::visit(session, list, index)
        else {
            continue;
        };
        let next = blocks.get(index + 1);
        let rendered = render_block(session, block, ctx);
        escape_window::leave(session, recorded);
        if writes_nothing(&rendered) {
            continue;
        }
        let mut separated = false;
        if let Some(prev_block) = prev {
            // A tight item joins every child with a single newline, including a
            // nested list. The blank line that used to be kept here existed to
            // work around nested looseness propagating to the outer item; with
            // that fixed in line_starts_paragraph, keeping it would insert a
            // blank the author never wrote and diverge from carve-js/carve-php.
            out.push('\n');
            if let Some(written) = definition_in_gap(session, prev_block, block, ctx) {
                out.push_str(&written);
                out.push('\n');
                // A definition written back BETWEEN the two blocks already ends
                // the paragraph above it, so the marker below is not needed -
                // and emitting it anyway changes the canonical form of corpus
                // 228, whose point is that a line at the definition's own
                // column forms its own tight block.
                separated = true;
            }
        }
        // THE QUESTION IS WHETHER THE BLOCK ABOVE LEAVES A PARAGRAPH OPEN, not
        // whether it IS one. A sub-list, a quote or a definition list ends in an
        // inline run that is not its own, and a block written at this item's
        // content column continues it: the image lands INSIDE the sub-list's
        // last item, not beside it (carve-rs#1595). carve-js settled the same
        // site in carve-js#1682; this engine asked only the bare-paragraph half.
        let folds_into_the_paragraph_above = prev.is_some_and(an_open_paragraph_reaches_down)
            && folds_into_an_open_paragraph(block, &rendered);
        let continues_a_run_at_the_marker_column = prev.is_some() && prev_at_marker_column;
        if matches!(block, BlockNode::List(_)) {
            if !separated && prev.is_some_and(|previous| adjacent_blocks_merge(previous, block)) {
                out.push_str(&hard_list_boundary_in_a_tight_item(session, &rendered));
            } else if !separated
                && needs_a_blank_line_above(prev, prev_at_marker_column, a_sub_list_already_opened)
            {
                out.push('\n');
                out.push_str(&rendered);
            } else {
                out.push_str(&rendered);
            }
            // Back at the content column, so a child below this one is read
            // against the list rather than against whatever stood at column 0
            // above it.
            prev = Some(block);
            prev_at_marker_column = false;
            a_sub_list_already_opened = true;
            continue;
        }
        // A LINE comment written at this item's content column lands ON the
        // marker column of a sub-list that stands above it, so the re-parse
        // reads the comment into that list's LAST ITEM rather than into this
        // one, and the next pass spells it a level deeper (carve-rs#1592,
        // carve-js#1676). The gate is the SIBLING'S KIND: `needs_a_blank_line_above`
        // answers the paragraph-folding question and is reachable only from the
        // list arm, so it can never see a comment.
        //
        // A blank line closes the sub-list. It does not loosen the item: a
        // comment spells no paragraph for the blank to part, so the re-parsed
        // list stays tight and the HTML is byte-identical.
        //
        // Only the `%% text` form. A `%%%` fence opener already closes the
        // sub-list, and a `{% text %}` delimited comment is not written at a
        // column that re-attaches - both measured, both left alone.
        if !separated
            && matches!(prev, Some(BlockNode::List(_)))
            && matches!(block, BlockNode::Comment(comment) if !comment.block && !comment.delimited)
        {
            out.push('\n');
        }
        // TWO BLOCKS THAT MERGE NEED THE MARKER ON THE LOWER ONE, not the upper
        // one. `at_marker_column` tags each LINE of what it is given and the tag
        // is undone BY POSITION - a line that starts with it. An item's first
        // block never starts a line of its own, because the list marker is
        // there, so tagging it wrote the sentinel mid-line where nothing undoes
        // it: the item came back spelling a literal `+` and both blocks escaped
        // to the top level (carve-rs#1595).
        //
        // Reading UP puts the marker on the second of the pair, which does start
        // its own line. Where a block already stands above the pair the old
        // reading still applies and the written form is unchanged.
        //
        // carve-js repairs the same site by stripping the tag off the item's
        // first line and opening the item with `- +` instead (carve-js#1681).
        // Measured here and NOT taken: that spelling loses the item inside a
        // blockquote host - `> - +` / `> > p` re-parses as an EMPTY item with the
        // quote hoisted out beside the list - on two of the grid's rows. Reading
        // up holds on all four hosts.
        let opens_a_merging_run_below = prev.is_some()
            && next.is_some_and(|next_block| adjacent_blocks_merge(block, next_block));
        let closes_a_merging_run_above =
            prev.is_some_and(|previous| adjacent_blocks_merge(previous, block));
        if !separated
            && (continues_a_run_at_the_marker_column
                || opens_a_merging_run_below
                || closes_a_merging_run_above
                || folds_into_the_paragraph_above)
        {
            out.push_str(&at_marker_column(session, "+"));
            out.push('\n');
            out.push_str(&at_marker_column(session, &rendered));
            prev = Some(block);
            prev_at_marker_column = true;
            continue;
        }
        out.push_str(&rendered);
        prev = Some(block);
        prev_at_marker_column = false;
    }
    ctx.block_depth -= 1;
    out
}

/// Render one block, charging what it writes to a unit of its own.
///
/// PART 11 §2b bounds an escalation to the smallest unit that fails, so the
/// escape pass has to know which unit each escaped character belongs to.
fn render_block(session: &RenderSession, node: &BlockNode, ctx: &mut CarveContext) -> String {
    let previous = ctx.escape_unit;
    ctx.escape_unit = next_escape_unit(session);
    let out = render_block_body(session, node, ctx);
    ctx.escape_unit = previous;
    out
}

fn render_block_body(session: &RenderSession, node: &BlockNode, ctx: &mut CarveContext) -> String {
    match node {
        // PART 12 section 18: renders nothing where it sits, on this target as
        // on every other. The Carve writer parses without the Citations
        // extension, so a definition line round-trips as the paragraph text it
        // is there; a tree carrying the node arrived from somewhere else.
        BlockNode::CitationDefinition(_) => String::new(),
        BlockNode::LinkReferenceDefinition(def) => {
            // Unless a definition list already wrote it on its own description
            // line, where the author put it - writing it twice would define the
            // label twice (markup-carve/carve#805).
            if def
                .pos
                .as_ref()
                .is_some_and(|pos| ctx.written_in_place.contains(&pos.start_line))
            {
                return String::new();
            }
            // PART 12 §10 gave this a node precisely so the writer can put the
            // line back. Before that there was nowhere to write it from, which is
            // why every resolved reference was INLINED instead (carve-rs#631).
            let title = def
                .title
                .as_ref()
                .map(|t| format!(" \"{}\"", escape_quoted(t)))
                .unwrap_or_default();
            let attrs = render_attrs(&def.attrs);
            let attrs = if attrs.is_empty() {
                String::new()
            } else {
                format!(" {attrs}")
            };
            // The href is re-escaped the way the inline tail's is: the reader
            // resolves the three destination escapes, so writing the resolved
            // value bare would hand back a line whose parentheses no longer
            // balance.
            format!(
                "[{}]: {}{title}{attrs}",
                def.label,
                escape_destination_escapes(&def.href)
            )
        }
        BlockNode::Heading(heading) => {
            // A heading is SINGLE-LINE (PART 2), so its text must not contain a
            // newline: emitting one would end the heading and silently re-parse
            // the remainder as a following block. No parse builds such a
            // heading, but an ingested AST can - PART 12 lets any inline sit in
            // a heading, break nodes included - so a break collapses to a
            // single space here rather than corrupting the document it is
            // written back to. Matches carve-js.
            let rendered = render_inlines(session, &heading.children, ctx);
            let text = collapse_breaks(trim_heading_edges(&rendered));
            let body = format!("{} {}", "#".repeat(heading.level as usize), text);
            // A generated id a fresh parse would re-derive is not the author's
            // source (carve-js#741); one it would not - an edited ingested tree -
            // is written, because the id lives nowhere else.
            let attrs = match heading.attrs.as_ref() {
                Some(attrs) => match attrs.id.as_ref() {
                    Some(id)
                        if !attrs.order.iter().any(|slot| matches!(slot, AttrSlot::Id))
                            && session
                                .redundant_ids
                                .with(|cell| cell.borrow().contains(id)) =>
                    {
                        let mut without = attrs.clone();
                        without.id = None;
                        Some(without)
                    }
                    _ => Some(attrs.clone()),
                },
                None => None,
            };
            with_block_attrs(&attrs, &body)
        }
        BlockNode::Paragraph(paragraph) => {
            let caption_can_open = render_attrs(&paragraph.attrs).is_empty()
                && ctx.paragraph_starts_after_caption_host;
            let body = guard_thematic_break_lines(
                session,
                &render_inlines_with_caption(session, &paragraph.children, ctx, caption_can_open),
                ctx.line_block_depth > 0,
            );
            with_block_attrs(&paragraph.attrs, &body)
        }
        BlockNode::CodeBlock(code) => {
            let fence = safe_fence(&code.content, 3);
            let info = code_fence_info(
                code.lang.as_deref(),
                code.title.as_deref(),
                code.label.as_deref(),
            );
            // The opener's quoted title is resolved onto `attrs.title` at parse
            // time so it reaches every consumer, but the fence carries it too -
            // emitting both says it twice and re-parses with an attribute ORDER
            // slot the source never had (carve#369). The fence is the authored
            // spelling, so it wins.
            let attrs = match (&code.title, &code.attrs) {
                (Some(title), Some(a)) if a.key_values.get("title") == Some(title) => {
                    without_key(a, "title")
                }
                _ => code.attrs.clone(),
            };
            // Staging takes lines without their final separator.
            let payload = if code.content.is_empty() {
                String::new()
            } else {
                let lines = code.content.strip_suffix('\n').unwrap_or(&code.content);
                format!("{}\n", protect_verbatim(session, lines))
            };
            with_block_attrs(&attrs, &format!("{fence}{info}\n{payload}{fence}"))
        }
        BlockNode::BlockQuote(quote) => {
            // Written back in the spelling it was read in
            // (markup-carve/carve#1718). Choosing structurally instead - the
            // fence whenever the quote holds a non-paragraph block -
            // rewrites the spelling of authored multi-block quotes, so the node
            // carries the author's choice rather than the writer inferring one.
            if quote.fenced {
                let fence = colon_fence_for(ctx);
                let body = render_inside_colon_container(session, &quote.children, ctx);
                return with_block_attrs(&quote.attrs, &format!("{fence} >\n{body}\n{fence}"));
            }
            let inner = with_reset_colon_fence_depth(ctx, |ctx| {
                render_blocks(session, &quote.children, ctx)
            });
            let body = inner
                .split('\n')
                .map(|line| {
                    if line.is_empty() {
                        ">".to_string()
                    } else {
                        format!("> {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            with_block_attrs(&quote.attrs, &body)
        }
        BlockNode::List(list) => {
            let body = with_reset_colon_fence_depth(ctx, |ctx| render_list(session, list, ctx));
            with_loose_key(list_needs_loose_key(list, &body), &list.attrs, &body)
        }
        // PART 11 §6 writes the marker the author used, now that the AST
        // records it (carve#976, carve-rs#843). Only the HYPHEN spelling can be
        // read back as a frontmatter fence, so it is the only one the fallback
        // moves, and only for a document whose emitted bytes would really be
        // misread - see `render_with_escapes`, where that is decided.
        BlockNode::ThematicBreak(rule) => {
            let mut marker = rule.marker.unwrap_or('-');
            if marker == '-'
                && session
                    .hyphen_breaks_are_unsafe
                    .with(|unsafe_| unsafe_.get())
            {
                marker = '*';
            }
            with_block_attrs(&rule.attrs, &marker.to_string().repeat(3))
        }
        BlockNode::Table(table) => {
            let mut attrs = table.attrs.clone();
            if let Some(attrs) = &mut attrs {
                if attrs.order.is_empty() {
                    if attrs.id.is_some() {
                        attrs.order.push(AttrSlot::Id);
                    }
                    if !attrs.classes.is_empty() {
                        attrs.order.push(AttrSlot::Class);
                    }
                    attrs
                        .order
                        .extend(attrs.key_values.keys().cloned().map(AttrSlot::Key));
                }
            }
            if !table.columns.is_empty() {
                let attrs = attrs.get_or_insert_with(Attrs::default);
                let align = table
                    .columns
                    .iter()
                    .map(|c| {
                        c.align
                            .map(|v| match v {
                                TableAlign::Left => "left",
                                TableAlign::Right => "right",
                                TableAlign::Center => "center",
                            })
                            .unwrap_or("")
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let valign = table
                    .columns
                    .iter()
                    .map(|c| {
                        c.valign
                            .map(|v| match v {
                                TableVerticalAlign::Top => "top",
                                TableVerticalAlign::Middle => "middle",
                                TableVerticalAlign::Bottom => "bottom",
                            })
                            .unwrap_or("")
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let widths = table
                    .columns
                    .iter()
                    .map(|c| {
                        c.width
                            .map(crate::table_width::percentage)
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                for (key, value) in [("aligns", align), ("valigns", valign), ("widths", widths)] {
                    if !value.chars().all(|c| c == ',') && !attrs.key_values.contains_key(key) {
                        attrs.key_values.insert(key.to_owned(), value);
                        attrs.order.push(AttrSlot::Key(key.to_owned()));
                    }
                }
            }
            if let Some(groups) = &table.row_groups {
                crate::table_source_metadata::add_row_groups(
                    attrs.get_or_insert_with(Attrs::default),
                    groups,
                );
            }
            with_block_attrs(&attrs, &render_table(session, table, ctx))
        }
        BlockNode::Admonition(admonition) => {
            let title = admonition
                .title
                .as_ref()
                .map(|title| {
                    format!(
                        " \"{}\"",
                        escape_quoted_title(&render_inlines(session, title, ctx), "admonition")
                    )
                })
                .unwrap_or_default();
            let label = admonition
                .label
                .as_ref()
                .map(|label| format!(" [{}]", write_flat_bracket_run(label)))
                .unwrap_or_default();
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(session, &admonition.children, ctx);
            with_block_attrs(
                &admonition.attrs,
                &format!("{fence} {}{title}{label}\n{body}\n{fence}", admonition.kind),
            )
        }
        BlockNode::Directive(directive) => {
            let title = directive
                .title
                .as_ref()
                .map(|title| {
                    format!(
                        " \"{}\"",
                        escape_quoted_title(&render_inlines(session, title, ctx), "directive")
                    )
                })
                .unwrap_or_default();
            let label = directive
                .label
                .as_ref()
                .map(|label| format!(" [{}]", write_flat_bracket_run(label)))
                .unwrap_or_default();
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(session, &directive.children, ctx);
            with_block_attrs(
                &directive.attrs,
                &format!("{fence} {}{title}{label}\n{body}\n{fence}", directive.kind),
            )
        }
        BlockNode::LineBlock(lb) => {
            // `::: |` is the line-block opener (PART 3, line_block_open).
            // Emitting a bare `:::` and tagging the node with a `.line-block`
            // class instead re-parsed as an ordinary div, so the node type
            // changed across a format round trip and
            // `parse(fmt(x)) == parse(x)` did not hold (carve issue 359).
            //
            // Inside the fence every newline IS a hard break (PART 3,
            // line_block_body), so the explicit backslash the inline writer
            // emits for a HardBreak would double it on re-parse.
            ctx.line_block_depth += 1;
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(session, &lb.children, ctx);
            ctx.line_block_depth -= 1;
            with_block_attrs(&lb.attrs, &format!("{fence} |\n{body}\n{fence}"))
        }
        BlockNode::Div(div) => {
            let label = div
                .label
                .as_ref()
                .map(|label| format!(" [{}]", write_flat_bracket_run(label)))
                .unwrap_or_default();
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(session, &div.children, ctx);
            with_block_attrs(&div.attrs, &format!("{fence}{label}\n{body}\n{fence}"))
        }
        BlockNode::Section(section) => render_blocks(session, &section.children, ctx),
        BlockNode::DefinitionList(list) => {
            let body = with_reset_colon_fence_depth(ctx, |ctx| {
                render_definition_list(session, &list.items, ctx)
            });
            with_loose_key(list.loose, &list.attrs, &body)
        }
        BlockNode::Figure(figure) => {
            with_block_attrs(&figure.attrs, &render_figure(session, figure, ctx))
        }
        BlockNode::FigureGroup(group) => {
            // §10g: the authored form - the attribute line where attributes
            // exist, the bare opener, the children, the closer at the opener's
            // width, and the group caption as a `^ ` line AFTER the closer.
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(session, &group.children, ctx);
            let caption = group
                .caption
                .as_ref()
                .and_then(|caption| caption_row(session, caption, ctx))
                .map(|row| format!("\n{row}"))
                .unwrap_or_default();
            with_block_attrs(
                &group.attrs,
                &format!("{fence} figure\n{body}\n{fence}{caption}"),
            )
        }
        BlockNode::BlockImage(image) => render_image(image),
        BlockNode::RawBlock(raw) => {
            let fence = safe_fence(&raw.content, 3);
            // The payload's own lines, then the ending after its LAST line. An
            // EMPTY payload has no line and writes none (carve-rs#2166).
            //
            // An all-blank payload carries its trailing ending IN `content`,
            // which is how it says how many lines it has, so
            // `protect_verbatim` is handed the payload WITHOUT that ending or it
            // stages a line too many. Everything goes through the staging now: a
            // blank line that bypassed it reached the pass that folds a run of
            // blank lines, so two authored blank lines came back as one and three
            // as one (carve-rs#2168). The code fence never had that bug because
            // its payload was always staged.
            let payload = if raw.content.is_empty() {
                String::new()
            } else {
                let lines = raw
                    .content
                    .strip_suffix('\n')
                    .filter(|_| !crate::ast::fenced_payload_needs_ending(&raw.content))
                    .unwrap_or(&raw.content);
                format!("{}\n", protect_verbatim(session, lines))
            };
            with_block_attrs(
                &raw.attrs,
                &format!("{fence}={}\n{payload}{fence}", escape_format(&raw.format)),
            )
        }
        BlockNode::AbbreviationDef(abbr) => {
            format!(
                "*[{}]: {}",
                escape_abbr(&abbr.abbr),
                escape_plain_line(&abbr.expansion)
            )
        }
        BlockNode::Comment(comment) => {
            if comment.delimited {
                format!("{{%{} %}}", pad_delimited(&comment.content))
            } else if comment.block {
                render_block_comment(session, &comment.content)
            } else {
                let content = comment.content.trim_end_matches([' ', '\t']);
                if content.is_empty() {
                    // The line form cannot end in ASCII whitespace, including
                    // when the comment content becomes empty after trimming.
                    "%%".to_string()
                } else {
                    format!("%% {content}")
                }
            }
        }
        // The canonical writer has no spelling for a block extension - Carve
        // 0.1 source spells none - so it writes the fallback, which is what the
        // node MEANS to a reader without the extension. The name, version and
        // payload are lost, which is the loss PART 12 §33 defines.
        BlockNode::BlockExtension(n) => {
            with_block_attrs(&n.attrs, &render_block(session, &n.fallback, ctx))
        }
        BlockNode::ExtensionCarrier(extension) => with_block_attrs(
            &extension.attrs,
            &render_blocks(session, &extension.children, ctx),
        ),
    }
}

/// A copy of `attrs` without one key-value, dropping the slot from `order`.
/// Returns `None` when the removal leaves nothing to render.
fn without_key(attrs: &Attrs, key: &str) -> Option<Attrs> {
    let mut next = attrs.clone();
    next.key_values.remove(key);
    next.order
        .retain(|slot| !matches!(slot, AttrSlot::Key(k) if k == key));
    if next.id.is_none() && next.classes.is_empty() && next.key_values.is_empty() {
        return None;
    }
    Some(next)
}

/// PART 9 §17 L7: the writer spells the looseness with `{loose}` ONLY where the
/// blank-line spelling cannot say it.
fn with_loose_key(needs_key: bool, attrs: &Option<Attrs>, body: &str) -> String {
    if !needs_key {
        return with_block_attrs(attrs, body);
    }
    let mut attrs = attrs.clone().unwrap_or_default();
    attrs.key_values.insert("loose".to_string(), String::new());
    attrs
        .order
        .retain(|slot| !matches!(slot, AttrSlot::Key(key) if key == "loose"));
    if attrs.order.is_empty() {
        // An EMPTY order means `render_attrs` falls back to id, classes, then
        // keys - and naming one slot switches it to the ordered branch, which
        // would then drop the id and the classes it no longer lists. Spell the
        // fallback out so the key can lead without moving anything else.
        attrs.order = vec![
            AttrSlot::Key("loose".to_string()),
            AttrSlot::Id,
            AttrSlot::Class,
        ];
    } else {
        attrs.order.insert(0, AttrSlot::Key("loose".to_string()));
    }
    // The key LEADS, which is where an author writes it and where the corpus
    // shows it. Its position among the other slots is not observable in the
    // output - it is consumed before any renderer sees it - so leading is a
    // spelling choice rather than a fact being moved.
    format!("{}\n{body}", render_attrs(&Some(attrs)))
}

/// Whether a LIST needs the key: §17 L7's re-parse, with the one shortcut that
/// is sound in a single direction.
fn list_needs_loose_key(list: &List, body: &str) -> bool {
    if list.tight || list.items.is_empty() {
        return false;
    }
    // TWO OR MORE ITEMS ALWAYS RE-PARSE LOOSE. §17 L2 loosens on a blank line
    // between items, and this writer emits one between every pair of a loose
    // list's items, so the re-parse below can only ever answer "already spelled"
    // here. A shortcut in ONE direction: it never suppresses a key the re-parse
    // would have emitted.
    if list.items.len() > 1 {
        return false;
    }
    // ONE ITEM has no "between items" for a blank line to stand in, so the only
    // spelling left is one the item's own CONTENT produces - and whether it does
    // is the parser's question. §17 L1, L2 and L6 decide it together, so a
    // second copy of them here would answer differently the day any of them
    // moves: a lead container holding a blank line re-reads LOOSE, while the
    // same blank line before a fence does not.
    //
    // A body with NO blank line in it cannot re-read loose either way, so the
    // shape the clause exists for - a one-item list holding one paragraph - is
    // answered without a parse.
    if !body_has_blank_line(body) {
        return true;
    }
    match comparable_tree(body).as_ref().and_then(|doc| {
        doc.children
            .iter()
            .find(|child| !matches!(child, BlockNode::Comment(_)))
    }) {
        Some(BlockNode::List(reparsed)) => reparsed.tight,
        // Anything else means the body did not read back as a list at all, so
        // the looseness certainly did not survive.
        _ => true,
    }
}

/// A blank line INSIDE `body`, which is the only place one can loosen it.
///
/// Interior, so a body's own leading or trailing newline does not count: those
/// are the writer's joins rather than content, and reading one as a blank line
/// would send every single-item list through the re-parse for nothing.
fn body_has_blank_line(body: &str) -> bool {
    body.match_indices('\n').any(|(at, _)| {
        body[at + 1..].split('\n').next().is_some_and(|line| {
            line.len() < body.len() - at - 1
                && line
                    .trim_matches(|ch: char| ch == ' ' || ch == '\t')
                    .is_empty()
        })
    })
}

fn with_block_attrs(attrs: &Option<Attrs>, body: &str) -> String {
    let rendered = render_attrs(attrs);
    if rendered.is_empty() {
        body.to_string()
    } else {
        format!("{rendered}\n{body}")
    }
}

fn render_list(session: &RenderSession, node: &List, ctx: &mut CarveContext) -> String {
    ctx.list_depth += 1;
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    // The marker is semantic (§11: a different bullet char / ordered delim
    // starts a new list), so emit it as authored - normalizing would merge
    // adjacent sibling lists on re-parse (carve issue 286).
    let delim = node.delim.unwrap_or('.');
    let bullet = node.bullet_char.unwrap_or('-');
    let items = node.items.as_ptr() as usize;
    for (idx, item) in node.items.iter().enumerate() {
        let escape_window::Visit::Render(recorded) = escape_window::visit(session, items, idx)
        else {
            if node.ordered {
                counter += 1;
            }
            continue;
        };
        // NO absolute depth term. The parent item's continuation prefix is
        // already the child list's indentation, so adding `"  " * (depth - 1)`
        // on top indented every level twice - and the two-space strip below was
        // compensating for it. Output grew as O(depth^3) where the source is
        // O(depth^2), and `05-lists-5` came back with four spaces where it was
        // written with two (carve-rs#594, the same defect carve-js fixed in its
        // #653).
        let mut prefix = if node.ordered {
            let marker = if node.bare_marker {
                String::new()
            } else {
                ordered_marker(counter, node.ol_type)
            };
            counter += 1;
            format!("{marker}{delim} ")
        } else if item.checked.is_some() {
            format!("{bullet} {} ", task_marker(item))
        } else {
            format!("{bullet} ")
        };
        let continuation_width = if node.ordered { prefix.len() } else { 2 };
        let item_attrs = render_attrs(&item.attrs);
        if !item_attrs.is_empty() {
            prefix = if node.ordered {
                format!("{}{item_attrs} ", prefix.trim_end())
            } else if item.checked.is_some() {
                format!("{bullet}{item_attrs} {} ", task_marker(item))
            } else {
                format!("{bullet}{item_attrs} ")
            };
        }
        let mut content = if item.children.is_empty() {
            item.pos
                .as_ref()
                .and_then(|pos| definition_at_line(session, pos.start_line, ctx))
                .unwrap_or_default()
        } else {
            render_item_blocks(session, &item.children, node.tight, ctx)
        };
        // EMPTINESS is the only thing that earns `+`. A second, text-shaped test
        // (content starts with `[^` and holds a colon-space) stood for "the item
        // spelled a footnote definition that collection hoisted" - but collection
        // always empties the item, so that test could only fire on authored
        // inline content, and it replaced the whole item with `+` (carve-rs#2097).
        if trim_non_nbsp(&content).is_empty() {
            content = "+".to_string();
        }
        let content = trim_non_nbsp(&content).to_string();
        // COUNT THE MARKER-COLUMN TAGS STANDING IN THE ASSEMBLED ITEM, here and
        // not at restore time: this loop is what consumes them, so this is the
        // last moment an authored one is still visible. Counted over the whole
        // item rather than only over the lines the loop strips - a tag the item
        // dropped would otherwise hide an authored occurrence behind a matching
        // insertion count, and answering from the item's own text cannot.
        note_seen(
            session,
            S_MARKER_COLUMN,
            content.matches(marker_column(session)).count(),
        );
        let mut lines = if content.is_empty() {
            vec!["".to_string()]
        } else {
            content.split('\n').map(str::to_string).collect()
        };
        let first = lines.remove(0);
        out.push_str(&format!("{prefix}{first}\n"));
        // A task checkbox is content and an attribute block is item metadata;
        // neither moves the bare marker's content column (carve#1701).
        //
        // Writing the continuation at the marker's full width put every block
        // after the item's first, four columns too far in, where an INDENTED
        // block opener opens nothing: a heading, a fence or a quote came back as
        // text of the marker line's paragraph.
        let continuation = " ".repeat(continuation_width);
        for line in lines {
            if line.is_empty() || line.chars().eq([verbatim_blank(session)]) {
                out.push_str(&line);
                out.push('\n');
            } else if let Some(rest) = line.strip_prefix(marker_column(session)) {
                // The continuation marker and its attached block sit at the
                // ITEM's marker column, not its content column (§17 L3).
                out.push_str(&format!("{rest}\n"));
            } else {
                out.push_str(&format!("{continuation}{line}\n"));
            }
        }
        let ends_with_nested_list = content.lines().last().is_some_and(|line| {
            line.starts_with(' ') && is_rendered_list_marker(line.trim_start())
        });
        if !node.tight && idx < node.items.len() - 1 && !ends_with_nested_list {
            out.push('\n');
        }
        escape_window::leave(session, recorded);
    }
    ctx.list_depth -= 1;
    trim_end_non_nbsp(&out).to_string()
}

fn ordered_marker(n: usize, ty: Option<OrderedListType>) -> String {
    match ty {
        Some(OrderedListType::LowerAlpha) => alpha_marker(n, false),
        Some(OrderedListType::UpperAlpha) => alpha_marker(n, true),
        Some(OrderedListType::LowerRoman) => roman_marker(n).to_ascii_lowercase(),
        Some(OrderedListType::UpperRoman) => roman_marker(n),
        None => n.to_string(),
    }
}

fn is_rendered_list_marker(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("- [")
        || line.starts_with("* [")
        || [". ", ") "].iter().any(|sep| {
            line.split_once(sep).is_some_and(|(marker, _)| {
                !marker.is_empty() && marker.chars().all(|ch| ch.is_ascii_alphanumeric())
            })
        })
}

fn alpha_marker(n: usize, upper: bool) -> String {
    let base = ((n.saturating_sub(1) % 26) as u8) + if upper { b'A' } else { b'a' };
    (base as char).to_string()
}

fn roman_marker(mut n: usize) -> String {
    let values = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::new();
    for (value, token) in values {
        while n >= value {
            out.push_str(token);
            n -= value;
        }
    }
    if out.is_empty() {
        "I".to_string()
    } else {
        out
    }
}

/// Every entry writes its own description line, so consecutive `::` lines never
/// end up sharing one: a `<dl>` writes back as ONE list with the grouping it
/// parsed from, and no term acquires the next entry's description.
fn render_definition_list(
    session: &RenderSession,
    items: &[DefinitionItem],
    ctx: &mut CarveContext,
) -> String {
    let mut out: Vec<String> = Vec::new();
    // NO SEPARATING BLANK between entries. This used to emit one before the next
    // term whenever the previous body spanned more than its own `: ` line, to
    // stop a following flush-left `::` from folding into the earlier `<dd>` when
    // that body ended in a nested closed code fence (carve-rs#1559, corpus
    // 455-4). markup-carve/carve#1970 ruled that shape reads as TWO entries with
    // no blank - the oracle, carve-js and carve-php all read it that way - and
    // the parser now agrees (a nested lead fence closed by an indented closer
    // releases its ownership, so the term below opens a new entry). With the
    // absorption gone the blank was a workaround for a bug that no longer
    // exists, and dropping it converges this writer's `carve` output with the
    // other engines.
    let list = items.as_ptr() as usize;
    for (index, item) in items.iter().enumerate() {
        let escape_window::Visit::Render(recorded) = escape_window::visit(session, list, index)
        else {
            continue;
        };
        for term in &item.terms {
            // A comment opening a line must stay past the term's column, or it
            // would end the term (carve#2411).
            let outer_term = std::mem::replace(&mut ctx.in_term, true);
            let rendered = render_inlines(session, term, ctx);
            ctx.in_term = outer_term;
            if rendered.is_empty() {
                crate::render_carve_error::record_unspellable(
                    "definition_term",
                    "an empty definition term has no Carve source spelling",
                );
            }
            out.push(format!(":: {rendered}"));
        }
        for def in &item.definitions {
            // An EMPTY description whose line carries a hoisted definition is one
            // the author wrote that definition on: write it back there
            // (markup-carve/carve#805). Without this the line came out as a bare
            // `:`, which re-parses into the term above it.
            if def.children.is_empty() {
                let line = def.pos.as_ref().map(|pos| pos.start_line);
                let written = line.and_then(|line| definition_at_line(session, line, ctx));
                if let Some(written) = written {
                    let mut written_lines = written.split('\n');
                    out.push(format!(": {}", written_lines.next().unwrap_or_default()));
                    // A footnote body can be multi-line; its continuation lines
                    // carry the body's own indent and sit under the description.
                    for written_line in written_lines {
                        out.push(format!("  {written_line}"));
                    }
                    continue;
                }
            }
            let body = trim_non_nbsp(&render_blocks(session, def, ctx)).to_string();
            if body.is_empty() {
                out.push(": {empty}".to_string());
                continue;
            }
            let mut lines = body.split('\n');
            out.push(format!(": {}", lines.next().unwrap_or_default()));
            for line in lines {
                out.push(format!("  {line}"));
            }
        }
        escape_window::leave(session, recorded);
    }
    out.join("\n")
}

fn colon_fence_for(ctx: &CarveContext) -> String {
    ":".repeat(3 + ctx.colon_fence_depth)
}

/// Tables prefer the NATIVE header form: an `=` on each header cell, plus the
/// per-cell `<`/`>`/`~` alignment markers.
///
/// A colspan cell is always written plain (`| < |`), so a header row can keep
/// the native form when its span markers form a TRAILING run of colspans after
/// at least one real header cell: each `<` absorbs into the `|=` header on its
/// left, and the row is still promoted by those markers. Everything else needs
/// a delimiter row: a LEADING span has no `|=` anchor before it; a real cell
/// AFTER a span would have to be written `|=< K`, read as an aligned header; and
/// a trailing ROWSPAN (`^`) does not absorb left, so a native `| ^ |` in the
/// first row is not a header cell and the row would fall out of the head.
fn render_table(session: &RenderSession, node: &Table, ctx: &mut CarveContext) -> String {
    let mut rows = Vec::new();
    let header_row = node
        .rows
        .first()
        .is_some_and(|row| !row.cells.is_empty() && row.cells.iter().all(|cell| cell.header));
    let needs_delimiter = header_row
        && node.rows.first().is_some_and(|row| {
            let first_span = row.cells.iter().position(|cell| cell.span.is_some());
            match first_span {
                None => false,
                Some(index) => {
                    // Native only for a trailing run of colspans after a real
                    // header cell; every other span shape needs the delimiter.
                    !(index >= 1
                        && row.cells[index..]
                            .iter()
                            .all(|cell| cell.span == Some(TableCellSpan::Colspan)))
                }
            }
        });

    for (row_index, row) in node.rows.iter().enumerate() {
        let mut cells = Vec::new();
        for (cell_index, cell) in row.cells.iter().enumerate() {
            ctx.cell_not_last = cell_index + 1 < row.cells.len();
            // In the delimiter form the promoted row is written as ordinary
            // data cells - the row after it is what makes them headers.
            let mark_header = !(needs_delimiter && row_index == 0);
            cells.push(render_table_cell(session, cell, ctx, mark_header));
        }
        ctx.cell_not_last = false;
        // A row whose every cell is blank is not a table row
        // (markup-carve/carve#1954); a row attribute does not save it, because
        // the reader strips that before deciding.
        if cells.iter().all(|cell| matches!(cell.trim(), "" | "=")) {
            crate::render_carve_error::record_unspellable(
                "table_row",
                "a table row whose every cell is blank has no Carve source spelling",
            );
        }
        rows.push(render_table_row(&cells, &render_attrs(&row.attrs)));
    }
    if needs_delimiter {
        let sep = vec!["---"; node.rows[0].cells.len()].join("|");
        rows.insert(1, format!("|{sep}|"));
    }
    if let Some(row) = node
        .caption
        .as_ref()
        .and_then(|caption| caption_row(session, caption, ctx))
    {
        rows.push(row);
    }
    rows.join("\n")
}

/// A cell's written form: its PREFIX glued to the opening pipe, then one
/// space, then the content, then one space before the closing pipe.
///
/// The prefix has to touch the pipe - a space in front of `=` or of an
/// attribute block makes it literal content - but the CONTENT does not, and
/// the padded form is the readable one. It is also the safe one: the alignment
/// sigil and the attribute slot are both read GLUED off the untrimmed cell, so
/// a glued content character was handed to one of them (carve-rs#819). This
/// used to be two guards, each enumerating the characters that merge; the
/// space covers every cell without a list.
///
/// An EMPTY cell takes a single space, not two, so a column does not grow a
/// space each time the document is formatted.
fn pad_cell(prefix: &str, content: &str) -> String {
    if content.is_empty() {
        format!("{prefix} ")
    } else {
        format!("{prefix} {content} ")
    }
}

fn render_table_row(cells: &[String], attrs: &str) -> String {
    format!("|{}|{}", cells.join("|"), attrs)
}

fn render_table_cell(
    session: &RenderSession,
    cell: &TableCell,
    ctx: &mut CarveContext,
    mark_header: bool,
) -> String {
    if cell.blocks.is_some() && !cell.children.is_empty() {
        crate::render_carve_error::record_unspellable(
            "table_cell",
            "a cell cannot carry both inline children and blocks",
        );
    }
    let attrs = render_attrs(&cell.attrs);
    // A lone span marker keeps a SPACE before it. Glued to the opening pipe, `<`
    // is also the left-alignment sigil, and the two readings differ: the
    // executable spec reads `|<|` as alignment on an empty cell where all three
    // engines read a colspan (markup-carve/carve#710). `alignment_marker` is defined
    // as glued and `colspan_marker` may carry surrounding whitespace, so the
    // padded form means the same thing to every reader and the writer must not
    // emit the ambiguous one. `^` is not an alignment sigil, but takes the same
    // shape so a row of span cells stays readable.
    //
    // A cell attribute stays GLUED to the pipe, where the grammar puts it; the
    // space goes between it and the marker.
    if let Some(span) = cell.span {
        let marker = if span == TableCellSpan::Rowspan {
            "^"
        } else {
            "<"
        };
        return pad_cell(&attrs, marker);
    }
    let align = align_marker(cell.align);
    let valign = match cell.valign {
        Some(TableVerticalAlign::Top) => "^",
        Some(TableVerticalAlign::Middle) => "~",
        Some(TableVerticalAlign::Bottom) => "v",
        None => "",
    };
    let inherited_horizontal = if align.is_empty() && !valign.is_empty() {
        "?"
    } else {
        ""
    };
    // CELL ATTRIBUTES BIND LAST (grammar §20 T10): the kind marker first, then
    // the alignment marker, then the attribute block, glued to the marker run.
    // Writing the block AHEAD of the markers had no spelling for an attributed
    // header cell at all -- it emitted `|{#x}=R|`, which the reader takes as a
    // data cell whose content is `=R`, so `toHtml(fmt(x)) != toHtml(x)` on
    // every attributed header cell.
    let prefix = format!(
        "{}{}{}{}{}",
        if cell.header && mark_header { "=" } else { "" },
        align,
        inherited_horizontal,
        valign,
        attrs
    );
    ctx.table_cell_depth += 1;
    let mut content = match &cell.blocks {
        Some(blocks) => render_inlines(
            session,
            &crate::render_plain::flatten_cell_block_inlines(blocks),
            ctx,
        ),
        None => match merge_break_layout(&cell.children) {
            Some(merged) => render_inlines(session, &merged, ctx),
            None => render_inlines(session, &cell.children, ctx),
        },
    };
    ctx.table_cell_depth -= 1;
    content = content.replace(['\r', '\n'], " ");
    // A break at the cell's edge separates nothing, so its space is not written.
    let is_break =
        |node: &&InlineNode| matches!(node, InlineNode::SoftBreak(_) | InlineNode::HardBreak(_));
    let leading = cell.children.iter().take_while(is_break).count();
    if cell.blocks.is_some() {
        // Block content has already been flattened to a single cell line.
    } else if leading == cell.children.len() {
        content.clear();
    } else {
        let trailing = cell.children.iter().rev().take_while(is_break).count();
        content.truncate(content.len() - trailing);
        content.drain(..leading);
    }
    // The space `pad_cell` writes after the prefix is what keeps the content's
    // first character content. The header `=` is read glued to the pipe and the
    // alignment sigil glued after it, off the UNTRIMMED cell, and a cell whose
    // text begins with an attribute block used to hand that block to the
    // reader as the CELL's attributes: `| ~x~ |` came back as `|=~x~|`, a
    // CENTERED column holding `x~` (carve-rs#819). Padding every cell parts
    // them without enumerating which characters merge.
    pad_cell(&prefix, &content)
}

/// A cell's hard break is written as one space, so layout on either side of it
/// would double that space (PART 11 §1b). Trims it off the neighboring text at
/// every depth; `None` when no break has any.
fn merge_break_layout(nodes: &[InlineNode]) -> Option<Vec<InlineNode>> {
    let mut out = nodes.to_vec();
    trim_break_layout(&mut out).then_some(out)
}

fn trim_break_layout(nodes: &mut Vec<InlineNode>) -> bool {
    let layout = |c: char| c == ' ' || c == '\t';
    let mut changed = false;
    for index in 0..nodes.len() {
        if let Some(children) = formatted_children(&mut nodes[index]) {
            changed |= trim_break_layout(children);
        }
        if !matches!(nodes[index], InlineNode::HardBreak(_)) {
            continue;
        }
        if let Some(InlineNode::Text(t)) = index.checked_sub(1).and_then(|i| nodes.get_mut(i)) {
            let kept = t.value.trim_end_matches(layout).len();
            changed |= kept != t.value.len();
            t.value.truncate(kept);
        }
        if let Some(InlineNode::Text(t)) = nodes.get_mut(index + 1) {
            let kept = t.value.trim_start_matches(layout);
            if kept.len() != t.value.len() {
                changed = true;
                t.value = kept.to_string();
            }
        }
    }
    if changed {
        nodes.retain(|node| !matches!(node, InlineNode::Text(t) if t.value.is_empty()));
    }
    changed
}

fn formatted_children(node: &mut InlineNode) -> Option<&mut Vec<InlineNode>> {
    match node {
        InlineNode::Emphasis(n) => Some(&mut n.children),
        InlineNode::Span(n) => Some(&mut n.children),
        InlineNode::Link(n) => Some(&mut n.children),
        InlineNode::CriticInsert(n) => Some(&mut n.children),
        InlineNode::CriticDelete(n) => Some(&mut n.children),
        _ => None,
    }
}

/// THE TARGET KEEPS ITS OWN ATTRIBUTES (ruling markup-carve/carve#1721).
fn render_figure(session: &RenderSession, node: &Figure, ctx: &mut CarveContext) -> String {
    let target = match &*node.target {
        FigureTarget::Image(image) => render_image(image),
        FigureTarget::Table(table) => render_block(session, &BlockNode::Table(table.clone()), ctx),
        FigureTarget::BlockQuote(quote) => {
            render_block(session, &BlockNode::BlockQuote(quote.clone()), ctx)
        }
        FigureTarget::CodeBlock(code) => {
            render_block(session, &BlockNode::CodeBlock(code.clone()), ctx)
        }
        FigureTarget::Paragraph(paragraph) => {
            render_block(session, &BlockNode::Paragraph(paragraph.clone()), ctx)
        }
    };
    match caption_row(session, &node.caption, ctx) {
        Some(row) => format!("{target}\n{row}"),
        // A caption that writes nothing leaves the target on its own. What is
        // lost is the figure ROLE; what writing the bare `^` cost was worse -
        // the caret came back as literal text INSIDE the target's paragraph.
        None => target,
    }
}

fn render_footnote_def_source(
    session: &RenderSession,
    label: &str,
    blocks: &[BlockNode],
    ctx: &mut CarveContext,
) -> String {
    // A bare `[^label]:` is paragraph text, not a definition. PART 11 §7b
    // gives an empty definition an explicit spelling so formatting preserves
    // the definition and references to it keep resolving.
    if blocks.is_empty() {
        return format!("[^{}]: {{empty}}", write_flat_bracket_run(label));
    }
    let raw_body = render_blocks(session, blocks, ctx);
    if raw_body.lines().any(|line| {
        line.trim_start_matches([' ', '\t'])
            .strip_prefix(sentinel(session, S_CODE_LINE))
            .is_some_and(|rest| rest.starts_with('>'))
    }) {
        crate::render_carve_error::record_unspellable(
            "code",
            "a block-marker continuation cannot stay inside a footnote paragraph",
        );
    }
    let single_body;
    let body = trim_non_nbsp(if blocks.len() == 1 {
        single_body = raw_body.replace("\n\n", "\n");
        &single_body
    } else {
        &raw_body
    })
    .to_string();
    // A body holding NO blocks takes the SENTINEL `{empty}` (PART 11 §7b).
    //
    // `[^f]:` with nothing after the colon is not a definition at all -- MARKER
    // REQUIRES CONTENT (PART 2) -- so writing it degrades the definition to a
    // paragraph and every reference to it to literal text. §1a is what licenses
    // departing from the per-construct spelling: the emitted bytes have to
    // re-parse to the tree they came from.
    //
    // The sentinel has to be a VALID ATTRIBUTE BLOCK, which is why it is not
    // `{ }` or `{}`: a block-attribute line requires at least one attribute, so
    // both of those stay literal text inside the note. `{empty}` is a boolean
    // attribute, collected on the definition line and discarded with the rest
    // of the note's pending attributes, so it reaches neither the endnote item
    // nor anything after it.
    if body.is_empty() {
        return format!("[^{}]: {{empty}}", write_flat_bracket_run(label));
    }
    let mut lines = body.split('\n');
    let mut def_lines = vec![format!(
        "[^{}]: {}",
        write_flat_bracket_run(label),
        lines.next().unwrap_or_default()
    )];
    for line in lines {
        // TWO spaces, the body's own column (PART 9 §16). A wider indent is legal
        // continuation but leaves the body's blocks at a relative column above
        // zero, and an indented block opener does not open a block - so a table
        // or a quote written at three came back as a paragraph.
        def_lines.push(format!("  {line}"));
    }
    def_lines.join("\n")
}

/// An inline sequence with every include directive isolated into a single text
/// node, and the indices of those nodes.
struct PreparedInlines {
    nodes: Vec<InlineNode>,
    verbatim: BTreeSet<usize>,
}

/// Split a run so each shape-well-formed include directive is exactly one node,
/// or `None` when the sequence holds none - the common case, kept clone-free.
///
/// Without this the writer escapes a directive like any other punctuation-
/// bearing text. That still renders as the same literal text, which is exactly
/// why the formatter's round-trip invariant never caught it, but it destroys
/// the include: nothing looks wrong until a resolver runs and the chapters have
/// silently vanished.
///
/// Isolating the directive into its own NODE, rather than overriding a rendered
/// string, keeps the writer's own escaping in charge of everything that is not
/// a directive - the prose either side takes the ordinary path, with the
/// ordinary neighbour context.
///
/// The run split itself is `includes::split_run_directives`, so the writer and
/// the expander cannot disagree about what a directive is.
fn isolate_directives(nodes: &[InlineNode]) -> Option<PreparedInlines> {
    if !nodes
        .iter()
        .any(|n| matches!(n, InlineNode::Text(t) if t.value.contains("{{")))
    {
        return None;
    }

    let mut out: Vec<InlineNode> = Vec::with_capacity(nodes.len());
    let mut verbatim = BTreeSet::new();
    let mut found = false;
    let mut i = 0usize;
    while i < nodes.len() {
        if !crate::includes::is_run_node(&nodes[i]) {
            out.push(nodes[i].clone());
            i += 1;
            continue;
        }
        let mut end = i;
        while end < nodes.len() && crate::includes::is_run_node(&nodes[end]) {
            end += 1;
        }
        match crate::includes::split_run_directives(&nodes[i..end]) {
            Some(pieces) => {
                found = true;
                for piece in pieces {
                    match piece {
                        crate::includes::RunPiece::Directive(src) => {
                            // Carried on a text node so the writer's loop sees
                            // one shape; the index marks it for verbatim emit.
                            verbatim.insert(out.len());
                            out.push(InlineNode::Text(crate::ast::Text {
                                value: src,
                                pos: None,
                            }));
                        }
                        crate::includes::RunPiece::Nodes(run) => out.extend(run),
                    }
                }
            }
            None => out.extend_from_slice(&nodes[i..end]),
        }
        i = end;
    }
    if !found {
        return None;
    }
    Some(PreparedInlines {
        nodes: out,
        verbatim,
    })
}

fn render_inlines(session: &RenderSession, nodes: &[InlineNode], ctx: &mut CarveContext) -> String {
    render_inlines_with_caption(session, nodes, ctx, false)
}

fn render_inlines_with_caption(
    session: &RenderSession,
    nodes: &[InlineNode],
    ctx: &mut CarveContext,
    caption_can_open: bool,
) -> String {
    if ctx.inline_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
        return String::new();
    }
    if ctx.inline_depth + ctx.block_depth + 1 >= crate::parse::MAX_NESTING_DEPTH && nodes.len() == 1
    {
        if let InlineNode::Text(text) = &nodes[0] {
            return text.value.clone();
        }
    }
    // Flatten before the writer checks neighboring nodes. A ruby base can
    // start with `[` or a code fence, which changes how preceding text escapes.
    let original = nodes;
    let flattened;
    let nodes = if nodes.iter().any(|node| matches!(node, InlineNode::Ruby(_))) {
        flattened = nodes
            .iter()
            .flat_map(|node| match node {
                InlineNode::Ruby(ruby) if ruby.attrs.is_some() => {
                    vec![InlineNode::Span(Span {
                        attrs: ruby.attrs.clone(),
                        children: ruby.flattened(),
                        injected: false,
                        pos: ruby.pos.clone(),
                    })]
                }
                InlineNode::Ruby(ruby) => ruby.flattened(),
                other => vec![other.clone()],
            })
            .collect::<Vec<_>>();
        flattened.as_slice()
    } else {
        nodes
    };
    // A scope an enclosing run claimed keyed the nodes before this flattening
    // cloned them.
    let aliased = if ctx.brackets.claimed && !std::ptr::eq(nodes, original) {
        alias_flattened(original, nodes, &mut ctx.bracket_aliases)
    } else {
        Vec::new()
    };
    if ctx.inline_depth == 0 && holds_unspellable_empty_code(nodes, false, false, ctx.cell_not_last)
    {
        crate::render_carve_error::record_unspellable(
            "code",
            "a code span has no Carve source spelling where its open run does not end",
        );
    }
    ctx.inline_depth += 1;
    // Isolate any include directive into its own node FIRST, so the loop below
    // needs to know nothing about them: a directive is one node it emits
    // verbatim, and everything around it takes the ordinary escaping path with
    // the ordinary neighbour context.
    let prepared = isolate_directives(nodes);
    // The scope is keyed by node address, so it is taken over the nodes that
    // are actually written: the passes above clone.
    let outer = (!ctx.brackets.claimed).then(|| {
        let written = prepared.as_ref().map_or(nodes, |p| p.nodes.as_slice());
        let scope = bracket_scope(written, ctx.brackets.bracketed);
        std::mem::replace(&mut ctx.brackets, scope)
    });
    let out = match prepared {
        Some(prepared) => render_nodes_with_verbatim(
            session,
            &prepared.nodes,
            ctx,
            caption_can_open,
            &prepared.verbatim,
        ),
        None => render_nodes(session, nodes, ctx, caption_can_open),
    };
    if let Some(outer) = outer {
        ctx.brackets = outer;
    }
    for clone in aliased {
        ctx.bracket_aliases.remove(&clone);
    }
    ctx.inline_depth -= 1;
    if ctx.inline_depth == 0 {
        spell_empty_code_runs(out)
    } else {
        out
    }
}

/// Stands in for an empty code span's backtick run until the inline run it
/// sits in is written. No tree text holds a NUL: every reader replaces one.
const EMPTY_CODE_MARK: char = '\0';

/// Give each empty code span a run no later run in the block matches.
///
/// A backtick run's closer is searched for across the rest of the block (ruling
/// markup-carve/carve#2079), so the usual two backticks would close on a later
/// two-backtick run instead of ending at a braced closer. The marks are spelled
/// from the last to the first, so each sees the lengths chosen after it.
fn spell_empty_code_runs(out: String) -> String {
    if !out.contains(EMPTY_CODE_MARK) {
        return out;
    }
    let mut completed = BTreeSet::new();
    let mut pending_run = 0usize;
    let mut next_free = 2usize;
    let mut replacements = Vec::new();
    for (at, byte) in out.bytes().enumerate().rev() {
        if byte == b'`' {
            pending_run += 1;
        } else if byte == EMPTY_CODE_MARK as u8 {
            while completed.contains(&next_free) {
                next_free += 1;
            }
            let mut len = next_free;
            while len == pending_run || completed.contains(&len) {
                len += 1;
            }
            replacements.push((at, len));
            pending_run += len;
        } else if pending_run > 0 {
            completed.insert(pending_run);
            pending_run = 0;
        }
    }
    let extra: usize = replacements.iter().map(|(_, len)| len - 1).sum();
    let mut written = String::with_capacity(out.len() + extra);
    let mut cursor = 0;
    for (at, len) in replacements.into_iter().rev() {
        written.push_str(&out[cursor..at]);
        written.extend(std::iter::repeat('`').take(len));
        cursor = at + 1;
    }
    written.push_str(&out[cursor..]);
    written
}

fn code_needs_open_run(value: &str) -> bool {
    value.is_empty() || (value.starts_with(['\r', '\n']) && value.ends_with('`'))
}

/// An empty code span is an open backtick run, which ends only at the end of a
/// block or at a braced closer (PART 3, UNCLOSED RUN). `followed` is true when
/// something after this sequence would be read into the run; `labelled` when a
/// link, span or note label must close after it.
fn holds_unspellable_empty_code(
    nodes: &[InlineNode],
    followed: bool,
    labelled: bool,
    cell_not_last: bool,
) -> bool {
    nodes.iter().enumerate().any(|(index, node)| {
        let after = followed || run_reads_on(&nodes[index + 1..]);
        match node {
            InlineNode::Code(code) if code_needs_open_run(&code.value) => {
                !empty_code_position_ends_its_run(after, labelled, cell_not_last)
                    || !render_attrs(&code.attrs).is_empty()
            }
            _ => match empty_code_run_children(node) {
                Some((true, children)) => {
                    holds_unspellable_empty_code(children, false, labelled, cell_not_last)
                }
                Some((false, children)) => {
                    holds_unspellable_empty_code(children, after, true, cell_not_last)
                }
                None => false,
            },
        }
    })
}

pub(crate) fn empty_code_position_ends_its_run(
    followed: bool,
    labelled: bool,
    cell_not_last: bool,
) -> bool {
    !(followed || labelled || cell_not_last)
}

pub(crate) fn run_reads_on(rest: &[InlineNode]) -> bool {
    rest.iter()
        .any(|next| !matches!(next, InlineNode::Text(text) if text.value.is_empty()))
}

/// The inline children an open run can reach, and whether the container
/// closes with a braced closer, which ends the run.
fn empty_code_run_children(node: &InlineNode) -> Option<(bool, &[InlineNode])> {
    match node {
        InlineNode::Emphasis(n) => Some((true, &n.children)),
        InlineNode::CriticInsert(n) => Some((true, &n.children)),
        InlineNode::CriticDelete(n) => Some((true, &n.children)),
        InlineNode::Link(n) => Some((false, &n.children)),
        InlineNode::Span(n) => Some((false, &n.children)),
        InlineNode::Extension(n) => Some((false, &n.children)),
        InlineNode::Footnote(n) => n.inline.as_deref().map(|children| (false, children)),
        _ => None,
    }
}

pub(crate) fn empty_code_run_children_mut(
    node: &mut InlineNode,
) -> Option<(bool, &mut Vec<InlineNode>)> {
    match node {
        InlineNode::Emphasis(n) => Some((true, &mut n.children)),
        InlineNode::CriticInsert(n) => Some((true, &mut n.children)),
        InlineNode::CriticDelete(n) => Some((true, &mut n.children)),
        InlineNode::Link(n) => Some((false, &mut n.children)),
        InlineNode::Span(n) => Some((false, &mut n.children)),
        InlineNode::Extension(n) => Some((false, &mut n.children)),
        InlineNode::Footnote(n) => n.inline.as_mut().map(|children| (false, children)),
        _ => None,
    }
}

fn render_nodes(
    session: &RenderSession,
    nodes: &[InlineNode],
    ctx: &mut CarveContext,
    caption_can_open: bool,
) -> String {
    render_nodes_with_verbatim(session, nodes, ctx, caption_can_open, &BTreeSet::new())
}

fn render_nodes_with_verbatim(
    session: &RenderSession,
    nodes: &[InlineNode],
    ctx: &mut CarveContext,
    mut caption_can_open: bool,
    verbatim: &BTreeSet<usize>,
) -> String {
    let mut out = String::new();
    let mut verbatim_tail = false;
    let mut first_line = true;
    let mut line_node_count = 0usize;
    let mut line_hosts_caption = false;
    for (idx, node) in nodes.iter().enumerate() {
        let prev = idx
            .checked_sub(1)
            .and_then(|i| last_boundary(&nodes[i]))
            .unwrap_or_default();
        let next = nodes
            .get(idx + 1)
            .and_then(first_boundary)
            .unwrap_or_default();
        // The NEXT unit's mode, because this decision is about the escape the
        // node below is going to write and `render_inline` has not claimed its
        // ordinal yet. Taking `ctx.escape_mode` here would hold every node's
        // `^[` to the conservative answer while §2b had narrowed the rest of
        // the document.
        let opens_a_note = next_node_opens_a_note(
            nodes.get(idx + 1),
            ctx.note_content_depth > 0,
            ctx.next_unit_escape_mode(session),
        );
        let opens_verbatim = next_node_opens_a_verbatim_span(nodes.get(idx + 1));
        let braced_before = ctx.braced_spans.len();
        let opens_bracket = nodes
            .get(idx + 1)
            .is_some_and(|next| leading_bracket_run(next).is_some());
        let layout_space = ctx.line_block_depth > 0
            && matches!(node, InlineNode::NonBreakingSpace(n) if render_attrs(&n.attrs).is_empty())
            && (idx == 0
                || matches!(
                    nodes.get(idx.wrapping_sub(1)),
                    Some(InlineNode::HardBreak(_) | InlineNode::NonBreakingSpace(_))
                )
                || matches!(nodes.get(idx + 1), Some(InlineNode::NonBreakingSpace(_))));
        let carried = ctx.paired_closer_carry.replace(false);
        let is_text = matches!(node, InlineNode::Text(_)) && !verbatim.contains(&idx);
        ctx.paired_closer_carry.set(carried && is_text);
        ctx.rendered_verbatim_tail = false;
        let rendered = if layout_space {
            note_inserted(session, S_STAGED_SPACE);
            staged_space(session).to_string()
        } else if verbatim.contains(&idx) {
            // The directive's own source, as the author wrote it: no escaping,
            // and no smart typography either, so a quoted path keeps its
            // straight quotes instead of being curled into a path that names a
            // different file.
            match node {
                InlineNode::Text(t) => t.value.clone(),
                _ => unreachable!("only a text node is marked verbatim"),
            }
        } else {
            render_inline(
                session,
                node,
                ctx,
                prev,
                next,
                caption_can_open,
                opens_a_note,
                opens_verbatim,
                opens_bracket,
                idx + 1 == nodes.len()
                    && (!out.is_empty()
                        || ctx.inline_depth > 1
                        || matches!(node, InlineNode::Code(code) if code_span_fence(&code.value).len() < 3)),
            )
        };
        if !is_text {
            ctx.paired_closer_carry.set(false);
        }
        // A COMMENT'S SEPARATING SPACE IS DECIDED ON THE EMITTED BYTES, not on
        // the previous NODE (carve#1028). `%%` opens a comment only at the
        // start of a line or after whitespace, so the writer owes one space
        // whenever anything has already been written on this line. Asking the
        // previous node for its last character cannot answer that: emphasis, a
        // link, an image and a span all report NO boundary character, which is
        // indistinguishable from "nothing precedes me" - so `{,y,} %% c` came
        // back as `{,y,}%% c`, and re-parsing carve-rs's own output turned the
        // comment into literal text. PART 11 section 1a states the test: read
        // the bytes the writer just produced, not the source it came from.
        if matches!(node, InlineNode::Comment(c) if !c.delimited) && needs_comment_space(&out) {
            out.push(' ');
        }
        // PART 11 §7c, stated as the PROPERTY it rests on: a `hard_break` in a
        // line block is written BARE where, and only where, re-reading that
        // newline yields the same tree; everywhere else it is the PART 3 form.
        // A bare newline re-derives a break at a boundary BETWEEN two body
        // lines and nowhere else, because that is the boundary PART 9 §23
        // hardens. The cases below are consequences of that property, not a
        // list to check - the clause WAS a list, and the case it did not reach
        // is the first one under it.
        let mut rendered = rendered;
        // At a line start a joined `%%%` would open a comment fence, so a
        // comment folded into a term keeps its separator there (carve#2411).
        if let InlineNode::Comment(c) = node {
            if ctx.in_term && !c.delimited && !c.block && out.ends_with('\n') {
                if c.content.starts_with('%') {
                    rendered = format!("%% {}", c.content);
                }
                rendered.insert(0, ' ');
            }
        }
        // A BARE OPENER IS DECIDED ON THE EMITTED BYTES too: emphasis, links and
        // spans report no boundary character, so `{/x/}{/y/}` wrote `/x//y/`
        // and the second opener, after its own marker, read back as text
        // (markup-carve/carve-rs#1649). The second span takes the braced form.
        if let InlineNode::Emphasis(emphasis) = node {
            rendered = brace_a_refused_bare_opener(rendered, emphasis, out.chars().next_back());
        }
        note_braced_span(node, &mut rendered, braced_before, ctx);
        if ctx.line_block_depth > 0 && matches!(node, InlineNode::HardBreak(_)) {
            // THE LAST BODY LINE, WHATEVER IT ENDS IN. The body's end is not a
            // boundary between two lines, so nothing hardens there and the
            // break can only be the AUTHOR'S own. The newline after it belongs
            // to the closing fence, or to the blank line before the next
            // stanza, so the backslash is written WITHOUT one. Measured on a
            // last body line ending in a backslash, with and without a run of
            // spaces before it: both lose the break outright, and neither has a
            // lone trailing space for the case below to catch.
            //
            // WHICH LINE IS LAST IS DECIDED BY THE BREAKS, however the author
            // spelled them: a break ENDS the line it stands at the end of, and
            // what follows it is the next body line - including one that
            // renders nothing. So this is the last NODE of the stanza's own
            // sequence, and `inline_depth == 1` keeps it to that sequence: a
            // break that merely ends the children of an emphasis has content
            // after it on the same line.
            let ends_the_stanza = ctx.inline_depth == 1 && idx + 1 == nodes.len();
            // A LINE WHOSE LAST NODE IS A COMMENT IS EXEMPT, and the exemption
            // is keyed on the NODE rather than on the line's position. The
            // marker runs to the END of its line, so a trailing space there is
            // INSIDE the note and not content PART 2 is about to take - and a
            // backslash written to protect it lands in the note's own content,
            // because the block layer claims the whole line before the inline
            // parser sees it. An EMPTY comment line is where this bites.
            let inside_a_comment = idx
                .checked_sub(1)
                .is_some_and(|prev| matches!(&nodes[prev], InlineNode::Comment(c) if !c.delimited));
            if !inside_a_comment && (ends_the_stanza || verse_break_needs_backslash(session, &out))
            {
                out.push('\\');
            }
            if ends_the_stanza {
                rendered = String::new();
            }
        }
        // A mention or tag needs a non-word character before its sigil and ends
        // at its last name character, so a word character against either side
        // has no spelling (ruling markup-carve/carve-js#1807).
        let is_sigil_node =
            |node: &InlineNode| matches!(node, InlineNode::Mention(_) | InlineNode::Tag(_));
        let word_before = is_sigil_node(node)
            && out
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        let word_after = idx
            .checked_sub(1)
            .is_some_and(|prev| is_sigil_node(&nodes[prev]))
            && rendered
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric());
        if word_before || word_after {
            let sigil_node = if word_before { node } else { &nodes[idx - 1] };
            crate::render_carve_error::record_unspellable(
                if matches!(sigil_node, InlineNode::Tag(_)) {
                    "tag"
                } else {
                    "mention"
                },
                "a mention or tag against a word character has no Carve source spelling",
            );
        }
        // Two touching backtick runs merge into one, so an empty delimited
        // comment keeps them apart (PART 11 §10k N3, ruling
        // markup-carve/carve-js#1818). Padded as the comment writer pads one.
        let run = ['`', EMPTY_CODE_MARK];
        if out.ends_with(run)
            && rendered.starts_with(run)
            && (verbatim_tail || !ends_in_an_escape(&out))
        {
            out.push_str("{%  %}");
        }
        out.push_str(&rendered);
        if !rendered.is_empty() {
            verbatim_tail = rendered.ends_with('`')
                && (ctx.rendered_verbatim_tail
                    || matches!(
                        node,
                        InlineNode::Code(_) | InlineNode::Math(_) | InlineNode::LiteralInline(_)
                    ));
        }
        if matches!(node, InlineNode::SoftBreak(_)) {
            caption_can_open = first_line && line_node_count == 1 && line_hosts_caption;
            first_line = false;
            line_node_count = 0;
            line_hosts_caption = false;
        } else {
            line_node_count += 1;
            line_hosts_caption = line_node_count == 1 && inline_hosts_caption(node);
            caption_can_open = false;
        }
    }
    ctx.rendered_verbatim_tail = verbatim_tail;
    out
}

fn inline_hosts_caption(node: &InlineNode) -> bool {
    match node {
        InlineNode::Image(image) => !image.src.is_empty(),
        InlineNode::Math(math) => math.display,
        _ => false,
    }
}

/// Render one inline node, charging what it writes to a unit of its own (see
/// [`render_block`]).
#[allow(clippy::too_many_arguments)]
fn render_inline(
    session: &RenderSession,
    node: &InlineNode,
    ctx: &mut CarveContext,
    prev_char: char,
    next_char: char,
    caption_can_open: bool,
    next_opens_a_note: bool,
    next_opens_a_verbatim_span: bool,
    next_opens_a_bracket: bool,
    may_run_to_end: bool,
) -> String {
    let previous = ctx.escape_unit;
    ctx.escape_unit = next_escape_unit(session);
    let out = render_inline_body(
        session,
        node,
        ctx,
        prev_char,
        next_char,
        caption_can_open,
        next_opens_a_note,
        next_opens_a_verbatim_span,
        next_opens_a_bracket,
        may_run_to_end,
    );
    ctx.escape_unit = previous;
    out
}

#[allow(clippy::too_many_arguments)]
fn render_inline_body(
    session: &RenderSession,
    node: &InlineNode,
    ctx: &mut CarveContext,
    prev_char: char,
    next_char: char,
    caption_can_open: bool,
    next_opens_a_note: bool,
    next_opens_a_verbatim_span: bool,
    next_opens_a_bracket: bool,
    may_run_to_end: bool,
) -> String {
    match node {
        // The one target that publishes it: the author wrote `%% note`, and
        // the canonical form writes it back verbatim. The parser drops the
        // whitespace before the marker (it is not part of the text); the space
        // that puts it back is decided in `render_inlines`, on the bytes
        // already emitted for this line.
        InlineNode::Comment(c) if c.delimited => {
            format!("{{%{} %}}", pad_delimited(&c.content))
        }
        // Only the delimiters need padding to stay inside the term.
        InlineNode::Comment(c) if c.block => {
            let rendered = render_block_comment(session, &c.content);
            let (body, closer) = rendered.rsplit_once('\n').unwrap();
            format!(" {body}\n {closer}")
        }
        // An EMPTY comment is the marker and nothing else. The space after the
        // marker separates it from content, and with no content it is line
        // TRAILING whitespace, which PART 2 discards on the way back in and §7
        // therefore lets the writer drop. Emitting it left every empty comment
        // line one space long, and in a line block that space is exactly what
        // §7c's LONE SPACE case looks for, so the writer proposed a backslash
        // for a line that had nothing to protect (PART 11 §7c).
        InlineNode::Comment(c) if c.content.is_empty() => "%%".to_string(),
        // THE UNIT IS THE OPENER (PART 11 §2, and §2a naming this exact
        // rewrite): a content that itself opens with `%` JOINS the marker, so
        // `%%%` is written back whole instead of as an opener plus a stray
        // character. The separator is what splits it, and it is free for every
        // other content. Only the INLINE arm does this - the block arm six
        // hundred lines up must keep its separator unconditionally, because a
        // run of three at block level is a comment FENCE (PART 9 §28) that
        // pairs with any later run of the same width and swallows the document
        // between them (markup-carve/carve-js#1674, declined at carve-js#1675).
        InlineNode::Comment(c) if c.content.starts_with('%') => format!("%%{}", c.content),
        InlineNode::Comment(c) => format!("%% {}", c.content),
        InlineNode::Text(text) => escape_text(
            session,
            &resolve_nbsp_placeholder(session, &text.value, ctx.line_block_depth > 0),
            &|ordinal| ctx.bracket_role(text as *const Text as usize, ordinal),
            &ctx.paired_closer_carry,
            ctx.escape_mode_here(session),
            ctx.escape_unit,
            // Does this node's first character sit at the start of a block
            // line? Only there can a `^` be read back as a caption marker.
            (prev_char == '\0' || prev_char == '\n') && ctx.table_cell_depth == 0,
            caption_can_open && ctx.table_cell_depth == 0,
            ctx.table_cell_depth > 0,
            prev_char,
            next_char,
            NeighbourEscape {
                in_note_content: ctx.note_content_depth > 0,
                next_node_opens_a_note: next_opens_a_note,
                next_node_opens_a_verbatim_span: next_opens_a_verbatim_span,
                next_node_opens_a_bracket: next_opens_a_bracket,
            },
        ),
        InlineNode::EscapedText(text) => format!("\\{}", text.value),
        InlineNode::SmartPunctuation(s) => s.value.clone(),
        InlineNode::Emphasis(emphasis) => {
            let kinds = emphasis_delimiters(emphasis.kind);
            let before_attributes = ctx.attribute_markers.clone();
            ctx.open_kinds.extend_from_slice(kinds);
            let content = if writes_own_brackets(emphasis) {
                render_bracketed_content(session, &emphasis.children, ctx)
            } else {
                render_inlines(session, &emphasis.children, ctx)
            };
            ctx.open_kinds.truncate(ctx.open_kinds.len() - kinds.len());
            let attributes_conflict = kinds.iter().any(|marker| {
                ctx.attribute_markers.get(marker).copied().unwrap_or(0)
                    > before_attributes.get(marker).copied().unwrap_or(0)
            });
            if attributes_conflict {
                let body = if emphasis.kind == EmphasisKind::BoldItalic {
                    let conflicts = |marker: char| {
                        ctx.attribute_markers.get(&marker).copied().unwrap_or(0)
                            > before_attributes.get(&marker).copied().unwrap_or(0)
                    };
                    let inner = if conflicts('/') {
                        render_forced_emphasis("/", &content)
                    } else {
                        render_emphasis("/", &content, '*', '*')
                    };
                    if conflicts('*') {
                        render_forced_emphasis("*", &inner)
                    } else {
                        render_emphasis("*", &inner, prev_char, next_char)
                    }
                } else if let Some(marker) = bare_delimiter(emphasis.kind) {
                    render_forced_emphasis(marker, &content)
                } else {
                    String::new()
                };
                if !body.is_empty() {
                    return format!("{}{}", body, render_inline_attrs(&emphasis.attrs, ctx));
                }
            }
            // An empty brace pair is not a construct, and `{--}` is the braced
            // en dash (markup-carve/carve#1608), so an empty mark has no spelling.
            if content.is_empty() && emphasis.kind != EmphasisKind::SmallCaps {
                crate::render_carve_error::record_unspellable(
                    emphasis_node_type(emphasis.kind),
                    "an empty mark has no Carve source spelling",
                );
                return String::new();
            }
            if ctx.line_block_depth == 0
                && emphasis.children.iter().any(
                    |child| matches!(child, InlineNode::Comment(comment) if !comment.delimited),
                )
            {
                if let Some(delim) = bare_delimiter(emphasis.kind) {
                    return format!(
                        "{}{}",
                        render_forced_emphasis(delim, &content),
                        render_inline_attrs(&emphasis.attrs, ctx)
                    );
                }
            }
            // An EMPTY code span has one spelling, a backtick run that its
            // container ends, and only the braced closer ends it inside an
            // emphasis: a bare closer is swallowed by the open run.
            if let Some(InlineNode::Code(code)) = emphasis.children.last() {
                if code_needs_open_run(&code.value) && code.attrs.is_none() {
                    if let Some(delim) = bare_delimiter(emphasis.kind) {
                        return format!(
                            "{}{}",
                            render_forced_emphasis(delim, &content),
                            render_inline_attrs(&emphasis.attrs, ctx)
                        );
                    }
                }
            }
            let (delim, body) = match emphasis.kind {
                EmphasisKind::Italic => ("/", render_emphasis("/", &content, prev_char, next_char)),
                EmphasisKind::Strong => ("*", render_emphasis("*", &content, prev_char, next_char)),
                EmphasisKind::Underline => {
                    ("_", render_emphasis("_", &content, prev_char, next_char))
                }
                EmphasisKind::Strike => ("~", render_emphasis("~", &content, prev_char, next_char)),
                EmphasisKind::Super => ("^", render_forced_emphasis("^", &content)),
                EmphasisKind::Sub => (",", render_forced_emphasis(",", &content)),
                EmphasisKind::Highlight => {
                    ("=", render_emphasis("=", &content, prev_char, next_char))
                }
                // `/*` needs content that hugs it: `/* x*/` reparses as an
                // emphasis holding literal stars, so nest it instead.
                EmphasisKind::BoldItalic if !hugs_its_delimiters(&content) => (
                    "",
                    render_emphasis(
                        "*",
                        &render_emphasis("/", &content, '*', '*'),
                        prev_char,
                        next_char,
                    ),
                ),
                EmphasisKind::BoldItalic => ("", format!("/*{content}*/")),
                // CARVE-P12-050: there is no delimiter, because Carve source
                // has no spelling for the wrapper. It comes off, and any
                // attributes land on an ordinary attributed span instead - so
                // this arm settles the whole node rather than a body to which
                // the shared attribute suffix below would then be appended.
                EmphasisKind::SmallCaps => {
                    let flattened = escape_note_reference_label(&content, ctx);
                    return match render_inline_attrs(&emphasis.attrs, ctx) {
                        attrs if attrs.is_empty() => flattened,
                        attrs => format!("[{flattened}]{attrs}"),
                    };
                }
            };
            let _ = delim;
            format!("{body}{}", render_inline_attrs(&emphasis.attrs, ctx))
        }
        InlineNode::Code(code) => {
            let value = spell_verse_empty_lines(&code.value, ctx.line_block_depth > 0);
            if value.is_empty() {
                // Its run length is chosen once the whole run is written.
                format!("{EMPTY_CODE_MARK}{}", render_inline_attrs(&code.attrs, ctx))
            } else {
                format!(
                    "{}{}",
                    guard_code_lines(
                        session,
                        &render_code_with_unclosed(
                            &value,
                            may_run_to_end
                                && ctx.table_cell_depth == 0
                                && render_inline_attrs(&code.attrs, ctx).is_empty(),
                            ctx.in_term,
                        ),
                        ctx
                    ),
                    render_inline_attrs(&code.attrs, ctx)
                )
            }
        }
        InlineNode::Link(link) => render_link(session, link, ctx),
        InlineNode::Image(image) => render_image_with_attrs(
            image,
            render_inline_attrs(&image.attrs, ctx),
            ctx.table_cell_depth > 0,
        ),
        InlineNode::Span(span) => {
            let attrs = render_inline_attrs(&span.attrs, ctx);
            format!(
                "[{}]{}",
                escape_note_reference_label(
                    &render_bracketed_content(session, &span.children, ctx),
                    ctx
                ),
                if attrs.is_empty() { "{}" } else { &attrs }
            )
        }
        InlineNode::Ruby(r) => {
            let flattened = r.flattened();
            if r.attrs.is_some() {
                render_inlines(
                    session,
                    &[InlineNode::Span(Span {
                        attrs: r.attrs.clone(),
                        children: flattened,
                        injected: false,
                        pos: r.pos.clone(),
                    })],
                    ctx,
                )
            } else {
                render_inlines(session, &flattened, ctx)
            }
        }
        // CARVE-P12-051: the writer preserves the formula and loses `label` and
        // `number`, which Carve source cannot spell. It does not refuse the tree.
        // The loss goes unreported here because the published render-loss
        // vocabulary admits only `raw-format-dropped` and `ruby-flattened`
        // (docs/public/render-loss-report.schema.json); markup-carve/carve#2245
        // asks for the code.
        InlineNode::Math(math) => format!(
            "{}{}{}",
            if math.display { "$$" } else { "$" },
            render_code(
                &spell_verse_empty_lines(&math.content, ctx.line_block_depth > 0),
                ctx
            ),
            render_inline_attrs(&math.attrs, ctx)
        ),
        InlineNode::RawInline(raw) => {
            if raw.content.is_empty() {
                crate::render_carve_error::record_unspellable(
                    "raw_inline",
                    "an empty raw inline has no Carve source spelling",
                );
                return String::new();
            }
            let content = spell_verse_empty_lines(&raw.content, ctx.line_block_depth > 0);
            let verbatim = if raw.format.eq_ignore_ascii_case("html") {
                let fence = code_span_fence(&content);
                if verbatim_needs_padding(&content) {
                    format!("{fence} {content} {fence}")
                } else {
                    format!("{fence}{content}{fence}")
                }
            } else {
                render_code(&content, ctx)
            };
            format!("{verbatim}{{={}}}", escape_format(&raw.format))
        }
        InlineNode::LiteralInline(lit) => {
            // §27: `!` prefix on a verbatim span. A trailing attribute block is
            // the ordinary inline attribute block (same as a code span carries).
            // `render_code` widens the backtick fence when the content holds
            // backticks, so the round-trip re-parses identically.
            let content = spell_verse_empty_lines(&lit.content, ctx.line_block_depth > 0);
            format!(
                "!{}{}",
                render_code(&content, ctx),
                render_inline_attrs(&lit.attrs, ctx)
            )
        }
        InlineNode::Symbol(symbol) => format!(
            ":{}:{}",
            escape_symbol_name(&symbol.name),
            render_inline_attrs(&symbol.attrs, ctx)
        ),
        InlineNode::AutoLink(link) => {
            // Emit the raw autolink content verbatim (keeps a URI scheme like
            // `mailto:`), so it re-parses to the same autolink.
            format!(
                "<{}>{}",
                escape_autolink_href(&link.text),
                render_inline_attrs(&link.attrs, ctx)
            )
        }
        InlineNode::Mention(mention) => {
            refuse_attributes_on_sigil(&mention.attrs, "mention");
            refuse_unspellable_name(&mention.user, "mention");
            format!("@{}", mention.user)
        }
        InlineNode::Tag(tag) => {
            refuse_attributes_on_sigil(&tag.attrs, "tag");
            refuse_unspellable_name(&tag.name, "tag");
            format!("#{}", tag.name)
        }
        InlineNode::Extension(extension) => format!(
            ":{}[{}]{}",
            escape_identifier(&extension.name),
            render_inlines(session, &extension.children, ctx),
            render_inline_attrs(&extension.attrs, ctx)
        ),
        // The neighbour characters are the REAL ones, not `\0`: this arm writes
        // the abbreviation's own text into the same run as everything around
        // it, so a `^` it ends on sits against whatever the next node writes.
        // A `\0` here told the caret decision there was no neighbour, and an
        // ingested abbreviation ending in `^` before a bracket run came back
        // bare - bytes that re-parse as an inline note.
        InlineNode::Abbreviation(abbr) => escape_text(
            session,
            &abbr.abbr,
            &|ordinal| ctx.bracket_role(abbr as *const Abbreviation as usize, ordinal),
            &std::cell::Cell::new(false),
            ctx.escape_mode_here(session),
            ctx.escape_unit,
            false,
            false,
            ctx.table_cell_depth > 0,
            prev_char,
            next_char,
            NeighbourEscape {
                in_note_content: ctx.note_content_depth > 0,
                next_node_opens_a_note: next_opens_a_note,
                next_node_opens_a_verbatim_span: next_opens_a_verbatim_span,
                next_node_opens_a_bracket: next_opens_a_bracket,
            },
        ),
        InlineNode::Footnote(footnote) => {
            let body = if let Some(inline) = &footnote.inline {
                // PART 9 §16 parses a note's content with footnote recognition
                // DISABLED, so a `^[` or a `[^` written inside it is ordinary
                // text on the way back in and the writer owes it no escape.
                ctx.note_content_depth += 1;
                let content = render_bracketed_content(session, inline, ctx);
                ctx.note_content_depth -= 1;
                format!("^[{content}]")
            } else {
                format!(
                    "[^{}]",
                    write_flat_bracket_run(footnote.id.as_deref().unwrap_or_default())
                )
            };
            format!("{body}{}", render_inline_attrs(&footnote.attrs, ctx))
        }
        InlineNode::NonBreakingSpace(n) => {
            note_inserted(session, S_ESCAPED_SPACE);
            let body = escaped_space(session);
            let attrs = render_inline_attrs(&n.attrs, ctx);
            if attrs.is_empty() {
                body
            } else {
                format!("[{body}]{attrs}")
            }
        }
        InlineNode::SoftBreak(_) => {
            if ctx.table_cell_depth > 0 {
                " ".to_string()
            } else {
                "\n".to_string()
            }
        }
        InlineNode::HardBreak(_) => {
            // A cell is one line, so its break flattens to one space (PART 11
            // §1b, ruling markup-carve/carve#2067).
            if ctx.table_cell_depth > 0 {
                " ".to_string()
            } else if ctx.line_block_depth > 0 {
                "\n".to_string()
            } else {
                "\\\n".to_string()
            }
        }
        InlineNode::CriticInsert(insert) => {
            ctx.open_kinds.push('+');
            let content = render_inlines(session, &insert.children, ctx);
            ctx.open_kinds.pop();
            if content.is_empty() {
                crate::render_carve_error::record_unspellable(
                    "insert",
                    "an empty mark has no Carve source spelling",
                );
                return String::new();
            }
            format!("{{+{content}+}}{}", render_inline_attrs(&insert.attrs, ctx))
        }
        InlineNode::CriticDelete(delete) => {
            ctx.open_kinds.push('-');
            let content = render_inlines(session, &delete.children, ctx);
            ctx.open_kinds.pop();
            if content.is_empty() {
                crate::render_carve_error::record_unspellable(
                    "delete",
                    "an empty mark has no Carve source spelling",
                );
                return String::new();
            }
            format!("{{-{content}-}}{}", render_inline_attrs(&delete.attrs, ctx))
        }
        InlineNode::CriticSubstitute(sub) => {
            // The halves are inline content. Where one holds an arrow or a
            // closer of its own, PART 11 §2b's escalation escapes it: the
            // minimal form re-parses as a different tree and the narrowed unit
            // is written conservatively.
            ctx.open_kinds.push('~');
            let old = render_inlines(session, &sub.old, ctx);
            let new = render_inlines(session, &sub.new, ctx);
            ctx.open_kinds.pop();
            format!("{{~{old}~>{new}~}}")
        }
        InlineNode::CriticComment(comment) => {
            format!("{{#{}#}}", escape_critic_text(&comment.text))
        }
        InlineNode::CrossRef(crossref) => {
            format!("</#{}>", spell_crossref_target(&crossref.target))
        }
        InlineNode::CaptionNumber(_) => "#".to_string(),
        InlineNode::CitationGroup(group) => group.raw.clone(),
    }
}

/// Map every bracket-bearing node `flattened` cloned from `original` back to its
/// source, returning the clones' addresses. Mirrors the flattening above: an
/// unattributed ruby becomes base, `(`, annotation, `)` per pair, an
/// attributed one a span (a run of its own), anything else a clone.
fn alias_flattened(
    original: &[InlineNode],
    flattened: &[InlineNode],
    aliases: &mut HashMap<usize, usize>,
) -> Vec<usize> {
    fn same(
        source: &InlineNode,
        clone: &InlineNode,
        aliases: &mut HashMap<usize, usize>,
        out: &mut Vec<usize>,
    ) {
        match (source, clone) {
            (InlineNode::Text(a), InlineNode::Text(b)) => {
                let at = b as *const Text as usize;
                aliases.insert(at, a as *const Text as usize);
                out.push(at);
            }
            (InlineNode::Abbreviation(a), InlineNode::Abbreviation(b)) => {
                let at = b as *const Abbreviation as usize;
                aliases.insert(at, a as *const Abbreviation as usize);
                out.push(at);
            }
            (InlineNode::Emphasis(a), InlineNode::Emphasis(b)) => {
                all(&a.children, &b.children, aliases, out);
            }
            (InlineNode::CriticInsert(a), InlineNode::CriticInsert(b)) => {
                all(&a.children, &b.children, aliases, out);
            }
            (InlineNode::CriticDelete(a), InlineNode::CriticDelete(b)) => {
                all(&a.children, &b.children, aliases, out);
            }
            (InlineNode::CriticSubstitute(a), InlineNode::CriticSubstitute(b)) => {
                all(&a.old, &b.old, aliases, out);
                all(&a.new, &b.new, aliases, out);
            }
            (InlineNode::Ruby(a), InlineNode::Ruby(b)) => {
                for (x, y) in a.pairs.iter().zip(&b.pairs) {
                    all(&x.base, &y.base, aliases, out);
                    all(&x.annotation, &y.annotation, aliases, out);
                }
            }
            _ => {}
        }
    }
    fn all(
        a: &[InlineNode],
        b: &[InlineNode],
        aliases: &mut HashMap<usize, usize>,
        out: &mut Vec<usize>,
    ) {
        for (x, y) in a.iter().zip(b) {
            same(x, y, aliases, out);
        }
    }
    let mut out = Vec::new();
    let mut at = 0;
    for node in original {
        match node {
            InlineNode::Ruby(ruby) if ruby.attrs.is_some() => at += 1,
            InlineNode::Ruby(ruby) => {
                for pair in &ruby.pairs {
                    let base = &flattened[at..at + pair.base.len()];
                    all(&pair.base, base, aliases, &mut out);
                    at += pair.base.len() + 1;
                    let annotation = &flattened[at..at + pair.annotation.len()];
                    all(&pair.annotation, annotation, aliases, &mut out);
                    at += pair.annotation.len() + 1;
                }
            }
            other => {
                same(other, &flattened[at], aliases, &mut out);
                at += 1;
            }
        }
    }
    out
}

/// Small caps carrying attributes are written as an attributed span.
fn writes_own_brackets(emphasis: &Emphasis) -> bool {
    emphasis.kind == EmphasisKind::SmallCaps && !render_attrs(&emphasis.attrs).is_empty()
}

/// Content written between a construct's own `[` and `]`, with its lone
/// brackets escaped in every form (PART 11 §5).
fn render_bracketed_content(
    session: &RenderSession,
    children: &[InlineNode],
    ctx: &mut CarveContext,
) -> String {
    let unclaimed = BracketScope {
        bracketed: true,
        ..BracketScope::default()
    };
    let outer = std::mem::replace(&mut ctx.brackets, unclaimed);
    let outer_spans = std::mem::take(&mut ctx.braced_spans);
    ctx.attribute_bracket_depth += 1;
    let out = render_inlines(session, children, ctx);
    ctx.attribute_bracket_depth -= 1;
    ctx.brackets = outer;
    ctx.braced_spans = outer_spans;
    out
}

/// The first reference-shaped run in literal text, whose backslashes and markup
/// delimiters will be escaped by the writer rather than read as source syntax.
fn literal_reference_opener(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut open = Vec::new();
    let mut reference = None;
    for (offset, &ch) in bytes.iter().enumerate() {
        if ch == b']' && reference.is_some() {
            return reference;
        }
        match ch {
            b'\n' | b'\r' => reference = None,
            b'[' => open.push(offset),
            b']' => {
                if let Some(opener) = open.pop() {
                    if bytes.get(offset + 1) == Some(&b'[') {
                        reference = Some(opener);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Pair the text brackets of one inline run in order. A nested construct that
/// writes its own brackets takes no part: it balances its own content, and an
/// inline extension's content is not bracketed because its reader stops at the
/// first `]`. Only bracketed content has lone brackets to escape, and a run
/// holding an empty code span is left to the search (PART 11 §5).
///
/// A pair whose brackets sit under different formatting nodes is recorded as
/// CROSSING, and the escape falls on its opener: PART 8 resolves the bracket run
/// first, so a run spanning a formatting boundary isolates the delimiter inside
/// it. `host` is the node the text under it belongs to - the run's own container
/// at the top, and the nested construct below it.
fn bracket_scope(nodes: &[InlineNode], bracketed: bool) -> BracketScope {
    /// An open `[`: where the scope keys it, and the host it was read under.
    type Open = ((usize, usize), usize);
    /// One bracket in document order: its key, its host, and whether it opens.
    type Bracket = ((usize, usize), usize, bool);
    /// One reading of a run: its pairs in closing order, then the openers left
    /// unclosed at the end.
    type Reading = (Vec<(Open, Open)>, Vec<(usize, usize)>);

    fn walk(
        nodes: &[InlineNode],
        host: usize,
        open: &mut Vec<Bracket>,
        scope: &mut BracketScope,
    ) -> bool {
        for node in nodes {
            let nested = node as *const InlineNode as usize;
            match node {
                InlineNode::Code(code) if code.value.is_empty() => return false,
                InlineNode::Text(text) => pair(
                    text as *const Text as usize,
                    &text.value,
                    host,
                    open,
                    scope,
                    false,
                ),
                InlineNode::Abbreviation(abbr) => {
                    pair(
                        abbr as *const Abbreviation as usize,
                        &abbr.abbr,
                        host,
                        open,
                        scope,
                        false,
                    );
                }
                InlineNode::Link(link) if link.ref_label.is_some() && link.raw_ref.is_some() => {
                    pair(
                        link as *const Link as usize,
                        link.raw_ref.as_deref().unwrap(),
                        host,
                        open,
                        scope,
                        true,
                    );
                }
                InlineNode::Image(image)
                    if image.ref_label.is_some() && image.raw_ref.is_some() =>
                {
                    pair(
                        image as *const Image as usize,
                        image.raw_ref.as_deref().unwrap(),
                        host,
                        open,
                        scope,
                        true,
                    );
                }
                // Written as `[content]{attrs}`: a bracketed run of its own.
                InlineNode::Emphasis(emphasis) if writes_own_brackets(emphasis) => {}
                InlineNode::Emphasis(emphasis)
                    if !walk(&emphasis.children, nested, open, scope) =>
                {
                    return false;
                }
                InlineNode::CriticInsert(insert)
                    if !walk(&insert.children, nested, open, scope) =>
                {
                    return false;
                }
                InlineNode::CriticDelete(delete)
                    if !walk(&delete.children, nested, open, scope) =>
                {
                    return false;
                }
                InlineNode::CriticSubstitute(sub)
                    if !walk(&sub.old, nested, open, scope)
                        || !walk(&sub.new, nested, open, scope) =>
                {
                    return false;
                }
                // Written flattened, base then annotation; with attributes it is a
                // span, which is a bracketed run of its own.
                InlineNode::Ruby(ruby) if ruby.attrs.is_none() => {
                    for pair in &ruby.pairs {
                        if !walk(&pair.base, nested, open, scope)
                            || !walk(&pair.annotation, nested, open, scope)
                        {
                            return false;
                        }
                    }
                }
                _ => {}
            }
        }
        true
    }
    fn pair(
        at: usize,
        value: &str,
        host: usize,
        seq: &mut Vec<Bracket>,
        scope: &mut BracketScope,
        verbatim: bool,
    ) {
        scope.keyed.insert(at);
        let reference = (!verbatim)
            .then(|| literal_reference_opener(value))
            .flatten()
            .map(|offset| {
                value[..offset]
                    .chars()
                    .filter(|c| matches!(c, '[' | ']'))
                    .count()
            });
        let structural = if verbatim {
            match crate::parse::structural_bracket_offsets(value) {
                Some(sites) => Some(sites),
                None => {
                    scope.incomplete_raw = true;
                    return;
                }
            }
        } else {
            None
        };
        let brackets = value.char_indices().filter(|(_, c)| matches!(c, '[' | ']'));
        for (ordinal, (offset, ch)) in brackets.enumerate() {
            if structural
                .as_ref()
                .is_some_and(|sites| sites.binary_search(&offset).is_err())
            {
                continue;
            }
            let key = (at, ordinal);
            if verbatim {
                scope.fixed.insert(key);
            }
            if reference == Some(ordinal) {
                let same_host = scope.literal_hosts.remove(&host);
                let crossing = !scope.literal_hosts.is_empty();
                for (_, openers) in scope.literal_hosts.drain() {
                    scope
                        .crossing_openers
                        .extend(openers.into_iter().filter(|key| !scope.fixed.contains(key)));
                }
                if let Some(openers) = same_host {
                    scope.literal_hosts.insert(host, openers);
                }
                if crossing {
                    scope.crossing_openers.insert(key);
                }
            }
            seq.push((key, host, ch == '['));
            if ch == '[' && !scope.crossing_openers.contains(&key) {
                scope.literal_openers.push((key, host));
                scope.literal_hosts.entry(host).or_default().insert(key);
            } else if ch == ']' {
                while let Some((opener, opener_host)) = scope.literal_openers.pop() {
                    if scope.crossing_openers.contains(&opener) {
                        continue;
                    }
                    if let Some(openers) = scope.literal_hosts.get_mut(&opener_host) {
                        openers.remove(&opener);
                        if openers.is_empty() {
                            scope.literal_hosts.remove(&opener_host);
                        }
                    }
                    break;
                }
            }
        }
    }

    /// Pair the sequence, leaving out the openers `escaped` names. Reports each
    /// pair in closing order and the openers still unclosed at the end.
    ///
    /// `escapes_crossing_closer` is the second reading. There a pair that still
    /// crosses has its `]` escaped, and an escaped `]` answers nothing, so the
    /// opener it fell through to stays open for the next `]` instead of being
    /// spent on it (carve-rs#2214).
    fn resolve(
        seq: &[Bracket],
        escaped: &HashSet<(usize, usize)>,
        escapes_crossing_closer: bool,
        fixed: &HashSet<(usize, usize)>,
    ) -> Reading {
        let mut open: Vec<Open> = Vec::new();
        let mut pairs = Vec::new();
        for &(key, host, opens) in seq {
            if opens {
                if !escaped.contains(&key) {
                    open.push((key, host));
                }
            } else if let Some(opener) = open.pop() {
                pairs.push((opener, (key, host)));
                if escapes_crossing_closer
                    && opener.1 != host
                    && !fixed.contains(&key)
                    && !fixed.contains(&opener.0)
                {
                    open.push(opener);
                }
            }
        }
        (pairs, open.into_iter().map(|(key, _)| key).collect())
    }

    let mut scope = BracketScope {
        claimed: true,
        ..BracketScope::default()
    };
    let mut seq = Vec::new();
    if !walk(nodes, 0, &mut seq, &mut scope) || scope.incomplete_raw {
        return BracketScope {
            claimed: true,
            ..BracketScope::default()
        };
    }
    // PASS ONE names the crossing openers. PASS TWO asks again with those
    // openers GONE, because that is the run the reader will see: a `]` whose own
    // `[` is escaped falls through to the outer opener, and where that outer pair
    // crosses the same boundary the CLOSER takes the escape. One pass cannot
    // decide it - which brackets survive is not known until the run is complete -
    // and leaving it undecided is what made the formatter non-idempotent: `fmt`
    // ran pass two itself and escaped one more bracket every time (carve-rs#2209).
    let (first, _) = resolve(&seq, &scope.crossing_openers, false, &scope.fixed);
    for (opener, closer) in first {
        if opener.1 != closer.1 && !scope.fixed.contains(&opener.0) {
            scope.crossing_openers.insert(opener.0);
        }
    }
    let (second, unclosed) = resolve(&seq, &scope.crossing_openers, true, &scope.fixed);
    for (opener, closer) in second {
        if opener.1 == closer.1
            || scope.fixed.contains(&opener.0)
            || scope.fixed.contains(&closer.0)
        {
            scope.paired_closers.insert(closer.0);
        } else {
            scope.crossing_closers.insert(closer.0);
        }
        scope.closer_openers.insert(closer.0, opener.0);
    }
    for &(key, _, opens) in &seq {
        if !opens && !scope.closer_openers.contains_key(&key) {
            scope.lone.insert(key);
        }
    }
    if bracketed {
        scope.lone.extend(unclosed);
    } else {
        scope.lone.clear();
        scope.closer_openers.clear();
    }
    scope
}

fn render_link(session: &RenderSession, node: &Link, ctx: &mut CarveContext) -> String {
    if node.ref_label.is_some() && node.raw_ref.is_some() {
        return node.raw_ref.clone().unwrap_or_default();
    }
    if node.from_crossref {
        if let Some(target) = node.href.strip_prefix('#') {
            return format!("</#{}>", spell_crossref_target(target));
        }
    }
    let text =
        escape_note_reference_label(&render_bracketed_content(session, &node.children, ctx), ctx);
    let title = node
        .title
        .as_ref()
        .map(|title| {
            format!(
                " \"{}\"",
                if ctx.table_cell_depth > 0 {
                    escape_quoted(title).replace('`', "\\`").replace('|', "\\|")
                } else {
                    escape_quoted(title)
                }
            )
        })
        .unwrap_or_default();
    format!(
        "[{text}]({}{title}){}",
        escape_destination(&node.href),
        render_inline_attrs(&node.attrs, ctx)
    )
}

/// A LABEL SLOT OPENS WITH `[`, AND `[^x]` IS A NOTE REFERENCE (PART 11 §2).
///
/// A span and an inline link both write their content between brackets, so
/// content that BEGINS with a caret re-parses as a reference to a note instead
/// of as the thing that was written. `<abbr title="y">^1</abbr>` came back as
/// `[^1]{abbr=y}`: the span is gone, the attribute block is read as literal
/// text, and the paragraph renders `[^1]`. An anchor loses its destination the
/// same way - `[^1](u)` renders the characters `[^1](u)`.
///
/// THE TEST READS THE SOURCE THE WRITER WILL EMIT, not the tree it emits from.
/// That is §2's own wording and it is why this sits here rather than in the
/// importer: the caret only collides once the span has been spelled in its
/// compact bracket form, and the tree says nothing about which form that is.
///
/// ONLY THE LABELED HALF COLLIDES, and this is the half that is wrong in
/// silence: the reference rule needs at least one character after the caret and
/// cannot cross `]` or a line break, so `[^]` is NOT a reference and must not be
/// escaped. A caret anywhere but the first position is ordinary punctuation.
/// `note-reference-in-a-span` carries both halves precisely so a fix cannot
/// over-escape its way to green.
///
/// An IMAGE label is not a slot this reaches: `![^1](u)` is an image whose
/// alternative text is `^1`, because the `!` takes the `[` first.
fn escape_note_reference_label(label: &str, ctx: &CarveContext) -> String {
    // A NOTE'S CONTENT RECOGNIZES NO NOTE (PART 9 §16), so inside one the
    // bracket run is already read as what it is and the escape would be idle -
    // which §2 forbids exactly as squarely as a missing one.
    if ctx.note_content_depth > 0 {
        return label.to_string();
    }
    let mut chars = label.chars();
    let opens_reference = chars.next() == Some('^')
        && matches!(chars.next(), Some(next) if next != ']' && next != '\r' && next != '\n');
    if opens_reference {
        format!("\\{label}")
    } else {
        label.to_string()
    }
}

fn render_image(node: &Image) -> String {
    render_image_with_attrs(node, render_attrs(&node.attrs), false)
}

fn render_image_with_attrs(node: &Image, attrs: String, in_table: bool) -> String {
    // An unresolved reference image round-trips via its verbatim source, exactly
    // like an unresolved reference link (render_link); `![alt]()` would change
    // the rendered text and break the to_html(fmt(x)) == to_html(x) invariant.
    //
    // A RESOLVED reference image keeps its authored form too, for the same reason
    // as a link: §10 gives the definition a node and render_block writes the line,
    // so there is no longer anything to gain by inlining - and inlining lost
    // `ref`/`raw_ref` and duplicated the destination (carve-rs#631).
    if node.ref_label.is_some() && node.raw_ref.is_some() {
        return node.raw_ref.clone().unwrap_or_default();
    }
    let title = node
        .title
        .as_ref()
        .map(|title| {
            format!(
                " \"{}\"",
                if in_table {
                    escape_quoted(title).replace('`', "\\`").replace('|', "\\|")
                } else {
                    escape_quoted(title)
                }
            )
        })
        .unwrap_or_default();
    format!(
        "![{}]({}{title}){}",
        escape_image_alt(&node.alt, in_table),
        escape_destination(&node.src),
        attrs
    )
}

fn render_raw_frontmatter(raw: &crate::ast::Frontmatter) -> String {
    format!("---{}\n{}\n---", raw.format, raw.content)
}

fn render_frontmatter(
    session: &RenderSession,
    frontmatter: &std::collections::BTreeMap<String, String>,
) -> String {
    let mut out = String::from("---");
    for (key, value) in frontmatter {
        out.push('\n');
        out.push_str(key);
        out.push_str(": ");
        out.push_str(&protect_verbatim(session, value));
    }
    out.push_str("\n---");
    out
}

/// The text of a `{% ... %}` comment, with the opener's pad where one separates
/// it from the text. A pad before a line break would be line-trailing whitespace
/// the writer invented (markup-carve/carve#2425).
fn pad_delimited(content: &str) -> String {
    if content.starts_with('\n') {
        content.to_string()
    } else {
        format!(" {content}")
    }
}

fn render_block_comment(session: &RenderSession, content: &str) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in content.chars() {
        if ch == '%' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    let fence = "%".repeat(3.max(longest + 1));
    format!("{fence}\n{}\n{fence}", protect_verbatim(session, content))
}

// Superscript and subscript have no bare delimiter form -- always emit the
// braced `{^x^}` / `{,x,}` form.
/// Whether content can sit against a bare delimiter: non-empty, and not
/// starting or ending in one of Carve's four whitespace characters (PART 7).
fn hugs_its_delimiters(content: &str) -> bool {
    let ws = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    match (content.chars().next(), content.chars().next_back()) {
        (Some(first), Some(last)) => !ws(first) && !ws(last),
        _ => false,
    }
}

fn emphasis_node_type(kind: EmphasisKind) -> &'static str {
    match kind {
        EmphasisKind::Italic => "emphasis",
        EmphasisKind::Strong | EmphasisKind::BoldItalic => "strong",
        EmphasisKind::Underline => "underline",
        EmphasisKind::Strike => "strike",
        EmphasisKind::Highlight => "highlight",
        EmphasisKind::Super => "superscript",
        EmphasisKind::Sub => "subscript",
        EmphasisKind::SmallCaps => "small_caps",
    }
}

fn render_forced_emphasis(delim: &str, content: &str) -> String {
    format!("{{{delim}{content}{delim}}}")
}

fn render_emphasis(delim: &str, content: &str, prev_char: char, next_char: char) -> String {
    // A leading `*` would form the combined bold-italic opener.
    let reads_as_bold_italic = delim == "/" && content.starts_with('*');
    let needs_forced = is_word_boundary(prev_char)
        || is_word_boundary(next_char)
        || reads_as_bold_italic
        || content.starts_with(delim)
        || content.ends_with(delim)
        // A bare opener may not be followed, nor a closer preceded, by `ws`
        // (CARVE-P3-013; space, tab, CR, LF). A trailing hard break also puts
        // the closer at the start of the next line (PART 11 §1a).
        || !hugs_its_delimiters(content);
    if needs_forced {
        format!("{{{delim}{content}{delim}}}")
    } else {
        format!("{delim}{content}{delim}")
    }
}

fn bare_delimiter(kind: EmphasisKind) -> Option<&'static str> {
    match kind {
        EmphasisKind::Italic => Some("/"),
        EmphasisKind::Strong => Some("*"),
        EmphasisKind::Underline => Some("_"),
        EmphasisKind::Strike => Some("~"),
        EmphasisKind::Highlight => Some("="),
        _ => None,
    }
}

fn is_word_boundary(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// The braced form of a bare emphasis whose opener the previous emitted
/// character would refuse (CARVE-P3-013): after a word character, after its
/// own marker, or for `/` and `_`, after a `/`.
fn brace_a_refused_bare_opener(
    rendered: String,
    emphasis: &crate::ast::Emphasis,
    before: Option<char>,
) -> String {
    let marker = match emphasis.kind {
        EmphasisKind::Italic => '/',
        EmphasisKind::Strong => '*',
        EmphasisKind::Underline => '_',
        EmphasisKind::Strike => '~',
        EmphasisKind::Highlight => '=',
        _ => return rendered,
    };
    let Some(before) = before else {
        return rendered;
    };
    let refused = before == marker
        || is_word_boundary(before)
        || (matches!(marker, '/' | '_') && before == '/');
    if !refused || !rendered.starts_with(marker) {
        return rendered;
    }
    let attrs = render_attrs(&emphasis.attrs);
    let body = &rendered[..rendered.len() - attrs.len()];
    format!("{{{body}}}{attrs}")
}

/// Spell an EMPTY LINE inside a verbatim value the one way verse can spell one.
///
/// A run that stays open in a line block swallows the line boundaries it crosses
/// as newlines, and a comment-only line is emptied above it (PART 9 §23), so its
/// value can hold an empty line. The writer cannot emit that line as a blank
/// one: a blank line ENDS THE STANZA, and the run comes back split. A `\` is no
/// help either - inside the run it is content, not a break.
///
/// A comment line is what is left, and it is exact rather than a workaround: it
/// is removed at the BLOCK layer, before the run exists, so it leaves the
/// emptied line the value already holds.
///
/// The FIRST and LAST segments are skipped, and neither is a line of its own:
/// the first is the tail of the line the run OPENED on, and the last is the line
/// the CLOSING fence goes out on. Spelling the last one would put the fence
/// inside the comment, where the block layer takes it with the rest of the line
/// and the run never closes at all.
fn spell_verse_empty_lines(content: &str, in_line_block: bool) -> String {
    let normalized;
    let content = if content.contains('\r') {
        normalized = content.replace("\r\n", "\n").replace('\r', "\n");
        normalized.as_str()
    } else {
        content
    };
    if !in_line_block || !content.contains('\n') {
        return content.to_string();
    }
    let segments: Vec<&str> = content.split('\n').collect();
    let last = segments.len() - 1;
    let mut out = String::with_capacity(content.len());
    for (i, segment) in segments.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if i > 0 && i < last && segment.is_empty() {
            out.push_str("%%");
            continue;
        }
        out.push_str(segment);
    }
    out
}

fn code_span_fence(content: &str) -> String {
    if content.starts_with(['\r', '\n']) {
        let mut widths = [false; 3];
        let mut run = 0;
        for ch in content.chars().chain(std::iter::once(' ')) {
            if ch == '`' {
                run += 1;
            } else {
                if run < widths.len() {
                    widths[run] = true;
                }
                run = 0;
            }
        }
        for (width, used) in widths.iter().enumerate().skip(1) {
            if !used {
                return "`".repeat(width);
            }
        }
    }
    safe_fence(content, 1)
}

fn guard_code_lines(session: &RenderSession, written: &str, ctx: &CarveContext) -> String {
    if ctx.line_block_depth > 0 {
        return written.to_owned();
    }
    static MARKER: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let marker =
        MARKER.get_or_init(|| regex::Regex::new(r"^(?:>(?: |$)|\[[^\]\n]+\]:[ \t])").unwrap());
    written
        .split('\n')
        .enumerate()
        .map(|(index, line)| {
            if index > 0 && marker.is_match(line) {
                if ctx.in_term {
                    crate::render_carve_error::record_unspellable(
                        "code",
                        "a block marker on a continuation line ends the definition term",
                    );
                    return line.to_owned();
                }
                note_inserted(session, S_CODE_LINE);
                format!("{}{line}", sentinel(session, S_CODE_LINE))
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn verbatim_needs_padding(content: &str) -> bool {
    content.starts_with('`')
        || content.ends_with('`')
        || (content.starts_with(' ')
            && content.ends_with(' ')
            && !content.chars().all(|c| c == ' '))
}

fn render_code(content: &str, ctx: &CarveContext) -> String {
    render_code_with_unclosed(content, false, ctx.in_term)
}

fn render_code_with_unclosed(content: &str, allow_unclosed: bool, in_term: bool) -> String {
    let normalized;
    let content = if content.contains('\r') {
        normalized = content.replace("\r\n", "\n").replace('\r', "\n");
        normalized.as_str()
    } else {
        content
    };
    let fence = code_span_fence(content);
    // Pad exactly where the parser strips, so the strip is reversible and fmt
    // stays idempotent; the padding sits inside the fence, so a trailing
    // attribute block still attaches to the closing run. The parser strips one
    // leading and one trailing space when the content BOTH begins and ends with
    // a space but is NOT entirely spaces (see strip_verbatim_padding in
    // parse.rs), and needs a space around backtick-adjacent content. All-space
    // content must therefore NOT be padded: it is emitted verbatim and read back
    // unchanged. Padding it instead grew the span by two spaces on every fmt
    // pass. One-sided space is left as-is (the parser only strips when both
    // sides are spaces).
    let needs_pad = verbatim_needs_padding(content);
    let reason = if content
        .as_bytes()
        .windows(2)
        .any(|pair| matches!(pair[0], b' ' | b'\t') && pair[1] == b'\n')
    {
        Some("a line of the value ends in whitespace, which the block layer strips")
    } else if !in_term
        && content
            .as_bytes()
            .windows(2)
            .any(|pair| pair[0] == b'\n' && matches!(pair[1], b' ' | b'\t'))
    {
        Some("a line of the value starts with whitespace, which the block layer strips")
    } else if needs_pad && content.ends_with('\n') {
        Some("a padded value ending in a line terminator loses the pad")
    } else {
        None
    };
    if let Some(reason) = reason {
        crate::render_carve_error::record_unspellable("code", reason);
    }
    // A leading pad before a newline is stripped by block normalization.
    // At the end of a run, an unclosed span preserves the original value.
    if needs_pad
        && content.starts_with(['\r', '\n'])
        && allow_unclosed
        && !content.ends_with(['\r', '\n', ' ', '\t'])
        && !content.as_bytes().windows(2).any(|pair| {
            (matches!(pair[0], b' ' | b'\t') && matches!(pair[1], b'\r' | b'\n'))
                || (pair[0] == b'\n' && matches!(pair[1], b' ' | b'\t' | b'\r' | b'\n'))
        })
    {
        return format!("{fence}{content}");
    }
    if needs_pad && content.starts_with(['\r', '\n']) {
        crate::render_carve_error::record_unspellable(
            "code",
            "a leading newline loses its padding where the code span cannot run to the end",
        );
    }
    if needs_pad {
        format!("{fence} {content} {fence}")
    } else {
        format!("{fence}{content}{fence}")
    }
}

fn code_fence_info(lang: Option<&str>, title: Option<&str>, label: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(lang) = lang.filter(|s| !s.is_empty()) {
        parts.push(escape_fence_token(lang));
    }
    if let Some(title) = title {
        parts.push(format!("\"{}\"", escape_quoted_title(title, "code_block")));
    }
    if let Some(label) = label {
        parts.push(format!("[{}]", write_flat_bracket_run(label)));
    }
    // NO SPACE between the fence run and the info string. `fenced_code_block`
    // names the slot OPTIONAL and the no-space form CANONICAL: "The no-space
    // form (```php) is canonical and is what the X->Carve converters emit." The
    // reader stays lenient and accepts both, which is why a single-pass output
    // check never caught this: ``` js re-parses to the same tree.
    //
    // The separators BETWEEN the parts are a different slot and stay: inside
    // `code_fence_info` they are `space+`, mandatory, so ```js"t" is not a
    // fence opener at all and joining without one would lose the header.
    parts.join(" ")
}

fn safe_fence(content: &str, min: usize) -> String {
    let mut longest = 0usize;
    let mut current = 0usize;
    for ch in content.chars() {
        if ch == '`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    "`".repeat(min.max(longest + 1))
}

/// An attribute block after a mention or a tag stays literal, so a tree that
/// carries one has no spelling: `[@a]{.c}` is a span holding a mention and
/// `@a{.c}` leaves the braces as text (ruling markup-carve/carve-php#2083).
fn refuse_attributes_on_sigil(attrs: &Option<Attrs>, node_type: &'static str) {
    let carries = attrs.as_ref().is_some_and(|attrs| {
        attrs.id.is_some() || !attrs.classes.is_empty() || !attrs.key_values.is_empty()
    });
    if carries {
        crate::render_carve_error::record_unspellable(
            node_type,
            "an attribute block after a mention or tag reads back as text",
        );
    }
}

fn render_attrs(attrs: &Option<Attrs>) -> String {
    render_attrs_with_markers(attrs, &[], false)
}

fn render_inline_attrs(attrs: &Option<Attrs>, ctx: &mut CarveContext) -> String {
    let rendered =
        render_attrs_with_markers(attrs, &ctx.open_kinds, ctx.attribute_bracket_depth > 0);
    for &marker in &ctx.open_kinds {
        if (marker == '=' && rendered.contains("=\""))
            || attrs.as_ref().is_some_and(|attrs| {
                attrs.id.as_ref().is_some_and(|id| id.contains(marker))
                    || attrs.classes.iter().any(|class| class.contains(marker))
                    || attrs
                        .key_values
                        .iter()
                        .any(|(key, value)| key.contains(marker) || value.contains(marker))
            })
        {
            *ctx.attribute_markers.entry(marker).or_default() += 1;
        }
    }
    rendered
}

fn render_attrs_with_markers(attrs: &Option<Attrs>, markers: &[char], bracketed: bool) -> String {
    let conflicts = |value: &str| markers.iter().any(|&marker| value.contains(marker));
    let quote = |value: &str| {
        if (!markers.is_empty() || bracketed)
            && (conflicts(value) || value.contains(['{', '}', '[', ']', '`']))
        {
            format!(
                "\"{}\"",
                escape_quoted_run(value, &['"', '|', '{', '}', '[', ']', '`'])
            )
        } else {
            quote_attr_value(value)
        }
    };
    let Some(attrs) = attrs else {
        return String::new();
    };
    let mut parts = Vec::new();
    let id_as_key = attrs
        .id
        .as_ref()
        .is_some_and(|id| !is_explicit_id_or_class_identifier(id) || conflicts(id));
    let mut seen_keys: Vec<&str> = Vec::new();
    let mut seen_key_set = (attrs.order.len() > 8).then(HashSet::new);
    let emit_id = |parts: &mut Vec<String>| {
        if let Some(id) = &attrs.id {
            if id_as_key {
                parts.push(format!("id={}", quote(id)));
            } else {
                parts.push(format!("#{}", escape_attr_name_value(id)));
            }
        }
    };
    let emit_classes = |parts: &mut Vec<String>| {
        for cls in &attrs.classes {
            // THE SHORTHAND ONLY REACHES WHAT A FENCE WORD REACHES. `.` takes the
            // `explicit_identifier` (carve#2435), so `-col`, `w-1/2` and the empty
            // class have no `.` spelling and take the key-value form the parser
            // now folds back into this same slot (CARVE-P4-007). Written as `.`
            // they were source this engine's own parser reads as a paragraph.
            if crate::parse::is_css_identifier(cls) && !conflicts(cls) {
                parts.push(format!(".{}", escape_attr_name_value(cls)));
            } else {
                parts.push(format!(
                    "class={}",
                    if markers.is_empty() && !bracketed {
                        quoted_attr_value(cls)
                    } else {
                        format!(
                            "\"{}\"",
                            escape_quoted_run(cls, &['"', '|', '{', '}', '[', ']', '`'])
                        )
                    }
                ));
            }
        }
    };
    let emit_key = |parts: &mut Vec<String>, key: &str| {
        if let Some(value) = attrs.key_values.get(key) {
            // EXACT key match, not case-insensitive: `LANG` and `lang` are
            // different attribute names, so folding here rewrote
            // `[x]{LANG=fr}` into `[x]{:fr}` and changed the name, which
            // breaks PART 11 §1 (carve#1137).
            if key == "lang" && is_language_tag(value) {
                parts.push(format!(":{value}"));
            } else if value.is_empty() && is_boolean_attr_name(key) && !conflicts(key) {
                // PART 11 §6c: a value-less attribute comes back as the bare
                // name, which is the production the language has for it. A key
                // needing escaping has no bare spelling to fall back to, and
                // neither does a `_`-first one (carve#1450) -- see
                // `is_boolean_attr_name`.
                parts.push(escape_attr_key(key));
            } else {
                parts.push(format!("{}={}", escape_attr_key(key), quote(value)));
            }
        }
    };
    if attrs.order.is_empty() {
        emit_id(&mut parts);
        emit_classes(&mut parts);
        for key in attrs.key_values.keys() {
            emit_key(&mut parts, key);
        }
    } else {
        for slot in &attrs.order {
            match slot {
                AttrSlot::Id => emit_id(&mut parts),
                AttrSlot::Class => emit_classes(&mut parts),
                AttrSlot::Key(key) => {
                    let unique = if let Some(seen) = &mut seen_key_set {
                        seen.insert(key.as_str())
                    } else if seen_keys.contains(&key.as_str()) {
                        false
                    } else {
                        seen_keys.push(key.as_str());
                        true
                    };
                    if unique {
                        emit_key(&mut parts, key);
                    }
                }
            }
        }
        for key in attrs.key_values.keys() {
            let seen = if let Some(seen) = &seen_key_set {
                seen.contains(key.as_str())
            } else {
                seen_keys.contains(&key.as_str())
            };
            if !seen {
                emit_key(&mut parts, key);
            }
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("{{{}}}", parts.join(" "))
    }
}

fn is_language_tag(value: &str) -> bool {
    value.is_empty()
        || value.split('-').all(|subtag| {
            !subtag.is_empty()
                && subtag.len() <= 8
                && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
}

/// A value in the QUOTED form whatever it holds.
///
/// `unquoted_value` is `(letter | digit | '-' | '_' | '.' | ':')+`, so a class
/// like `w-1/2` has no bare spelling and [`quote_attr_value`] hands one back
/// anyway (carve#2440). The classes that reach the key-value form are exactly
/// the ones `.` cannot spell, so this asks no questions and quotes - which is
/// also the spelling carve#2435 ruled for the importer, `{class="-col"}`.
fn quoted_attr_value(value: &str) -> String {
    format!("\"{}\"", escape_quoted_run(value, &['"', '|']))
}

fn quote_attr_value(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '\\' | '{' | '}' | '|'))
    {
        value.to_string()
    } else {
        // `\|` is the only pipe a table row's cell cut leaves in place
        // ([CARVE-P2-019]), so a pipe is escaped wherever the value sits.
        format!("\"{}\"", escape_quoted_run(value, &['"', '|']))
    }
}

fn align_marker(align: Option<TableAlign>) -> &'static str {
    match align {
        Some(TableAlign::Left) => "<",
        Some(TableAlign::Right) => ">",
        Some(TableAlign::Center) => "~",
        None => "",
    }
}

/// The staging characters an AUTHORED occurrence can be mistaken for.
const SENTINEL_DEFAULTS: [char; SENTINEL_COUNT] = [
    '\u{e003}', '\u{e004}', '\u{e005}', '\u{e010}', '\u{e011}', '\u{e012}', '\u{e013}',
];

const SENTINEL_COUNT: usize = 7;
const S_CODE_LINE: usize = 6;

const S_BLANK: usize = 0;
const S_GUARD: usize = 1;
const S_MARKER_COLUMN: usize = 2;
const S_ESCAPED_SPACE: usize = 3;
const S_STAGED_SPACE: usize = 4;
const S_STAGED_TAB: usize = 5;

/// One candidate site the escape search can offer back.
///
/// THE UNIT, THE RUN AND THE OFFSET, all three. The offset alone is not a key:
/// a unit is the node whose arm wrote the character, and a BLOCK's arm can
/// write several runs -- a table row's cells, a fence title beside its info
/// string -- each with its own offsets starting at zero. The whole triple
/// survives a re-render because relaxing an occurrence changes which characters
/// are emitted and never which arms run, so a unit writes the same runs in the
/// same order with the same offsets on every render.
type Occurrence = (usize, usize, usize);

/// The index of the run about to be escaped, within `unit`.
fn next_escape_call_index(session: &RenderSession, unit: usize) -> usize {
    session.pass.escape_call_indexes.with(|cell| {
        let mut map = cell.borrow_mut();
        let index = map.entry(unit).or_insert(0);
        let current = *index;
        *index += 1;
        current
    })
}

/// Whether the search has handed the candidate at `key` back its bare form.
///
/// CALLED AT EVERY OFFERED SITE, relaxed or not, because the log is what the
/// search walks: a site the pass never reported is a site the search can never
/// offer, and the escape stays for a reason nobody wrote down.
///
/// THE OCCURRENCE IS THE RUN, WHICH IS §2's OWN UNIT. "THE UNIT IS THE OPENER,
/// NOT THE CHARACTER" -- where a construct opens on a run of characters the
/// whole run is escaped, so `\#\# H` and never `\## H`. A search that offered
/// the two hashes separately relaxes the second one, because with the first
/// still escaped no heading forms either way, and emits precisely the
/// half-escaped run §2 calls "a shape that happens to work rather than one that
/// says what it means". So a candidate repeating the character before it
/// inherits that character's decision instead of taking one.
fn occurrence_is_relaxed(session: &RenderSession, key: Occurrence, continues_run: bool) -> bool {
    if continues_run {
        return session.last_occurrence_relaxed.with(std::cell::Cell::get);
    }
    session.occurrence_log.with(|cell| {
        if let Some(log) = cell.borrow_mut().as_mut() {
            log.push(key);
        }
    });
    let relaxed = session
        .relaxed_occurrences
        .with(|cell| cell.borrow().as_ref().is_some_and(|set| set.contains(&key)));
    session
        .last_occurrence_relaxed
        .with(|cell| cell.set(relaxed));
    relaxed
}

/// Claim the next unit ordinal for the node about to render.
fn next_escape_unit(session: &RenderSession) -> usize {
    let next = session.pass.unit_counter.with(|c| {
        let next = c.get() + 1;
        c.set(next);
        next
    });
    escape_window::claimed(session, next);
    next
}

impl CarveContext {
    /// How PART 11 §5 reads bracket `ordinal` of the node at `at`.
    fn bracket_role(&self, mut at: usize, ordinal: usize) -> BracketRole {
        // A node can be cloned more than once on the way down; follow the
        // chain only until the active scope knows the address.
        while !self.brackets.keyed.contains(&at) {
            match self.bracket_aliases.get(&at) {
                Some(&source) => at = source,
                None => break,
            }
        }
        BracketRole {
            lone: self.brackets.lone.contains(&(at, ordinal)),
            crossing_opener: self.brackets.crossing_openers.contains(&(at, ordinal)),
            crossing_closer: self.brackets.crossing_closers.contains(&(at, ordinal)),
            paired_closer: self.brackets.paired_closers.contains(&(at, ordinal)),
            key: (at, ordinal),
            opener: self.brackets.closer_openers.get(&(at, ordinal)).copied(),
        }
    }

    /// Which form a character written by `unit` takes (PART 11 §2b).
    fn escape_mode_for(&self, session: &RenderSession, unit: usize) -> EscapeMode {
        session.asked_units.with(|cell| {
            if let Some(asked) = cell.borrow_mut().as_mut() {
                asked.insert(unit);
            }
        });
        session
            .escalated_units
            .with(|cell| match cell.borrow().as_ref() {
                None => self.escape_mode,
                Some(escalated) => {
                    if escalated.contains(&unit) {
                        EscapeMode::Conservative
                    } else {
                        EscapeMode::Minimal
                    }
                }
            })
    }

    /// Which form the character being written now takes.
    fn escape_mode_here(&self, session: &RenderSession) -> EscapeMode {
        self.escape_mode_for(session, self.escape_unit)
    }

    /// The mode of the node that is about to claim the next ordinal.
    fn next_unit_escape_mode(&self, session: &RenderSession) -> EscapeMode {
        self.escape_mode_for(session, session.pass.unit_counter.with(|c| c.get()) + 1)
    }
}

fn sentinel(session: &RenderSession, which: usize) -> char {
    session.sentinels.with(|s| s.get()[which])
}

fn note_inserted(session: &RenderSession, which: usize) {
    session.inserted.with(|c| {
        let mut n = c.get();
        n[which] += 1;
        c.set(n);
    });
}

/// Record sentinels standing in the assembled document, at the site that is
/// about to consume them.
fn note_seen(session: &RenderSession, which: usize, count: usize) {
    if count == 0 {
        return;
    }
    session.seen.with(|c| {
        let mut n = c.get();
        n[which] += count;
        c.set(n);
    });
}

fn verbatim_blank(session: &RenderSession) -> char {
    sentinel(session, S_BLANK)
}

fn thematic_guard(session: &RenderSession) -> char {
    sentinel(session, S_GUARD)
}

fn escaped_space(session: &RenderSession) -> String {
    sentinel(session, S_ESCAPED_SPACE).to_string()
}

fn staged_space(session: &RenderSession) -> char {
    sentinel(session, S_STAGED_SPACE)
}

fn staged_tab(session: &RenderSession) -> char {
    sentinel(session, S_STAGED_TAB)
}

fn free_sentinel(text: &str, taken: &[char; SENTINEL_COUNT]) -> char {
    ('\u{e020}'..='\u{f8ff}')
        .find(|c| !taken.contains(c) && !text.contains(*c))
        .unwrap_or('\u{f8ff}')
}

fn resolve_nbsp_placeholder(session: &RenderSession, text: &str, in_line_block: bool) -> String {
    if !in_line_block {
        let marker = escaped_space(session);
        for _ in text.matches(crate::NBSP_PLACEHOLDER) {
            note_inserted(session, S_ESCAPED_SPACE);
        }
        return text.replace(crate::NBSP_PLACEHOLDER, &marker);
    }
    text.split('\n')
        .map(|line| stage_line_block_layout(session, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Write a line block's preserved whitespace back as plain spaces.
///
/// The runs staged here are exactly the ones the parser reproduces from plain
/// spaces: a LEADING run of any width, and a medial or trailing run of two or
/// more (grammar §23). A lone medial placeholder can then only have come from
/// an escaped space, so `a\ b` still round-trips as written. Two ADJACENT
/// escaped spaces are the one form that changes - `a\ \ b` is written back as
/// `a  b` - because inside a line block those are the same document: both parse
/// to the same pair of placeholders.
fn stage_line_block_layout(session: &RenderSession, line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut seen_content = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != crate::NBSP_PLACEHOLDER {
            out.push(ch);
            seen_content = true;
            continue;
        }

        let mut run = 1usize;
        while chars.peek() == Some(&crate::NBSP_PLACEHOLDER) {
            chars.next();
            run += 1;
        }

        if !seen_content || run >= 2 {
            for _ in 0..run {
                note_inserted(session, S_STAGED_SPACE);
                out.push(staged_space(session));
            }
        } else {
            // A single placeholder mid-line is an escaped space, not layout.
            note_inserted(session, S_ESCAPED_SPACE);
            out.push_str(&escaped_space(session));
        }
    }

    out
}

fn normalize(session: &RenderSession, text: &str) -> String {
    // Count the escaped-space marker BEFORE the replace below consumes it.
    // Everything else is counted further down, just before `restore_verbatim`,
    // but this one is resolved first and would already be gone by then - which
    // is exactly how an authored U+E010 went on being eaten after the other
    // four were fixed (carve-rs#630).
    let marker = escaped_space(session);
    session.staged.with(|c| c.borrow_mut().push_str(text));
    session.seen.with(|c| {
        let mut n = c.get();
        n[S_ESCAPED_SPACE] += text.matches(&marker).count();
        c.set(n);
    });
    // U+E010 marks an escaped space, and it resolves HERE rather than during
    // rendering because the backslash it expands to is itself an unconditional
    // escape: expanding earlier let escapeText double it, giving `10\\ kg`.
    // An escaped space at end of line has already lost its trailing SPACE by
    // PART 11 §2a: canonical source must not depend on editors preserving that
    // byte. Expand it to the bare backslash in every container, not only at
    // document level. The list writer used to indent first and preserve the
    // expanded space as mid-paragraph content (carve-rs#855).
    let mut expanded = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == sentinel(session, S_ESCAPED_SPACE) {
            expanded.push('\\');
            if !matches!(chars.peek(), None | Some('\n')) {
                expanded.push(' ');
            }
        } else {
            expanded.push(ch);
        }
    }
    let text = expanded;
    // Strip a line's trailing whitespace only where it cannot be content. At the
    // end of a paragraph the parser drops it too, so the writer must; before a
    // SOFT BREAK the parser keeps it, and stripping it there changed the
    // rendered output (carve#359). A line whose successor is blank ends its
    // block; one followed by more text is mid-paragraph.
    let trimmed = trim_non_nbsp(&text);
    let raw: Vec<&str> = trimmed.split('\n').collect();
    let lines = raw
        .iter()
        .enumerate()
        .map(|(i, line)| {
            // A line whose only content is ASCII space or tab is emitted EMPTY,
            // wherever it sits (PART 11 section 7). Editors and CI that strip
            // trailing whitespace rewrite such a line, so `fmt` would report a
            // diff on a file nobody edited (carve#375). This is separate from
            // the block-final rule below, which is about a line WITH content:
            // that whitespace can be document content, and stripping it before
            // a soft break changed rendered output (carve#359).
            if !line.is_empty() && line.trim_matches([' ', '\t']).is_empty() {
                return String::new();
            }
            let ends_block = raw.get(i + 1).map_or(true, |next| next.trim().is_empty());
            if ends_block {
                trim_end_non_nbsp(line).to_string()
            } else {
                (*line).to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let staged = trim_non_nbsp(&collapse_blank_lines(&lines)).to_string();
    let current = session.sentinels.with(|s| s.get());
    session.staged.with(|c| c.borrow_mut().push_str(&staged));
    session.seen.with(|c| {
        let mut n = c.get();
        for i in 0..SENTINEL_COUNT {
            // Two are counted elsewhere, both because this text is past the
            // point that consumes them: the escaped-space marker at the top of
            // `normalize`, before the replace that resolves it, and the
            // marker-column tag in the list writer's line loop, which strips it.
            if i != S_ESCAPED_SPACE && i != S_MARKER_COLUMN {
                n[i] += staged.matches(current[i]).count();
            }
        }
        c.set(n);
    });
    format!("{}\n", restore_verbatim(session, &staged))
}

/// Whole-document normalization (trailing-whitespace strip, blank-line
/// collapsing) must not reach inside verbatim content - code blocks, raw
/// blocks, frontmatter, and block comments reproduce their content byte-exact
/// (carve-js issue 340). Sentinel-encode the vulnerable bytes before the
/// content joins the document string; `normalize` restores them at the end.
/// Markers are allocated from characters absent from the document.
fn protect_verbatim(session: &RenderSession, content: &str) -> String {
    let mut lines = Vec::new();
    for line in content.split('\n') {
        if line.is_empty() {
            note_inserted(session, S_BLANK);
            lines.push(verbatim_blank(session).to_string());
            continue;
        }
        let stripped = line.trim_end_matches([' ', '\t']);
        let tail: String = line[stripped.len()..]
            .chars()
            .map(|ch| {
                if ch == ' ' {
                    note_inserted(session, S_STAGED_SPACE);
                    staged_space(session)
                } else {
                    note_inserted(session, S_STAGED_TAB);
                    staged_tab(session)
                }
            })
            .collect();
        lines.push(format!("{stripped}{tail}"));
    }
    lines.join("\n")
}

/// Protect a paragraph line that would re-parse as a thematic break.
///
/// Source indentation is not in the AST, so an indented `---` - a paragraph
/// holding an em dash - is emitted at column 0, where it stops being a
/// paragraph and becomes a thematic break.
///
/// Text nodes are already covered: the conservative form escapes the hyphens,
/// so the round-trip check sees the difference and picks that form. A
/// smart-punctuation run is not, because its source run is emitted verbatim in
/// BOTH forms - that is the point of the node - so the check never has a
/// difference to act on. Escaping the run in the conservative form does not
/// work either: it would make that form change the document, after which the
/// check could never prefer the minimal one.
///
/// It marks rather than escapes: escaping would split the run (a leading
/// escaped hyphen plus an en dash) and change the document just as surely,
/// while a leading space keeps the line a paragraph and keeps the em dash -
/// which is what the source said. The marker is a sentinel because normalize()
/// trims the document's leading whitespace, which would silently undo the guard
/// whenever the paragraph is the first block.
fn guard_thematic_break_lines(session: &RenderSession, body: &str, in_line_block: bool) -> String {
    // Line-block bodies parse as inline content; padding would add a no-break space.
    if in_line_block || !body.contains('-') {
        return body.to_string();
    }
    body.split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches([' ', '\t']);
            if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-') {
                note_inserted(session, S_GUARD);
                format!("{}{line}", thematic_guard(session))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Undo `protect_verbatim` and the thematic-break guard, POSITIONALLY.
fn restore_verbatim(session: &RenderSession, text: &str) -> String {
    let text = text
        .split('\n')
        .map(|line| {
            if let Some((prefix, rest)) = line.split_once(sentinel(session, S_CODE_LINE)) {
                if prefix.chars().all(|ch| matches!(ch, ' ' | '\t' | '>')) {
                    let guarded = if rest.starts_with('[') {
                        format!("{prefix} ")
                    } else if let Some(last_quote) = prefix.rfind('>') {
                        format!("{}  ", &prefix[..=last_quote])
                    } else {
                        " ".to_owned()
                    };
                    return format!("{guarded}{rest}");
                }
            }
            line.replace(sentinel(session, S_CODE_LINE), "")
        })
        .collect::<Vec<_>>()
        .join("\n");
    text.split('\n')
        .map(|line| {
            // The marker may arrive INDENTED: inside a container the host adds
            // its columns before this runs, so the line is `  ` + marker rather
            // than the marker alone. Testing for the marker by itself missed
            // those and left a raw U+E003 in the output - caught by
            // `verbatim_content_stable_inside_containers` and by the corpus
            // formatter's semantic check on
            // `69-opaque-spans-inside-a-container-6`.
            //
            // Drop the marker. A marker sitting next to real text is left alone,
            // which is the point.
            let prefix = line.trim_end_matches(verbatim_blank(session));
            if prefix.len() != line.len()
                && prefix.chars().all(|c| c == ' ' || c == '\t' || c == '>')
            {
                return prefix.trim_end_matches([' ', '\t']).to_string();
            }
            let line = match line.split_once(thematic_guard(session)) {
                Some((prefix, rest)) if prefix.chars().all(|ch| matches!(ch, ' ' | '\t' | '>')) => {
                    format!("{prefix} {rest}")
                }
                _ => line.to_string(),
            };
            // The staged pair IS positional, once you read both insertion sites
            // together rather than looking for one position:
            //
            //   protect_verbatim stages a line's TRAILING run (any length)
            //   the line-block layout path stages a LEADING run (any length) or
            //     any run of TWO OR MORE - `!seen_content || run >= 2`
            //
            // So a run the writer inserted is always leading, trailing, or at
            // least two long. A SINGLE staged character sitting mid-line is
            // therefore never the writer's, and is left alone - which is the case
            // an author hits by typing one U+E011 or U+E012 in a code block.
            //
            // My earlier attempt at this restored only the trailing run, dropped
            // the medial case and broke `line_block_medial_gaps`; the note left
            // behind said separate sentinels were needed. They are not - the
            // run-length half of the layout condition is what was missing.
            //
            // RESIDUE, stated rather than implied: an authored run of two or more,
            // or a single one at the start or end of a line, still collides. That
            // needs the insertion counts.
            restore_staged_runs(session, &line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Undo the staged whitespace pair only where the writer inserts it: a LEADING
/// run, a TRAILING run, or any run of two or more (see `restore_verbatim`).
///
/// A leading run is measured past the container prefix the host may have added
/// before this runs (spaces, tabs, `>`), the same allowance the blank-line marker
/// makes a few lines above.
fn restore_staged_runs(session: &RenderSession, line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let staged = |c: char| c == staged_space(session) || c == staged_tab(session);
    let prefix_end = chars
        .iter()
        .position(|&c| !(c == ' ' || c == '\t' || c == '>'))
        .unwrap_or(chars.len());
    let mut out = String::with_capacity(line.len());
    let mut i = 0usize;
    while i < chars.len() {
        if !staged(chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && staged(chars[i]) {
            i += 1;
        }
        let run = i - start;
        let writer_inserted = start == prefix_end || i == chars.len() || run >= 2;
        for &ch in &chars[start..i] {
            if writer_inserted {
                out.push(if ch == staged_space(session) {
                    ' '
                } else {
                    '\t'
                });
            } else {
                out.push(ch);
            }
        }
    }
    out
}

fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::new();
    let mut newlines = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push(ch);
            }
        } else {
            newlines = 0;
            out.push(ch);
        }
    }
    out
}

/// Fold every line break in `text` (a hard break's `\` included) to one space,
/// then trim. Used where the target construct occupies exactly one line, so a
/// break in the tree would otherwise be written out as a real newline and
/// change the block structure on re-parse.
fn collapse_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut slashes = 0usize;
    while let Some(c) = chars.next() {
        if c == '\\' {
            slashes += 1;
            out.push(c);
            continue;
        }
        if c != '\n' {
            slashes = 0;
            out.push(c);
            continue;
        }
        // Only an ODD run of backslashes before the newline is a hard break's
        // marker; an even run is literal backslashes that happen to end the
        // line. Dropping one unconditionally turned `a\` plus a soft break into
        // `a\ b`, where the escape swallows the space and the backslash is lost.
        if slashes % 2 == 1 {
            out.pop();
        }
        slashes = 0;
        // Emit one space for the break and swallow the next line's indentation.
        out.push(' ');
        while chars.peek().is_some_and(|c| *c == ' ' || *c == '\t') {
            chars.next();
        }
    }
    trim_heading_edges(&out).to_string()
}

/// The whitespace a heading cannot hold at its edges.
///
/// A heading's marker separator is a run of SPACES and none of it is content
/// (markup-carve/carve#1587), so a leading TAB is content the source can hold:
/// `## \tx` is an h2 whose text opens with the tab. A separator that STARTS
/// with a tab opens no heading at all, which is why leading spaces still go -
/// the separator run absorbs them and the writer re-emits exactly one. Trimming
/// the tab alongside them wrote `## x` and lost it on the re-parse.
///
/// The trailing run goes whole: any parse drops it, and stripping the newline
/// here is what leaves a hard break's backslash standing for `collapse_breaks`
/// to keep.
fn trim_heading_edges(text: &str) -> &str {
    trim_end_non_nbsp(text.trim_start_matches([' ', '\n', '\r']))
}

/// What an escape decision needs that one text node cannot say.
///
/// Both facts here are about what SURROUNDS the node: whether it sits inside a
/// note's content, and what the node after it writes. A text node holds neither,
/// and `boundary_text` cannot supply the second - it reports a code span's
/// CONTENT while the span writes a backtick ahead of it.
#[derive(Clone, Copy)]
struct NeighbourEscape {
    /// Inside an inline note's content, where PART 9 §16 disables note
    /// recognition at every depth.
    in_note_content: bool,
    /// The `[` arrived as the NEXT node's boundary character, and that node
    /// opens a note with it.
    next_node_opens_a_note: bool,
    /// The next node writes [`render_code`]'s backtick fence at byte zero, so a
    /// `$`, `$$` or `!` this node ends on binds to it.
    next_node_opens_a_verbatim_span: bool,
    /// The next node writes a `[` at byte zero, so a trailing `:name` would
    /// open an inline extension with it.
    next_node_opens_a_bracket: bool,
}

/// Whether the `^` before the `[` at `bracket` needs its escape.
///
/// PART 11 §2 escapes a character IF AND ONLY IF omitting the escape would
/// change the re-parsed AST, and it takes the decision per OPENER OCCURRENCE.
/// `^[` is only an occurrence where the note can FORM, and PART 9 §16 gives two
/// shapes where it cannot: an empty or whitespace-only body is literal, and note
/// recognition is DISABLED inside a note's own content, at every depth. Escaping
/// either one is the over-escaping §2 calls a defect rather than a safe default.
///
/// Three of the four answers are certain and are taken here. The fourth is not:
/// the run does not close in THIS text node, and a later node may still supply
/// the `]` - or may not, which is the `x ^[a` that needs nothing. That one is
/// handed to the minimal/conservative vote (§4) rather than guessed, which is
/// what the `mode` argument is for: the two passes then differ, W3 parses both,
/// and the bare form is emitted exactly when it re-parses the same.
fn caret_needs_its_escape(
    text: &str,
    bracket: usize,
    note: NeighbourEscape,
    mode: EscapeMode,
) -> bool {
    if note.in_note_content {
        return false;
    }
    let run = match text.get(bracket..) {
        Some(rest) if rest.starts_with('[') => rest,
        // The `[` is the next NODE's, so the run is not this node's to weigh -
        // and it is not even certain to follow the caret in the output, since a
        // node reporting a boundary character emits its own opener ahead of it.
        // `next_node_opens_a_note` is that question, answered where the node was
        // in hand.
        _ => return note.next_node_opens_a_note,
    };
    match crate::parse::bracketed_run_body(run) {
        Some(body) => !body.trim().is_empty(),
        None => mode == EscapeMode::Conservative,
    }
}

/// The characters a node writes VERBATIM at the very front of its output.
///
/// A different question from [`boundary_text`], which answers "what character is
/// adjacent" for the emphasis and comment-spacing decisions and is happy to name
/// one the node does not write first: a code span reports its content while
/// writing a backtick ahead of it, escaped text reports the character while
/// writing a backslash, and a mention, a tag and a symbol all write their sigil
/// first. Only these three put their own value at byte zero.
///
/// The nodes that open with a bare `[` are [`leading_bracket_run`]'s.
fn leading_verbatim_text(node: &InlineNode) -> Option<&str> {
    match node {
        InlineNode::Text(text) => Some(&text.value),
        InlineNode::SmartPunctuation(punctuation) => Some(&punctuation.value),
        InlineNode::Abbreviation(abbr) => Some(&abbr.abbr),
        _ => None,
    }
}

/// An opener is text while a span of its kind is open (PART 9 §9 E3), and the
/// forced form shares the stack (markup-carve/carve#2078), so an emphasis span
/// inside one of its own kind has no spelling. An insertion or deletion keeps
/// the braced-only rule of markup-carve/carve#2066. `braced_before` is where
/// this node's descendants start in `ctx.braced_spans`.
fn note_braced_span(
    node: &InlineNode,
    rendered: &mut String,
    braced_before: usize,
    ctx: &mut CarveContext,
) {
    let (delimiters, pos): (&[char], _) = match node {
        InlineNode::Emphasis(emphasis) => {
            (emphasis_delimiters(emphasis.kind), emphasis.pos.as_ref())
        }
        InlineNode::CriticInsert(insert) if rendered.starts_with("{+") => {
            (&['+'], insert.pos.as_ref())
        }
        InlineNode::CriticDelete(delete) if rendered.starts_with("{-") => {
            (&['-'], delete.pos.as_ref())
        }
        _ => return,
    };
    let nested: Vec<Option<usize>> = ctx.braced_spans[braced_before..]
        .iter()
        .filter(|(inner, _)| delimiters.contains(inner))
        .map(|(_, mark)| *mark)
        .collect();
    if !nested.is_empty() {
        crate::render_carve_error::record_unspellable(
            "emphasis",
            "a span inside a span of the same kind has no Carve source spelling",
        );
        crate::render_carve_error::record_nested_same_kind(nested.into_iter().flatten().collect());
    }
    // A braced inline starts its own scope (ruling markup-carve/carve#2091), so
    // a bare span holding a kind open outside it takes the braced form.
    if let InlineNode::Emphasis(emphasis) = node {
        let holds_an_outer_kind = ctx.braced_spans[braced_before..]
            .iter()
            .any(|(inner, _)| ctx.open_kinds.contains(inner));
        if holds_an_outer_kind {
            if let Some(delim) = bare_delimiter(emphasis.kind) {
                if rendered.starts_with(delim) {
                    let attrs = render_attrs(&emphasis.attrs);
                    let body = rendered[..rendered.len() - attrs.len()].to_string();
                    *rendered = format!("{{{body}}}{attrs}");
                }
            }
        }
    }
    if rendered.starts_with('{') {
        ctx.braced_spans.truncate(braced_before);
    }
    for delimiter in delimiters {
        ctx.braced_spans
            .push((*delimiter, pos.map(|pos| pos.start_offset)));
    }
}

fn emphasis_delimiters(kind: EmphasisKind) -> &'static [char] {
    match kind {
        EmphasisKind::Italic => &['/'],
        EmphasisKind::Strong => &['*'],
        EmphasisKind::Underline => &['_'],
        EmphasisKind::Strike => &['~'],
        EmphasisKind::Highlight => &['='],
        EmphasisKind::Super => &['^'],
        EmphasisKind::Sub => &[','],
        EmphasisKind::BoldItalic => &['/', '*'],
        // No delimiter, because no source spelling: the writer flattens it.
        EmphasisKind::SmallCaps => &[],
    }
}

/// The offset of a `:` that, with the name after it, ends `text` as an inline
/// extension opener short only of the next node's `[` (markup-carve/carve#2068).
fn trailing_extension_colon(text: &str) -> Option<usize> {
    let name_start = text
        .trim_end_matches(|c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        .len();
    let colon = name_start.checked_sub(1)?;
    let first = text[name_start..].chars().next()?;
    (text.as_bytes()[colon] == b':' && (first.is_ascii_alphabetic() || first == '_'))
        .then_some(colon)
}

/// The bracket run a node writes at byte zero, with a stand-in label that is
/// blank exactly when the written label is: `^[](u)` opens no note, `^[n](u)`
/// does (markup-carve/carve-rs#1710).
fn leading_bracket_run(node: &InlineNode) -> Option<Cow<'_, str>> {
    fn label(children: &[InlineNode]) -> Cow<'static, str> {
        let blank = children
            .iter()
            .all(|child| matches!(child, InlineNode::Text(t) if t.value.trim().is_empty()));
        Cow::Borrowed(if blank { "[]" } else { "[x]" })
    }
    match node {
        InlineNode::Link(link) if link.ref_label.is_some() && link.raw_ref.is_some() => {
            link.raw_ref.as_deref().map(Cow::Borrowed)
        }
        InlineNode::Link(link) if link.from_crossref && link.href.starts_with('#') => None,
        InlineNode::Link(link) => Some(label(&link.children)),
        InlineNode::Span(span) => Some(label(&span.children)),
        InlineNode::Footnote(note) if note.inline.is_none() => Some(Cow::Borrowed("[^x]")),
        InlineNode::CitationGroup(group) => Some(Cow::Borrowed(&group.raw)),
        _ => None,
    }
}

/// Does the node FOLLOWING a text node begin, in the output, with the BACKTICK
/// FENCE a `$`, `$$` or `!` sigil would bind to?
///
/// Asked of the node rather than of [`boundary_text`], which reports a code
/// span's CONTENT and so cannot answer it: `a $` before a code span holding
/// `x+y` sees `x` as its neighbour character while the output puts a backtick
/// there.
///
/// A code span and a raw inline are the two nodes that write [`render_code`]'s
/// fence at byte zero. Math and an inline literal write their own sigil first
/// (`$` and `!`), so the fence is not adjacent to the text at all.
fn next_node_opens_a_verbatim_span(next: Option<&InlineNode>) -> bool {
    matches!(
        next,
        Some(InlineNode::Code(_)) | Some(InlineNode::RawInline(_))
    )
}

/// Does the node FOLLOWING a text node begin, in the output, with a bracket run
/// that opens a note?
///
/// This is what stops ``x ^`[t]` `` coming back ``x \^`[t]` ``: the code span
/// reports a `[` as the adjacent character but writes a backtick, so the caret
/// is not in front of a bracket in the output at all.
fn next_node_opens_a_note(
    next: Option<&InlineNode>,
    in_note_content: bool,
    mode: EscapeMode,
) -> bool {
    let text = match next {
        Some(node) => leading_verbatim_text(node)
            .map(Cow::Borrowed)
            .or_else(|| leading_bracket_run(node)),
        None => None,
    };
    match text {
        Some(text) => caret_needs_its_escape(
            &text,
            0,
            NeighbourEscape {
                in_note_content,
                next_node_opens_a_note: false,
                next_node_opens_a_verbatim_span: false,
                next_node_opens_a_bracket: false,
            },
            mode,
        ),
        None => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn escape_text(
    session: &RenderSession,
    text: &str,
    bracket_role: &dyn Fn(usize) -> BracketRole,
    paired_closer_carry: &std::cell::Cell<bool>,
    mode: EscapeMode,
    unit: usize,
    opens_block_line: bool,
    caption_can_open: bool,
    in_table_cell: bool,
    previous_boundary: char,
    next_boundary: char,
    note: NeighbourEscape,
) -> String {
    let verbatim_sigil_at = note
        .next_node_opens_a_verbatim_span
        .then(|| {
            let trimmed = text.trim_end_matches('$');
            if trimmed.len() < text.len() {
                return Some(trimmed.len());
            }
            text.strip_suffix('!').map(str::len)
        })
        .flatten();
    let extension_colon_at = note
        .next_node_opens_a_bracket
        .then(|| trailing_extension_colon(text))
        .flatten();
    let mut out = String::new();
    // PART 11 §2's decision is taken per OPENER OCCURRENCE, so every candidate
    // site in this run gets an index the search can address it by
    // (markup-carve/carve#1533).
    let call = next_escape_call_index(session, unit);
    let mut at_line_start = opens_block_line;
    let mut chars = text.char_indices().peekable();
    let mut previous = previous_boundary;
    let mut bracket_ordinal = 0;
    let mut after_paired_closer = paired_closer_carry.get();
    while let Some((offset, ch)) = chars.next() {
        // A CONTROL CHARACTER IS CONTENT, and the writer has to write it back.
        // This dropped 61 codepoints - every C0 control but tab/newline/return,
        // DEL, and the whole C1 block - none of which the parser or the HTML
        // renderer drops, so `to_html(fmt(x)) == to_html(x)` failed on any
        // document holding one. PART 2 keeps a FORM FEED and a VERTICAL TAB
        // explicitly (carve#926), and corpus
        // `261-a-blank-line-holds-spaces-and-tabs-and-nothing-else-3` pins a
        // line holding one as CONTENT rather than as a blank.
        //
        // U+0000 stays dropped, and only it: `normalize_source` removes it
        // before the parser sees it, so keeping it here would write back a byte
        // no re-parse can read. Every other control survives the round trip
        // because it survives the parse.
        //
        // This is not the Trojan-Source hardening, which is a different set in
        // a different place: `escape::is_bidi_control` strips the bidi
        // overrides and isolates (U+202A-E, U+2066-9), none of which are in the
        // range this line held.
        if ch == '\u{0000}' {
            continue;
        }
        // The caption marker is `^` followed by a SPACE. `^sup^` at the start
        // of a line is not one - superscript is braced-only, so it is literal
        // text and needs no escape, which two of this repo's own tests already
        // pinned.
        let next = chars.peek().map(|&(_, c)| c).unwrap_or(next_boundary);
        // SPACE ONLY, which is what the comment above already said and what the
        // code did not do. A tab after the marker leaves the line as prose -
        // corpus
        // `231-a-tab-after-a-heading-quote-or-caption-marker-leaves-the-line-as-prose-2`
        // is that document - so `^<TAB>` re-parses as text either way and PART 11
        // §4 asks for the minimal form when dropping the escape changes nothing.
        let caret_opens_a_caption = ch == '^' && at_line_start && caption_can_open && next == ' ';
        let caret_opens_inline = ch == '^'
            && (next == '[' || (chars.peek().is_none() && note.next_node_opens_a_note))
            && caret_needs_its_escape(text, offset + ch.len_utf8(), note, mode);
        let colon_cannot_open =
            ch == ':' && !at_line_start && !symbol_opens_at(text, offset, previous);
        at_line_start = ch == '\n';
        let opens_a_verbatim_construct = verbatim_sigil_at.is_some_and(|start| offset >= start);
        let role = if matches!(ch, '[' | ']') {
            bracket_ordinal += 1;
            bracket_role(bracket_ordinal - 1)
        } else {
            BracketRole::default()
        };
        let opens_a_destination = ch == '('
            && after_paired_closer
            && crate::parse::opens_inline_link_target(&text[offset..]);
        let unconditional = role.lone
            || role.crossing_opener
            || role.crossing_closer
            || opens_a_destination
            || matches!(ch, '\\' | '`' | '"' | '\'')
            || extension_colon_at == Some(offset)
            || caret_opens_a_caption
            || caret_opens_inline
            || opens_a_verbatim_construct;
        let caret_is_a_span_marker = ch == '^' && in_table_cell;
        // A comment opens on the first TWO unescaped percent signs of a run, so
        // the escape on the first one already makes the whole run literal:
        // `\%%c`, and `\%%%c` for a longer run. A percent repeating the one
        // before it is therefore never a site, which keeps it out of the run
        // rule below that would otherwise hand it its predecessor's escape and
        // write `\%\%c` (carve-rs#2443). The shared escaper corpus pins the
        // one-escape spelling and both peer engines write it.
        let continues_percent_run = ch == '%' && offset > 0 && previous == '%';
        let candidate = !continues_percent_run
            && (caret_is_a_span_marker
                || matches!(
                    ch,
                    '*' | '_'
                        | '{'
                        | '}'
                        | '['
                        | ']'
                        | '('
                        | ')'
                        | '#'
                        | '+'
                        | '-'
                        | '.'
                        | '!'
                        | '~'
                        | '/'
                        | '<'
                        | '>'
                        | '@'
                        | '%'
                        | '|'
                        | '='
                        | ':'
                        | ';'
                        | '^'
                ));
        // In a unit the search has escalated, each candidate site is offered
        // back on its own, so the one occurrence that needed the escape no
        // longer drags the rest of the unit with it (PART 11 §2). A character
        // whose own guard already decided it -- the caret, a sigil binding to a
        // verbatim run, the unconditional set -- is not a candidate and is
        // never offered.
        let escaped = if let (']', Some(opener)) = (ch, role.opener) {
            // A paired closer follows its opener, which may sit in another unit
            // -- unless the second reading already owes this `]` an escape of
            // its own. Bracketed content keeps `closer_openers`, so without
            // this the mirror answered for every closer and the crossing escape
            // never reached one inside a link label or a span
            // (carve-rs#2215).
            let escaped = role.crossing_closer
                || session
                    .escaped_openers
                    .with(|cell| cell.borrow().get(&opener).copied().unwrap_or(false));
            session
                .last_occurrence_relaxed
                .with(|cell| cell.set(!escaped));
            escaped
        } else {
            let offered = mode == EscapeMode::Conservative && candidate && !unconditional;
            let relaxed = offered
                && occurrence_is_relaxed(
                    session,
                    (unit, call, offset),
                    offset > 0 && previous == ch,
                );
            unconditional || (offered && !relaxed && !colon_cannot_open)
        };
        if ch == '[' {
            session
                .escaped_openers
                .with(|cell| cell.borrow_mut().insert(role.key, escaped));
        }
        if escaped {
            out.push('\\');
        }
        out.push(ch);
        previous = ch;
        after_paired_closer = ch == ']' && !escaped && role.paired_closer;
    }
    paired_closer_carry.set(after_paired_closer);
    out
}

/// How PART 11 §5 reads one text bracket.
#[derive(Default, Clone, Copy)]
struct BracketRole {
    lone: bool,
    /// The `[` of a pair that crosses a formatting boundary.
    crossing_opener: bool,
    /// The `]` of such a pair, where an outer opener survives to answer it.
    crossing_closer: bool,
    paired_closer: bool,
    /// This bracket, as the scope keys it.
    key: (usize, usize),
    /// The `[` a paired `]` in bracketed content closes.
    opener: Option<(usize, usize)>,
}

/// Whether the `:` at `offset` opens a symbol shortcode.
///
/// MIRRORS [`crate::parse::parse_symbol`] and is deliberately not a second
/// reading of the same production: the opener's preceding-character test, the
/// first name character's narrower class (`_` is excluded so `:_x_:` cannot
/// steal from underline) and the closing colon are the parser's, so a writer
/// that escapes here and a parser that opens there cannot drift apart.
///
/// `previous` is the character before the run, which is what carries the
/// preceding-character test across a node boundary - the text node this is
/// called on may begin mid-line.
fn symbol_opens_at(text: &str, offset: usize, previous: char) -> bool {
    let bytes = text.as_bytes();
    if bytes.get(offset) != Some(&b':') {
        return false;
    }
    let prev = if offset == 0 {
        previous
    } else {
        // A multi-byte character before the colon is not ASCII alphanumeric and
        // is not `_`, so the boundary answer is the same either way.
        text[..offset].chars().next_back().unwrap_or(previous)
    };
    if prev.is_ascii_alphanumeric() || prev == '_' {
        return false;
    }
    let Some(&first) = bytes.get(offset + 1) else {
        return false;
    };
    if !first.is_ascii_alphanumeric() && first != b'+' && first != b'-' {
        return false;
    }
    let mut len = 1;
    while let Some(&b) = bytes.get(offset + 1 + len) {
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'+' || b == b'-' {
            len += 1;
        } else {
            break;
        }
    }
    bytes.get(offset + 1 + len) == Some(&b':')
}

fn escape_plain_line(text: &str) -> String {
    text.replace('\n', " ")
}

/// An image's ALT TEXT, written between `![` and `]`.
fn escape_image_alt(text: &str, in_table: bool) -> String {
    let escaped = if crate::parse::raw_bracket_run_closes(text)
        && crate::parse::unescape_title(text) == text
    {
        text.to_string()
    } else {
        text.replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]")
            .replace('`', "\\`")
    };
    if in_table {
        escaped.replace('|', "\\|")
    } else {
        escaped
    }
}

/// Which characters the destination scan would read differently if emitted
/// bare: a parenthesis with no partner, and a backslash sitting in front of one
/// of the three escapable characters. Balanced parentheses are deliberately
/// absent -- they re-parse as themselves, and escaping them would be churn
/// against the minimal-escaping rule in PART 11 section 4.
fn unbalanced_destination_chars(text: &str) -> std::collections::HashSet<usize> {
    let mut openers: Vec<usize> = Vec::new();
    let mut marked = std::collections::HashSet::new();
    for (i, ch) in text.char_indices() {
        if ch == '(' {
            openers.push(i);
        } else if ch == ')' && openers.pop().is_none() {
            marked.insert(i);
        }
    }
    marked.extend(openers);
    marked
}

/// Backslash-escape exactly what the destination scan would otherwise read
/// differently: a parenthesis with no partner, and a backslash sitting in front
/// of one of the three escapable characters. A balanced pair re-parses as
/// itself, so leaving it bare is the minimal escaping PART 11 section 4 asks
/// for.
fn escape_destination_escapes(text: &str) -> String {
    let needs_marking = text
        .as_bytes()
        .iter()
        .any(|&b| matches!(b, b'(' | b')' | b'\\'));
    if !needs_marking {
        return text.to_string();
    }
    let marked = unbalanced_destination_chars(text);
    let bytes = text.as_bytes();
    let mut out = String::new();
    for (i, ch) in text.char_indices() {
        let escapable =
            ch == '\\' && matches!(bytes.get(i + 1), Some(b'(') | Some(b')') | Some(b'\\'));
        if marked.contains(&i) || escapable {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn escape_destination(text: &str) -> String {
    // Almost every destination holds neither a parenthesis nor a backslash, so
    // there is nothing for the scan to misread and nothing to mark. Skipping
    // the walk keeps that case free of the set entirely.
    let needs_marking = text
        .as_bytes()
        .iter()
        .any(|&b| matches!(b, b'(' | b')' | b'\\'));
    let marked = if needs_marking {
        unbalanced_destination_chars(text)
    } else {
        std::collections::HashSet::new()
    };
    let bytes = text.as_bytes();
    let mut out = String::new();
    for (i, ch) in text.char_indices() {
        let escapable =
            ch == '\\' && matches!(bytes.get(i + 1), Some(b'(') | Some(b')') | Some(b'\\'));
        if marked.contains(&i) || escapable {
            out.push('\\');
        }
        match ch {
            // Whitespace is percent-encoded (it would end the destination
            // otherwise). A backslash before anything the scan does not treat
            // as an escape is emitted verbatim, so URLs carrying backslashes
            // need no doubling.
            ch if ch.is_whitespace() => {
                if ch == ' ' {
                    out.push_str("%20");
                } else {
                    out.push_str(&format!("%{:02X}", ch as u32));
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

/// A link, an image and a `[ref]: url "t"` definition, whose `link_title` reads
/// escapes back.
fn escape_quoted(text: &str) -> String {
    escape_quoted_run(text, &['"'])
}

/// Escape inside a quoted slot only what a bare spelling would lose.
///
/// `unescape_title` resolves `\\X` only when X is ASCII punctuation, so a
/// backslash before anything else reads back literally and needs no partner -
/// PART 11 §2 escapes a character only if omitting it would change the
/// re-parse. A TRAILING backslash still doubles: the character after it is the
/// closing delimiter, which is punctuation, so a bare one would swallow it.
fn escape_quoted_run(text: &str, escaped: &[char]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if chars
                .peek()
                .map_or(true, |next| next.is_ascii_punctuation())
            {
                out.push('\\');
            }
            out.push('\\');
            continue;
        }
        if escaped.contains(&ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// NOT the same rule: `quoted_title` takes `{character - '"'}` verbatim and has
/// no escape mechanism, so escaping here changes the title instead of
/// preserving it (carve-rs#1946). That leaves a `"` unspellable rather than
/// escapable - markup-carve/carve-php#2375's reading of the same slot - and
/// writing it as `\"` would emit source no conforming parser reads back, so the
/// writer refuses instead.
fn escape_quoted_title(text: &str, node_type: &'static str) -> String {
    if text.contains('"') {
        crate::render_carve_error::record_unspellable(node_type, QUOTE_IN_QUOTED_TITLE);
    }
    text.replace('"', "")
}

const QUOTE_IN_QUOTED_TITLE: &str = "a double quote has no spelling inside a quoted title";

/// Whether a container's quoted title slot can hold `title`, asked of the
/// writer and the parser rather than predicted: a `"` is refused (an attribute
/// value or a link title writes one too), and a line break writes an opener
/// that no longer reads as one.
pub(crate) fn quoted_title_is_spellable(title: &[InlineNode]) -> bool {
    let probe = Document {
        frontmatter: Default::default(),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        footnote_def_pos: Default::default(),
        children: vec![BlockNode::Admonition(Admonition {
            attrs: None,
            kind: "note".into(),
            title: Some(title.to_vec()),
            label: None,
            children: vec![BlockNode::Paragraph(Paragraph {
                attrs: None,
                children: vec![InlineNode::text("x")],
                at_content_column: true,
                block_image: false,
                pos: None,
            })],
            pos: None,
        })],
        source_len: 0,
        ingest_payload_len: 0,
    };
    match render_carve(&probe) {
        Ok(source) => matches!(
            crate::parse::parse(&source).children.as_slice(),
            [BlockNode::Admonition(admonition)] if admonition.title.is_some()
        ),
        Err(crate::RenderCarveError::SourceUnspellable(error)) => {
            error.reason() != QUOTE_IN_QUOTED_TITLE
        }
        // Another refusal is another pass's to answer.
        Err(_) => true,
    }
}

/// A container label's source, written from the inline content of the
/// `<p class="div-label">` the HTML renderer degraded it to, or `None` when the
/// writer has no spelling for it.
///
/// The label is an inline run (ruled on markup-carve/carve#2572), so markup in
/// it HAS a spelling on the opener and the paragraph can be lifted whole. The
/// importer used to refuse anything but text, because the label published
/// escaped and lifting a `<strong>` would have flattened it without a word.
///
/// NO SPELLING IS A REFUSAL, NOT A FAILURE. An empty `<code>` has no Carve
/// source while its open run does not end, and the writer says so; the paragraph
/// then stays in the body, where the ordinary walk reports the loss exactly as it
/// did before the label was a run. A depth refusal takes the same exit: what it
/// means is that this run cannot be written, which is the one thing the caller
/// asked.
pub(crate) fn container_label_source(inlines: &[InlineNode]) -> Option<String> {
    let probe = Document {
        frontmatter: Default::default(),
        frontmatter_raw: None,
        footnote_defs: Default::default(),
        footnote_def_pos: Default::default(),
        children: vec![BlockNode::Paragraph(Paragraph {
            attrs: None,
            children: inlines.to_vec(),
            at_content_column: true,
            block_image: false,
            pos: None,
        })],
        source_len: 0,
        ingest_payload_len: 0,
    };
    render_carve(&probe)
        .ok()
        .map(|source| source.trim_end().to_string())
}

/// Does the opener `::: [label]` read that label back?
///
/// The two characters an opener cannot carry are refused by name at the call
/// site, because each has its own reason. Everything else is asked of the PARSER,
/// for the reason [`quoted_title_is_spellable`] gives: enumerating the spellings
/// that break an opener is a second copy of the grammar and goes stale. An empty
/// code span writes two backticks, which turn the opener into a paragraph.
pub(crate) fn container_label_reads_back(label: &str) -> bool {
    let source = format!("::: [{label}]\nx\n:::\n");
    matches!(
        crate::parse::parse(&source).children.as_slice(),
        [BlockNode::Div(div)] if div.label.as_deref() == Some(label)
    )
}

/// A FLAT raw bracketed run: a colon-fence or code-fence `[label]`, and a
/// footnote's `[^id]` in both its definition and its references.
fn write_flat_bracket_run(text: &str) -> &str {
    text
}

/// NOT the same rule, deliberately.
///
/// [`detect_abbreviation_def`](crate::parse) reads the term as
/// `is_ascii_alphanumeric`, per PART 5's `(letter | digit)+`, so neither
/// character this escapes can reach it from a parse - and an ingested
/// abbreviation carrying one has no `*[…]:` spelling with or without the
/// backslash. Left as it stands rather than folded into the function above,
/// which would claim a shared rule where there is only a shared shape.
fn escape_abbr(text: &str) -> String {
    text.replace('\\', "\\\\").replace(']', "\\]")
}

fn escape_identifier(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect()
}

// A symbol name may contain `+` and `-` (so `:+1:` / `:-1:` round-trip),
// unlike an extension identifier.
fn escape_symbol_name(text: &str) -> String {
    text.chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '+' || *ch == '-')
        .collect()
}

/// A name has no escape, so one the parser would not read whole has no
/// spelling (ruling markup-carve/carve-php#2159).
fn refuse_unspellable_name(name: &str, node_type: &'static str) {
    if name.is_empty() || crate::parse::name_run_len(name) != name.len() {
        crate::render_carve_error::record_unspellable(
            node_type,
            "its name has no Carve source spelling",
        );
    }
}

fn escape_format(text: &str) -> String {
    let safe: String = text
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '-')
        .collect();
    if safe.is_empty() {
        "text".to_string()
    } else {
        safe
    }
}

fn escape_fence_token(text: &str) -> String {
    text.split_whitespace()
        .next()
        .unwrap_or_default()
        .replace('`', "")
}

fn escape_attr_key(text: &str) -> String {
    let mut out = String::new();
    let mut started = false;
    for ch in text.chars() {
        if !started {
            if ch.is_ascii_alphabetic() || ch == '_' {
                out.push(ch);
                started = true;
            }
        } else if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        }
    }
    if out.is_empty() {
        "x".to_string()
    } else {
        out
    }
}

fn escape_attr_name_value(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

/// Whether a name has a BARE spelling in Carve attribute syntax.
///
/// The writer's rule, shared with the HTML importer so the importer cannot keep
/// a name the writer would silently rewrite: `escape_attr_key` strips every
/// character this rejects, so `xlink:href` would come back as `xlinkhref` and
/// the document would claim an attribute the author never wrote
/// (carve-rs#1060).
/// Whether a name can be written as a BOOLEAN attribute -- a bare word with no
/// value. Narrower than [`is_attr_identifier`] by exactly one character: a
/// leading `_` is legal in an id, a class and a key, and refused here, because
/// `{_x_}` is a forced underline (markup-carve/carve#1450). PART 11 §6c
/// shortens a value-less attribute to its bare name and cannot do that for such
/// a name: `{_u}` is text and `{_x_}` is an underline, either way a document the
/// writer changed, which §1 forbids.
pub(crate) fn is_boolean_attr_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

pub(crate) fn is_attr_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

fn is_explicit_id_or_class_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
}

/// Whether a container KIND can be spelled as a colon-fence type word.
///
/// `render_admonition` writes the kind verbatim after the fence, so a kind this
/// rejects would be emitted as source that does not read back as the container
/// it came from: `::: -2col` is an ordinary paragraph, because the opener
/// grammar (PART 9, `admonition_open`) reads the word as
/// `[a-zA-Z0-9_][\w-]*` because it is an explicit class value.
///
/// Used by `html_import` to decide whether an element's class can become the
/// fence word of a rebuilt container, for the same reason it asks
/// [`is_attr_identifier`] about a name: the answer has to be the writer's,
/// rather than a second copy that drifts from it (carve-rs#1240).
pub(crate) fn is_container_kind(text: &str) -> bool {
    is_explicit_id_or_class_identifier(text)
}

fn escape_autolink_href(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('<', "\\<")
        .replace('>', "\\>")
}

fn spell_crossref_target(text: &str) -> String {
    if text.is_empty()
        || text
            .bytes()
            .any(|b| matches!(b, b'>' | b' ' | b'\t' | b'\r' | b'\n' | 0))
    {
        crate::render_carve_error::record_unspellable(
            "heading_ref",
            "an empty target or a target with a closer, whitespace or NUL has no Carve source spelling",
        );
    }
    text.to_owned()
}

fn escape_critic_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

fn first_boundary(node: &InlineNode) -> Option<char> {
    match node {
        InlineNode::Mention(_) => return Some('@'),
        InlineNode::Tag(_) => return Some('#'),
        InlineNode::Symbol(_) => return Some(':'),
        _ => {}
    }
    boundary_text(node).and_then(|s| {
        let mut chars = s.chars();
        match chars.next() {
            // In carve parse mode, text nodes preserve backslash escapes, so a
            // formatted `\_b\_` reaches us with a leading `\`. The escape marker
            // is not the adjacency-relevant character -- the escaped punctuation
            // char is. Skip a single leading backslash that escapes an ASCII
            // punctuation char so the emphasis bracing decision stays a function
            // of the semantic next character (e.g. `_`), matching `last_boundary`
            // (which already returns the escaped char) and keeping the formatter
            // idempotent and byte-identical to carve-js / carve-php.
            Some('\\') => match chars.next() {
                Some(next) if next.is_ascii_punctuation() => Some(next),
                _ => Some('\\'),
            },
            other => other,
        }
    })
}

/// Does an inline comment need a space before it, given what is already
/// emitted on its line?
///
/// Nothing emitted yet means the comment opens the run, and `%%` at the start
/// of a line is already a comment marker. Anything else that is not itself
/// whitespace has to be separated, or the marker glues to it and re-parses as
/// literal text.
fn needs_comment_space(emitted: &str) -> bool {
    match emitted.chars().next_back() {
        None => false,
        Some(last) => last != '\n' && !last.is_whitespace(),
    }
}

/// Does a line block's hard break need its backslash, given the bytes already
/// emitted for the line it ends (PART 11 §7c)?
///
/// Consequences §7c draws from its property, all of them about the line the
/// break ENDS and all of them places where §7's own precondition - "where the
/// PARSER discards trailing whitespace the writer may too" - does not hold:
///
///   - the line's content is EMPTY. A bare newline leaves a BLANK line, which
///     ends the stanza, so one stanza is written back as two.
///   - the line's content ends in a LONE trailing column. A bare newline makes
///     it line-trailing, where PART 2 drops it. A run of TWO OR MORE columns is
///     already NBSP content (PART 9 §23 MEDIAL GAPS) and survives on its own.
///
/// A LONE TRAILING COLUMN IS NOT ONLY A SPACE. An ESCAPED space is one too, and
/// it is lost harder: §2a writes an escaped space at the END of a line as a bare
/// backslash, on the ground that canonical source must not depend on an editor
/// preserving the byte after it - and in verse a bare backslash at end of line
/// is a HARD BREAK, so the column does not come back at all. The `\` this
/// returns is what puts the escape back INSIDE the line, where §2a's expansion
/// keeps its space. Derived from the property rather than read off the clause's
/// list, which names the plain space only.
///
/// THE LAST BODY LINE, the remaining consequence, is decided by the caller: it
/// is a fact about the break's place in the stanza, not about the bytes on its
/// line.
fn verse_break_needs_backslash(session: &RenderSession, emitted: &str) -> bool {
    let line = match emitted.rfind('\n') {
        Some(at) => &emitted[at + 1..],
        None => emitted,
    };
    if line.is_empty() {
        return true;
    }
    if line.ends_with(sentinel(session, S_ESCAPED_SPACE)) {
        return true;
    }
    line.ends_with(' ') && !line.ends_with("  ")
}

fn last_boundary(node: &InlineNode) -> Option<char> {
    boundary_text(node).and_then(|s| s.chars().next_back())
}

fn boundary_text(node: &InlineNode) -> Option<&str> {
    match node {
        InlineNode::Text(text) => Some(&text.value),
        // The CHARACTER, not the backslash that precedes it in the output. A
        // text node holding `_b_` and an escaped-text node holding `_` describe
        // the same neighbour, and the writer has to brace an adjacent delimiter
        // the same way for both - otherwise the first pass (plain text) and the
        // second (escaped text) disagree and `fmt(fmt(x)) != fmt(x)`.
        InlineNode::EscapedText(text) => Some(&text.value),
        InlineNode::SmartPunctuation(s) => Some(&s.value),
        InlineNode::Code(text) => Some(&text.value),
        InlineNode::Abbreviation(abbr) => Some(&abbr.abbr),
        InlineNode::Mention(mention) => Some(&mention.user),
        InlineNode::Tag(tag) => Some(&tag.name),
        InlineNode::Symbol(symbol) => Some(&symbol.name),
        _ => None,
    }
}

/// Whether the last character of `written` is escaped: an odd run of
/// backslashes stands before it.
fn ends_in_an_escape(written: &str) -> bool {
    let mut chars = written.chars().rev();
    chars.next();
    chars.take_while(|&c| c == '\\').count() % 2 == 1
}

#[cfg(test)]
mod tests {
    #[test]
    fn empty_code_runs_account_for_adjacent_backticks() {
        for (input, expected) in [
            ("\0 a \0", "``` a ``"),
            ("\0`` a ``", "````` a ``"),
            ("\0\0", "`````"),
            ("é [\0] [\0]", "é [```] [``]"),
        ] {
            assert_eq!(super::spell_empty_code_runs(input.into()), expected);
        }
    }

    #[test]
    fn many_empty_code_runs_are_assembled_from_the_suffix_lengths() {
        let count = 2000;
        let output = super::spell_empty_code_runs("[\0] ".repeat(count));
        let lengths: Vec<_> = output
            .split("[")
            .skip(1)
            .map(|part| part.find(']').unwrap())
            .collect();
        assert_eq!(lengths, (2..count + 2).rev().collect::<Vec<_>>());
    }

    use std::cell::Cell;

    thread_local! {
        pub(super) static PROBES: Cell<usize> = const { Cell::new(0) };
        pub(super) static WINDOW_PROBES: Cell<usize> = const { Cell::new(0) };
        pub(super) static PARSED_BYTES: Cell<usize> = const { Cell::new(0) };
        pub(super) static WHOLE_DOCUMENT_PROBES: Cell<bool> = const { Cell::new(false) };
        pub(super) static PROBE_PARSES: Cell<usize> = const { Cell::new(0) };
    }

    /// Probes and parses of the escalation search while `html` is imported.
    fn search_cost(html: &str) -> (usize, usize) {
        PROBES.with(|n| n.set(0));
        PROBE_PARSES.with(|n| n.set(0));
        crate::html_import::html_to_carve(html, &Default::default()).expect("imports");
        (PROBES.with(Cell::get), PROBE_PARSES.with(Cell::get))
    }

    /// A literal `/b/` in every paragraph fails the minimal form, so the
    /// escalation search runs to its budget. A candidate the halving has
    /// already judged must not be parsed again.
    #[test]
    fn the_escalation_search_does_not_reparse_a_judged_candidate() {
        WHOLE_DOCUMENT_PROBES.with(|flag| flag.set(true));
        let (probes, parses) = search_cost(&"<p>a /b/ c</p>".repeat(64));
        WHOLE_DOCUMENT_PROBES.with(|flag| flag.set(false));
        assert!(probes > 0, "the search did not run");
        assert!(parses < probes, "{parses} parses for {probes} probes");
    }

    /// Documents' worth of source the escape narrowing re-parses for `html`.
    fn reparsed_documents(html: &str) -> f64 {
        PARSED_BYTES.with(|n| n.set(0));
        let written = crate::html_import::html_to_carve(html, &Default::default())
            .expect("imports")
            .value;
        PARSED_BYTES.with(Cell::get) as f64 / written.len() as f64
    }

    /// A page nested in containers, as imported web pages are, with a failing
    /// unit in every paragraph: each probe re-parses a window around the units
    /// it relaxes, not the whole document.
    #[test]
    fn the_escape_narrowing_reparses_windows_not_the_document() {
        let paragraphs = "<p>a /b/ c</p><p>plain d.</p>".repeat(128);
        let items = "<li><p>a /b/ c</p><p>plain d.</p></li>".repeat(128);
        let entries = "<dt>t</dt><dd><p>a /b/ c</p><p>plain d.</p></dd>".repeat(128);
        for body in [
            paragraphs,
            format!("<ul>{items}</ul>"),
            format!("<dl>{entries}</dl>"),
        ] {
            let html = format!("<div class=a><div class=b><div class=c>{body}</div></div></div>");
            let windowed = reparsed_documents(&html);
            WHOLE_DOCUMENT_PROBES.with(|flag| flag.set(true));
            let whole = reparsed_documents(&html);
            WHOLE_DOCUMENT_PROBES.with(|flag| flag.set(false));
            assert!(whole > 100.0, "the search did not run: {whole:.1}");
            assert!(windowed < 48.0, "{windowed:.1} documents re-parsed");
        }
    }

    /// The lone brackets the scan reaches through nodes that write no brackets
    /// of their own are escaped in the minimal form already, including ruby
    /// flattened inside emphasis, whose nodes the writer clones.
    #[test]
    fn the_bracket_scan_reaches_through_transparent_nodes() {
        let session = &super::RenderSession::new();
        for (inline, escaped) in [
            (
                r#"{"type":"substitution","old":[{"type":"text","value":"["}],"new":[{"type":"text","value":"x"}]}"#,
                "\\[",
            ),
            (
                r#"{"type":"insert","children":[{"type":"text","value":"["}]}"#,
                "\\[",
            ),
            (
                r#"{"type":"delete","children":[{"type":"text","value":"a ]"}]}"#,
                "\\]",
            ),
            (
                r#"{"type":"abbreviation","abbr":"[a","expansion":"x"}"#,
                "\\[",
            ),
            (
                r#"{"type":"small_caps","children":[{"type":"text","value":"a ["}]}"#,
                "\\[",
            ),
            (
                r#"{"type":"emphasis","children":[{"type":"ruby","pairs":[{"base":[{"type":"text","value":"["}],"annotation":[{"type":"text","value":"r"}]}]}]}"#,
                "\\[",
            ),
            // Cloned twice: each ruby flattening clones the nested one again.
            (
                r#"{"type":"emphasis","children":[{"type":"ruby","pairs":[{"base":[{"type":"strong","children":[{"type":"ruby","pairs":[{"base":[{"type":"text","value":"["}],"annotation":[{"type":"text","value":"r"}]}]}]}],"annotation":[{"type":"text","value":"s"}]}]}]}"#,
                "\\[",
            ),
            // A scope keyed by clones is not redirected to their sources.
            (
                r#"{"type":"emphasis","children":[{"type":"ruby","pairs":[{"base":[{"type":"text","value":"x"}],"annotation":[{"type":"text","value":"r"}]}]},{"type":"small_caps","attrs":{"classes":["sc"]},"children":[{"type":"text","value":"["}]}]}"#,
                "\\[",
            ),
        ] {
            let json = format!(
                r#"{{"type":"document","children":[{{"type":"paragraph","children":[{{"type":"span","attrs":{{"classes":["c"]}},"children":[{inline}]}}]}}],"srcByteLength":0}}"#
            );
            let doc = crate::from_json(&json).expect("decode AST");
            let minimal = super::render_with_escapes(session, &doc, super::EscapeMode::Minimal);
            assert!(minimal.contains(escaped), "{inline}: {minimal}");
        }
    }

    /// PART 11 §5 puts a lone bracket in the minimal form, so a page of
    /// `[edit]` markers needs no search at all.
    #[test]
    fn a_lone_bracket_needs_no_search() {
        let paragraph =
            "<p><span class=b>[</span><a href=/x>edit</a><span class=b>]</span> a [x](y) b.</p>";
        let (probes, _) = search_cost(&paragraph.repeat(64));
        assert_eq!(probes, 0);
    }
}

#[cfg(test)]
mod raw_html_fence_tests {
    #[test]
    fn raw_html_backticks_survive_writing() {
        for content in [
            "<a href=\"`\">",
            "<span title=\"``\">x</span>",
            "<!-- `x` -->",
            "`x`",
        ] {
            let value = serde_json::json!({
                "type": "document", "srcByteLength": 0,
                "children": [{"type": "paragraph", "children": [{
                    "type": "raw_inline", "format": "html", "content": content
                }]}]
            });
            let document = crate::from_json(&value.to_string()).unwrap();
            let written = crate::render_carve(&document).unwrap();
            let parsed = crate::parse(&written);
            let crate::BlockNode::Paragraph(paragraph) = &parsed.children[0] else {
                panic!("{written}");
            };
            let crate::InlineNode::RawInline(raw) = &paragraph.children[0] else {
                panic!("{written}");
            };
            assert_eq!(raw.content, content);
            assert_eq!(
                crate::to_html(&written),
                format!("<p>{content}</p>"),
                "{written}"
            );
        }
    }
    #[test]
    fn multiline_raw_html_keeps_importing() {
        for source in [
            "a <span\n  class=\"x\">y</span> b",
            "a <b \nclass=\"x\">y</b> c",
        ] {
            let written = crate::markdown_to_carve(source);
            assert!(
                crate::to_html(&written).contains("class=\"x\">y</"),
                "{written}"
            );
        }
    }
}
