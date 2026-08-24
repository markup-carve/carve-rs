use crate::ast::*;
use crate::ast_json::block_pos;
use crate::render::MAX_RENDER_DEPTH;
use crate::render_text::{trim_end_non_nbsp, trim_non_nbsp};
use std::collections::{HashMap, HashSet};

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
    colon_fence_depth: usize,
    /// Inside a table cell, where a leading `^` cannot open a caption: a
    /// caption marker is a BLOCK line, and a cell's content is not one.
    table_cell_depth: usize,
    /// Inside an inline note's content, where PART 9 §16 disables note
    /// recognition at every depth - so a `^[` written there opens nothing and
    /// needs no escape.
    note_content_depth: usize,
    after_caption_host: bool,
    paragraph_starts_after_caption_host: bool,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EscapeMode {
    Minimal,
    Conservative,
}

/// Render a tree as canonical Carve source.
///
/// Returns [`crate::RenderCarveError::Depth`] when a hand-built or ingested tree
/// reaches the render ceiling, or [`crate::RenderCarveError::SourceUnspellable`]
/// when emitting source would change the tree. Parser-produced trees cannot
/// contain either condition.
pub fn render_carve(doc: &Document) -> Result<String, crate::RenderCarveError> {
    let source_watch = crate::render_carve_error::SourceSpellWatch::new();
    let watch = crate::render_depth::RenderDepthWatch::new();
    let output = protect_leading_bom(render_carve_unguarded(doc));
    if let Some(error) = source_watch.error() {
        return Err(error);
    }
    watch.into_result(output).map_err(Into::into)
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

thread_local! {
    /// Heading ids that a fresh parse would re-derive, so the writer must not
    /// turn them into source.
    ///
    /// PART 12 §5 publishes a heading's slugged id and PART 11 §1 writes the
    /// DOCUMENT back, so the two have to be told apart: an AUTHORED id carries
    /// an `#id` slot, a GENERATED one carries none. Dropping every unslotted id
    /// would be wrong as well - an ingested tree whose heading text was edited
    /// carries an id the text no longer slugs to, and there the id is the only
    /// place that information lives. So the test is MINIMAL FORM, the same one
    /// PART 11 §4 uses for escapes: write it only where dropping it would
    /// change the document (carve-js#741).
    static REDUNDANT_IDS: std::cell::RefCell<std::collections::BTreeSet<String>> =
        const { std::cell::RefCell::new(std::collections::BTreeSet::new()) };
}

/// The ids a fresh parse would assign, for headings that carry an unslotted id.
///
/// Computed with `assigned_heading_ids` - the pass the renderer itself uses -
/// over a copy with those ids removed, so this cannot answer differently from
/// the parse it is predicting.
pub(crate) fn redundant_heading_ids(doc: &Document) -> std::collections::BTreeSet<String> {
    let mut stripped = doc.clone();
    let mut had_any = false;
    // The SAME two walks `assigned_heading_ids` makes, in the same order:
    // `doc.children`, then the footnote definitions. Both halves are needed and
    // in this order, because the answer is read off a POSITIONAL zip against
    // that pass. Stopping at `doc.children` truncated the zip, so no heading in
    // a footnote definition could ever be answered and every one of them was
    // written back as authored source: `[^a]: # h` returned as `[^a]: {#h}`
    // over an indented `# h`, the carve-rs#1105 shape in the one place this
    // predicate could not see.
    strip_generated_ids(&mut stripped.children, &mut had_any);
    for blocks in stripped.footnote_defs.values_mut() {
        strip_generated_ids(blocks, &mut had_any);
    }
    if !had_any {
        return std::collections::BTreeSet::new();
    }
    let fresh = crate::document_ids::assigned_heading_ids(
        &stripped,
        crate::extension::HeadingIdOptions::PLAIN,
    );
    let mut present = Vec::new();
    collect_heading_ids(&doc.children, &mut present);
    for blocks in doc.footnote_defs.values() {
        collect_heading_ids(blocks, &mut present);
    }
    present
        .into_iter()
        .zip(fresh)
        .filter_map(|(current, fresh)| match current {
            Some(id) if id == fresh => Some(id),
            _ => None,
        })
        .collect()
}

fn strip_generated_ids(blocks: &mut [BlockNode], had_any: &mut bool) {
    for block in blocks.iter_mut() {
        if let BlockNode::Heading(h) = block {
            if let Some(attrs) = h.attrs.as_mut() {
                let unslotted = attrs.id.is_some()
                    && !attrs.order.iter().any(|slot| matches!(slot, AttrSlot::Id));
                if unslotted {
                    attrs.id = None;
                    *had_any = true;
                }
            }
        }
        match block {
            BlockNode::BlockQuote(b) => strip_generated_ids(&mut b.children, had_any),
            BlockNode::Div(d) => strip_generated_ids(&mut d.children, had_any),
            BlockNode::Admonition(a) => strip_generated_ids(&mut a.children, had_any),
            BlockNode::List(l) => {
                for item in l.items.iter_mut() {
                    strip_generated_ids(&mut item.children, had_any);
                }
            }
            BlockNode::Figure(f) => {
                if let FigureTarget::BlockQuote(b) = &mut *f.target {
                    strip_generated_ids(&mut b.children, had_any);
                }
            }
            BlockNode::FigureGroup(g) => strip_generated_ids(&mut g.children, had_any),
            BlockNode::DefinitionList(dl) => {
                for entry in dl.items.iter_mut() {
                    for definition in entry.definitions.iter_mut() {
                        strip_generated_ids(&mut definition.children, had_any);
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_heading_ids(blocks: &[BlockNode], out: &mut Vec<Option<String>>) {
    for block in blocks {
        if let BlockNode::Heading(h) = block {
            out.push(h.attrs.as_ref().and_then(|a| a.id.clone()));
        }
        match block {
            BlockNode::BlockQuote(b) => collect_heading_ids(&b.children, out),
            BlockNode::Div(d) => collect_heading_ids(&d.children, out),
            BlockNode::Admonition(a) => collect_heading_ids(&a.children, out),
            BlockNode::List(l) => {
                for item in l.items.iter() {
                    collect_heading_ids(&item.children, out);
                }
            }
            BlockNode::Figure(f) => {
                if let FigureTarget::BlockQuote(b) = &*f.target {
                    collect_heading_ids(&b.children, out);
                }
            }
            BlockNode::FigureGroup(g) => collect_heading_ids(&g.children, out),
            BlockNode::DefinitionList(dl) => {
                for entry in dl.items.iter() {
                    for definition in entry.definitions.iter() {
                        collect_heading_ids(&definition.children, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn render_carve_unguarded(doc: &Document) -> String {
    // One render with the default sentinels. If the document turns out to
    // contain one of them itself, the counts disagree and the whole render is
    // repeated with a character it does not contain (see SENTINELS). Only a
    // document that actually holds a private-use sentinel pays for the second
    // pass, and nothing else changes: the retry runs the same code.
    let first = render_carve_once(doc);
    let current = SENTINELS.with(|s| s.get());
    let inserted = INSERTED.with(|c| c.get());
    let seen = SEEN.with(|c| c.get());
    if (0..SENTINEL_COUNT).all(|i| seen[i] <= inserted[i]) {
        return first;
    }
    // Choose against the STAGED text: `first` has been through restore, so an
    // authored occurrence is no longer visible in it.
    let staged = STAGED.with(|c| c.borrow().clone());
    let mut next = current;
    for i in 0..SENTINEL_COUNT {
        if seen[i] > inserted[i] {
            next[i] = free_sentinel(&staged, &next);
        }
    }
    SENTINELS.with(|s| s.set(next));
    let second = render_carve_once(doc);
    SENTINELS.with(|s| s.set(SENTINEL_DEFAULTS));
    second
}

/// One full render, with the insertion counters reset for it.
fn render_carve_once(doc: &Document) -> String {
    let redundant = redundant_heading_ids(doc);
    REDUNDANT_IDS.with(|cell| *cell.borrow_mut() = redundant);
    INSERTED.with(|c| c.set([0; SENTINEL_COUNT]));
    SEEN.with(|c| c.set([0; SENTINEL_COUNT]));
    STAGED.with(|c| c.borrow_mut().clear());
    let minimal = render_with_escapes(doc, EscapeMode::Minimal);
    let conservative = render_with_escapes(doc, EscapeMode::Conservative);
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
    narrow_escalation(doc, conservative, conservative_tree)
}

/// The conservative form of the units that need it, and the minimal form of
/// every other unit (PART 11 §2b).
///
/// WHY THIS IS A SEARCH AND NOT A LOOKUP. The comparison stays document-scoped
/// -- §4's argument holds, a unit re-parsed alone has lost the document's link
/// reference and footnote definitions -- so what a failure reports is THAT the
/// document changed, never WHERE. The unit is found by trying: start from the
/// conservative form, which is known to hold, and hand each unit back its
/// minimal form only while the whole document still re-parses to the same tree.
/// Every state this walks through is verified, and the one returned is the last
/// that passed.
///
/// HALVED RATHER THAN SWEPT, because a document is mostly units that need
/// nothing. A group is offered its minimal form all at once and only split when
/// that fails, so a document with one failing unit costs about log(n) renders
/// instead of n.
///
/// THE FIRST RENDER IS A CONTROL. With every unit escalated this must reproduce
/// the conservative form byte for byte; if it does not, the selection is
/// deciding something other than the escape mode -- a unit the walk did not
/// reach, for instance -- and the document-scoped form is returned rather than
/// a narrowing built on a state that is not what it claims.
fn narrow_escalation(
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
    let discovered = render_with_escapes(doc, EscapeMode::Conservative);
    let total = UNIT_COUNTER.with(|c| c.get());
    if discovered != conservative || total == 0 {
        return conservative;
    }

    let all: Vec<usize> = (1..=total).collect();
    ESCALATED_UNITS.with(|cell| *cell.borrow_mut() = Some(all.iter().copied().collect()));
    // THE CONTROL RENDER LOGS WHICH UNITS THE WRITER ACTUALLY ASKS ABOUT, so the
    // search below can skip the ones it cannot move. The walk that hands out
    // ordinals reaches every node that COULD carry an escaped character; the
    // units that DO are whatever the writer's own escape arms charge a character
    // to, and only those read `ESCALATED_UNITS`. A unit the writer never asks
    // about renders the same bytes in or out of the set, so offering it its
    // minimal form is a render and a parse spent to learn nothing.
    //
    // The gap is not small on nested documents. On the deepest corpus document
    // -- 203 nested colon fences, whose overflow past the nesting cap is the
    // text the writer must keep from re-opening a div -- the walk hands out 208
    // ordinals and the writer asks about SEVEN. The other 201 were halved over
    // at a render and a parse each, and that document's own output is 21x its
    // source (a colon fence widens by one per level, PART 9 §12), so each of
    // those cost about what parsing 42 KB costs.
    //
    // Seven where carve-js and carve-php ask about four, because
    // `next_unit_escape_mode` charges the ordinal a node has not claimed yet.
    // A superset, and the safe direction: a unit logged that never writes is a
    // group the search relaxes in one render, where a unit MISSED would be one
    // it can no longer offer its minimal form.
    //
    // Logging it rather than predicting it is the same choice the ordinal walk
    // makes and for the same reason: the set is whatever the arms visit, so an
    // arm that grows a new escape cannot fall out of the search. And a unit
    // wrongly left out cannot produce wrong output -- every state the search
    // returns is re-parsed against `conservative_tree` below, exactly as before.
    ASKED_UNITS.with(|cell| *cell.borrow_mut() = Some(HashSet::new()));
    let control = render_with_escapes(doc, EscapeMode::Conservative);
    let asked = ASKED_UNITS
        .with(|cell| cell.borrow_mut().take())
        .unwrap_or_default();
    if control != conservative {
        ESCALATED_UNITS.with(|cell| *cell.borrow_mut() = None);
        return conservative;
    }
    let units: Vec<usize> = all
        .into_iter()
        .filter(|unit| asked.contains(unit))
        .collect();
    let mut best = control;
    // No guard for an EMPTY `units`: `relax_units` returns on an empty group,
    // and a check here would be one no corpus document can reach -- the control
    // render asks about a unit for every byte the two forms differ in, and they
    // differ or this is not running.
    // Eight times the depth of the halving, which is what narrowing four
    // independent failing units costs. See `budget` on `relax_units`.
    let mut budget = 8 * (usize::BITS - units.len().leading_zeros()) as usize + 8;
    relax_units(doc, &units, &conservative_tree, &mut best, &mut budget);
    // PART 11 §2 TAKES THE DECISION PER OPENER OCCURRENCE, and a unit is still
    // ONE KNOB: a unit that fails is written conservatively IN FULL, so every
    // candidate character beside the one that needed it is escaped for nothing
    // -- `\{\.note\}` where §2 wants `\{.note}`. §2b bounds how far the fallback
    // reaches; this is what is left inside the bound (markup-carve/carve#1533).
    narrow_occurrences(doc, &conservative_tree, &mut best);
    ESCALATED_UNITS.with(|cell| *cell.borrow_mut() = None);
    best
}

/// The candidate escapes an escalated unit can still hand back, one occurrence
/// at a time (PART 11 §2).
///
/// SAME SEARCH, ONE LEVEL FINER. The comparison is still document-scoped, so a
/// failure still reports THAT the document changed and never WHERE; the
/// occurrence is found by trying, and every state kept is one that re-parsed to
/// the tree the conservative form parses to.
///
/// THE OCCURRENCES ARE LOGGED, NOT PREDICTED. A candidate site is whatever the
/// writer's own escape arms visit, so they are collected by rendering once with
/// the log switched on rather than by a second enumeration here that could
/// drift from the one that emits.
///
/// THE FIRST RENDER IS A CONTROL, as it is one level up. With nothing relaxed
/// it must reproduce the state the unit search settled on byte for byte; if
/// logging changed what was written, the unit-scoped answer stands rather than
/// a narrowing built on a pass that is not the pass being measured.
///
/// BOUNDED THE SAME WAY AND FOR THE SAME REASON. A group holding no failing
/// occurrence is relaxed in one render, so a document with a handful of them
/// costs about log(n) renders -- but a document where every occurrence is load
/// bearing drives the halving to its leaves and pays a render and a parse per
/// occurrence, which is a render of the whole document per escaped character. A
/// paragraph of indented table rows is exactly that, and it is ordinary input
/// rather than an adversarial one. The OUTPUT is unchanged where the budget
/// binds: those occurrences are the opener runs §2 requires escaped in full.
fn narrow_occurrences(doc: &Document, conservative_tree: &Document, best: &mut String) {
    let unit_scoped = best.clone();
    RELAXED_OCCURRENCES.with(|cell| *cell.borrow_mut() = Some(HashSet::new()));
    OCCURRENCE_LOG.with(|cell| *cell.borrow_mut() = Some(Vec::new()));
    let control = render_with_escapes(doc, EscapeMode::Conservative);
    let occurrences = OCCURRENCE_LOG
        .with(|cell| cell.borrow_mut().take())
        .unwrap_or_default();
    if control != unit_scoped || occurrences.is_empty() {
        RELAXED_OCCURRENCES.with(|cell| *cell.borrow_mut() = None);
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
    let mut budget = 8 * (usize::BITS - order.len().leading_zeros()) as usize + 8;
    relax_occurrences(doc, &order, conservative_tree, best, &mut budget);
    // AND THEN ONE SWEEP OF WHAT IS LEFT, because the halving is not a FIXPOINT.
    // Relaxing occurrences is not monotone: an occurrence rejected while a
    // neighbour was still escaped can be free once that neighbour is relaxed,
    // and the halving never revisits a group it has descended past. Corpus 160
    // is the case -- the closing `:::` line cannot go bare while the OPENING one
    // is escaped, because then it is the only fence marker on the page, and it
    // can once the opener is bare. The sweep spends the same budget, so where
    // the budget is already gone it costs nothing, which is the pathological
    // document.
    for key in &order {
        if budget == 0 {
            break;
        }
        if RELAXED_OCCURRENCES
            .with(|cell| cell.borrow().as_ref().is_some_and(|set| set.contains(key)))
        {
            continue;
        }
        relax_occurrences(
            doc,
            std::slice::from_ref(key),
            conservative_tree,
            best,
            &mut budget,
        );
    }
    RELAXED_OCCURRENCES.with(|cell| *cell.borrow_mut() = None);
}

/// Hand `group` its bare form where the document still holds, halving the group
/// on failure.
fn relax_occurrences(
    doc: &Document,
    group: &[Occurrence],
    conservative_tree: &Document,
    best: &mut String,
    budget: &mut usize,
) {
    if group.is_empty() || *budget == 0 {
        return;
    }
    *budget -= 1;
    set_relaxed(group, true);
    let candidate = render_with_escapes(doc, EscapeMode::Conservative);
    if comparable_tree(&candidate).as_ref() == Some(conservative_tree) {
        *best = candidate;
        return;
    }
    set_relaxed(group, false);
    if group.len() == 1 {
        return;
    }
    let half = group.len() / 2;
    relax_occurrences(doc, &group[..half], conservative_tree, best, budget);
    relax_occurrences(doc, &group[half..], conservative_tree, best, budget);
}

fn set_relaxed(group: &[Occurrence], relaxed: bool) {
    RELAXED_OCCURRENCES.with(|cell| {
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
/// `best` carries the render of the CURRENT escalation set, so the caller always
/// holds bytes that were verified: an accepted relaxation replaces it, a
/// rejected one restores the set it was measured against.
///
/// `budget` BOUNDS THE SEARCH, because its cost is proportional to how many
/// units FAIL. A group holding no failing unit is relaxed in one render, so a
/// document with a handful of them costs about log(n) renders -- but one where
/// nearly every unit fails drives the recursion to its leaves and pays a render
/// and a parse per unit, which is quadratic in the document.
///
/// Such a document gains almost nothing from narrowing: it IS the conservative
/// form, arrived at because every block needed it. So the search stops when the
/// budget runs out and returns the state it has reached, which is verified like
/// every other -- the escalation is wider than §2b's minimum there, never
/// narrower, and no document's output can be wrong for it.
///
/// MEASURED over the 1358 pinned corpus documents: 51 reach the search at all,
/// and once the control render has narrowed the candidates to the units the
/// writer asks about, the widest holds eight and none holds more.
fn relax_units(
    doc: &Document,
    units: &[usize],
    conservative_tree: &Document,
    best: &mut String,
    budget: &mut usize,
) {
    if units.is_empty() || *budget == 0 {
        return;
    }
    *budget -= 1;
    set_escalated(units, false);
    let candidate = render_with_escapes(doc, EscapeMode::Conservative);
    if comparable_tree(&candidate).as_ref() == Some(conservative_tree) {
        *best = candidate;
        return;
    }
    set_escalated(units, true);
    if units.len() == 1 {
        return;
    }
    let half = units.len() / 2;
    relax_units(doc, &units[..half], conservative_tree, best, budget);
    relax_units(doc, &units[half..], conservative_tree, best, budget);
}

fn set_escalated(units: &[usize], escalated: bool) {
    ESCALATED_UNITS.with(|cell| {
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
    std::panic::catch_unwind(|| comparable_document(crate::parse::parse_for_carve_shape(source)))
        .ok()
}

/// The lines every EMPTIED marker-line container sits on, anywhere in the tree.
///
/// Empty is the only case that matters: a description holding content writes
/// that content and needs nothing from here. Collecting the set first keeps the
/// map below empty - and its clones unmade - for every document that has no
/// such description, which is all but two of the 638 corpus documents.
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
            BlockNode::Div(div) => emptied_marker_lines_at(&div.children, list_depth, into),
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
            BlockNode::Extension(extension) => {
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

thread_local! {
    /// Whether a HYPHEN-spelled thematic break would be misread in this render.
    ///
    /// PART 11 §6 writes the marker the author used, now that the AST records it
    /// (carve#976, carve-rs#843). The one document that gets another spelling is
    /// the one whose emitted bytes would open a frontmatter block it does not
    /// have, and `render_with_escapes` is where that is decided.
    static HYPHEN_BREAKS_ARE_UNSAFE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Render, and fall back to a break spelling that cannot be read as frontmatter
/// when the finished bytes would be.
///
/// A frontmatter block is an opening fence AT BYTE 0 plus a bare `---` CLOSER
/// anywhere below it, so the collision is a property of the WHOLE emitted
/// document rather than of its first line. Two writer decisions reach it, and
/// this seam is the only thing they share:
///
/// - a break the author spelled `---` opens the document and gains a closer from
///   any later `---` break. `---` / blank / `---` is an EMPTY frontmatter block
///   rendering nothing where the input rendered two rules (carve-rs#732).
/// - §7 writes a hoisted link or footnote definition after the body, promoting
///   whatever stood second to byte 0 - and that block can be a PARAGRAPH whose
///   first line is `---yaml`-shaped. NO HEAD-OF-DOCUMENT RESPELLING REPAIRS
///   THAT ONE, because the paragraph's text is not the writer's to change. It is
///   saved by respelling the CLOSER instead, which is why the fallback moves
///   every hyphen break in the document rather than the one at the head
///   (carve-rs#819).
///
/// The second is why the previous seam is replaced rather than extended. That
/// one asked whether the FIRST RENDERED BLOCK was the string `---` and rewrote
/// that single line; a `---yaml` paragraph is not that block, and the break that
/// has to move is four lines further down.
///
/// THE DEPARTURE IS THE SMALLEST ONE THAT RESTORES §1, which is what §1a asks
/// for: only the HYPHEN spelling can be read as a fence, so only hyphen breaks
/// move and every other authored marker survives untouched. A document whose
/// breaks are all `***` or `___` never reaches the second pass at all.
///
/// The FINISHED bytes are handed to the PARSER'S own opener test, twice: once to
/// ask whether the authored spelling is misread, and once to confirm the
/// fallback is not. A document still misread with `***` keeps the authored
/// spelling rather than paying a respelling that buys nothing, which is the case
/// where the `---` closer came from somewhere other than a break, such as the
/// inside of a fenced block.
///
/// A leading `---` break with nothing below it to close a block keeps its
/// marker, which is what corpus
/// `132-thematic-break-requires-contiguous-markers-4` asks for. It is a CONTROL:
/// no mutation of this fallback moves it.
///
/// The `doc.frontmatter` arm is a COST GATE, not a correctness one, and saying
/// so is the honest reading. A document that really carries frontmatter has it
/// written by `render_frontmatter`, whose closer is not a break, so the fallback
/// pass opens frontmatter too and the authored form is returned anyway. Removing
/// the arm changes no output, only the number of renders paid by every document
/// that has frontmatter. Verified by mutation.
///
/// The test runs on the output of `normalize`, which is where `restore_verbatim`
/// turns staged content back into the bytes the next parse will actually see.
fn render_with_escapes(doc: &Document, escape_mode: EscapeMode) -> String {
    let authored = render_with_escapes_once(doc, escape_mode);
    if !doc.frontmatter.is_empty() || !crate::parse::opens_frontmatter(&authored) {
        return authored;
    }
    HYPHEN_BREAKS_ARE_UNSAFE.with(|unsafe_| unsafe_.set(true));
    let fallback = render_with_escapes_once(doc, escape_mode);
    HYPHEN_BREAKS_ARE_UNSAFE.with(|unsafe_| unsafe_.set(false));
    if crate::parse::opens_frontmatter(&fallback) {
        authored
    } else {
        fallback
    }
}

fn render_with_escapes_once(doc: &Document, escape_mode: EscapeMode) -> String {
    // PER PASS, like `written_in_place` above and for the same reason: the unit
    // ordinals name positions in THIS walk, and a counter carried across passes
    // would name a different node in each of them.
    UNIT_COUNTER.with(|c| c.set(0));
    // PER PASS for the same reason: `render_with_escapes` can render twice for
    // the frontmatter fallback, and a counter carried across the two would
    // number the second pass's runs on from the end of the first.
    ESCAPE_CALL_INDEXES.with(|cell| cell.borrow_mut().clear());
    OCCURRENCE_LOG.with(|cell| {
        if let Some(log) = cell.borrow_mut().as_mut() {
            log.clear();
        }
    });
    let mut ctx = CarveContext {
        block_depth: 0,
        inline_depth: 0,
        list_depth: 0,
        line_block_depth: 0,
        colon_fence_depth: 0,
        table_cell_depth: 0,
        note_content_depth: 0,
        after_caption_host: false,
        paragraph_starts_after_caption_host: false,
        escape_mode,
        escape_unit: 0,
        definitions_by_line: definitions_by_description_line(doc),
        written_in_place: HashSet::new(),
    };
    let mut parts = Vec::new();
    if !doc.frontmatter.is_empty() {
        parts.push(render_frontmatter(&doc.frontmatter));
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
    for entry in crate::ast_json::ordered_document_entries(doc, &footnote_defs) {
        let text = match entry {
            crate::ast_json::DocEntry::Block(child) => {
                ctx.paragraph_starts_after_caption_host = ctx.after_caption_host;
                let text = render_block(child, &mut ctx);
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
                    render_footnote_def_source(label, blocks, &mut ctx)
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
        if !writes_nothing(&text) {
            rendered.push(if separated_from_previous && !rendered.is_empty() {
                hard_list_boundary(&text)
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
    normalize(&parts.join("\n\n"))
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
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticSubstitute(_)
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
        BlockNode::Div(d) => {
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
        BlockNode::Extension(e) => {
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
                }
            }
        }
        FigureTarget::Paragraph(p) => normalize_escapes_inlines(&mut p.children),
        FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
    }
}

/// Whether two adjacent sibling lists would read back as ONE list.
///
/// PART 9 §11 N1's axes: the kind, the plain-vs-task classification, and the
/// marker character the author chose -- the ordered delimiter and dialect, or
/// the bullet. Where any of them differs the lists separate on their own and
/// the writer owes them nothing, which is what carve#286 established.
fn lists_would_merge(a: &List, b: &List) -> bool {
    if a.ordered != b.ordered || is_task_list(a) != is_task_list(b) {
        return false;
    }
    if a.ordered {
        return a.delim.unwrap_or('.') == b.delim.unwrap_or('.') && a.ol_type == b.ol_type;
    }
    a.bullet_char.unwrap_or('-') == b.bullet_char.unwrap_or('-')
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
fn hard_list_boundary(text: &str) -> String {
    let blank = verbatim_blank();
    note_inserted(S_BLANK);
    note_inserted(S_BLANK);
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
fn hard_list_boundary_in_a_tight_item(text: &str) -> String {
    let blank = verbatim_blank();
    note_inserted(S_BLANK);
    note_inserted(S_BLANK);
    note_inserted(S_BLANK);
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

/// Whether a sub-list written at the item's content column needs a blank line
/// above it to open at all.
///
/// THE MARKER COLUMN. A block attached by §17 L3's marker sits at column 0, and
/// a sub-list at the item's content column below it is INDENTED under an open
/// paragraph - lazy continuation, so the list never opens and its markers come
/// back as text.
///
/// A BLOCKQUOTE. It takes any non-blank line below it as lazy continuation,
/// bullet line included, so an item holding a quote and then a bullet at the
/// content column came back as a quote whose paragraph carries the bullet line
/// as its own text. That shape holds no §11 N1a boundary at all: it failed on
/// its own account before markup-carve/carve#1501, and the same rule settles it.
///
/// A PARAGRAPH BELOW A SUB-LIST THAT ALREADY OPENED. Once a sub-list has opened
/// at the item's content column, a bullet written at that column below a
/// paragraph joins THAT list instead of opening under the paragraph - so the
/// paragraph keeps the line and the list keeps the marker. Without an earlier
/// sub-list the same two lines open a list, which is why this is conditional
/// rather than a blanket blank line after every paragraph: writing one there
/// would re-spell every nested list in the corpus.
///
/// A BLANK LINE IS SAFE HERE. It loosens an item only before a PARAGRAPH; before
/// a sub-list the item stays tight, which is why an item whose sub-list follows
/// a blank line and one whose sub-list follows the marker line directly are the
/// same document.
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
///
/// The empty string is the obvious member. A run of spaces, tabs and newlines
/// is the one that ambushed both callers: the writer trims every line's trailing
/// run and then collapses the blank run around it, so such a block reaches the
/// output as nothing at all - but `is_empty` called it content, and the two
/// callers below decide against that answer what stands BETWEEN two lists.
///
/// The cost was §11 N1a's hard boundary. Two lists with a whitespace-only
/// paragraph between them are two lists, and both callers concluded the
/// paragraph separated them, so neither wrote the boundary; the paragraph then
/// trimmed away and the lists merged on re-parse. An EMPTY paragraph in the same
/// position was handled correctly all along, which is the tell that this is one
/// predicate's defect rather than a question about what a blank paragraph means
/// (markup-carve/carve-rs#1290).
///
/// U+00A0 IS CONTENT and is deliberately not swept: `trim_non_nbsp` is the
/// writer's own trimming and preserves it, a lone U+00A0 line parses back as a
/// paragraph, so a block holding one really does put something in the source.
/// Sharing the helper with the trimming is what keeps the two answers equal - a
/// hand-written character set here would drift from it silently.
fn writes_nothing(text: &str) -> bool {
    trim_non_nbsp(text).is_empty()
}

fn render_blocks(blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
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
    for block in blocks {
        ctx.paragraph_starts_after_caption_host = ctx.after_caption_host;
        let text = render_block(block, ctx);
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
                hard_list_boundary(&text)
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

fn render_inside_colon_container(blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    ctx.colon_fence_depth += 1;
    let body = render_blocks(blocks, ctx);
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
        DefinitionAtLine::Link(def) => render_block(&BlockNode::LinkReferenceDefinition(*def), ctx),
        DefinitionAtLine::Footnote(label, blocks) => {
            render_footnote_def_source(&label, &blocks, ctx)
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
fn definition_at_line(line: usize, ctx: &mut CarveContext) -> Option<String> {
    if ctx.written_in_place.contains(&line) {
        return None;
    }
    let definition = ctx.definitions_by_line.get(&line)?.clone();
    let written = match definition {
        DefinitionAtLine::Link(def) => render_block(&BlockNode::LinkReferenceDefinition(*def), ctx),
        DefinitionAtLine::Footnote(label, blocks) => {
            render_footnote_def_source(&label, &blocks, ctx)
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
fn marker_column() -> char {
    sentinel(S_MARKER_COLUMN)
}

fn at_marker_column(text: &str) -> String {
    let marker = marker_column();
    text.split('\n')
        .map(|line| {
            note_inserted(S_MARKER_COLUMN);
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
            left.ordered == right.ordered
                && left.delim == right.delim
                && left.bullet_char == right.bullet_char
                && left.ol_type == right.ol_type
        }
        _ => false,
    }
}

fn render_item_blocks(blocks: &[BlockNode], tight: bool, ctx: &mut CarveContext) -> String {
    if !tight {
        return render_blocks(blocks, ctx);
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
    for (index, block) in blocks.iter().enumerate() {
        let next = blocks.get(index + 1);
        let rendered = render_block(block, ctx);
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
            if let Some(written) = definition_in_gap(prev_block, block, ctx) {
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
        // §17 L3: a block after a paragraph needs its continuation marker
        // written back whenever the block's own first line would FOLD into that
        // paragraph. Indented under the item it is a lazy continuation of the
        // paragraph above (§10 I2), so the item comes back holding ONE block
        // where the author wrote two (carve#861).
        //
        // "ONLY A PARAGRAPH REACHES THIS" WAS FALSE, and the comment that said
        // so said why the corpus never caught it: it pins a fence and a quote,
        // both of which OPEN a block at the item's content column and so never
        // needed the marker. An IMAGE line opens nothing. `- x` / `+` /
        // `![a](i.png)` / `^ cap` came back as `- x` / `  ![a](i.png)` /
        // `  ^ cap`, where the image is no longer a standalone image paragraph,
        // PART 9 §4 does not attach the caption, and the `<figure>` is gone with
        // the caption left as literal text (carve-rs#819). The bare image
        // without a caption loses its block just the same, and that one is not
        // on the ticket.
        //
        // So the test is the PARSER'S OWN opener test on the bytes about to be
        // emitted, rather than a list of block kinds maintained by hand here -
        // the same deviation `markup-carve/carve#961` records for the leading
        // thematic break.
        let folds_into_the_paragraph_above = rendered
            .lines()
            .next()
            .is_some_and(crate::parse::line_starts_paragraph);
        // ONCE ONE CHILD IS AT THE MARKER COLUMN, EVERY LATER ONE IN THE RUN
        // MUST BE.
        //
        // The marker column is the ITEM's column, to the LEFT of the item's
        // content column, so a later child written at the content column is
        // INDENTED relative to the block above it - it becomes that block's
        // lazy continuation (§10 I2) or is absorbed into it outright. `- x` /
        // `+` / image / `+` / image came back as an item holding ONE image
        // paragraph with the second image's source as literal text; with a
        // caption on each, the second figure's whole source landed inside the
        // first one's `<figcaption>` (carve-rs#819).
        //
        // The condition is the PREVIOUS child's COLUMN, not its kind. Its kind
        // is what the arm above already asks, and that answers a different
        // question - whether this child folds into an open PARAGRAPH. This one
        // is about where the child sits relative to the block before it, which
        // no property of the child alone can decide.
        let continues_a_run_at_the_marker_column = prev.is_some() && prev_at_marker_column;
        // A LIST CHILD NEVER GOES TO THE MARKER COLUMN. The marker column is
        // column 0, which is where the list this item belongs to writes ITS
        // markers - so a sub-list put there is not attached to the item, it is
        // dissolved into the list around it, and the `+` above it is read as the
        // sibling item's own text. The ticket document came back as one flat
        // list of three items with both sub-lists and the boundary between them
        // gone (markup-carve/carve#1501). §17 L3's marker cannot help here: it
        // attaches a block that could not open at column 0 on its own, and a
        // list opens there in preference to being attached.
        //
        // So a sub-list is written at the item's CONTENT column, and what it
        // needs there is the right separator above it. Three shapes, one
        // question each - what would eat this list if nothing separated it:
        //
        //   - THE LIST ABOVE IT WOULD SWALLOW IT. Two sibling sub-lists whose
        //     markers match are one list when written adjacent, which is the
        //     whole of §11 N1's merge rule; N1a's boundary is the language's way
        //     of saying they are two, and §10i fixes its length at three blank
        //     lines.
        //   - THE BLOCK ABOVE IT SITS AT COLUMN 0, or is a BLOCKQUOTE. Either
        //     way a line at the item's content column is INDENTED under it and
        //     reads as its lazy continuation, so the list never opens. One blank
        //     line closes the block above without loosening the item - a blank
        //     line before a sub-list does not make a list loose, only a blank
        //     line before a paragraph does.
        //   - NOTHING ABOVE IT REACHES DOWN. Every other block kind was swept:
        //     heading, fence, table, break, div, admonition, and a sub-list with
        //     a different marker all close at their last line, and the list
        //     opens on the next one with no separator at all.
        if matches!(block, BlockNode::List(_)) {
            if !separated && prev.is_some_and(|previous| adjacent_blocks_merge(previous, block)) {
                out.push_str(&hard_list_boundary_in_a_tight_item(&rendered));
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
        if !separated
            && (continues_a_run_at_the_marker_column
                || next.is_some_and(|next_block| adjacent_blocks_merge(block, next_block))
                || (matches!(prev, Some(BlockNode::Paragraph(_)))
                    && folds_into_the_paragraph_above))
        {
            out.push_str(&at_marker_column("+"));
            out.push('\n');
            out.push_str(&at_marker_column(&rendered));
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
fn render_block(node: &BlockNode, ctx: &mut CarveContext) -> String {
    let previous = ctx.escape_unit;
    ctx.escape_unit = next_escape_unit();
    let out = render_block_body(node, ctx);
    ctx.escape_unit = previous;
    out
}

fn render_block_body(node: &BlockNode, ctx: &mut CarveContext) -> String {
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
            format!("[{}]: {}{title}{attrs}", def.label, def.href)
        }
        BlockNode::Heading(heading) => {
            // A heading is SINGLE-LINE (PART 2), so its text must not contain a
            // newline: emitting one would end the heading and silently re-parse
            // the remainder as a following block. No parse builds such a
            // heading, but an ingested AST can - PART 12 lets any inline sit in
            // a heading, break nodes included - so a break collapses to a
            // single space here rather than corrupting the document it is
            // written back to. Matches carve-js.
            let rendered = render_inlines(&heading.children, ctx);
            let text = collapse_breaks(trim_heading_edges(&rendered));
            let body = format!("{} {}", "#".repeat(heading.level as usize), text);
            // A generated id a fresh parse would re-derive is not the author's
            // source (carve-js#741); one it would not - an edited ingested tree -
            // is written, because the id lives nowhere else.
            let attrs = match heading.attrs.as_ref() {
                Some(attrs) => match attrs.id.as_ref() {
                    Some(id)
                        if !attrs.order.iter().any(|slot| matches!(slot, AttrSlot::Id))
                            && REDUNDANT_IDS.with(|cell| cell.borrow().contains(id)) =>
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
            let body = guard_thematic_break_lines(&render_inlines_with_caption(
                &paragraph.children,
                ctx,
                caption_can_open,
            ));
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
            with_block_attrs(
                &attrs,
                &format!(
                    "{fence}{info}\n{}\n{fence}",
                    protect_verbatim(&code.content)
                ),
            )
        }
        BlockNode::BlockQuote(quote) => {
            let inner =
                with_reset_colon_fence_depth(ctx, |ctx| render_blocks(&quote.children, ctx));
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
            let body = with_reset_colon_fence_depth(ctx, |ctx| render_list(list, ctx));
            with_loose_key(list_needs_loose_key(list, &body), &list.attrs, &body)
        }
        // PART 11 §6 writes the marker the author used, now that the AST
        // records it (carve#976, carve-rs#843). Only the HYPHEN spelling can be
        // read back as a frontmatter fence, so it is the only one the fallback
        // moves, and only for a document whose emitted bytes would really be
        // misread - see `render_with_escapes`, where that is decided.
        BlockNode::ThematicBreak(rule) => {
            let mut marker = rule.marker.unwrap_or('-');
            if marker == '-' && HYPHEN_BREAKS_ARE_UNSAFE.with(|unsafe_| unsafe_.get()) {
                marker = '*';
            }
            with_block_attrs(&rule.attrs, &marker.to_string().repeat(3))
        }
        BlockNode::Table(table) => {
            let mut attrs = table.attrs.clone();
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
                    .map(|c| c.width.map(|v| (v * 100.0).to_string()).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join(",");
                for (key, value) in [("aligns", align), ("valigns", valign), ("widths", widths)] {
                    if !value.chars().all(|c| c == ',') && !attrs.key_values.contains_key(key) {
                        attrs.key_values.insert(key.to_owned(), value);
                        attrs.order.push(AttrSlot::Key(key.to_owned()));
                    }
                }
            }
            with_block_attrs(&attrs, &render_table(table, ctx))
        }
        BlockNode::Admonition(admonition) => {
            let title = admonition
                .title
                .as_ref()
                .map(|title| format!(" \"{}\"", escape_quoted(&render_inlines(title, ctx))))
                .unwrap_or_default();
            let label = admonition
                .label
                .as_ref()
                .map(|label| format!(" [{}]", write_flat_bracket_run(label)))
                .unwrap_or_default();
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(&admonition.children, ctx);
            with_block_attrs(
                &admonition.attrs,
                &format!("{fence} {}{title}{label}\n{body}\n{fence}", admonition.kind),
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
            let body = render_inside_colon_container(&lb.children, ctx);
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
            let body = render_inside_colon_container(&div.children, ctx);
            with_block_attrs(&div.attrs, &format!("{fence}{label}\n{body}\n{fence}"))
        }
        BlockNode::DefinitionList(list) => {
            let body =
                with_reset_colon_fence_depth(ctx, |ctx| render_definition_list(&list.items, ctx));
            with_loose_key(list.loose, &list.attrs, &body)
        }
        BlockNode::Figure(figure) => with_block_attrs(&figure.attrs, &render_figure(figure, ctx)),
        BlockNode::FigureGroup(group) => {
            // §10g: the authored form - the attribute line where attributes
            // exist, the bare opener, the children, the closer at the opener's
            // width, and the group caption as a `^ ` line AFTER the closer.
            let fence = colon_fence_for(ctx);
            let body = render_inside_colon_container(&group.children, ctx);
            let caption = group
                .caption
                .as_ref()
                .map(|caption| format!("\n^ {}", render_inlines(caption, ctx)))
                .unwrap_or_default();
            with_block_attrs(
                &group.attrs,
                &format!("{fence} figure\n{body}\n{fence}{caption}"),
            )
        }
        BlockNode::BlockImage(image) => render_image(image),
        BlockNode::RawBlock(raw) => {
            let fence = safe_fence(&raw.content, 3);
            let all_blank = !raw.content.is_empty() && raw.content.chars().all(|c| c == '\n');
            let body = if all_blank {
                raw.content.clone()
            } else {
                protect_verbatim(&raw.content)
            };
            let separator = if all_blank { "" } else { "\n" };
            format!(
                "{fence}={}\n{}{separator}{fence}",
                escape_format(&raw.format),
                body
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
                format!("{{% {} %}}", comment.content)
            } else if comment.block {
                render_block_comment(&comment.content)
            } else if comment.content.is_empty() {
                // An empty comment writes its marker and nothing else. The
                // inline arm below has always done this; the block arm formatted
                // unconditionally and produced `%% `, a trailing space on a
                // writer-produced line that no clause asks for and that made
                // this engine disagree with carve-js on the corpus
                // (markup-carve/carve#1472).
                "%%".to_string()
            } else {
                format!("%% {}", comment.content)
            }
        }
        BlockNode::Extension(extension) => {
            with_block_attrs(&extension.attrs, &render_blocks(&extension.children, ctx))
        }
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
///
/// The decision procedure is markup-carve/carve#1639's, and it is a RE-PARSE
/// OVER THE DOCUMENT: write the body without the key, read it back, and emit
/// the key exactly where the container's own looseness field did not survive.
/// PART 11 §1's equality is taken over the document, not over the render, which
/// is why an HTML fixture cannot see this - the key is a render no-op, so the
/// `.fmt` sidecars are the expectation.
///
/// AN ITEM COUNT IS WRONG IN BOTH DIRECTIONS, and both were measured. It ADDS a
/// key to a one-item list whose blank line sits inside the item (corpus
/// `05-lists-11`, an ordered list whose single item holds two paragraphs), and
/// it OMITS the key from a definition list, whose entries count two or more
/// while a blank line between entries does not loosen a `<dl>` at any count.
///
/// `attrs` is the node's own set, which never contains `loose`: the parser
/// CONSUMED it, so the writer re-derives it from the tree rather than echoing
/// what the author wrote. That is what makes a redundant `{loose}` a no-op
/// through a format pass as well as through a render.
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

fn render_list(node: &List, ctx: &mut CarveContext) -> String {
    ctx.list_depth += 1;
    let mut out = String::new();
    let mut counter = node.start.unwrap_or(1);
    // The marker is semantic (§11: a different bullet char / ordered delim
    // starts a new list), so emit it as authored - normalizing would merge
    // adjacent sibling lists on re-parse (carve issue 286).
    let delim = node.delim.unwrap_or('.');
    let bullet = node.bullet_char.unwrap_or('-');
    for (idx, item) in node.items.iter().enumerate() {
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
        } else if let Some(checked) = item.checked {
            format!("{bullet} [{}] ", if checked { "x" } else { " " })
        } else {
            format!("{bullet} ")
        };
        let item_attrs = render_attrs(&item.attrs);
        if !item_attrs.is_empty() {
            prefix = if node.ordered {
                format!("{}{item_attrs} ", prefix.trim_end())
            } else if let Some(checked) = item.checked {
                format!(
                    "{bullet}{item_attrs} [{}] ",
                    if checked { "x" } else { " " }
                )
            } else {
                format!("{bullet}{item_attrs} ")
            };
        }
        let (mut content, restored_marker_definition) = if item.children.is_empty() {
            let restored = item
                .pos
                .as_ref()
                .and_then(|pos| definition_at_line(pos.start_line, ctx));
            let restored_marker_definition = restored.is_some();
            (restored.unwrap_or_default(), restored_marker_definition)
        } else {
            (render_item_blocks(&item.children, node.tight, ctx), false)
        };
        let trimmed_content = trim_non_nbsp(&content);
        if trimmed_content.is_empty()
            || (!restored_marker_definition
                && trimmed_content.starts_with("[^")
                && trimmed_content.contains(": "))
        {
            content = "+".to_string();
        }
        let content = trim_non_nbsp(&content).to_string();
        // COUNT THE MARKER-COLUMN TAGS STANDING IN THE ASSEMBLED ITEM, here and
        // not at restore time: this loop is what consumes them, so this is the
        // last moment an authored one is still visible. Counted over the whole
        // item rather than only over the lines the loop strips - a tag the item
        // dropped would otherwise hide an authored occurrence behind a matching
        // insertion count, and answering from the item's own text cannot.
        note_seen(S_MARKER_COLUMN, content.matches(marker_column()).count());
        let mut lines = if content.is_empty() {
            vec!["".to_string()]
        } else {
            content.split('\n').map(str::to_string).collect()
        };
        let first = lines.remove(0);
        out.push_str(&format!("{prefix}{first}\n"));
        let continuation = " ".repeat(prefix.len());
        for line in lines {
            if line.is_empty() || line.chars().eq([verbatim_blank()]) {
                // A blank continuation line is emitted EMPTY, never indented to
                // the content column: PART 11 section 7 forbids a whitespace-only
                // line, because editors and CI that strip trailing whitespace
                // rewrite one, and `fmt` would then report a diff on a file
                // nobody edited (carve#375).
                //
                // A blank line INSIDE verbatim content arrives as the sentinel
                // rather than as "", because `protect_verbatim` encodes it to
                // keep whole-document normalization off it. That made it look
                // like content here, so it was indented, and the indent stayed
                // behind when the sentinel was restored to nothing - a
                // whitespace-only line, from a code block in a list item
                // (carve-rs#440). The sentinel is written through UNindented so
                // it keeps protecting the line it stands for.
                out.push_str(&line);
                out.push('\n');
            } else if let Some(rest) = line.strip_prefix(marker_column()) {
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

fn render_definition_list(items: &[DefinitionItem], ctx: &mut CarveContext) -> String {
    let mut out = Vec::new();
    for item in items {
        for term in &item.terms {
            out.push(format!(":: {}", render_inlines(term, ctx)));
        }
        for def in &item.definitions {
            // An EMPTY description whose line carries a hoisted definition is one
            // the author wrote that definition on: write it back there
            // (markup-carve/carve#805). Without this the line came out as a bare
            // `:`, which re-parses into the term above it.
            if def.children.is_empty() {
                let line = def.pos.as_ref().map(|pos| pos.start_line);
                let written = line.and_then(|line| definition_at_line(line, ctx));
                if let Some(written) = written {
                    let mut written_lines = written.split('\n');
                    out.push(format!(":  {}", written_lines.next().unwrap_or_default()));
                    // A footnote body can be multi-line; its continuation lines
                    // carry the body's own indent and sit under the description.
                    for written_line in written_lines {
                        out.push(format!("   {written_line}"));
                    }
                    continue;
                }
            }
            let body = trim_non_nbsp(&render_blocks(def, ctx)).to_string();
            let mut lines = body.split('\n');
            out.push(format!(":  {}", lines.next().unwrap_or_default()));
            for line in lines {
                out.push(format!("   {line}"));
            }
        }
    }
    out.join("\n")
}

fn colon_fence_for(ctx: &CarveContext) -> String {
    ":".repeat(3 + ctx.colon_fence_depth)
}

/// Tables prefer the NATIVE header form: an `=` on each header cell, plus the
/// per-cell `<`/`>`/`~` alignment markers.
///
/// The GFM delimiter row is an accepted alias on input, but it says something
/// the AST does not: its alignment applies to the WHOLE column, header and body
/// alike (PART 9 T7), while alignment on the AST belongs to each cell. Writing a
/// delimiter row for the ordinary shape - an aligned header over unaligned body
/// cells - brought every body cell back aligned, so `parse(fmt(x)) == parse(x)`
/// did not hold (carve issue 359).
///
/// One header shape has no native spelling, because `span_cell` is an
/// ALTERNATIVE to `header_cell` in the grammar rather than a suffix of one:
///
/// ```text
/// | < | b |     a span marker promoted to a header cell
/// ```
///
/// An attributed header cell used to be the second such shape, and is not one
/// any more: `header_cell` now reads `'=' [alignment_marker] [cell_attributes]
/// content` (PART 9 §5 T10), so `|={.x} a |` spells it natively. Writing it as a
/// data cell under a delimiter row was never wrong, but it is no longer the
/// canonical form, and the fallback that produced it was the very shape the
/// clause exists to retire.
///
/// The span shape still needs a delimiter row to promote the first row. It is emitted BARE
/// (`|---|---|`), never with colons: the cells keep their own alignment markers,
/// so the delimiter contributes structure only and cannot spill alignment down
/// the column.
fn render_table(node: &Table, ctx: &mut CarveContext) -> String {
    let mut rows = Vec::new();
    let header_row = node
        .rows
        .first()
        .is_some_and(|row| !row.cells.is_empty() && row.cells.iter().all(|cell| cell.header));
    let needs_delimiter = header_row
        && node
            .rows
            .first()
            .is_some_and(|row| row.cells.iter().any(|cell| cell.span.is_some()));

    for (row_index, row) in node.rows.iter().enumerate() {
        let mut cells = Vec::new();
        for cell in &row.cells {
            // In the delimiter form the promoted row is written as ordinary
            // data cells - the row after it is what makes them headers.
            let mark_header = !(needs_delimiter && row_index == 0);
            cells.push(render_table_cell(cell, ctx, mark_header));
        }
        rows.push(render_table_row(&cells, &render_attrs(&row.attrs)));
    }
    if needs_delimiter {
        let sep = vec!["---"; node.rows[0].cells.len()].join("|");
        rows.insert(1, format!("|{sep}|"));
    }
    if let Some(caption) = &node.caption {
        rows.push(format!("^ {}", render_inlines(caption, ctx)));
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

fn render_table_cell(cell: &TableCell, ctx: &mut CarveContext, mark_header: bool) -> String {
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
    let content = render_inlines(&cell.children, ctx);
    ctx.table_cell_depth -= 1;
    // The space `pad_cell` writes after the prefix is what keeps the content's
    // first character content. The header `=` is read glued to the pipe and the
    // alignment sigil glued after it, off the UNTRIMMED cell, and a cell whose
    // text begins with an attribute block used to hand that block to the
    // reader as the CELL's attributes: `| ~x~ |` came back as `|=~x~|`, a
    // CENTERED column holding `x~` (carve-rs#819). Padding every cell parts
    // them without enumerating which characters merge.
    pad_cell(&prefix, &content)
}

fn render_figure(node: &Figure, ctx: &mut CarveContext) -> String {
    let target = match &*node.target {
        FigureTarget::Image(image) => render_image(image),
        FigureTarget::Table(table) => render_table(table, ctx),
        FigureTarget::BlockQuote(quote) => render_block(&BlockNode::BlockQuote(quote.clone()), ctx),
        FigureTarget::CodeBlock(code) => render_block(&BlockNode::CodeBlock(code.clone()), ctx),
        FigureTarget::Paragraph(paragraph) => {
            render_block(&BlockNode::Paragraph(paragraph.clone()), ctx)
        }
    };
    format!("{target}\n^ {}", render_inlines(&node.caption, ctx))
}

fn render_footnote_def_source(label: &str, blocks: &[BlockNode], ctx: &mut CarveContext) -> String {
    // A bare `[^label]:` is paragraph text, not a definition. PART 11 §7b
    // gives an empty definition an explicit spelling so formatting preserves
    // the definition and references to it keep resolving.
    if blocks.is_empty() {
        return format!("[^{}]: {{empty}}", write_flat_bracket_run(label));
    }
    let raw_body = render_blocks(blocks, ctx);
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

fn render_inlines(nodes: &[InlineNode], ctx: &mut CarveContext) -> String {
    render_inlines_with_caption(nodes, ctx, false)
}

fn render_inlines_with_caption(
    nodes: &[InlineNode],
    ctx: &mut CarveContext,
    mut caption_can_open: bool,
) -> String {
    if ctx.inline_depth >= MAX_RENDER_DEPTH {
        crate::render_depth::record("carve");
        return String::new();
    }
    ctx.inline_depth += 1;
    let mut out = String::new();
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
            ctx.next_unit_escape_mode(),
        );
        let opens_verbatim = next_node_opens_a_verbatim_span(nodes.get(idx + 1));
        let rendered = render_inline(
            node,
            ctx,
            prev,
            next,
            caption_can_open,
            opens_a_note,
            opens_verbatim,
        );
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
            if !inside_a_comment && (ends_the_stanza || verse_break_needs_backslash(&out)) {
                out.push('\\');
            }
            if ends_the_stanza {
                rendered = String::new();
            }
        }
        out.push_str(&rendered);
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
    ctx.inline_depth -= 1;
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
fn render_inline(
    node: &InlineNode,
    ctx: &mut CarveContext,
    prev_char: char,
    next_char: char,
    caption_can_open: bool,
    next_opens_a_note: bool,
    next_opens_a_verbatim_span: bool,
) -> String {
    let previous = ctx.escape_unit;
    ctx.escape_unit = next_escape_unit();
    let out = render_inline_body(
        node,
        ctx,
        prev_char,
        next_char,
        caption_can_open,
        next_opens_a_note,
        next_opens_a_verbatim_span,
    );
    ctx.escape_unit = previous;
    out
}

#[allow(clippy::too_many_arguments)]
fn render_inline_body(
    node: &InlineNode,
    ctx: &mut CarveContext,
    prev_char: char,
    next_char: char,
    caption_can_open: bool,
    next_opens_a_note: bool,
    next_opens_a_verbatim_span: bool,
) -> String {
    match node {
        // The one target that publishes it: the author wrote `%% note`, and
        // the canonical form writes it back verbatim. The parser drops the
        // whitespace before the marker (it is not part of the text); the space
        // that puts it back is decided in `render_inlines`, on the bytes
        // already emitted for this line.
        InlineNode::Comment(c) if c.delimited => format!("{{% {} %}}", c.content),
        // An EMPTY comment is the marker and nothing else. The space after the
        // marker separates it from content, and with no content it is line
        // TRAILING whitespace, which PART 2 discards on the way back in and §7
        // therefore lets the writer drop. Emitting it left every empty comment
        // line one space long, and in a line block that space is exactly what
        // §7c's LONE SPACE case looks for, so the writer proposed a backslash
        // for a line that had nothing to protect (PART 11 §7c).
        InlineNode::Comment(c) if c.content.is_empty() => "%%".to_string(),
        InlineNode::Comment(c) => format!("%% {}", c.content),
        InlineNode::Text(text) => escape_text(
            &resolve_nbsp_placeholder(&text.value, ctx.line_block_depth > 0),
            ctx.escape_mode_here(),
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
            },
        ),
        InlineNode::EscapedText(text) => format!("\\{}", text.value),
        InlineNode::SmartPunctuation(s) => s.value.clone(),
        InlineNode::Emphasis(emphasis) => {
            let content = render_inlines(&emphasis.children, ctx);
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
                EmphasisKind::BoldItalic => ("", format!("/*{content}*/")),
            };
            let _ = delim;
            format!("{body}{}", render_attrs(&emphasis.attrs))
        }
        InlineNode::Code(code) => {
            let value = spell_verse_empty_lines(&code.value, ctx.line_block_depth > 0);
            format!("{}{}", render_code(&value), render_attrs(&code.attrs))
        }
        InlineNode::Link(link) => render_link(link, ctx),
        InlineNode::Image(image) => render_image(image),
        InlineNode::Span(span) => {
            let attrs = render_attrs(&span.attrs);
            format!(
                "[{}]{}",
                escape_note_reference_label(&render_inlines(&span.children, ctx), ctx),
                if attrs.is_empty() { "{}" } else { &attrs }
            )
        }
        InlineNode::Math(math) => format!(
            "{}{}{}",
            if math.display { "$$" } else { "$" },
            render_code(&spell_verse_empty_lines(
                &math.content,
                ctx.line_block_depth > 0
            )),
            render_attrs(&math.attrs)
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
            format!(
                "{}{{={}}}",
                render_code(&content),
                escape_format(&raw.format)
            )
        }
        InlineNode::LiteralInline(lit) => {
            // §27: `!` prefix on a verbatim span. A trailing attribute block is
            // the ordinary inline attribute block (same as a code span carries).
            // `render_code` widens the backtick fence when the content holds
            // backticks, so the round-trip re-parses identically.
            let content = spell_verse_empty_lines(&lit.content, ctx.line_block_depth > 0);
            format!("!{}{}", render_code(&content), render_attrs(&lit.attrs))
        }
        InlineNode::Symbol(symbol) => format!(
            ":{}:{}",
            escape_symbol_name(&symbol.name),
            render_attrs(&symbol.attrs)
        ),
        InlineNode::AutoLink(link) => {
            // Emit the raw autolink content verbatim (keeps a URI scheme like
            // `mailto:`), so it re-parses to the same autolink.
            format!(
                "<{}>{}",
                escape_autolink_href(&link.text),
                render_attrs(&link.attrs)
            )
        }
        InlineNode::Mention(mention) => format!("@{}", escape_name(&mention.user)),
        InlineNode::Tag(tag) => format!("#{}", escape_name(&tag.name)),
        InlineNode::Extension(extension) => format!(
            ":{}[{}]{}",
            escape_identifier(&extension.name),
            render_inlines(&extension.children, ctx),
            render_attrs(&extension.attrs)
        ),
        // The neighbour characters are the REAL ones, not `\0`: this arm writes
        // the abbreviation's own text into the same run as everything around
        // it, so a `^` it ends on sits against whatever the next node writes.
        // A `\0` here told the caret decision there was no neighbour, and an
        // ingested abbreviation ending in `^` before a bracket run came back
        // bare - bytes that re-parse as an inline note.
        InlineNode::Abbreviation(abbr) => escape_text(
            &abbr.abbr,
            ctx.escape_mode_here(),
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
            },
        ),
        InlineNode::Footnote(footnote) => {
            let body = if let Some(inline) = &footnote.inline {
                // PART 9 §16 parses a note's content with footnote recognition
                // DISABLED, so a `^[` or a `[^` written inside it is ordinary
                // text on the way back in and the writer owes it no escape.
                ctx.note_content_depth += 1;
                let content = render_inlines(inline, ctx);
                ctx.note_content_depth -= 1;
                format!("^[{content}]")
            } else {
                format!(
                    "[^{}]",
                    write_flat_bracket_run(footnote.id.as_deref().unwrap_or_default())
                )
            };
            format!("{body}{}", render_attrs(&footnote.attrs))
        }
        InlineNode::SoftBreak(_) => "\n".to_string(),
        InlineNode::HardBreak(_) => {
            if ctx.line_block_depth > 0 {
                "\n".to_string()
            } else {
                "\\\n".to_string()
            }
        }
        InlineNode::CriticInsert(insert) => {
            format!(
                "{{+{}+}}{}",
                render_inlines(&insert.children, ctx),
                render_attrs(&insert.attrs)
            )
        }
        InlineNode::CriticDelete(delete) => {
            format!(
                "{{-{}-}}{}",
                render_inlines(&delete.children, ctx),
                render_attrs(&delete.attrs)
            )
        }
        InlineNode::CriticSubstitute(sub) => {
            format!(
                "{{~{}~>{}~}}",
                escape_critic_text(&sub.old_text),
                escape_critic_text(&sub.new_text)
            )
        }
        InlineNode::CriticComment(comment) => {
            format!("{{#{}#}}", escape_critic_text(&comment.text))
        }
        InlineNode::CrossRef(crossref) => {
            format!("</#{}>", escape_crossref_target(&crossref.target))
        }
        InlineNode::CaptionNumber(_) => "#".to_string(),
        InlineNode::CitationGroup(group) => group.raw.clone(),
    }
}

fn render_link(node: &Link, ctx: &mut CarveContext) -> String {
    // UNRESOLVED means no destination, not "carries a label": PART 12 §3a keeps
    // `ref` and `raw_ref` on a RESOLVED reference too, so the label alone no
    // longer answers this and a working reference round-tripped as its own
    // source instead of normalizing to the inline form (carve#597).
    // The AUTHORED source, in two cases. UNRESOLVED: there is no destination to
    // write instead. HEADING-DERIVED (PART 11 R1, carve#478): there is no
    // definition line, so the reference is the only record of what the author
    // wrote, and resolving it bakes a generated id into the source on every fmt
    // pass. An explicit definition normalizes to the inline form - its
    // definition line is dropped either way.
    //
    // A RESOLVED explicit reference now takes this path too. Inlining it
    // satisfied to_html(fmt(x)) == to_html(x) and broke PART 11 §1: `ref` and
    // `raw_ref` were absent from the reparse, and one destination became N after
    // a single pass - the duplication the definition form exists to avoid. The
    // definition line is no longer "dropped either way": §10 gives it a node and
    // render_block above writes it (carve-rs#631, carve#642).
    if node.ref_label.is_some() && node.raw_ref.is_some() {
        return node.raw_ref.clone().unwrap_or_default();
    }
    if node.from_crossref {
        if let Some(target) = node.href.strip_prefix('#') {
            return format!("</#{}>", escape_crossref_target(target));
        }
    }
    let text = escape_note_reference_label(&render_inlines(&node.children, ctx), ctx);
    let title = node
        .title
        .as_ref()
        .map(|title| format!(" \"{}\"", escape_quoted(title)))
        .unwrap_or_default();
    format!(
        "[{text}]({}{title}){}",
        escape_destination(&node.href),
        render_attrs(&node.attrs)
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
        .map(|title| format!(" \"{}\"", escape_quoted(title)))
        .unwrap_or_default();
    format!(
        "![{}]({}{title}){}",
        escape_image_alt(&node.alt),
        escape_destination(&node.src),
        render_attrs(&node.attrs)
    )
}

fn render_frontmatter(frontmatter: &std::collections::BTreeMap<String, String>) -> String {
    let mut out = String::from("---");
    for (key, value) in frontmatter {
        out.push('\n');
        out.push_str(key);
        out.push_str(": ");
        out.push_str(&protect_verbatim(value));
    }
    out.push_str("\n---");
    out
}

fn render_block_comment(content: &str) -> String {
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
    format!("{fence}\n{}\n{fence}", protect_verbatim(content))
}

// Superscript and subscript have no bare delimiter form -- always emit the
// braced `{^x^}` / `{,x,}` form.
fn render_forced_emphasis(delim: &str, content: &str) -> String {
    format!("{{{delim}{content}{delim}}}")
}

fn render_emphasis(delim: &str, content: &str, prev_char: char, next_char: char) -> String {
    let needs_forced = is_word_boundary(prev_char)
        || is_word_boundary(next_char)
        || content.starts_with(delim)
        || content.ends_with(delim)
        || content.starts_with(' ')
        || content.ends_with(' ')
        || content.is_empty();
    if needs_forced {
        format!("{{{delim}{content}{delim}}}")
    } else {
        format!("{delim}{content}{delim}")
    }
}

fn is_word_boundary(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
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

fn render_code(content: &str) -> String {
    let fence = safe_fence(content, 1);
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
    let needs_pad = content.starts_with('`')
        || content.ends_with('`')
        || (content.starts_with(' ')
            && content.ends_with(' ')
            && !content.chars().all(|c| c == ' '));
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
        parts.push(format!("\"{}\"", escape_quoted(title)));
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

fn render_attrs(attrs: &Option<Attrs>) -> String {
    let Some(attrs) = attrs else {
        return String::new();
    };
    let mut parts = Vec::new();
    let id_as_key = attrs.id.as_ref().is_some_and(|id| !is_attr_identifier(id));
    let mut seen_keys: Vec<&str> = Vec::new();
    let emit_id = |parts: &mut Vec<String>| {
        if let Some(id) = &attrs.id {
            if id_as_key {
                parts.push(format!("id={}", quote_attr_value(id)));
            } else {
                parts.push(format!("#{}", escape_attr_name_value(id)));
            }
        }
    };
    let emit_classes = |parts: &mut Vec<String>| {
        for cls in &attrs.classes {
            parts.push(format!(".{}", escape_attr_name_value(cls)));
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
            } else if value.is_empty() && is_boolean_attr_name(key) {
                // PART 11 §6c: a value-less attribute comes back as the bare
                // name, which is the production the language has for it. A key
                // needing escaping has no bare spelling to fall back to, and
                // neither does a `_`-first one (carve#1450) -- see
                // `is_boolean_attr_name`.
                parts.push(escape_attr_key(key));
            } else {
                parts.push(format!(
                    "{}={}",
                    escape_attr_key(key),
                    quote_attr_value(value)
                ));
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
                    if !seen_keys.contains(&key.as_str()) {
                        emit_key(&mut parts, key);
                        seen_keys.push(key);
                    }
                }
            }
        }
        for key in attrs.key_values.keys() {
            if !seen_keys.contains(&key.as_str()) {
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

fn quote_attr_value(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '{' | '}'))
    {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
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
///
/// Six, in two groups, and both groups have the same failure:
///
///   VERBATIM_BLANK  a line that was blank inside verbatim content
///   THEMATIC_GUARD  prefixes a line that would re-parse as a thematic break
///   MARKER_COLUMN   prefixes a line owed the item's marker column (§17 L3)
///   ESCAPED_SPACE   stands in for `\ ` until normalize expands it
///   STAGED_SPACE    a space that must survive escaping
///   STAGED_TAB      a tab that must survive escaping
///
/// Why `ESCAPED_SPACE` exists at all: an escaped space is written back AS an
/// escape, not as a real U+00A0. Resolving it to the character lost the
/// distinction the parser draws - `10\ kg` came back carrying a literal nbsp,
/// which re-parses as text rather than as an escape, so the node differed even
/// though the HTML did not (carve#352, corpus 29-non-breaking-space). It
/// resolves in `normalize` rather than during rendering because the backslash
/// it expands to is itself an unconditional escape, and expanding earlier let
/// the escaper double it.
///
/// The last three live at U+E010 and up deliberately. U+E000 is a PUBLISHED
/// value - the no-break-space placeholder a parsed document carries - so a
/// writer marker sharing it would be indistinguishable from document content.
/// They used to sit at U+E001 and U+E002 (carve-rs#404).
///
/// The first three are undone BY POSITION - a line that is nothing but the
/// marker, and two line prefixes. The last three are undone by a GLOBAL
/// replace, because each has more than one insertion site. Either way a
/// character the author wrote is indistinguishable from one the writer
/// inserted, and restore ate it: carve-rs#607 for the first positional pair,
/// carve-rs#630 for the global three, carve-rs#1226 for the marker column,
/// which was the one site left out of this scheme.
///
/// Narrowing the positional group to its exact sites (carve-rs#613) fixed every
/// INLINE placement and could not fix the line-alone one, because that
/// ambiguity IS positional. The global three have no narrowing available at
/// all. So the CHARACTER moves instead: the writer counts what it inserts, and
/// if the document holds more than that, the extra ones are the author's and
/// the render repeats with characters the document does not contain. carve-js
/// reached the same place from the other side in markup-carve/carve-js#666, and
/// moved its own marker-column tag in markup-carve/carve-js#1289.
///
/// OCCUPANCY IS ANSWERED BY COUNTING, NOT BY WALKING THE TREE. The writer is
/// handed an AST, not the source, so "which private-use code points does the
/// document hold" would mean a hand-written walk over every node type - and a
/// field missed there would not delete a character, it would INVENT one, which
/// is the worse direction to fail in. Counting inserted against seen asks the
/// assembled document instead, and the assembled document is what the sentinels
/// actually live in. carve-rs#1219 made the same call for the Markdown target.
///
/// A document with no private-use character - every real one - takes the first
/// render and pays six integer compares.
const SENTINEL_DEFAULTS: [char; SENTINEL_COUNT] = [
    '\u{e003}', '\u{e004}', '\u{e005}', '\u{e010}', '\u{e011}', '\u{e012}',
];

const SENTINEL_COUNT: usize = 6;

const S_BLANK: usize = 0;
const S_GUARD: usize = 1;
const S_MARKER_COLUMN: usize = 2;
const S_ESCAPED_SPACE: usize = 3;
const S_STAGED_SPACE: usize = 4;
const S_STAGED_TAB: usize = 5;

thread_local! {
    static SENTINELS: std::cell::Cell<[char; SENTINEL_COUNT]> =
        const { std::cell::Cell::new(SENTINEL_DEFAULTS) };
    /// How many of each the writer inserted during the current render.
    static INSERTED: std::cell::Cell<[usize; SENTINEL_COUNT]> =
        const { std::cell::Cell::new([0; SENTINEL_COUNT]) };
    /// How many of each were actually PRESENT just before the pass that
    /// CONSUMES them ran - restore for five of them, the list writer's own line
    /// loop for the marker column. Counted there and not at the end, because by
    /// the time the render returns an authored one has already been eaten and is
    /// indistinguishable from never having been there.
    static SEEN: std::cell::Cell<[usize; SENTINEL_COUNT]> =
        const { std::cell::Cell::new([0; SENTINEL_COUNT]) };
    /// The pre-restore text, kept so a replacement can be chosen against what
    /// the document actually holds.
    static STAGED: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// How many escape units the CURRENT pass has handed out (PART 11 §2b).
    static UNIT_COUNTER: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// The units written in the conservative form, when the writer is deciding
    /// unit by unit rather than document by document.
    ///
    /// `None` means the whole pass follows `ctx.escape_mode`, which is what the
    /// two exploratory renders in `render_carve_once` do. `Some` is §2b's pass:
    /// a unit in the set is escaped in full, every other unit is emitted by §2's
    /// own test, and for a character nothing needs that means bare.
    static ESCALATED_UNITS: std::cell::RefCell<Option<HashSet<usize>>> =
        const { std::cell::RefCell::new(None) };
    /// Where the writer records the unit a character it is escaping belongs to.
    ///
    /// `Some` only for `narrow_escalation`'s control render, which uses it to
    /// learn which units the escape arms actually ask about -- see the comment
    /// there. `None` everywhere else, so no other render pays for the
    /// bookkeeping.
    static ASKED_UNITS: std::cell::RefCell<Option<HashSet<usize>>> =
        const { std::cell::RefCell::new(None) };
    /// The occurrences handed back their bare form by the search (PART 11 §2).
    ///
    /// `None` means every candidate in an escalated unit is escaped, which is
    /// §2b's per-unit knob and the control the occurrence search is verified
    /// against.
    static RELAXED_OCCURRENCES: std::cell::RefCell<Option<HashSet<Occurrence>>> =
        const { std::cell::RefCell::new(None) };
    /// Where a pass records the occurrences it visited, in emission order.
    static OCCURRENCE_LOG: std::cell::RefCell<Option<Vec<Occurrence>>> =
        const { std::cell::RefCell::new(None) };
    /// How many escaped runs each unit has written in this pass.
    static ESCAPE_CALL_INDEXES: std::cell::RefCell<HashMap<usize, usize>> =
        std::cell::RefCell::new(HashMap::new());
    /// The decision the last candidate site took, so a RUN can inherit it.
    static LAST_OCCURRENCE_RELAXED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

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
fn next_escape_call_index(unit: usize) -> usize {
    ESCAPE_CALL_INDEXES.with(|cell| {
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
fn occurrence_is_relaxed(key: Occurrence, continues_run: bool) -> bool {
    if continues_run {
        return LAST_OCCURRENCE_RELAXED.with(std::cell::Cell::get);
    }
    OCCURRENCE_LOG.with(|cell| {
        if let Some(log) = cell.borrow_mut().as_mut() {
            log.push(key);
        }
    });
    let relaxed = RELAXED_OCCURRENCES
        .with(|cell| cell.borrow().as_ref().is_some_and(|set| set.contains(&key)));
    LAST_OCCURRENCE_RELAXED.with(|cell| cell.set(relaxed));
    relaxed
}

/// Claim the next unit ordinal for the node about to render.
fn next_escape_unit() -> usize {
    UNIT_COUNTER.with(|c| {
        let next = c.get() + 1;
        c.set(next);
        next
    })
}

impl CarveContext {
    /// Which form a character written by `unit` takes (PART 11 §2b).
    fn escape_mode_for(&self, unit: usize) -> EscapeMode {
        ASKED_UNITS.with(|cell| {
            if let Some(asked) = cell.borrow_mut().as_mut() {
                asked.insert(unit);
            }
        });
        ESCALATED_UNITS.with(|cell| match cell.borrow().as_ref() {
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
    fn escape_mode_here(&self) -> EscapeMode {
        self.escape_mode_for(self.escape_unit)
    }

    /// The mode of the node that is about to claim the next ordinal.
    fn next_unit_escape_mode(&self) -> EscapeMode {
        self.escape_mode_for(UNIT_COUNTER.with(|c| c.get()) + 1)
    }
}

fn sentinel(which: usize) -> char {
    SENTINELS.with(|s| s.get()[which])
}

fn note_inserted(which: usize) {
    INSERTED.with(|c| {
        let mut n = c.get();
        n[which] += 1;
        c.set(n);
    });
}

/// Record sentinels standing in the assembled document, at the site that is
/// about to consume them.
fn note_seen(which: usize, count: usize) {
    if count == 0 {
        return;
    }
    SEEN.with(|c| {
        let mut n = c.get();
        n[which] += count;
        c.set(n);
    });
}

fn verbatim_blank() -> char {
    sentinel(S_BLANK)
}

fn thematic_guard() -> char {
    sentinel(S_GUARD)
}

fn escaped_space() -> String {
    sentinel(S_ESCAPED_SPACE).to_string()
}

fn staged_space() -> char {
    sentinel(S_STAGED_SPACE)
}

fn staged_tab() -> char {
    sentinel(S_STAGED_TAB)
}

fn free_sentinel(text: &str, taken: &[char; SENTINEL_COUNT]) -> char {
    ('\u{e020}'..='\u{f8ff}')
        .find(|c| !taken.contains(c) && !text.contains(*c))
        .unwrap_or('\u{f8ff}')
}

fn resolve_nbsp_placeholder(text: &str, in_line_block: bool) -> String {
    if !in_line_block {
        let marker = escaped_space();
        for _ in text.matches(crate::NBSP_PLACEHOLDER) {
            note_inserted(S_ESCAPED_SPACE);
        }
        return text.replace(crate::NBSP_PLACEHOLDER, &marker);
    }
    text.split('\n')
        .map(stage_line_block_layout)
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
fn stage_line_block_layout(line: &str) -> String {
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
                note_inserted(S_STAGED_SPACE);
                out.push(staged_space());
            }
        } else {
            // A single placeholder mid-line is an escaped space, not layout.
            note_inserted(S_ESCAPED_SPACE);
            out.push_str(&escaped_space());
        }
    }

    out
}

fn normalize(text: &str) -> String {
    // Count the escaped-space marker BEFORE the replace below consumes it.
    // Everything else is counted further down, just before `restore_verbatim`,
    // but this one is resolved first and would already be gone by then - which
    // is exactly how an authored U+E010 went on being eaten after the other
    // four were fixed (carve-rs#630).
    let marker = escaped_space();
    STAGED.with(|c| c.borrow_mut().push_str(text));
    SEEN.with(|c| {
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
        if ch == sentinel(S_ESCAPED_SPACE) {
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
    let current = SENTINELS.with(|s| s.get());
    STAGED.with(|c| c.borrow_mut().push_str(&staged));
    SEEN.with(|c| {
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
    format!("{}\n", restore_verbatim(&staged))
}

/// Whole-document normalization (trailing-whitespace strip, blank-line
/// collapsing) must not reach inside verbatim content - code blocks, raw
/// blocks, frontmatter, and block comments reproduce their content byte-exact
/// (carve-js issue 340). Sentinel-encode the vulnerable bytes before the
/// content joins the document string; `normalize` restores them at the end.
/// U+E000 is already the NBSP sentinel; U+E001..U+E003 extend the scheme.
fn protect_verbatim(content: &str) -> String {
    let mut lines = Vec::new();
    for line in content.split('\n') {
        if line.is_empty() {
            note_inserted(S_BLANK);
            lines.push(verbatim_blank().to_string());
            continue;
        }
        let stripped = line.trim_end_matches([' ', '\t']);
        let tail: String = line[stripped.len()..]
            .chars()
            .map(|ch| {
                if ch == ' ' {
                    note_inserted(S_STAGED_SPACE);
                    staged_space()
                } else {
                    note_inserted(S_STAGED_TAB);
                    staged_tab()
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
fn guard_thematic_break_lines(body: &str) -> String {
    if !body.contains('-') {
        return body.to_string();
    }
    body.split('\n')
        .map(|line| {
            let trimmed = line.trim_end_matches([' ', '\t']);
            if trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-') {
                note_inserted(S_GUARD);
                format!("{}{line}", thematic_guard())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Undo `protect_verbatim` and the thematic-break guard, POSITIONALLY.
///
/// This used to be four global `replace` calls, which cannot tell a sentinel the
/// writer inserted from one the AUTHOR wrote. So an authored U+E003 was deleted
/// and an authored U+E004 became a space - in 16 of 17 constructs measured, not
/// just in a code block (carve-rs#607).
///
/// Each sentinel is only ever inserted in ONE position, so each is only undone
/// there:
///
///   VERBATIM_BLANK  a line consisting of nothing else (protect_verbatim emits it
///                   for an empty line, and never inside one)
///   U+E004          a line PREFIX (guard_thematic_break_lines prepends it)
///   STAGED_SPACE    within the TRAILING whitespace run of a line, which is the
///   STAGED_TAB      only place protect_verbatim stages them
///
/// That leaves a much smaller residue than the global form: an authored sentinel
/// still collides if it sits in the exact position the writer uses one. Closing
/// that needs the insertion COUNTS, which is the design sketched on carve-rs#607
/// - this is the part that needs no bookkeeping and no AST traversal.
fn restore_verbatim(text: &str) -> String {
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
            let prefix = line.trim_end_matches(verbatim_blank());
            if prefix.len() != line.len()
                && prefix.chars().all(|c| c == ' ' || c == '\t' || c == '>')
            {
                // `>` belongs in the set: inside a block quote the line reaching
                // here is `> ` + marker, not the marker alone, and requiring pure
                // whitespace left a raw U+E003 in the output - which
                // `verbatim_content_stable_inside_containers` and the corpus
                // formatter's semantic check both caught. A line that is nothing
                // but container prefix plus the marker is the blank the marker
                // stands for, at any nesting.
                //
                // A PURELY WHITESPACE PREFIX IS DROPPED WITH IT. PART 11 section
                // 7 emits the STRUCTURAL INDENT of an empty verbatim line as
                // nothing: "when the verbatim content on that line is EMPTY the
                // indent alone is what remains -- that is layout, and it is
                // omitted". Keeping it left a whitespace-only line, which editors
                // that strip on save, `git apply --whitespace=fix` and CI
                // whitespace checks all rewrite behind the formatter.
                //
                // The comment here used to say "a later trim removes a
                // whitespace-only line". Nothing does: `normalize` runs its
                // whitespace-only pass BEFORE this function, when the line still
                // carries the marker and so is not whitespace-only yet. That was
                // a check that could not fail, and a blank line inside a fenced
                // block under a footnote definition or a definition-list
                // description came out indented (carve#1040).
                //
                // The block-quote prefix is not layout and stays: an EMPTY line
                // would close the quote, taking the open fence with it. What
                // goes with the marker is the prefix's TRAILING whitespace, and
                // that is how the host itself spells a blank line: a quote
                // writes `>`, not `> `. Keeping the space wrote a line with a
                // trailing run - the same tooling hazard §7 names, and a
                // divergence from carve-js and carve-php, which both spell the
                // boundary inside a nested quote `> >`. A purely whitespace
                // prefix trims away to nothing, which is §7's structural-indent
                // rule and needs no branch of its own.
                return prefix.trim_end_matches([' ', '\t']).to_string();
            }
            let line = match line.strip_prefix(thematic_guard()) {
                Some(rest) => format!(" {rest}"),
                None => line.to_string(),
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
            restore_staged_runs(&line)
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
fn restore_staged_runs(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let staged = |c: char| c == staged_space() || c == staged_tab();
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
                out.push(if ch == staged_space() { ' ' } else { '\t' });
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
/// The nodes that DO open with a bare `[` - a link, a span, a reference - report
/// no boundary character at all, so they never reach this decision.
fn leading_verbatim_text(node: &InlineNode) -> Option<&str> {
    match node {
        InlineNode::Text(text) => Some(&text.value),
        InlineNode::SmartPunctuation(punctuation) => Some(&punctuation.value),
        InlineNode::Abbreviation(abbr) => Some(&abbr.abbr),
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
    match next.and_then(leading_verbatim_text) {
        Some(text) => caret_needs_its_escape(
            text,
            0,
            NeighbourEscape {
                in_note_content,
                next_node_opens_a_note: false,
                next_node_opens_a_verbatim_span: false,
            },
            mode,
        ),
        None => false,
    }
}

#[allow(clippy::too_many_arguments)]
fn escape_text(
    text: &str,
    mode: EscapeMode,
    unit: usize,
    opens_block_line: bool,
    caption_can_open: bool,
    in_table_cell: bool,
    previous_boundary: char,
    next_boundary: char,
    note: NeighbourEscape,
) -> String {
    // The offset of the first `$` of a trailing `$`-run, or of a trailing `!`,
    // when a verbatim span follows this text in the OUTPUT. Both sigils bind to
    // the backtick fence that node writes: `$` makes the span inline math, `$$`
    // display math, `!` an inline literal - so text the source meant as text
    // re-parses as markup that was never written, silently and with no
    // diagnostic. PART 11 §2 escapes exactly this, "if and only if omitting the
    // escape would change the re-parsed AST", and corpus-convert
    // 05-markdown-verbatim-sigils-stay-text is the document that asks.
    //
    // The whole run is escaped, not just the last one: in `\$$`x`` the SECOND
    // dollar still opens inline math. This mirrors carve-js
    // (`escapeCarveConstructsSpelledLikeText`) and carve-php, which do it in
    // their line-rewriting Markdown converters; carve-rs's importers are
    // AST-first and hand the job to this writer, so this is where the rule has
    // to live - and living here covers every importer at once rather than one.
    //
    // Only a run that REACHES the end of this node matters. A sigil with
    // anything after it inside the node is not adjacent to the fence, and a
    // literal backtick inside the node is escaped unconditionally below, so no
    // span opens against it either.
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
    let mut out = String::new();
    // PART 11 §2's decision is taken per OPENER OCCURRENCE, so every candidate
    // site in this run gets an index the search can address it by
    // (markup-carve/carve#1533).
    let call = next_escape_call_index(unit);
    // A `^` is only dangerous where a caption marker could be read: at the
    // start of a line. Anywhere else it is literal text - superscript is
    // braced-only (`{^x^}`), so `10^6^` carries no markup - and forcing the
    // escape there put `10\^6\^` in the output where the other two engines
    // write `10^6^`. PART 11 §4 asks for the minimal form when dropping the
    // escape changes nothing, and this one changed nothing (carve-rs#555).
    //
    // Line-initial stays forced rather than left to the minimal/conservative
    // vote, because that vote is per DOCUMENT: letting `^ Figure 1` render
    // unescaped in the minimal pass makes it a caption, the two passes differ,
    // and the whole document escalates to conservative - which then escapes
    // every candidate in it, including the `:` that needs nothing. The corpus
    // pins that exact shape at 158-indented-image-and-caption-stay-literal.
    let mut at_line_start = opens_block_line;
    let mut chars = text.char_indices().peekable();
    let mut previous = previous_boundary;
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
        // AN EMPTY BRACE PAIR IS NOT A CONSTRUCT (carve#1447, corpus 388), so
        // neither caret of `{^^}` opens anything and PART 11 §2 escapes a
        // character IF AND ONLY IF omitting the escape would change the
        // re-parsed AST. Against this engine's own parser `{^^}` and `{\^\^}`
        // differ in nothing but escape bytes, and §1's EQUALITY IS MODULO
        // ESCAPING makes them the same document - so §4 asks for the bare form
        // and §10g's unconditional set does not reach here either, because that
        // one is about a LEADING caret and neither of these leads.
        //
        // `parse_forced_emphasis` is the rule being mirrored: it takes the
        // first `^}` pair after the opener and returns `None` when that pair
        // meets the opener with nothing in between. So EMPTINESS is the whole
        // test, and `{^x^}` - which holds something, and IS a forced
        // superscript - keeps both of its escapes untouched.
        //
        // The neighbouring over-escapes stay open on purpose: §2a's `}^p` and
        // `[^` are open in all three engines, and corpus 388 deliberately does
        // not pin them.
        let empty_braced_super = ch == '^'
            && ((previous == '{' && text[offset..].starts_with("^^}"))
                || (next == '}' && text[..offset].ends_with("{^")));
        let caret_opens_inline = ch == '^'
            && !empty_braced_super
            && (previous == '{'
                || next == '}'
                || (next == '['
                    && caret_needs_its_escape(text, offset + ch.len_utf8(), note, mode)));
        // A `:` opens something only where a marker can START: `:: term`,
        // `:  def` and `::: fence` are all recognized at the beginning of a
        // line, so the FIRST colon of that run is the one that has to be
        // escaped and the rest cannot open anything.
        //
        // The conservative pass used to escape every candidate character it
        // saw, so a literal `:::` came out `\:\:\:` where carve-js and
        // carve-php write `\:::`, and `\[x\]: /u` picked up an escape on a
        // colon that no rule can read (carve-rs#566). PART 11 §4 asks for the
        // minimal form when dropping the escape changes nothing, and it
        // changes nothing for every colon but the first.
        //
        // Same shape as the caret above: ask what the character could open
        // HERE, rather than escaping the class it belongs to.
        //
        // A MID-LINE COLON IS NOT INERT, though: `:rocket:` is a symbol
        // shortcode, an inline construct that opens anywhere, and under a
        // configured symbol map it renders a GLYPH where the document held
        // text. `:` was already in §5's candidate set and this guard was
        // withholding it from the search on every line but the first
        // (markup-carve/carve#1609). Asking `symbol_opens_at` rather than
        // widening the guard keeps the writer's question the parser's own.
        let colon_cannot_open =
            ch == ':' && !at_line_start && !symbol_opens_at(text, offset, previous);
        at_line_start = ch == '\n';
        let opens_a_verbatim_construct = verbatim_sigil_at.is_some_and(|start| offset >= start);
        let unconditional = matches!(ch, '\\' | '`' | '"' | '\'')
            || caret_opens_a_caption
            || caret_opens_inline
            || opens_a_verbatim_construct;
        // A CELL PAYLOAD IS WHERE A CARET IS MARKUP AGAIN. `span_cell` is
        // `rowspan_marker | colspan_marker`, one production over two markers,
        // and only the `<` half was ever reachable here - `<` is in the
        // candidate set below and `^` was not - so a cell holding a caret was
        // written bare, re-read as a rowspan marker, and the cell was DELETED
        // while the cell above it grew a `rowspan="2"` (markup-carve/carve#1609).
        //
        // PART 11 §6f is why the cell's own padding does not already cover it:
        // `rowspan_marker = {space}, '^', {space}` is written WITH the padding
        // inside it, so the space `pad_cell` puts either side of the content
        // puts nothing out of the marker's reach.
        //
        // OFFERED, NOT FORCED, and that is the whole point of putting it here
        // rather than in a payload test of its own. Whether a caret in a cell
        // is a marker depends on what else the cell holds - `| ^ |` is a span
        // and `| a ^ b |` is text - and §2's search already answers that
        // question by re-parsing, the same way it answers it for `<`. A
        // predicate spelled here instead would be a second reading of
        // `span_cell` that could drift from the parser's.
        let caret_is_a_span_marker = ch == '^' && in_table_cell;
        let candidate = caret_is_a_span_marker
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
            );
        // In a unit the search has escalated, each candidate site is offered
        // back on its own, so the one occurrence that needed the escape no
        // longer drags the rest of the unit with it (PART 11 §2). A character
        // whose own guard already decided it -- the caret, a sigil binding to a
        // verbatim run, the unconditional set -- is not a candidate and is
        // never offered.
        let offered = mode == EscapeMode::Conservative && candidate && !unconditional;
        let relaxed =
            offered && occurrence_is_relaxed((unit, call, offset), offset > 0 && previous == ch);
        if unconditional || (offered && !relaxed && !colon_cannot_open) {
            out.push('\\');
        }
        out.push(ch);
        previous = ch;
    }
    out
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
///
/// ALT IS RAW. It is an HTML attribute, so nothing inside it is inline-parsed
/// and no escape inside it is resolved: `![t\]z](/i.png)` gives `alt="t\]z"`,
/// backslash and all. That is what makes escaping the wrong tool here - a `\]`
/// the writer emits is not a neutralized bracket, it is two more characters of
/// alt text, and the document says something else on the next read. It
/// compounded, too, because each pass escaped the backslash the last pass wrote
/// (markup-carve/carve#1197).
///
/// The run closes at the MATCHING `]`, by the same scan a link's text closes by,
/// so the alt an author can write is exactly the alt that re-reads as itself and
/// the writer's job is to put it back verbatim (markup-carve/carve#1206).
///
/// The fallback covers an alt with NO Carve spelling - a bare unbalanced `]`, or
/// a run ending inside an unclosed code span. `parse` cannot produce one; an
/// ingested AST can. Escaping is not a representation of that value either, but
/// it keeps the image a well-formed image instead of letting a stray `]` split
/// the line, and it settles: the escaped alt IS representable, so the pass after
/// it writes the same bytes.
fn escape_image_alt(text: &str) -> String {
    if crate::parse::raw_bracket_run_closes(text) {
        return text.to_string();
    }
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
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

fn escape_destination(text: &str) -> String {
    let sanitize_blank = dangerous_destination_scheme(text);
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
        if (marked.contains(&i) || escapable) && !sanitize_blank {
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
            '(' if sanitize_blank => out.push_str("%28"),
            ')' if sanitize_blank => out.push_str("%29"),
            _ => out.push(ch),
        }
    }
    out
}

fn dangerous_destination_scheme(text: &str) -> bool {
    let trimmed = text.trim_start_matches(|ch: char| {
        ch <= '\u{0020}'
            || matches!(
                ch,
                '\u{00a0}' | '\u{1680}' | '\u{2000}'
                    ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
            )
    });
    let Some(colon) = trimmed.find(':') else {
        return false;
    };
    let scheme = &trimmed[..colon];
    !scheme.is_empty()
        && scheme
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
        && matches!(
            scheme.to_ascii_lowercase().as_str(),
            "javascript" | "vbscript" | "data" | "file"
        )
}

fn escape_quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// A FLAT raw bracketed run: a colon-fence or code-fence `[label]`, and a
/// footnote's `[^id]` in both its definition and its references.
///
/// The same rule as an alt text and for the same reason - the value is raw, so
/// an escape the writer emits reaches the reader as two characters of content
/// rather than as a neutralized bracket - but a narrower close. These readers
/// take the run up to the FIRST `]`, with no balance and no escape, so a run is
/// representable exactly when it holds neither a `]` nor a line break.
///
/// One function for one rule. It was written twice and both spellings escaped,
/// so `::: [a\b]` and `[^n\m]` grew a backslash on every format pass - a div
/// label is rendered, so that document said something new each time, and the
/// other three merely refused to settle.
///
/// WRITTEN AS AUTHORED WITH NO FALLBACK, unlike an alt text. A value holding a
/// `]` has no spelling here either, but the escape is not a spelling of it: each
/// of these readers requires the run to be the whole of what follows, so
/// `[a\]b]` fails to match exactly as `[a]b]` does, and `::: [a\]b]` and
/// `::: [a]b]` render the same paragraph, container and all. Where the construct
/// survives as text instead - a code fence, a footnote definition - the escape
/// only adds a backslash the reader can see. The branch would change no output
/// anywhere, which is a branch that cannot fail, so it is not written.
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

fn escape_name(text: &str) -> String {
    let trimmed = text.trim_matches('.');
    trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '.' || *ch == '-')
        .collect()
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

/// Whether a container KIND can be spelled as a colon-fence type word.
///
/// `render_admonition` writes the kind verbatim after the fence, so a kind this
/// rejects would be emitted as source that does not read back as the container
/// it came from: `::: 2col` is an ordinary paragraph, because the opener
/// grammar (PART 9, `admonition_open`) reads the word as `[a-zA-Z_][\w-]*` and
/// a digit cannot open it.
///
/// Used by `html_import` to decide whether an element's class can become the
/// fence word of a rebuilt container, for the same reason it asks
/// [`is_attr_identifier`] about a name: the answer has to be the writer's,
/// rather than a second copy that drifts from it (carve-rs#1240).
pub(crate) fn is_container_kind(text: &str) -> bool {
    is_attr_identifier(text)
}

fn escape_autolink_href(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('<', "\\<")
        .replace('>', "\\>")
}

fn escape_crossref_target(text: &str) -> String {
    text.replace('\\', "\\\\").replace('>', "\\>")
}

fn escape_critic_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

fn first_boundary(node: &InlineNode) -> Option<char> {
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
fn verse_break_needs_backslash(emitted: &str) -> bool {
    let line = match emitted.rfind('\n') {
        Some(at) => &emitted[at + 1..],
        None => emitted,
    };
    if line.is_empty() {
        return true;
    }
    if line.ends_with(sentinel(S_ESCAPED_SPACE)) {
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
