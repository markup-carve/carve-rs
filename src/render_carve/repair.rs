use super::*;

/// One full render, with the insertion counters reset for it.
pub(super) fn render_carve_once(session: &RenderSession, doc: &Document) -> String {
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
        // Equal spellings can still lose an over-cap list marker on readback.
        if crate::render_depth::reaches_list_nesting_cap(doc) {
            refuse_unless_it_reads_back(comparable_tree(&minimal).as_ref(), doc);
        }
        return minimal;
    }
    // The minimal form that re-parses to THIS document is the document's
    // canonical spelling, whatever the conservative form does. Without this the
    // check below reads the conservative form as the reference, so an escape
    // that CHANGES the re-parse keeps itself: a backslash before the
    // punctuation that decides a smart quote's direction flipped the quote and
    // `fmt` changed what the document renders (carve-rs#2455).
    //
    // ONE-WAY, which is what keeps `escaping_is_redundant`'s reasoning intact:
    // the writer does not satisfy `parse(fmt(x)) == parse(x)` for every
    // construct, so a match emits the minimal form and a miss decides nothing.
    // carve-js takes the same shortcut in the same place.
    let minimal_tree = comparable_tree(&minimal);
    if minimal_tree
        .as_ref()
        .is_some_and(|tree| *tree == comparable_document(doc.clone()))
    {
        return minimal;
    }
    // ONE parse of the conservative form, shared by the redundancy check and the
    // narrowing below. Parsing it in each made every narrowed document pay a
    // second full parse of its own output for an answer it already had.
    let conservative_tree = comparable_tree(&conservative);
    // Every state the narrowing visits is verified against `conservative_tree`,
    // so when that is not the document's tree no state reads back either. Past
    // the cap that is the refuse arm of CARVE-P9-078, taken before a search whose
    // every probe re-parses the whole deep document. The equal-forms return
    // checks readback separately above.
    if crate::render_depth::reaches_list_nesting_cap(doc)
        && !refuse_unless_it_reads_back(conservative_tree.as_ref(), doc)
    {
        return conservative;
    }
    if escaping_is_redundant(minimal_tree.as_ref(), conservative_tree.as_ref()) {
        return minimal;
    }
    // The minimal form of the WHOLE document does not hold, which used to end
    // the decision here with the conservative form of the whole document. PART
    // 11 §2b says how far that fallback actually reaches: the smallest unit
    // whose minimal form fails, and §2's own test everywhere else.
    narrow_escalation(session, doc, conservative, conservative_tree)
}

/// CARVE-P9-078: past the nesting cap the writer refuses rather than emit source
/// that reads back as a different document. Records the refusal and returns
/// false when `output_tree` is not the document's tree.
fn refuse_unless_it_reads_back(output_tree: Option<&Document>, doc: &Document) -> bool {
    if output_tree.is_some_and(|tree| *tree == comparable_document(doc.clone())) {
        return true;
    }
    note_unspellable_list_nesting();
    false
}

pub(super) fn note_unspellable_list_nesting() {
    crate::render_carve_error::record_unspellable_list_nesting(
        "content nested past the parser nesting limit of 200 has no spelling that reads back",
    );
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
pub(super) fn comparable_tree(source: &str) -> Option<Document> {
    #[cfg(test)]
    tests::PARSED_BYTES.with(|n| n.set(n.get() + source.len()));
    std::panic::catch_unwind(|| comparable_document(crate::parse::parse_for_carve_shape(source)))
        .ok()
}
