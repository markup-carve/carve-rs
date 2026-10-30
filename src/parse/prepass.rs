use super::*;
use std::ops::ControlFlow;

/// How many BYTES the definition pre-pass may hand to the block parser to
/// answer [`line_folds_into_an_open_paragraph`], for one document.
///
/// The question is answered by parsing, and what has to be parsed is the run of
/// lines back to the last blank one - so a document that is one long blank-free
/// run of definition-shaped marker lines asks it once per line over a run that
/// grows by one each time, which is quadratic. The budget makes the pre-pass
/// linear again by capping the total, and running out is SAFE in the direction
/// that matters: an unanswered question is answered `false`, which collects the
/// definition exactly as the engine did before this guard existed. A document
/// has to be built to reach it - the question is only asked for a line that is
/// both a list marker and a definition behind it. Each pass that asks gets its
/// own budget: the footnote pre-pass and, since carve#1425, the link-reference
/// one.
///
/// BYTES RATHER THAN LINES, because a probe costs what it PARSES and a line is
/// not a unit of that. Counting lines prices one long line and one short one
/// the same, so a run holding a single huge line under many short
/// definition-shaped ones buys as many full re-parses of that line as it has
/// candidates - the amplification the cap exists to bound, priced at zero.
pub(in crate::parse) fn probe_budget_for(source_bytes: usize) -> usize {
    source_bytes.saturating_mul(4).saturating_add(4096)
}

/// One level of a probe's LAST-CHILD CHAIN: what kind of node the level ends
/// with, and - carried alongside in [`OpenFrame`] - how many nodes it holds.
///
/// A list's ITEMS and a definition list's are levels of their own rather than
/// blocks, and they have to be, because the count that separates a sibling item
/// from a lazy continuation lives exactly there: `- a` / `  lazy` and the same
/// with `- [^f]: t` under it hold the identical blocks at every OTHER level and
/// differ only in how many items the list has.
#[derive(PartialEq, Eq)]
pub(in crate::parse) enum ProbeLevel {
    Blocks(std::mem::Discriminant<BlockNode>),
    ListItems,
    DefinitionItems,
}

/// The chain of still-open levels a parse ends in, from the document level down
/// to the innermost last node.
pub(in crate::parse) struct OpenFrame {
    pub(in crate::parse) levels: Vec<(ProbeLevel, usize)>,
    /// Whether the innermost node the chain reached is a paragraph. Read from
    /// the chain rather than re-derived, so the two always describe one walk.
    pub(in crate::parse) ends_in_paragraph: bool,
    pub(in crate::parse) contains_quote: bool,
}

/// The child level a block holds, for the last-child walk [`open_frame`] makes.
///
/// EXHAUSTIVE ON PURPOSE, with no `_` arm: a block kind added later is a
/// compile error here rather than a silent answer. And the answer a `Leaf` gives
/// is the safe one anyway - a level the walk cannot enter ends the chain on a
/// node that is not a paragraph, so the caller declines to suppress.
pub(in crate::parse) enum ProbeChildren<'a> {
    Blocks(&'a [BlockNode]),
    ListItems(&'a [ListItem]),
    DefinitionItems(&'a [DefinitionItem]),
    Leaf,
}

pub(in crate::parse) fn probe_children(block: &BlockNode) -> ProbeChildren<'_> {
    match block {
        BlockNode::BlockQuote(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::Directive(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::Div(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::Section(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::Admonition(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::FigureGroup(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::LineBlock(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::BlockExtension(b) => ProbeChildren::Blocks(b.fallback_slice()),
        BlockNode::ExtensionCarrier(b) => ProbeChildren::Blocks(&b.children),
        BlockNode::List(b) => ProbeChildren::ListItems(&b.items),
        BlockNode::DefinitionList(b) => ProbeChildren::DefinitionItems(&b.items),
        BlockNode::Heading(_)
        | BlockNode::Paragraph(_)
        | BlockNode::CodeBlock(_)
        | BlockNode::Table(_)
        | BlockNode::Figure(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::LinkReferenceDefinition(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => ProbeChildren::Leaf,
    }
}

pub(in crate::parse) fn open_frame(blocks: &[BlockNode]) -> OpenFrame {
    let paragraph = std::mem::discriminant(&BlockNode::Paragraph(Paragraph::default()));
    let mut frame = OpenFrame {
        levels: Vec::new(),
        ends_in_paragraph: false,
        contains_quote: false,
    };
    let mut blocks = blocks;
    loop {
        let Some(last) = blocks.last() else {
            return frame;
        };
        frame.contains_quote |= matches!(last, BlockNode::BlockQuote(_));
        let kind = std::mem::discriminant(last);
        frame.ends_in_paragraph = kind == paragraph
            || matches!(last, BlockNode::Figure(figure) if !figure.caption.is_empty());
        frame.levels.push((ProbeLevel::Blocks(kind), blocks.len()));
        match probe_children(last) {
            ProbeChildren::Blocks(children) => blocks = children,
            ProbeChildren::ListItems(items) => {
                frame.ends_in_paragraph = false;
                let Some(item) = items.last() else {
                    return frame;
                };
                frame.levels.push((ProbeLevel::ListItems, items.len()));
                blocks = &item.children;
            }
            ProbeChildren::DefinitionItems(items) => {
                frame.ends_in_paragraph = false;
                let Some(item) = items.last() else {
                    return frame;
                };
                frame
                    .levels
                    .push((ProbeLevel::DefinitionItems, items.len()));
                let Some(def) = item.definitions.last() else {
                    return frame;
                };
                blocks = &def.children;
            }
            ProbeChildren::Leaf => return frame,
        }
    }
}

/// Is `line` a list marker line the probe may be asked about - THROUGH its
/// container prefixes?
///
/// This is the scope of the lazy guard, and only §10's property makes it a
/// question at all: a list does not interrupt an open paragraph, so a marker
/// line can be lazy text where a quote or a heading on the same line could not.
///
/// THE PREFIXES COME OFF FIRST (carve-rs#1142). Both passes used to test the RAW
/// line, so `> - [d]: u` matched no marker, never reached the probe, and was
/// collected out of the quote's open paragraph - which deleted the author's text
/// and left a bare `-` on the page. The container was never the question: the
/// run the probe parses carries the quote, so `>`, `:::` and the document level
/// get the same answer from the same code.
pub(in crate::parse) fn marker_line_may_be_lazy(line: &str) -> bool {
    detect_list_marker_full(without_blockquote_prefixes(line)).is_some()
}

pub(in crate::parse) fn marker_follows_sibling(body: &[impl AsRef<str>], line: &str) -> bool {
    body.last().is_some_and(|prior| {
        let prior = prior.as_ref();
        let prior_bare = without_blockquote_prefixes(prior);
        let line_bare = without_blockquote_prefixes(line);
        indent_columns(prior) == indent_columns(line)
            && prior[..prior.len() - prior_bare.len()] == line[..line.len() - line_bare.len()]
            && detect_list_marker_full(prior_bare).is_some()
    })
}

/// Does the BLOCK PARSER fold `line` into a paragraph that was already open?
///
/// A definition pre-pass must answer this before it may cut a definition out of
/// a line, and has no business answering it itself. §10 says a list does not
/// interrupt an open paragraph, so `r` then `. [^f]: t` is ONE paragraph holding
/// both lines - and a pre-pass that collects the second line deletes text the
/// block parser keeps and defines a note nobody wrote (carve-rs#1024, carve#1425).
///
/// THE ANSWER IS ASKED OF THE PARSER, NOT ENUMERATED. Listing the openers a line
/// can be makes every unlisted opener answer "a paragraph is open", which
/// SUPPRESSES a collection the engine would otherwise make. The list is
/// unbounded by construction: a custom `match_block` extension cannot be listed
/// at all, because the pre-pass does not know its syntax.
///
/// So the run is handed to the block parser TWICE, without `line` and with it,
/// and the open frames compared. A line that folds adds NO node: every level
/// holds what it held and the innermost is still a paragraph. Anything that
/// opens a block changes a count along that chain. Extensions are answered for
/// free, which is what no list could cover.
///
/// FAILING SAFE IS THE DEFAULT. Every early return is `false`, and `false`
/// declines to suppress, so an unrecognised shape can only collect the way the
/// engine did before this guard - never delete an author's line unasked.
///
/// THE RUN IS BOUNDED BY THE LAST BLANK LINE, soundly: a blank closes every
/// paragraph in every container, so one still open at the end of `body` began
/// after the last blank. Losing the container frame above it can only cost a
/// suppression, which is the safe direction again.
///
/// The run is taken from `body`, the document THIS PASS has extracted so far,
/// not from the raw source: a definition already collected is gone from the
/// parser's input, and a probe that still saw it would read the paragraph it is
/// not. Link definitions are the other half of the same point - they are
/// stripped downstream of here, so the probe strips them too, and `[a]: /u` is
/// the line it leaves rather than the paragraph a raw parse would find.
pub(in crate::parse) fn line_folds_into_an_open_paragraph(
    body: &[impl AsRef<str>],
    line: &str,
    options: &Options<'_>,
    budget: &mut usize,
    unaffordable: &mut bool,
) -> bool {
    paragraph_continuation_probe(body, line, options, budget, unaffordable, false)
}

pub(in crate::parse) fn paragraph_continuation_probe(
    body: &[impl AsRef<str>],
    line: &str,
    options: &Options<'_>,
    budget: &mut usize,
    unaffordable: &mut bool,
    collect_notes: bool,
) -> bool {
    paragraph_continuation_probe_with_start(
        body,
        line,
        options,
        budget,
        unaffordable,
        collect_notes,
        false,
    )
    .0
}

pub(in crate::parse) fn paragraph_continuation_probe_with_start(
    body: &[impl AsRef<str>],
    line: &str,
    options: &Options<'_>,
    budget: &mut usize,
    unaffordable: &mut bool,
    collect_notes: bool,
    capture_root: bool,
) -> (bool, ProbeRootBoundary) {
    let run_start = if collect_notes {
        0
    } else {
        body.iter()
            .rposition(|written| is_blank_line(written.as_ref()))
            .map_or(0, |blank| blank + 1)
    };
    let run = &body[run_start..];
    if run.is_empty() {
        return (false, ProbeRootBoundary::default());
    }
    // What the two probes will PARSE, which is the run twice over plus the
    // candidate line - so the price is the work rather than the question.
    let cost = run
        .iter()
        .map(|written| written.as_ref().len().saturating_add(1))
        .fold(0usize, |total, len| total.saturating_add(len))
        .saturating_mul(2)
        .saturating_add(line.len());
    // OUT OF BUDGET IS NOT AN ANSWER, and must not be reported as one.
    //
    // Answering "it does not fold" here made the pre-pass cut the definition out
    // of a line it had not established was a definition, so `- [^f]: t` rendered
    // as a bare `-`: fifteen such lines under an open paragraph reached the
    // boundary, and four thousand lost 3,857 of them. carve-js and carve-php
    // keep every line (carve-rs#1492).
    //
    // The caller now sees the unaffordable case separately and keeps the
    // author's line. Whether the definition should still REGISTER there is a
    // different question, and a live one for all three engines - see
    // markup-carve/carve#1881.
    if *budget < cost {
        *unaffordable = true;
        return (false, ProbeRootBoundary::default());
    }
    *budget -= cost;

    let mut before = String::with_capacity(run.iter().map(|line| line.as_ref().len() + 1).sum());
    for (index, written) in run.iter().enumerate() {
        if index != 0 {
            before.push('\n');
        }
        before.push_str(written.as_ref());
    }
    let mut after = String::with_capacity(before.len() + line.len() + 1);
    after.push_str(&before);
    let raw_before_len = before.len();
    after.push('\n');
    after.push_str(line);

    let _term_probe = ProbeGuard::enter();
    let reduce = |source: String| {
        let source = if collect_notes && source.contains("[^") {
            extract_footnote_defs(&source, 1, false, false, Some(&[]), options)
                .0
                .source
        } else {
            source
        };
        reduce_frame_probe_links(source, options)
    };
    let before = reduce(before);
    let unchanged = before == after[..raw_before_len]
        || before.strip_suffix('\n') == Some(&after[..raw_before_len]);
    let (before, root_start) = if capture_root && unchanged && options.extensions.is_empty() {
        let (blocks, root_start) = probe_blocks_at_document_level_with_start(&before, options);
        (
            open_frame(&blocks),
            ProbeRootBoundary {
                start: root_start.start.map(|start| run_start + start),
                closed_initial_container_end: root_start
                    .closed_initial_container_end
                    .map(|end| run_start + end),
            },
        )
    } else {
        (
            open_frame(&probe_blocks_at_document_level(&before, options)),
            ProbeRootBoundary::default(),
        )
    };
    if !before.ends_in_paragraph {
        return (false, root_start);
    }
    let after = reduce(after);
    let after = open_frame(&probe_blocks_at_document_level(&after, options));
    (
        after.ends_in_paragraph && before.levels == after.levels,
        root_start,
    )
}

pub(in crate::parse) fn reduce_frame_probe_links(source: String, options: &Options<'_>) -> String {
    if options.extensions.is_empty()
        && !source.contains('\r')
        && !source_may_contain_link_definition(&source)
    {
        let mut source = source;
        if !source.is_empty() && !source.ends_with('\n') {
            source.push('\n');
        }
        source
    } else {
        extract_link_defs(&source, options).0
    }
}

pub(in crate::parse) fn source_may_contain_link_definition(source: &str) -> bool {
    if !source.contains("]:") {
        return false;
    }
    source
        .lines()
        .rev()
        .filter(|line| line.contains("]:"))
        .any(|line| {
            let mut rest = trim_ascii_start(line);
            loop {
                let next = trim_ascii_start(strip_container_prefixes(rest, true).bare);
                if next.len() == rest.len() {
                    return parse_link_def_line_with_value(next).is_some();
                }
                rest = next;
            }
        })
}

#[derive(Clone)]
pub(in crate::parse) struct PrepassParagraph {
    list_columns: std::rc::Rc<[(usize, usize)]>,
    quote_columns: std::rc::Rc<[usize]>,
    absorption: bool,
}

impl PrepassParagraph {
    fn folds_marker(&self, content: &str) -> bool {
        let column = indent_columns(content);
        self.list_columns.is_empty()
            || (self
                .list_columns
                .first()
                .is_some_and(|(marker, _)| column >= *marker)
                && self
                    .list_columns
                    .last()
                    .is_some_and(|(_, floor)| column < *floor)
                && self
                    .list_columns
                    .binary_search_by_key(&column, |(marker, _)| *marker)
                    .is_err())
    }
}

/// The last line in `lines[run_start..start]` where the block parser starts a
/// document-level block after a blank line, flush left and with no block
/// attribute pending, together with the end of a closed container that opens
/// the run. Nothing above such a line reaches the parse of the lines below it.
pub(in crate::parse) fn last_document_root(
    lines: &[&str],
    run_start: usize,
    start: usize,
    options: &Options<'_>,
) -> Option<(usize, Option<usize>)> {
    let run = &lines[run_start..start];
    // The probe rejoins its run, which drops a trailing CR these lines keep.
    if !options.extensions.is_empty() || run.iter().any(|line| line.ends_with('\r')) {
        return None;
    }
    let scanned = document_block_starts(run, options);
    let root = scanned.starts.into_iter().rev().find(|&root| {
        root > 0 && is_blank_line(run[root - 1]) && !run[root].starts_with([' ', '\t'])
    })?;
    Some((run_start + root, scanned.closed_initial_container_end))
}

// Bytes of run the marker probe in `term_folded_lines` hands to the definition
// passes and the block parser, counted so the regression guard reads work
// rather than a clock.
#[cfg(test)]
thread_local! {
    pub(in crate::parse) static TERM_MARKER_PROBE_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Can the probe's definition passes rewrite `line`? Every definition they
/// lift starts on a `]:` line, so a run holding none is parsed as written.
pub(in crate::parse) fn probe_may_rewrite(line: &str) -> bool {
    line.contains("]:") || line.contains('\r')
}

/// Lines folded into a term and fences that end one at its container column.
#[derive(Default)]
pub(in crate::parse) struct TermLineOwnership {
    pub(in crate::parse) tracks_quote_context: bool,
    pub(in crate::parse) folded: std::collections::HashSet<usize>,
    pub(in crate::parse) fence_openers: HashMap<usize, TermFenceHost>,
    pub(in crate::parse) paragraph_quotes:
        std::cell::RefCell<HashMap<usize, Option<PrepassParagraph>>>,
    // Per line, whether the run of nonblank lines ending there began on a list
    // marker; memoized so the upward walk stays linear.
    pub(in crate::parse) list_runs: std::cell::RefCell<HashMap<usize, bool>>,
}

pub(in crate::parse) struct TermFenceHost {
    pub(in crate::parse) content_col: usize,
    pub(in crate::parse) quote_depth: usize,
    pub(in crate::parse) quote_columns: Vec<usize>,
    pub(in crate::parse) opener_offset: usize,
}

pub(in crate::parse) fn term_folded_lines(
    lines: &[&str],
    closer_maxima: &PrepassFenceClosers,
    options: &Options<'_>,
) -> TermLineOwnership {
    let ownership = TermLineOwnership {
        tracks_quote_context: lines.iter().any(|line| line.contains('>')),
        ..TermLineOwnership::default()
    };
    // Only a term folds anything, and the fence scans below are not free on a
    // document of unterminated openers, which nesting re-reads per level.
    if !lines
        .iter()
        .any(|raw| term_start_column(term_view(raw).0).is_some())
    {
        return ownership;
    }
    let mut scan = TermScan::new(lines, closer_maxima, options, ownership);
    while scan.index < lines.len() {
        scan.step();
    }
    scan.ownership.paragraph_quotes.borrow_mut().clear();
    scan.ownership
}

fn term_view_upto(raw: &str, limit: usize) -> (&str, usize, bool) {
    let mut view = raw;
    let mut quotes = 0usize;
    let mut through_list = false;
    while quotes < limit {
        let rest = strip_blockquote_prefix(trim_ascii_start(view)).or_else(|| {
            let mut content = view;
            while let Some(rest) = term_container_content(content) {
                content = rest;
            }
            if content.len() == view.len() {
                return None;
            }
            let rest = strip_blockquote_prefix(trim_ascii_start(content))?;
            through_list = true;
            Some(rest)
        });
        let Some(rest) = rest else {
            break;
        };
        view = rest;
        quotes += 1;
    }
    (view, quotes, through_list)
}

fn term_view(raw: &str) -> (&str, usize, bool) {
    term_view_upto(raw, usize::MAX)
}

/// Does `rest`, on a line under an open term, end that term?
fn ends_open_term(lines: &[&str], index: usize, deeper_quote: bool, rest: &str) -> bool {
    deeper_quote
        || (item_block_opener(rest) && detect_block_image(rest).is_none())
        || is_colon_fence_opener_shape(rest)
        || (rest.starts_with('{')
            && standalone_attrs_block_len_iter(
                lines[index..]
                    .iter()
                    .map(|line| trim_ascii_start(term_view(line).0)),
            )
            .is_some())
        || is_list_marker(rest)
        || rest.starts_with("%%")
        || strip_definition_marker(rest).is_some()
        || parse_link_def_line_with_value(rest)
            .is_some_and(|(label, target, _)| !label.starts_with('@') && !target.trim().is_empty())
}

/// `view_line` past the list markers, and after a term its definition markers,
/// that may host a fence, with how many were stripped.
fn strip_fence_host_markers(view_line: &str, after_term: bool) -> (&str, usize) {
    let mut fence_line = view_line;
    let mut fence_markers = 0;
    while let Some(content) = detect_list_marker_full(fence_line)
        .map(|marker| marker.content)
        .or_else(|| {
            after_term
                .then(|| strip_definition_marker(trim_ascii_start(fence_line)))
                .flatten()
                .map(|(body, _)| body)
        })
    {
        fence_line = content;
        fence_markers += 1;
    }
    (fence_line, fence_markers)
}

/// How far the term pass may still probe a marker line, and where the probe may
/// restart its run.
struct TermMarkerProbe {
    budget: usize,
    root_start: usize,
    reuse_root: bool,
    last_unproved_delimiter: Option<usize>,
    unresolved_code_delimiter: bool,
}

/// A fence opener line as the term pass read it.
struct TermFenceLine<'a> {
    view_line: &'a str,
    fence_line: &'a str,
    fence_rest: &'a str,
    quotes: usize,
    fence_markers: usize,
    after_term: bool,
    admitted_term_fence: bool,
    reusable_before_fence: bool,
}

/// An opened fence whose body the term pass skips.
struct TermFenceBody {
    open: FenceOpen,
    quotes: usize,
    term_hosted: bool,
    inline_marker: bool,
    owned: bool,
    floor: usize,
    quote_columns: Option<Vec<usize>>,
    closer_ahead: bool,
}

impl TermFenceBody {
    fn ends_at(&self, line: &str, paragraph: Option<&PrepassParagraph>) -> bool {
        inline_fence_paragraph_ends(line, self.open, paragraph)
            || !inline_fence_lazy_line(line, paragraph)
                && ((self.inline_marker && is_blank_line(term_view(line).0))
                    || (self.owned
                        && term_fence_ends_above(
                            line,
                            self.floor,
                            self.quote_columns.as_deref().unwrap_or(&[]),
                        ))
                    || prepass_fence_ends_above(line, self.open, self.closer_ahead))
    }
}

struct TermScan<'a, 'o> {
    lines: &'a [&'a str],
    closer_maxima: &'a PrepassFenceClosers,
    options: &'a Options<'o>,
    ownership: TermLineOwnership,
    // Lines by the length of their leading `%` run, so finding the closer of a
    // comment fence opened on `index` stays linear over many unclosed openers.
    comment_runs: HashMap<usize, Vec<usize>>,
    prefix_costs: Vec<usize>,
    probe: TermMarkerProbe,
    term: Option<(usize, usize)>,
    term_start: Option<usize>,
    term_quote_columns: Vec<usize>,
    admitted_term_host: Option<(usize, usize, usize)>,
    admitted_host_quote_columns: Vec<usize>,
    columns: ContentColumns,
    previous_non_blank: &'a str,
    index: usize,
}

impl<'a, 'o> TermScan<'a, 'o> {
    fn new(
        lines: &'a [&'a str],
        closer_maxima: &'a PrepassFenceClosers,
        options: &'a Options<'o>,
        ownership: TermLineOwnership,
    ) -> Self {
        let mut comment_runs: HashMap<usize, Vec<usize>> = HashMap::new();
        for (k, raw) in lines.iter().enumerate() {
            let run = comment_fence_width(trim_ascii_start(term_view(raw).0));
            if run >= 3 {
                comment_runs.entry(run).or_default().push(k);
            }
        }
        let mut prefix_costs = Vec::with_capacity(lines.len() + 1);
        prefix_costs.push(0usize);
        for line in lines {
            prefix_costs.push(prefix_costs.last().unwrap().saturating_add(line.len() + 1));
        }
        let budget = probe_budget_for(*prefix_costs.last().unwrap());
        Self {
            lines,
            closer_maxima,
            options,
            ownership,
            comment_runs,
            prefix_costs,
            probe: TermMarkerProbe {
                budget,
                root_start: 0,
                reuse_root: true,
                last_unproved_delimiter: None,
                unresolved_code_delimiter: false,
            },
            term: None,
            term_start: None,
            term_quote_columns: Vec::new(),
            admitted_term_host: None,
            admitted_host_quote_columns: Vec::new(),
            columns: ContentColumns::new(),
            previous_non_blank: "",
            index: 0,
        }
    }

    fn comment_closer(&self, index: usize, fence_len: usize) -> Option<usize> {
        let at = self.comment_runs.get(&fence_len)?;
        at.get(at.partition_point(|&k| k <= index)).copied()
    }

    fn marker_admitted(&mut self, start: usize) -> bool {
        let lines = self.lines;
        let options = self.options;
        if probing() || start == 0 || is_blank_line(term_view(lines[start - 1]).0) {
            return true;
        }
        let probe = &mut self.probe;
        let prefix_costs = &self.prefix_costs;
        let run_start =
            if probe.reuse_root && options.extensions.is_empty() && probe.root_start < start {
                probe.root_start
            } else {
                0
            };
        if probe.budget
            < prefix_costs[start]
                .saturating_sub(prefix_costs[run_start])
                .saturating_mul(2)
                .saturating_add(lines[start].len())
        {
            return false;
        }
        // Parse from the last document-level block start above the marker
        // rather than the whole run, but charge the budget for the whole run.
        // The run's root is reported only when the skipped lines hold nothing
        // the definition passes could rewrite, as the whole run would require.
        let restart = last_document_root(lines, run_start, start, options);
        let probe_start = restart.map_or(run_start, |(root, _)| root);
        #[cfg(test)]
        TERM_MARKER_PROBE_BYTES.with(|bytes| {
            bytes.set(bytes.get() + prefix_costs[start] - prefix_costs[probe_start]);
        });
        let mut unaffordable = false;
        let (folds, mut proved_start) = paragraph_continuation_probe_with_start(
            &lines[probe_start..start],
            lines[start],
            options,
            &mut probe.budget,
            &mut unaffordable,
            true,
            true,
        );
        if let Some((root, closed_initial_container_end)) = restart {
            probe.budget = probe.budget.saturating_sub(
                prefix_costs[root]
                    .saturating_sub(prefix_costs[run_start])
                    .saturating_mul(2),
            );
            proved_start = if lines[run_start..root]
                .iter()
                .any(|line| probe_may_rewrite(line))
            {
                ProbeRootBoundary::default()
            } else {
                ProbeRootBoundary {
                    start: proved_start.start.map(|proved| proved + root - run_start),
                    closed_initial_container_end: closed_initial_container_end
                        .filter(|_| proved_start.start.is_some()),
                }
            };
        }
        if run_start == 0
            && !probe.unresolved_code_delimiter
            && proved_start
                .closed_initial_container_end
                .is_some_and(|end| {
                    probe
                        .last_unproved_delimiter
                        .is_some_and(|delimiter| delimiter < end)
                })
        {
            probe.reuse_root = true;
        }
        if let Some(proved_start) = proved_start.start.map(|proved| run_start + proved) {
            if proved_start >= probe.root_start && proved_start < start {
                probe.root_start = proved_start;
            }
        }
        !(folds || unaffordable)
    }

    fn step(&mut self) {
        let index = self.index;
        let line = self.lines[index];
        self.columns.observe_indexed(
            line,
            prepass_quote_line_is_folded(&self.ownership, index, line),
            Some(index),
        );
        if self.admitted_term_host.is_some()
            && strip_term_quote_prefixes(line, &self.admitted_host_quote_columns).is_none()
        {
            self.admitted_term_host = None;
        }
        let after_term = opens_definition_entry(self.previous_non_blank);
        if !is_blank_line(line) {
            self.previous_non_blank = line;
        }
        let prefix = strip_container_prefixes_at(line, &self.columns, after_term);
        remember_prepass_paragraph_line(&self.ownership, index, line, &prefix, &self.columns);
        if self.quote_is_term_text(line) {
            self.ownership.folded.insert(index);
            self.index += 1;
            return;
        }
        let (view_line, quotes, through_list) = term_view(line);
        if through_list {
            self.term = None;
        }
        if self
            .admitted_term_host
            .is_some_and(|(_, _, depth)| depth != quotes)
        {
            self.admitted_term_host = None;
        }
        if is_blank_line(view_line) {
            self.term = None;
            self.index += 1;
            return;
        }
        let col = indent_columns(view_line);
        let rest = trim_ascii_start(view_line);
        self.track_admitted_host(view_line, col, quotes, rest);
        let comment = detect_comment_fence_line(rest)
            .and_then(|open| self.comment_closer(index, open.fence_len));
        let ControlFlow::Continue(admitted_term_fence) =
            self.continue_open_term(col, quotes, rest, comment)
        else {
            return;
        };
        // A fence's payload holds no term.
        if let Some(close) = comment {
            for opaque in index..=close {
                end_prepass_paragraph_at(&self.ownership, opaque);
            }
            self.previous_non_blank = self.lines[close];
            self.index = close + 1;
            return;
        }
        let (fence_line, fence_markers) = strip_fence_host_markers(view_line, after_term);
        let fence_rest = trim_ascii_start(fence_line);
        let reusable_before_fence = self.probe.reuse_root;
        // An unproved delimiter may acquire a closer in a later prefix.
        if (detect_fence_open(fence_rest).is_some() && !admitted_term_fence)
            || is_colon_fence_opener_shape(rest)
        {
            self.probe.reuse_root = false;
            self.probe.root_start = 0;
            self.probe.last_unproved_delimiter = Some(index);
        }
        if let Some(open) = detect_fence_open(fence_rest) {
            let at = TermFenceLine {
                view_line,
                fence_line,
                fence_rest,
                quotes,
                fence_markers,
                after_term,
                admitted_term_fence,
                reusable_before_fence,
            };
            self.scan_fence(&at, open);
            return;
        }
        // A term may follow list markers on its line; its column is then the
        // item's content column.
        if let Some(authored_col) = term_start_column(view_line) {
            let marker_line = detect_list_marker_full(view_line).is_some();
            let host_col = self.columns.reached_by_at(quotes, authored_col);
            let body_host = self
                .columns
                .frames
                .get(quotes)
                .is_some_and(|frame| frame.body_columns.contains(&host_col));
            if authored_col == 0 || marker_line || host_col == authored_col || body_host {
                // A term overindented inside a description still measures its
                // following lines against the description's content column.
                // The term marker's own column is not a content column (P2-028).
                self.term = Some((
                    if body_host && !marker_line {
                        host_col
                    } else {
                        authored_col
                    },
                    quotes,
                ));
                self.term_start = Some(index);
                self.term_quote_columns = term_quote_columns_for(line, &self.columns);
            }
        }
        self.index += 1;
    }

    /// A `>` past an open term's column is term text, not a deeper quote.
    fn quote_is_term_text(&self, line: &str) -> bool {
        let Some((term_col, term_quotes)) = self.term else {
            return false;
        };
        let (at_term, quotes, through_list) = term_view_upto(line, term_quotes);
        let rest = trim_ascii_start(at_term);
        !through_list
            && quotes == term_quotes
            && !is_blank_line(at_term)
            && (indent_columns(at_term) > term_col
                || (term_quotes > 0
                    && self
                        .term_quote_columns
                        .first()
                        .is_some_and(|column| indent_columns(line) > *column)))
            && (strip_blockquote_prefix(rest).is_some()
                || strip_definition_marker(rest).is_some()
                || strip_blockquote_prefix(trim_ascii_start(line)).is_some())
    }

    fn track_admitted_host(&mut self, view_line: &str, col: usize, quotes: usize, rest: &str) {
        if let Some((base, _, depth)) = self
            .admitted_term_host
            .filter(|&(base, _, depth)| depth == quotes && col == base)
        {
            if let Some((_, floor)) = detect_prepass_list_marker(view_line) {
                self.admitted_term_host = Some((base, floor, depth));
            }
        }
        if self
            .admitted_term_host
            .is_some_and(|(base, floor, _)| col < base || (col < floor && !is_list_marker(rest)))
        {
            self.admitted_term_host = None;
        }
    }

    /// Folds the line into an open term, breaking, or reports whether a fence at
    /// the term's column belongs to the term.
    fn continue_open_term(
        &mut self,
        col: usize,
        quotes: usize,
        rest: &str,
        comment: Option<usize>,
    ) -> ControlFlow<(), bool> {
        let index = self.index;
        let Some((term_col, term_quotes)) = self.term else {
            return ControlFlow::Continue(false);
        };
        if quotes == term_quotes && col > term_col && !is_list_marker(rest) {
            // A folded comment fence takes its body and closer with it.
            let last = comment.unwrap_or(index);
            self.ownership.folded.extend(index..=last);
            self.previous_non_blank = self.lines[last];
            self.index = last + 1;
            return ControlFlow::Break(());
        }
        let mut admitted_term_fence = false;
        if quotes == term_quotes
            && col == term_col
            && detect_fence_open(rest).is_some()
            && self.term_fence_admitted(term_col, quotes)
        {
            admitted_term_fence = true;
            let line = self.lines[index];
            let quote_columns =
                if strip_term_quote_prefixes(line, &self.term_quote_columns).is_some() {
                    self.term_quote_columns.clone()
                } else {
                    term_quote_columns_for(line, &self.columns)
                };
            self.ownership.fence_openers.insert(
                index,
                TermFenceHost {
                    content_col: col,
                    quote_depth: quotes,
                    quote_columns,
                    opener_offset: rest.as_ptr() as usize - line.as_ptr() as usize,
                },
            );
        }
        if ends_open_term(self.lines, index, quotes > term_quotes, rest) {
            self.term = None;
        }
        ControlFlow::Continue(admitted_term_fence)
    }

    /// Does the open term's marker line host a fence at the term's column?
    fn term_fence_admitted(&mut self, term_col: usize, quotes: usize) -> bool {
        let Some(start) = self.term_start else {
            return false;
        };
        let marker_view = term_view(self.lines[start]).0;
        let marker_col = indent_columns(marker_view);
        if term_col == 0
            || self.admitted_term_host.is_some_and(|(base, floor, depth)| {
                depth == quotes
                    && (marker_col == floor
                        || (marker_col == base && is_list_marker(trim_ascii_start(marker_view))))
            })
        {
            return true;
        }
        self.marker_admitted(start)
    }

    fn scan_fence(&mut self, at: &TermFenceLine<'a>, mut open: FenceOpen) {
        let lines = self.lines;
        let index = self.index;
        open.content_col = at.view_line
            [..at.fence_line.as_ptr() as usize - at.view_line.as_ptr() as usize]
            .chars()
            .count()
            + indent_columns(at.fence_line);
        open.quoted = at.quotes > 0;
        let fence = prepass_fence_opens(
            lines,
            index,
            open,
            self.closer_maxima,
            &self.ownership,
            &self.columns,
            at.after_term,
        );
        if !fence.opens {
            self.probe.unresolved_code_delimiter = true;
            self.index += 1;
            return;
        }
        // A column-zero term after a blank starts its own block. Its
        // immediate definition marker cannot continue an earlier paragraph.
        let direct_term_body = index >= 2
            && at.after_term
            && self.term_start == Some(index - 1)
            && strip_definition_marker(trim_ascii_start(at.view_line)).is_some()
            && term_view(lines[index - 1]).1 == at.quotes
            && term_start_column(term_view(lines[index - 1]).0) == Some(0)
            && is_blank_line(term_view(lines[index - 2]).0);
        if at.fence_markers == 1
            && (at.admitted_term_fence || direct_term_body || self.marker_admitted(index))
        {
            self.ownership.fence_openers.insert(
                index,
                TermFenceHost {
                    content_col: open.content_col,
                    quote_depth: at.quotes,
                    quote_columns: term_quote_columns_for(lines[index], &self.columns),
                    opener_offset: at.fence_rest.as_ptr() as usize - lines[index].as_ptr() as usize,
                },
            );
        }
        let term_host = self
            .ownership
            .fence_openers
            .get(&index)
            .map(|host| (host.content_col, host.quote_columns.clone()));
        let inline_marker = fence.inline_marker && term_host.is_none();
        let paragraph = if inline_marker {
            previous_prepass_paragraph(&self.ownership, index)
        } else {
            None
        };
        let mut body = TermFenceBody {
            open,
            quotes: at.quotes,
            term_hosted: term_host.is_some(),
            inline_marker,
            owned: term_host.is_some(),
            floor: term_host
                .as_ref()
                .map_or(fence.host_content_col, |(content_col, _)| *content_col),
            quote_columns: term_host.map(|(_, quote_columns)| quote_columns),
            closer_ahead: fence.closer_ahead,
        };
        if body.quote_columns.is_none() && !fence.quote_columns.is_empty() {
            body.quote_columns = Some(fence.quote_columns);
            body.owned = true;
        }
        end_prepass_paragraph_at(&self.ownership, index);
        self.index += 1;
        let consumed_closer = self.consume_fence_body(&body, paragraph);
        if at.reusable_before_fence && consumed_closer {
            self.probe.reuse_root = true;
        }
        if !at.admitted_term_fence && !consumed_closer {
            self.probe.unresolved_code_delimiter = true;
        }
        if at.admitted_term_fence {
            self.adopt_term_fence_host(
                at.quotes,
                open.content_col,
                body.quote_columns.unwrap_or_default(),
            );
        }
    }

    /// Skips a fence's body, reporting whether its closer was consumed.
    fn consume_fence_body(
        &mut self,
        body: &TermFenceBody,
        mut paragraph: Option<PrepassParagraph>,
    ) -> bool {
        while self.index < self.lines.len() {
            let index = self.index;
            let line = self.lines[index];
            if body.ends_at(line, paragraph.as_ref()) {
                if body.term_hosted {
                    self.record_term_fence_after(line);
                }
                return false;
            }
            end_prepass_paragraph_at(&self.ownership, index);
            if !body.owned {
                self.drop_hosts_left_by(line);
            }
            advance_inline_paragraph(line, paragraph.as_mut());
            let kept = strip_prepass_quote_levels(line, body.quotes).unwrap_or(line);
            let close_line = if leading_ws(kept) >= body.open.content_col {
                &kept[body.open.content_col..]
            } else {
                kept
            };
            let closes = is_fence_close(close_line, body.open);
            if !is_blank_line(line) {
                self.previous_non_blank = line;
            }
            self.index += 1;
            if closes {
                if body.inline_marker {
                    self.ownership
                        .paragraph_quotes
                        .borrow_mut()
                        .insert(index, paragraph);
                }
                return true;
            }
        }
        false
    }

    /// A fence that ends a term-hosted fence at its column is itself term-hosted.
    fn record_term_fence_after(&mut self, line: &str) {
        let (next, depth, _) = term_view(line);
        let next = trim_ascii_start(next);
        if detect_fence_open(next).is_some() {
            self.ownership.fence_openers.insert(
                self.index,
                TermFenceHost {
                    content_col: indent_columns(term_view(line).0),
                    quote_depth: depth,
                    quote_columns: term_quote_columns_for(line, &self.columns),
                    opener_offset: next.as_ptr() as usize - line.as_ptr() as usize,
                },
            );
        }
    }

    /// Forgets the list hosts a fence body line falls out of.
    fn drop_hosts_left_by(&mut self, line: &str) {
        let (body, depth, _) = term_view(line);
        for (frame_depth, frame) in self.columns.frames.iter_mut().enumerate() {
            if frame_depth > depth
                || (frame_depth == depth
                    && !is_blank_line(body)
                    && frame.first_list_col.is_some_and(|(base, floor)| {
                        let col = indent_columns(body);
                        col < base || (col < floor && !is_list_marker(trim_ascii_start(body)))
                    }))
            {
                frame.first_list_col = None;
                frame.first_list_start = None;
            }
        }
        if self
            .admitted_term_host
            .is_some_and(|(_, floor, host_depth)| {
                depth != host_depth
                    || strip_term_quote_prefixes(line, &self.admitted_host_quote_columns).is_none()
                    || (!is_blank_line(line) && indent_columns(body) < floor)
            })
        {
            self.admitted_term_host = None;
        }
    }

    // A proved term fence establishes an actual item host. Keep its
    // column through sibling markers and in-item continuations, but
    // discard it on every non-marker line below that column.
    fn adopt_term_fence_host(
        &mut self,
        quotes: usize,
        content_col: usize,
        host_quote_columns: Vec<usize>,
    ) {
        let lines = self.lines;
        let Some(start) = self
            .term_start
            .filter(|&start| detect_list_marker_full(term_view(lines[start]).0).is_some())
        else {
            return;
        };
        let marker_col = indent_columns(term_view(lines[start]).0);
        if self.admitted_term_host.is_some_and(|(base, floor, depth)| {
            depth == quotes && (marker_col == base || marker_col == floor)
        }) {
            return;
        }
        let own_host = (marker_col, content_col);
        let outer = self
            .columns
            .frames
            .get(quotes)
            .and_then(|frame| Some((frame.first_list_col?, frame.first_list_start?)));
        let (base, floor) = match outer {
            Some((host, outer_start))
                if host.0 == marker_col
                    || (term_quote_columns_for(lines[outer_start], &self.columns)
                        == host_quote_columns
                        && self.marker_admitted(outer_start)) =>
            {
                host
            }
            _ => own_host,
        };
        self.admitted_term_host = Some((base, floor, quotes));
        self.admitted_host_quote_columns = host_quote_columns;
    }
}

/// The column of a `:: ` term that opens this line, after any list or
/// description markers.
pub(in crate::parse) fn term_start_column(line: &str) -> Option<usize> {
    let mut base = 0;
    let mut rest = line;
    loop {
        let trimmed = trim_ascii_start(rest);
        if is_definition_list_start(trimmed) {
            return Some(base + indent_columns(rest));
        }
        if let Some((body, width)) = strip_definition_marker(trimmed) {
            base += indent_columns(rest) + width;
            rest = body;
            continue;
        }
        let marker = detect_list_marker_full(rest)?;
        let offset = (marker.content.as_ptr() as usize).checked_sub(rest.as_ptr() as usize)?;
        if offset == 0 || offset > rest.len() {
            return None;
        }
        base += rest[..offset].chars().count();
        rest = &rest[offset..];
    }
}

/// Closing runs indexed by normalized column and character.
///
/// The global maxima retain the prepass admission gate; each column bucket
/// stores suffix widths so quote exits cannot make later openers rescan payload.
pub(in crate::parse) struct PrepassFenceClosers {
    pub(in crate::parse) maxima: (Vec<usize>, Vec<usize>),
    pub(in crate::parse) by_column: HashMap<(bool, u8, usize), Vec<(usize, usize)>>,
}

impl PrepassFenceClosers {
    pub(in crate::parse) fn has_run_after(&self, index: usize, open: FenceOpen) -> bool {
        let suffix_max = if open.fence_char == b'`' {
            self.maxima.0[index + 1]
        } else {
            self.maxima.1[index + 1]
        };
        suffix_max >= open.fence_len
    }

    pub(in crate::parse) fn closes_after_at(
        &self,
        index: usize,
        open: FenceOpen,
        column: usize,
    ) -> bool {
        self.by_column
            .get(&(open.quoted, open.fence_char, column))
            .is_some_and(|runs| {
                let next = runs.partition_point(|&(at, _)| at <= index);
                runs.get(next)
                    .is_some_and(|&(_, width)| width >= open.fence_len)
            })
    }

    pub(in crate::parse) fn closes_after(&self, index: usize, open: FenceOpen) -> bool {
        let suffix_max = if open.fence_char == b'`' {
            self.maxima.0[index + 1]
        } else {
            self.maxima.1[index + 1]
        };
        if suffix_max < open.fence_len {
            return false;
        }
        [0, open.content_col]
            .into_iter()
            .any(|column| self.closes_after_at(index, open, column))
    }
}

pub(in crate::parse) fn fence_closer_suffix_maxima(lines: &[&str]) -> PrepassFenceClosers {
    let mut backtick = vec![0usize; lines.len() + 1];
    let mut tilde = vec![0usize; lines.len() + 1];
    for index in (0..lines.len()).rev() {
        backtick[index] = backtick[index + 1];
        tilde[index] = tilde[index + 1];
        let mut candidate = trim_ascii_start(lines[index]);
        while let Some(rest) = strip_blockquote_prefix(candidate) {
            candidate = trim_ascii_start(rest);
        }
        let Some(&fence_char) = candidate.as_bytes().first() else {
            continue;
        };
        if fence_char != b'`' && fence_char != b'~' {
            continue;
        }
        let run = candidate
            .as_bytes()
            .iter()
            .take_while(|byte| **byte == fence_char)
            .count();
        if run < 3
            || !candidate[run..]
                .bytes()
                .all(|byte| byte == b' ' || byte == b'\t')
        {
            continue;
        }
        if fence_char == b'`' {
            backtick[index] = backtick[index].max(run);
        } else {
            tilde[index] = tilde[index].max(run);
        }
    }
    let mut by_column: HashMap<(bool, u8, usize), Vec<(usize, usize)>> = HashMap::new();
    if backtick[0] > 0 || tilde[0] > 0 {
        for (index, &line) in lines.iter().enumerate() {
            for quoted in [false, true] {
                let line = if quoted {
                    strip_prepass_blockquote_prefix(line).unwrap_or(line)
                } else {
                    line
                };
                let kept = strip_container_prefixes_keep_indent(line);
                let column = leading_ws(&kept);
                let candidate = &kept[column..];
                let Some(&character @ (b'`' | b'~')) = candidate.as_bytes().first() else {
                    continue;
                };
                let width = candidate
                    .bytes()
                    .take_while(|&byte| byte == character)
                    .count();
                if width >= 3
                    && candidate[width..]
                        .bytes()
                        .all(|byte| byte == b' ' || byte == b'\t')
                {
                    by_column
                        .entry((quoted, character, column))
                        .or_default()
                        .push((index, width));
                }
            }
        }
    }
    for runs in by_column.values_mut() {
        let mut maximum = 0;
        for (_, width) in runs.iter_mut().rev() {
            maximum = maximum.max(*width);
            *width = maximum;
        }
    }
    PrepassFenceClosers {
        maxima: (backtick, tilde),
        by_column,
    }
}

/// Does the line above `index` leave a paragraph open for §10 I4 to ask about?
///
/// Three shapes open none. A blank line ends one. A collected definition's
/// placeholder marks a block boundary and never was a paragraph (carve-rs#2092).
/// And §24 C3 makes a `%%` comment close the paragraph AT ANY COLUMN, so the
/// fence below it interrupts nothing - which is the whole condition the
/// lookahead is guarding (carve-rs#2096).
pub(in crate::parse) fn prepass_paragraph_open_above(
    lines: &[&str],
    index: usize,
    quote_columns: &[usize],
    terms: &TermLineOwnership,
) -> bool {
    if index == 0 {
        return false;
    }
    let above = lines[index - 1];
    // AND IT HAS TO BE A PARAGRAPH THIS LINE COULD CONTINUE. A line indented
    // PAST this one belonged to a container this line leaves, so whatever it left
    // open closed with that container and nothing here can interrupt it: a
    // column-0 fence under a note body's `   a` really opens, which is what the
    // block parser reads and what the sentinel guard pins.
    let paragraph_quotes = terms.paragraph_quotes.borrow();
    let same_quote_paragraph = quote_columns.is_empty()
        || paragraph_quotes
            .get(&(index - 1))
            .and_then(Option::as_ref)
            .is_some_and(|host| host.quote_columns.starts_with(quote_columns));
    same_quote_paragraph
        && !is_blank_line(above)
        && indent_columns(above) <= indent_columns(lines[index])
        && !is_definition_placeholder(above)
        && !is_line_comment_any_column(above)
        && !prepass_definition_line(
            strip_term_quote_prefixes(above, quote_columns).unwrap_or(above),
        )
}

pub(in crate::parse) fn remember_prepass_paragraph_line(
    terms: &TermLineOwnership,
    index: usize,
    raw: &str,
    prefix: &StrippedContainerLine<'_>,
    columns: &ContentColumns,
) {
    if !terms.tracks_quote_context {
        return;
    }
    let mut contexts = terms.paragraph_quotes.borrow_mut();
    let mut paragraph = index
        .checked_sub(1)
        .and_then(|above| contexts.get(&above))
        .cloned()
        .flatten();
    let description_prefix;
    let prefix = if columns
        .frames
        .get(
            prefix
                .structural
                .bytes()
                .filter(|byte| *byte == b'>')
                .count(),
        )
        .is_some_and(|frame| frame.def_list.is_some())
        && strip_definition_marker(trim_ascii_start(prefix.bare)).is_some()
    {
        description_prefix = strip_container_prefixes_at(raw, columns, true);
        &description_prefix
    } else {
        prefix
    };
    let quotes = term_quote_columns_for(prefix.structural, columns);
    if quotes.is_empty()
        && paragraph
            .as_ref()
            .map_or(true, |host| host.quote_columns.is_empty())
    {
        return;
    }
    let same_host = paragraph
        .as_ref()
        .is_some_and(|host| host.quote_columns.starts_with(&quotes));
    let rest = paragraph
        .as_ref()
        .filter(|_| same_host)
        .map(|host| {
            strip_term_quote_prefixes(raw, &host.quote_columns[..quotes.len()]).unwrap_or(raw)
        })
        .unwrap_or(prefix.bare);
    if is_blank_line(raw)
        || is_plus_marker(raw)
        || is_definition_placeholder(raw)
        || prepass_definition_line(rest)
        || trim_ascii_start(rest).starts_with("%%")
    {
        paragraph = None;
    } else if !quotes.is_empty()
        || paragraph
            .as_ref()
            .is_some_and(|host| !host.quote_columns.is_empty())
    {
        let starts_item = same_host
            && paragraph.as_ref().is_some_and(|host| {
                strip_term_quote_prefixes(raw, &host.quote_columns).is_some()
                    && is_list_marker(trim_ascii_start(rest))
                    && !host.list_columns.is_empty()
                    && !host.folds_marker(rest)
            });
        let inherited =
            same_host && !starts_item && paragraph.as_ref().is_some_and(|host| host.absorption);
        let mut admission =
            ParaOpen::from_line(if starts_item { prefix.bare } else { rest }, inherited);
        if admission.get() {
            let quote_columns = if same_host {
                paragraph.as_ref().unwrap().quote_columns.clone()
            } else {
                std::rc::Rc::from(quotes)
            };
            let list_columns = if same_host && !starts_item {
                paragraph.as_ref().unwrap().list_columns.clone()
            } else {
                std::rc::Rc::from(
                    columns
                        .frames
                        .get(quote_columns.len())
                        .map_or(&[][..], |frame| frame.list_columns.as_slice()),
                )
            };
            paragraph = Some(PrepassParagraph {
                list_columns,
                quote_columns,
                absorption: admission.absorption(),
            });
        } else {
            paragraph = None;
        }
    } else {
        paragraph = Some(PrepassParagraph {
            list_columns: std::rc::Rc::from([]),
            quote_columns: std::rc::Rc::from(quotes),
            absorption: false,
        });
    }
    contexts.insert(index, paragraph);
}

pub(in crate::parse) fn prepass_quote_line_is_folded(
    terms: &TermLineOwnership,
    index: usize,
    line: &str,
) -> bool {
    if !terms.tracks_quote_context {
        return false;
    }
    let Some(paragraph) = previous_prepass_paragraph(terms, index) else {
        return false;
    };
    if paragraph.quote_columns.is_empty() {
        return false;
    }
    if inline_fence_lazy_line(line, Some(&paragraph)) {
        return true;
    }
    strip_term_quote_prefixes(line, &paragraph.quote_columns).is_some_and(|content| {
        is_list_marker(trim_ascii_start(content))
            && paragraph.folds_marker(content)
            && ParaOpen::from_line(content, paragraph.absorption).get()
    })
}

pub(in crate::parse) fn advance_inline_paragraph(
    line: &str,
    paragraph: Option<&mut PrepassParagraph>,
) {
    if let Some(paragraph) = paragraph {
        if let Some(content) = strip_term_quote_prefixes(line, &paragraph.quote_columns) {
            paragraph.absorption = ParaOpen::from_line(content, paragraph.absorption).absorption();
        }
    }
}

pub(in crate::parse) fn inline_fence_paragraph_ends(
    line: &str,
    open: FenceOpen,
    paragraph: Option<&PrepassParagraph>,
) -> bool {
    let Some(paragraph) = paragraph else {
        return false;
    };
    let Some(content) = strip_term_quote_prefixes(line, &paragraph.quote_columns) else {
        return false;
    };
    let close_line = if leading_ws(content) >= open.content_col {
        &content[open.content_col..]
    } else {
        content
    };
    !is_fence_close(close_line, open)
        && ((prepass_definition_line(content)
            && paragraph
                .list_columns
                .last()
                .is_some_and(|(_, floor)| indent_columns(content) < *floor))
            || !ParaOpen::from_line(content, paragraph.absorption).get())
}

pub(in crate::parse) fn inline_fence_lazy_line(
    line: &str,
    paragraph: Option<&PrepassParagraph>,
) -> bool {
    let Some(paragraph) = paragraph else {
        return false;
    };
    let unquoted = without_blockquote_prefixes(line);
    if strip_term_quote_prefixes(line, &paragraph.quote_columns).is_some()
        || is_blank_line(unquoted)
        || prepass_definition_line(unquoted)
    {
        return false;
    }
    ParaOpen::from_line(unquoted, paragraph.absorption).get()
}

pub(in crate::parse) fn previous_prepass_paragraph(
    terms: &TermLineOwnership,
    index: usize,
) -> Option<PrepassParagraph> {
    index.checked_sub(1).and_then(|previous| {
        terms
            .paragraph_quotes
            .borrow()
            .get(&previous)
            .cloned()
            .flatten()
    })
}

pub(in crate::parse) fn end_prepass_paragraph_at(terms: &TermLineOwnership, index: usize) {
    if terms.tracks_quote_context {
        terms.paragraph_quotes.borrow_mut().insert(index, None);
    }
}

/// Is `line` a definition, in any of the three spellings?
///
/// The COLLECTED form already answers no above - that is what the placeholder is
/// for - and the uncollected form has to answer the same, or the pass disagrees
/// with itself depending on which pre-pass ran first. A definition is a block
/// and opens no paragraph, so a fence under one interrupts nothing and really
/// opens: without this, a fence written under a definition inside a note body
/// lost its payload and the definition below it was collected out of verbatim.
pub(in crate::parse) fn prepass_definition_line(line: &str) -> bool {
    let bare = trim_ascii_start(line);
    parse_footnote_def_line(bare).is_some()
        || parse_link_def_line(bare).is_some_and(|(label, target)| {
            !label.starts_with('@') && !trim_ascii(target).is_empty()
        })
        || detect_abbreviation_def(bare).is_some()
}

/// Whether the run of nonblank lines ending at `index` began on a list marker.
fn list_run(lines: &[&str], index: usize, terms: &TermLineOwnership) -> bool {
    let mut memo = terms.list_runs.borrow_mut();
    let mut at = index;
    let found = loop {
        if let Some(&known) = memo.get(&at) {
            break known;
        }
        let bare = lines[at].trim_start_matches([' ', '\t', '>']);
        if is_blank_line(bare) {
            break false;
        }
        if is_list_marker(bare) || at == 0 {
            break is_list_marker(bare);
        }
        at -= 1;
    };
    for line in at..=index {
        memo.insert(line, found);
    }
    found
}

/// Is there a line below `index` that closes `open`?
///
/// The suffix maximum rejects absent runs; column buckets answer compatible
/// runs without scanning the remaining source.
pub(in crate::parse) fn prepass_fence_closer_ahead(
    _lines: &[&str],
    index: usize,
    open: FenceOpen,
    maxima: &PrepassFenceClosers,
) -> bool {
    if true {
        return true;
    }
    maxima.closes_after(index, open)
}

/// Whether a fence opener on `lines[index]` really opens a fence for a
/// definition pre-pass, and whether a closer is waiting for it.
///
/// §10 I4: an UNTERMINATED fence cannot interrupt an open paragraph. Going
/// opaque there hides every definition below it from collection, so the pass
/// must stay open instead (`:` / ` ``` ` / `[A]: b`). With no paragraph above,
/// or after a definition term at its container column (CARVE-P2-028), the
/// fence is real and its body is verbatim. A closer also makes it real.
pub(in crate::parse) fn prepass_fence_opens(
    lines: &[&str],
    index: usize,
    open: FenceOpen,
    maxima: &PrepassFenceClosers,
    terms: &TermLineOwnership,
    columns: &ContentColumns,
    after_term: bool,
) -> PrepassFence {
    let closer_ahead = prepass_fence_closer_ahead(lines, index, open, maxima);
    let prefix = strip_container_prefixes_at(lines[index], columns, after_term);
    let quote_columns = term_quote_columns_for(prefix.structural, columns);
    let paragraph_above = prepass_paragraph_open_above(lines, index, &quote_columns, terms);
    let item_paragraph = previous_prepass_paragraph(terms, index);
    let inline_marker_fence = closer_ahead
        && !terms.fence_openers.contains_key(&index)
        && !quote_columns.is_empty()
        && paragraph_above
        && index.checked_sub(1).is_some_and(|above| {
            strip_term_quote_prefixes(lines[index], &quote_columns).is_some_and(|current| {
                let previous =
                    strip_term_quote_prefixes(lines[above], &quote_columns).unwrap_or(lines[above]);
                is_list_marker(trim_ascii_start(current))
                    && item_paragraph
                        .as_ref()
                        .map_or(true, |paragraph| paragraph.folds_marker(current))
                    && (item_paragraph
                        .as_ref()
                        .is_some_and(|paragraph| !paragraph.list_columns.is_empty())
                        || indent_columns(previous) <= indent_columns(current))
            })
        });
    let follows_colon_opener = index
        .checked_sub(1)
        .is_some_and(|above| detect_container_open(trim_ascii_start(lines[above])).is_some());
    // A marker the stripper consumed starts an item body when a list is open
    // above it (I2), so no paragraph there can hold a fence on its line.
    let stripped_marker = prefix
        .structural
        .chars()
        .any(|character| !matches!(character, '>' | ' ' | '\t'));
    let marker_line = stripped_marker
        && index
            .checked_sub(1)
            .is_some_and(|above| list_run(lines, above, terms));
    // A fence opener on its own line interrupts an open paragraph, closer or not
    // (carve#4018); after a marker the paragraph folded in, it is that paragraph's text.
    PrepassFence {
        opens: !stripped_marker
            || terms.fence_openers.contains_key(&index)
            || !paragraph_above
            || follows_colon_opener
            || marker_line,
        quote_columns,
        host_content_col: if inline_marker_fence {
            0
        } else {
            columns.reached_by(indent_columns(prefix.bare)).max(
                if prefix
                    .structural
                    .chars()
                    .any(|character| !matches!(character, '>' | ' ' | '\t'))
                {
                    columns.current().cols.last().copied().unwrap_or(0)
                } else {
                    0
                },
            )
        },
        closer_ahead,
        inline_marker: inline_marker_fence,
    }
}

pub(in crate::parse) struct PrepassFence {
    pub(in crate::parse) quote_columns: Vec<usize>,
    pub(in crate::parse) host_content_col: usize,
    pub(in crate::parse) opens: bool,
    pub(in crate::parse) closer_ahead: bool,
    pub(in crate::parse) inline_marker: bool,
}

/// Does an open fence END above `line` because the line leaves its container?
///
/// An UNTERMINATED fence ends where its host does (PART 1 S4's otherwise), so a
/// line below the fence's own content column is no longer its payload and a
/// definition written there still has to be collected: `. ~~~` / `[d]: u` reads
/// the definition at the document's column (carve-rs#2096). A closer ahead
/// does not change that: the closer search passes a below-column line and the
/// line still ends the fence (CARVE-P0-014). A quoted line's raw indent does
/// not measure the fence's column, so it ends only a fence with no closer.
pub(in crate::parse) fn prepass_fence_ends_above(
    line: &str,
    open: FenceOpen,
    closer_ahead: bool,
) -> bool {
    !open.quoted
        && open.content_col > 0
        && !is_blank_line(line)
        && !(closer_ahead && prepass_line_is_quoted(line))
        && indent_columns(line) < open.content_col
}

pub(in crate::parse) fn term_container_content(line: &str) -> Option<&str> {
    detect_list_marker_full(line)
        .map(|marker| marker.content)
        .or_else(|| strip_definition_marker(trim_ascii_start(line)).map(|(body, _)| body))
}

pub(in crate::parse) fn term_quote_columns_for(
    mut line: &str,
    columns: &ContentColumns,
) -> Vec<usize> {
    let mut floors = Vec::new();
    loop {
        let base = line;
        while let Some(content) = term_container_content(line) {
            line = content;
        }
        let marker_col = line.as_ptr() as usize - base.as_ptr() as usize;
        let floor = marker_col.max(columns.reached_by_at(floors.len(), indent_columns(base)));
        let Some(rest) = strip_prepass_blockquote_prefix(line) else {
            break;
        };
        floors.push(floor);
        line = rest;
    }
    floors
}

pub(in crate::parse) fn strip_term_quote_prefixes<'a>(
    mut line: &'a str,
    quote_columns: &[usize],
) -> Option<&'a str> {
    for &floor in quote_columns {
        if indent_columns(line) < floor {
            return None;
        }
        line = strip_prepass_blockquote_prefix(line)?;
    }
    Some(line)
}

pub(in crate::parse) fn term_fence_ends_above(
    line: &str,
    content_col: usize,
    quote_columns: &[usize],
) -> bool {
    let Some(kept) = strip_term_quote_prefixes(line, quote_columns) else {
        return true;
    };
    !is_blank_line(kept) && indent_columns(kept) < content_col
}

pub(in crate::parse) fn extract_link_defs(
    source: &str,
    options: &Options<'_>,
) -> (String, BTreeMap<String, LinkDef>) {
    extract_link_defs_with_guard(source, options, None)
}

/// The pipeline's spelling, which asks the block parser before it cuts a
/// definition out of a LIST MARKER line.
///
/// `guard` is `None` for the probe inside
/// [`line_folds_into_an_open_paragraph`], which calls this pass to strip
/// definitions from the run it parses. That is what breaks the recursion: the
/// guarded pass asks the probe, the probe asks the unguarded pass, and the
/// unguarded pass asks nothing.
pub(in crate::parse) fn extract_link_defs_guarded(
    source: &str,
    options: &Options<'_>,
    budget: &mut usize,
) -> (String, BTreeMap<String, LinkDef>) {
    extract_link_defs_with_guard(source, options, Some(budget))
}

pub(in crate::parse) fn extract_link_defs_with_guard(
    source: &str,
    options: &Options<'_>,
    guard: Option<&mut usize>,
) -> (String, BTreeMap<String, LinkDef>) {
    if options.extensions.is_empty()
        && !source_may_contain_link_definition(source)
        && !source.contains('\r')
    {
        let lines: Vec<_> = source.lines().collect();
        return (joined_source(&lines), BTreeMap::new());
    }
    extract_link_defs_with_guard_full(source, options, guard)
}

pub(in crate::parse) fn extract_link_defs_with_guard_full(
    source: &str,
    options: &Options<'_>,
    mut guard: Option<&mut usize>,
) -> (String, BTreeMap<String, LinkDef>) {
    let has_candidate = source_may_contain_link_definition(source);
    let mut body: Vec<std::borrow::Cow<'_, str>> = Vec::new();
    let mut defs = BTreeMap::new();
    let mut in_fence: Option<FenceOpen> = None;
    let mut term_fence_quotes = 0;
    let mut term_fence_columns = Vec::new();
    let mut term_fence_owned = false;
    let mut description_fence = false;
    let mut description_blank = false;
    let mut inline_marker_fence = false;
    let mut inline_marker_paragraph = None;
    let mut term_fence_floor = 0;
    // Whether that fence has a closer, which decides whether a quoted line ends it.
    let mut fence_closer_ahead = false;
    // A LINE BLOCK's body is inline content (`line_block_line = {whitespace},
    // inline_content, newline`), so a definition-shaped line inside one is text,
    // not a definition. Without this the line was extracted here and never
    // reached the block parser, so it vanished from the output entirely -
    // carve-js and carve-php both render it (#491).
    //
    // The block parser supplies source-line ownership for both prepasses.
    let verse_lines = verse_owned_lines(source, options);
    // A `%%%` comment fence is opaque, so a literal `::: |` inside one is not
    // an opener. Entering the state there left it open past the comment's own
    // closer - which is not a colon fence - and every later definition in the
    // document was skipped. Comment bodies remain opaque here.
    let mut in_comment_fence: Option<OpenCommentFence> = None;
    // Track enclosing list item content columns so the strict fence test can be
    // re-based to the item's content column. This remains a line-based
    // approximation: tab-vs-space marker alignment is char-counted, the
    // post-blank baseIndent+2 continuation rule is not modeled, and lists
    // nested inside blockquotes are not fully modeled. Those residual cases can
    // still produce a spurious link, not content loss; the sound fix is
    // collecting definitions during block parsing.
    let mut columns = ContentColumns::new();
    // The innermost `:::` colon-fence container (div / admonition) this pre-pass
    // is inside, as (content column, fence length). A CONTAINER OWNS A LINE PAST
    // ITS OWN CONTENT COLUMN: a link definition written DEEPER than the fence's
    // column is the container's own text, not a hoistable definition (corpus
    // 451). Only the colon fence changes this - a plain list item still absorbs
    // an authored-base residual and registers (carve#1705) - and a definition AT
    // the fence's column still registers, since the div body is parsed normally.
    let mut colon_fence: Option<(usize, usize)> = None;
    // Collected so an unterminated `%%%` can be told from a real fenced comment
    // before the state is entered - see comment_fence_closes.
    let all_lines: Vec<&str> = source.lines().collect();
    let recent_list_marker = list_marker_since_blank(&all_lines);
    let comment_closers = comment_fence_close_index(&all_lines);
    let closer_maxima = fence_closer_suffix_maxima(&all_lines);
    let folded_into_term = term_folded_lines(&all_lines, &closer_maxima, options);
    // See the note in `extract_footnote_defs`: lazy, so a document with no
    // container-scoped comment fence pays nothing for it.
    let mut container_closers: Option<ContainerCommentClosers> = None;
    // See `extract_footnote_defs`: the line the `after_term` gate asks about.
    let mut previous_non_blank = "";
    let mut last_folded_marker = false;
    for (line_index, line) in all_lines.iter().copied().enumerate() {
        let folded_fence = detect_fence_open(trim_ascii_start(line)).is_some_and(|open| {
            !closer_maxima.has_run_after(line_index, open)
                && prepass_paragraph_open_above(&all_lines, line_index, &[], &folded_into_term)
        });
        // Verse text is opaque: a line-block body line like `- verse` is not a
        // list marker, and letting it push a content column left the NEXT
        // top-level opener unprotected. A COMMENT body is opaque the same way,
        // and was missing here where `extract_footnote_defs` already had it: a
        // `- hidden` inside a comment seeded a content column that outlived the
        // fence, so `  [r]: /u` after it was stripped to that phantom column and
        // registered, where the block parser sees a top-level line two columns
        // in and reads it as text.
        let mut marker_probe_unaffordable = false;
        let folded_marker = has_candidate
            && marker_line_may_be_lazy(line)
            && (last_folded_marker || !marker_follows_sibling(&body, line))
            && guard.as_mut().is_some_and(|budget| {
                line_folds_into_an_open_paragraph(
                    &body,
                    line,
                    options,
                    budget,
                    &mut marker_probe_unaffordable,
                )
            });
        last_folded_marker = folded_marker;
        let content_col = columns.observe(
            line,
            in_fence.is_some()
                || folded_fence
                || folded_marker
                || verse_lines[line_index].is_body()
                || in_comment_fence.is_some()
                || prepass_quote_line_is_folded(&folded_into_term, line_index, line),
        );
        // A quote nested in a list item sits AT the item's content column, so
        // the prefix scan needs that column to see it (carve-rs#588).
        //
        // THE PREVIOUS NON-BLANK LINE - see the note in `extract_footnote_defs`
        // (#1500). Carried rather than scanned backwards, so a document of
        // blank lines does not make this pre-pass quadratic.
        let after_term = opens_definition_entry(previous_non_blank);
        let stripped = strip_container_prefixes_at(line, &columns, after_term);
        remember_prepass_paragraph_line(&folded_into_term, line_index, line, &stripped, &columns);
        if !is_blank_line(line) {
            previous_non_blank = line;
        }
        let raw_is_quoted = prepass_line_is_quoted(line);
        if verse_lines[line_index].is_owned() {
            body.push(std::borrow::Cow::Borrowed(line));
            continue;
        }
        if let Some(open) = in_fence {
            if inline_fence_paragraph_ends(line, open, inline_marker_paragraph.as_ref())
                || (description_fence
                    && (description_blank || !inline_marker_fence)
                    && description_fence_leaves_after_blank(line, open, &term_fence_columns))
                || !inline_fence_lazy_line(line, inline_marker_paragraph.as_ref())
                    && ((inline_marker_fence && is_blank_line(without_blockquote_prefixes(line)))
                        || (term_fence_owned
                            && term_fence_ends_above(line, term_fence_floor, &term_fence_columns))
                        || prepass_fence_ends_above(line, open, fence_closer_ahead))
            {
                in_fence = None;
            } else {
                end_prepass_paragraph_at(&folded_into_term, line_index);
                body.push(std::borrow::Cow::Borrowed(line));
                // CLOSER: strip a blockquote prefix only when the fence was opened
                // quoted, and NEVER a list marker. A fence closer is a continuation
                // line of pure indentation, so a literal marker line inside a
                // document-level code sample stays content.
                let close_kept = if term_fence_quotes > 0 {
                    strip_prepass_quote_levels(line, term_fence_quotes).unwrap_or(line)
                } else if open.quoted {
                    strip_prepass_blockquote_prefix(line).unwrap_or(line)
                } else {
                    line
                };
                advance_inline_paragraph(line, inline_marker_paragraph.as_mut());
                let close_indent = leading_ws(close_kept);
                let close_line = if close_indent >= open.content_col {
                    &close_kept[open.content_col..]
                } else {
                    close_kept
                };
                description_blank = description_fence && is_blank_line(close_kept);
                if (!description_fence || leading_ws(close_kept) >= open.content_col)
                    && is_fence_close(close_line, open)
                {
                    if inline_marker_fence {
                        folded_into_term
                            .paragraph_quotes
                            .borrow_mut()
                            .insert(line_index, inline_marker_paragraph.clone());
                    }
                    in_fence = None;
                }
                continue;
            }
        }
        // OPENER: strip container prefixes (blockquote AND list marker), then
        // re-base to the current list-item content column. This recognizes a
        // fence on the marker line (`- ````) and on continuation lines.
        let opener_kept;
        let mut authored_fence_col = content_col;
        let term_fence = folded_into_term.fence_openers.get(&line_index);
        let fence_line = if let Some(host) = term_fence {
            authored_fence_col = host.content_col;
            &line[host.opener_offset..]
        } else if content_col == 0 || stripped.description {
            if stripped.description {
                authored_fence_col = description_fence_content_column(line, &stripped, &columns);
            }
            stripped.bare
        } else {
            opener_kept = strip_container_prefixes_keep_indent(line);
            let kept_indent = leading_ws(&opener_kept);
            if kept_indent >= content_col {
                let canonical = &opener_kept[content_col..];
                let authored = canonical.trim_start_matches([' ', '\t']);
                if detect_fence_open(authored).is_some() {
                    authored_fence_col = kept_indent;
                    authored
                } else {
                    canonical
                }
            } else {
                opener_kept.as_str()
            }
        };
        // Top-level and unindented only - see the note in extract_footnote_defs
        // for why a nested opener is refused rather than guessed at.
        if let Some(open) = in_comment_fence {
            end_prepass_paragraph_at(&folded_into_term, line_index);
            // ANY column, at the opener's quote depth - see the note in
            // `extract_footnote_defs`.
            if closes_open_comment_fence(line, open) {
                in_comment_fence = None;
            }
            // A comment's body is OPAQUE, so a definition-shaped line inside
            // one is comment text and registers nothing. The state was already
            // tracked here, but only to gate the line-block opener below - the
            // scan still walked into the body and registered from it, so
            // `%%%` / `[^a]: note` / `%%%` produced an endnote nobody wrote and
            // a live reference for `see [^a]` after it (#504).
            //
            // carve-js has never registered from inside a comment and carve-php
            // stopped (carve-php#698); this brings the third engine into line.
            body.push(std::borrow::Cow::Borrowed(line));
            continue;
        } else if let Some((open, scope)) = detect_comment_fence_opener_scoped(line, &columns) {
            // The same scopes as the footnote prepass above: the fence is gated
            // by the column it REACHES, and its span is bounded by the container
            // holding it rather than by the delimiter's own column.
            if comment_fence_scope_closes(
                scope,
                &all_lines,
                line_index,
                open.fence_len,
                &columns,
                &comment_closers,
                &mut container_closers,
            ) {
                end_prepass_paragraph_at(&folded_into_term, line_index);
                in_comment_fence = Some(OpenCommentFence {
                    fence_len: open.fence_len,
                    quote_depth: scope.quote_depth(),
                });
                body.push(std::borrow::Cow::Borrowed(line));
                continue;
            }
        }
        if let Some(mut open) =
            detect_fence_open(fence_line).filter(|_| !folded_into_term.folded.contains(&line_index))
        {
            open.content_col = authored_fence_col;
            open.quoted = term_fence.map_or(raw_is_quoted, |host| host.quote_depth > 0);
            let fence = prepass_fence_opens(
                &all_lines,
                line_index,
                open,
                &closer_maxima,
                &folded_into_term,
                &columns,
                after_term,
            );
            // A `:` marker starts a body, so no paragraph above can hold its fence.
            if fence.opens || (stripped.description && term_fence.is_none()) {
                end_prepass_paragraph_at(&folded_into_term, line_index);
                open.quoted |= !fence.quote_columns.is_empty();
                in_fence = Some(open);
                inline_marker_fence = fence.inline_marker;
                inline_marker_paragraph = if fence.inline_marker {
                    previous_prepass_paragraph(&folded_into_term, line_index)
                } else {
                    None
                };
                term_fence_quotes =
                    term_fence.map_or(fence.quote_columns.len(), |host| host.quote_depth);
                term_fence_columns = term_fence
                    .map(|host| host.quote_columns.clone())
                    .unwrap_or(fence.quote_columns);
                description_fence = stripped.description && term_fence.is_none();
                description_blank = false;
                term_fence_floor = if description_fence {
                    0
                } else {
                    term_fence.map_or(fence.host_content_col, |host| host.content_col)
                };
                term_fence_owned = term_fence.is_some() || !term_fence_columns.is_empty();
                fence_closer_ahead = fence.closer_ahead;
            }
            body.push(std::borrow::Cow::Borrowed(line));
            continue;
        }
        let def_indent = indent_columns(stripped.bare);
        let def_line = at_content_column(
            stripped.bare,
            stripped.structural,
            columns.reached_by(def_indent),
        );
        let authored_quote_def = recent_list_marker[line_index]
            && strip_blockquote_prefix(trim_ascii_start(def_line))
                .is_some_and(|content| parse_link_def_line_with_value(content).is_some());
        let def_line = if authored_quote_def {
            without_blockquote_prefixes(trim_ascii_start(def_line))
        } else {
            def_line
        };
        let bare_trim = trim_ascii_start(stripped.bare);
        match colon_fence {
            Some((fence_col, fence_len)) => {
                if indent_columns(line) == fence_col
                    && exact_colon_fence_len(bare_trim).is_some_and(|len| len >= fence_len)
                {
                    colon_fence = None;
                } else if !is_blank_line(line) && indent_columns(line) < fence_col {
                    // The container's list-item host ends at a line that dedents
                    // below its content column. An UNTERMINATED colon fence closes
                    // WITH its host, so its ownership must not reach a later
                    // sibling item - without this reset a `[r]: /url` in the next
                    // item was kept as text instead of hoisted. Clear the state
                    // so this line can hoist or reopen normally.
                    colon_fence = None;
                }
            }
            None => {
                // Only a LIST-ITEM host owns the line: a colon fence in a
                // DEFINITION-LIST body still hoists a definition past its column
                // (corpus 451-2), so the container is tracked for ownership only
                // when no definition list is open at this level.
                if indent_columns(line) == content_col && columns.current().def_list.is_none() {
                    if let Some(open) = detect_container_open(bare_trim) {
                        colon_fence = Some((content_col, open.fence_len));
                    }
                }
            }
        }
        // See the footnote pass: folding and not being able to tell are one
        // condition, and both collect nothing (PART 9R R1a).
        // See the footnote pass: the probe stays inside the filter so the shared
        // budget is spent only on lines that parsed as a definition.
        let mut unaffordable = false;
        if let Some((label_part, _target_part, mut def)) = parse_link_def_line_with_value(def_line)
            .filter(|_| {
                let folds = marker_line_may_be_lazy(line)
                    && guard.as_mut().is_some_and(|budget| {
                        line_folds_into_an_open_paragraph(
                            &body,
                            line,
                            options,
                            budget,
                            &mut unaffordable,
                        )
                    });
                !(folds || unaffordable || folded_into_term.folded.contains(&line_index))
            })
        {
            if label_part.starts_with('@') {
                body.push(std::borrow::Cow::Borrowed(line));
                continue;
            }
            // The line the author wrote it on, for PART 12 §10's node: its `pos`
            // and the SOURCE order of the hoisted definitions both come from here.
            def.line = Some(line_index);
            def.raw_label = Some(label_part.to_string());
            defs.insert(label_key(label_part), def);
            // Leave a blank line in place of the (invisible) definition so it
            // still acts as a block boundary (matches carve-js, where a
            // definition interrupts a paragraph / ends a lazy blockquote).
            // A definition matched through the CONTENT-COLUMN strip leaves the
            // container prefix behind, and inside a quoted list item that
            // prefix alone is a BLANK line - which loosens the list (§17 L1).
            // The definition rendered nothing, so it is not the item's second
            // block and must not loosen it (§17 L2), exactly as the
            // marker-consuming case above keeps the item non-empty. `%%` is
            // invisible at any column and closes nothing (§24 C3).
            let mut replacement = if authored_quote_def {
                line.strip_suffix(def_line)
                    .expect("quoted definition suffix")
                    .to_string()
            } else {
                stripped.replacement()
            };
            // NOT gated on a container prefix. The hazard is the blank the
            // removal leaves, and a definition at an item's content column with
            // no marker or quote in front of it - `- a` / `  [^f]: x` / `  more`
            // - leaves exactly the same blank. That read as an interior
            // separator and loosened the item, so corpus 228 rendered
            // `<p>a</p>` and `<p>more</p>` where the other two engines render
            // both bare (carve#801, the `list.tight` divergence).
            //
            // Top-level needs the marker too: a plain blank is caption_slot's
            // optional blank line, so replacing a definition with one allowed a
            // caption to attach THROUGH the definition (carve#1028).
            if !replacement.ends_with("%%") && !replacement.ends_with(&definition_placeholder()) {
                // AND IT HAS TO STAND AT THE DEFINITION'S OWN COLUMN. Inside a
                // container the structural prefix already carries it; at top
                // level that prefix is empty, so an unindented placeholder
                // lands at column 0 and CLOSES the item it exists to keep
                // open - the line after it leaves the list entirely.
                let document_column =
                    definition_at_quote_document_column(&replacement, stripped.bare);
                if !authored_quote_def {
                    replacement.push_str(&stripped.bare[..leading_ws(stripped.bare)]);
                }
                replacement.push_str(&if document_column {
                    document_definition_placeholder()
                } else {
                    definition_placeholder()
                });
            }
            replacement.push_str(
                &" ".repeat(
                    line.chars()
                        .count()
                        .saturating_sub(replacement.chars().count()),
                ),
            );
            body.push(std::borrow::Cow::Owned(replacement));
        } else {
            body.push(std::borrow::Cow::Borrowed(line));
        }
    }
    (joined_source(&body), defs)
}

pub(in crate::parse) fn prepass_line_is_quoted(line: &str) -> bool {
    strip_prepass_blockquote_prefix(line).is_some()
        || detect_list_marker_full(line)
            .is_some_and(|marker| strip_prepass_blockquote_prefix(marker.content).is_some())
}

pub(in crate::parse) fn strip_prepass_quote_levels(mut line: &str, depth: usize) -> Option<&str> {
    for _ in 0..depth {
        line = strip_prepass_blockquote_prefix(line)?;
    }
    Some(line)
}

pub(in crate::parse) fn strip_prepass_blockquote_prefix(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    if bytes.get(i) != Some(&b'>') {
        return None;
    }
    i += 1;
    match bytes.get(i) {
        None => {}
        Some(b' ') => i += 1,
        _ => return None,
    }
    Some(&line[i..])
}

/// The quote a line opens: how many markers deep, the COLUMN the first of them
/// starts at, and what is left after them.
///
/// Depth rather than a boolean because stripping the markers collapses
/// `> > %%%` and `> %%%` onto the same run, and they belong to different quotes.
/// `prepass_line_is_quoted` only ever needed to know THAT a line was quoted; the
/// comment-fence scope needs to know how deep, so a closer can be matched
/// against the quote its opener was written in.
///
/// A LIST MARKER comes off first, the way `prepass_line_is_quoted` already reads
/// one and `detect_comment_fence_opener_at_any_column` already walks one: a
/// quote may open on an item's marker line, so `- > %%%` is quoted at depth 1
/// and its body continues at `  > `. Reading only the leading markers left that
/// one spelling of the same fence unrecognized.
///
/// The column is what keeps the two prefixes from being confused for each other.
/// A `> %%%` written back at column 0 is not the closer of a `- > %%%`, because
/// it has left the item; the depth alone cannot tell them apart, and the column
/// is what the dedent half of the bound measures against. For a quote at column
/// 0 nothing can dedent below it, so that half never fires and the blank line is
/// the whole bound, exactly as it is for the plain spelling.
pub(in crate::parse) fn prepass_quote_scope(line: &str) -> (usize, usize, &str) {
    let mut rest = line;
    let mut col = 0usize;
    loop {
        let trimmed = trim_ascii_start(rest);
        col = advance_columns(&rest[..rest.len() - trimmed.len()], col);
        rest = trimmed;
        let Some(marker) = detect_list_marker_full(rest) else {
            break;
        };
        let consumed = marker.content.as_ptr() as usize - rest.as_ptr() as usize;
        if consumed == 0 {
            break;
        }
        col = advance_columns(&rest[..consumed], col);
        rest = marker.content;
    }
    let quote_col = col;
    let mut depth = 0;
    while let Some(inner) = strip_prepass_blockquote_prefix(rest) {
        depth += 1;
        rest = inner;
    }
    (depth, quote_col, rest)
}

/// The indent of a `:: ` TERM line, which opens a definition list.
pub(in crate::parse) fn detect_prepass_def_term(line: &str) -> Option<usize> {
    let indent = leading_ws(line);
    is_definition_list_start(&line[indent.min(line.len())..]).then_some(indent)
}

/// The indent of a `:  ` BODY line, which opens a definition body.
///
/// The separator is the marker's own, so a line that is only the marker opens
/// nothing: `parse_definition_list` reads `:  ` with `strip_prefix`, and an
/// empty remainder is the placeholder form rather than a body with content.
pub(in crate::parse) fn detect_prepass_def_body(line: &str) -> Option<(usize, usize)> {
    let indent = leading_ws(line);
    strip_definition_marker(&line[indent.min(line.len())..])
        .filter(|(body, _)| !is_blank_line(body))
        .map(|(_, width)| (indent, width))
}

/// The description marker on `line`: its body and its content column width.
/// The description marker `:` plus its separator run, and the body behind it.
///
/// MARKER REQUIRES CONTENT ignores TRAILING WHITESPACE, and NO TRAILING
/// WHITESPACE spells whitespace `' ' | '\t'` in this position - so a body that
/// is only tabs is a trailing run and opens nothing. `: <TAB>` therefore folds
/// into the term, the way `:` + tab already does without the space
/// (markup-carve/carve#1836).
///
/// The separator run is SPACES ONLY, so the body may still START with a tab:
/// `: <TAB>text` opens with the tab as content, and a vertical tab or a
/// no-break space is content too. Only space and tab are trailing whitespace.
pub(in crate::parse) fn strip_definition_marker(line: &str) -> Option<(&str, usize)> {
    let rest = line.strip_prefix(':')?;
    let spaces = rest.bytes().take_while(|b| *b == b' ').count();
    let body = &rest[spaces..];
    let has_content = body.bytes().any(|b| b != b' ' && b != b'\t');
    (spaces > 0 && has_content).then_some((body, 1 + spaces))
}

pub(in crate::parse) fn detect_prepass_list_marker(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    let marker_indent = i;
    if i >= bytes.len() {
        return None;
    }
    match bytes[i] {
        b'-' | b'*' => i += 1,
        // The bare dot is a decimal ordered marker whose authored width is one
        // column, exactly like a bullet. Leaving it out of the definition
        // prepass made definitions at its content column render as literal
        // item text even though the block parser recognized the list.
        b'.' => i += 1,
        b'0'..=b'9' => {
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if !matches!(bytes.get(i), Some(b'.' | b')')) {
                return None;
            }
            i += 1;
        }
        b'a'..=b'z' | b'A'..=b'Z' => {
            let marker_start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let marker = &line[marker_start..i];
            let ordered = marker.len() == 1
                || marker.bytes().all(|b| {
                    matches!(
                        b,
                        b'i' | b'v'
                            | b'x'
                            | b'l'
                            | b'c'
                            | b'd'
                            | b'm'
                            | b'I'
                            | b'V'
                            | b'X'
                            | b'L'
                            | b'C'
                            | b'D'
                            | b'M'
                    )
                });
            if !ordered || !matches!(bytes.get(i), Some(b'.' | b')')) {
                return None;
            }
            i += 1;
        }
        _ => return None,
    }
    if bytes.get(i) == Some(&b'{') {
        while i < bytes.len() && bytes[i] != b'}' {
            i += 1;
        }
        if bytes.get(i) != Some(&b'}') {
            return None;
        }
        i += 1;
    }
    let spaces_start = i;
    while bytes.get(i) == Some(&b' ') {
        i += 1;
    }
    if i == spaces_start || !bytes.get(i).is_some_and(|b| !b.is_ascii_whitespace()) {
        return None;
    }
    Some((marker_indent, i))
}

pub(in crate::parse) struct StrippedContainerLine<'a> {
    pub(in crate::parse) structural: &'a str,
    pub(in crate::parse) bare: &'a str,
    pub(in crate::parse) needs_empty_list_content: bool,
    // The canonical stripper consumed an admitted description marker.
    pub(in crate::parse) description: bool,
}

impl StrippedContainerLine<'_> {
    pub(in crate::parse) fn replacement(&self) -> String {
        let mut replacement = self.structural.to_string();
        if self.needs_empty_list_content {
            replacement.push_str(&definition_placeholder());
        }
        replacement
    }
}
