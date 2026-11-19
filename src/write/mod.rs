//! The canonical writer of `writer.md`: a tree, and optionally the sidecar
//! that was returned with it, as Carve source (C2-W-001). Blocks that a blank
//! line separates are written and tested on their own; a group is parsed again
//! and written again until it reads back as its blocks (C2-W-089).
//!
//! This engine finds every escape by the repair of C2-W-088 alone, with the
//! work limit of C2-W-133; the bytes are those the clauses define (C2-W-010).

mod attrs;
mod blocks;
mod check;
mod extension;
mod facts;
mod inline;
mod node;
mod state;
mod table;
mod width_data;

pub(crate) use blocks::include_spelling;
pub(crate) use table::display_width;

use crate::api::{Entry, Range};
use check::{Check, compare_blocks, escape_openers, exhausted, extend_chains, parse_chunk};
use node::{N, children, id, span, text, ty};
use serde_json::Value;
use state::{Writer, lose};
use std::collections::{HashMap, HashSet};

/// How one write runs.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// C2-W-068.
    pub align_tables: bool,
    /// The standard extensions the write enables, by name.
    pub extensions: Vec<String>,
    /// The sidecar facts; `None` writes the defaults of C2-W-083 (C2-W-102, C2-W-127).
    pub layout: Option<Vec<facts::Entry>>,
}

impl Settings {
    /// The facts of a sidecar JSON value (C2-L-009).
    #[must_use]
    pub fn with_layout(mut self, layout: Option<&Value>) -> Self {
        self.layout = layout.map(facts::from_layout);
        self
    }
}

/// C2-W-134: the codes that a call returning only source cannot hide.
#[must_use]
pub fn has_loss(entries: &[Entry]) -> bool {
    entries.iter().any(|e| {
        matches!(
            e.code.as_str(),
            "writer-refused" | "structure-unspellable" | "field-unspellable"
        )
    })
}

/// The canonical source of `src`: parsed, its facts read from the source unless
/// `normalize` (C2-W-127), and written. Returns the source and the writer's record.
#[must_use]
pub fn format_tree(doc: &Value, src: Option<&str>, settings: &Settings) -> (String, Vec<Entry>) {
    let mut settings = settings.clone();
    if let Some(src) = src {
        settings.layout = Some(facts::from_source(doc, src));
    }
    write_value(doc, &settings)
}

/// `format` of a body with every default, or `None` where the writer reports a loss.
pub(crate) fn format_plain(body: &str) -> Option<String> {
    let doc = parse_plain(body)?;
    let (out, losses) = format_tree(&doc, Some(body), &Settings::default());
    (!has_loss(&losses)).then_some(out)
}

pub(crate) fn parse_plain(body: &str) -> Option<Value> {
    crate::api::writer::tree_value(&crate::tree_json(body, &crate::Options::default())).ok()
}

/// Writes the tree `doc` (its JSON value, read and checked).
#[must_use]
pub fn write_value(doc: &Value, settings: &Settings) -> (String, Vec<Entry>) {
    let pool = typed_arena::Arena::new();
    let mut w = Writer::new(&pool, settings.align_tables);
    w.extensions.clone_from(&settings.extensions);
    if settings.extensions.iter().any(|e| e == "carve.tabs") {
        w.matchers.push(state::Matcher {
            form: "div".into(),
            name: "tabs".into(),
        });
    }
    w.link_keys = children(doc)
        .iter()
        .filter(|n| ty(n) == "link_reference_definition")
        .filter_map(|n| node::s(n, "key"))
        .map(str::to_string)
        .collect();
    if let Some(entries) = &settings.layout {
        facts::read(&mut w, doc, entries);
    }
    read_abbreviations(&mut w, doc);
    let source = write_document(&mut w, doc);
    let mut entries = locate(doc, &w);
    root_metadata_losses(doc, &mut entries);
    (source, entries)
}

/// C2-B-085: the first definition of a term wins.
fn read_abbreviations(w: &mut Writer<'_>, doc: &Value) {
    let mut seen = HashSet::new();
    for c in children(doc) {
        if ty(c) != "abbreviation_def" {
            continue;
        }
        let key = node::s(c, "key").unwrap_or("").to_string();
        if seen.insert(key.clone()) {
            w.abbreviations
                .push((key, node::s(c, "expansion").unwrap_or("").to_string()));
        }
    }
    w.abbreviations.sort_by(|a, b| a.0.cmp(&b.0));
}

fn locate(doc: &Value, w: &Writer<'_>) -> Vec<Entry> {
    let wanted: HashSet<usize> = w.sp.losses.iter().filter_map(|l| l.node).collect();
    let found = node::pointers(doc, &wanted);
    let mut nodes: HashMap<usize, &Value> = HashMap::new();
    if !wanted.is_empty() {
        let mut all = vec![doc];
        node::descendants(doc, &mut all);
        for n in all {
            if wanted.contains(&id(n)) {
                nodes.insert(id(n), n);
            }
        }
    }
    w.sp.losses
        .iter()
        .map(|l| {
            let place = l.node.and_then(|n| found.get(&n));
            let path = place.cloned().unwrap_or_default() + l.step.as_deref().unwrap_or("");
            let range = place
                .and(l.node)
                .and_then(|n| nodes.get(&n))
                .and_then(|n| span(n))
                .map(|(start_offset, end_offset)| Range {
                    start_offset,
                    end_offset,
                });
            loss_entry(l.code, l.message.clone(), path, range)
        })
        .collect()
}

fn loss_entry(code: &str, message: String, path: String, range: Option<Range>) -> Entry {
    Entry {
        code: code.to_string(),
        severity: "warning".into(),
        channel: "render-loss".into(),
        message,
        path: Some(path),
        range,
        file: None,
        foreign_type: None,
    }
}

fn root_metadata_losses(doc: &Value, out: &mut Vec<Entry>) {
    let metadata = doc.get("metadata");
    for name in ["headers", "footers", "abstract", "keywords"] {
        if metadata.and_then(|m| m.get(name)).is_some() {
            out.push(loss_entry(
                "field-unspellable",
                format!("Carve source cannot spell root metadata.{name}"),
                format!("/metadata/{name}"),
                None,
            ));
        }
    }
    for name in ["lang", "dir"] {
        if doc.get(name).is_some() {
            out.push(loss_entry(
                "field-unspellable",
                format!("Carve source cannot spell document {name}"),
                format!("/{name}"),
                None,
            ));
        }
    }
    if metadata.and_then(|m| m.get("fields")).is_some()
        && !children(doc).iter().any(|n| ty(n) == "frontmatter")
    {
        out.push(loss_entry(
            "field-unspellable",
            "Carve source cannot spell root metadata.fields".into(),
            "/metadata/fields".into(),
            None,
        ));
    }
}

fn last_block<'t>(
    nodes: &'t [Value],
    from: usize,
    to: usize,
    list: Option<N<'t>>,
) -> Option<N<'t>> {
    nodes
        .get(from..to)
        .and_then(|g| g.iter().rev().find(|n| ty(n) != "comment"))
        .or(list)
}

fn write_document<'t>(w: &mut Writer<'t>, doc: N<'t>) -> String {
    let nodes = children(doc);
    let mut out: Vec<String> = Vec::new();
    let mut before: Option<N<'t>> = None;
    let mut list: Option<N<'t>> = None;
    let mut at = 0;
    while at < nodes.len() {
        let from = at;
        at = repair_group(w, doc, from, out.is_empty());
        if w.lines.is_empty() {
            continue;
        }
        if out.is_empty() && w.blank_first {
            out.push(String::new());
        }
        let blanks = match (before, nodes.get(from)) {
            (Some(b), Some(n)) => blocks::blank_lines_between(w, b, list, n, false),
            _ => 0,
        };
        out.extend(std::iter::repeat_n(String::new(), blanks));
        out.append(&mut w.lines);
        before = at.checked_sub(1).and_then(|i| nodes.get(i));
        list = last_block(nodes, from, at, list);
    }
    if out.is_empty() {
        "\n".into()
    } else {
        out.join("\n") + "\n"
    }
}

/// Whether `node` is written into one group with the block `before` above it.
fn joins_group(w: &mut Writer<'_>, before: &Value, node: &Value) -> bool {
    if ty(node) == "paragraph" {
        return ty(before) == "definition_list";
    }
    ty(node) == "comment" && blocks::blank_lines_between(w, before, None, node, false) == 0
}

fn is_bridged(w: &Writer<'_>, nodes: &[Value], from: usize, at: usize) -> bool {
    let mut prior = at.checked_sub(1);
    while let Some(p) =
        prior.filter(|&p| p >= from && nodes.get(p).is_some_and(|n| ty(n) == "comment"))
    {
        prior = p.checked_sub(1);
    }
    let Some(prior) = prior.filter(|&p| p >= from).and_then(|p| nodes.get(p)) else {
        return false;
    };
    if !matches!(ty(prior), "list" | "definition_list" | "footnote") {
        return false;
    }
    let node = nodes.get(at);
    let following = if node.is_some_and(|n| ty(n) == "comment") {
        blocks::after_comments(nodes, at)
    } else {
        node
    };
    !blocks::following_attributes(w, following).is_empty()
}

fn group_end(w: &mut Writer<'_>, nodes: &[Value], from: usize) -> usize {
    let mut end = from + 1;
    while let (Some(before), Some(node)) = (nodes.get(end - 1), nodes.get(end)) {
        let bridged = is_bridged(w, nodes, from, end);
        let hoisted = ty(node) == "comment" && bridged;
        let belongs = ty(before) == "comment" && bridged;
        if !hoisted && !belongs && !joins_group(w, before, node) {
            break;
        }
        end += 1;
    }
    end
}

fn reset_lines(w: &mut Writer<'_>) {
    w.lines.clear();
    w.lead = None;
    w.bare_dot_lead = false;
    w.prewritten = None;
    w.clear_starts();
    w.raw_refs.clear();
}

fn group_source(w: &Writer<'_>, first: bool) -> String {
    let mut src = String::with_capacity(w.lines.iter().map(|l| l.len() + 1).sum::<usize>() + 1);
    if !first {
        src.push('\n');
    }
    for line in &w.lines {
        src.push_str(line);
        src.push('\n');
    }
    src
}

fn repair_group<'t>(w: &mut Writer<'t>, doc: N<'t>, from: usize, first: bool) -> usize {
    let reported = w.sp.losses.len();
    let end = repair_rounds(w, doc, from, first, reported);
    w.work_limit = usize::MAX;
    end
}

const MAX_REPAIR_CHARS: usize = 20_000_000;
const MIN_REPAIR_ROUNDS: usize = 32;
const WORK_FACTOR: usize = 1000;
const WORK_FLOOR: usize = 24_000_000;
const MAX_ROUNDS: usize = 8192;
const MAX_ORDERED_ESCAPES: usize = 4;

struct History {
    ids: HashMap<usize, usize>,
    recent: std::collections::VecDeque<String>,
    seen: HashSet<String>,
    rounds: usize,
    remaining: i64,
}

fn repair_rounds<'t>(
    w: &mut Writer<'t>,
    doc: N<'t>,
    from: usize,
    first: bool,
    reported: usize,
) -> usize {
    let nodes = children(doc);
    let mut history = History {
        ids: HashMap::new(),
        recent: std::collections::VecDeque::new(),
        seen: HashSet::new(),
        rounds: 0,
        remaining: i64::try_from(MAX_REPAIR_CHARS).unwrap_or(i64::MAX),
    };
    loop {
        let end = group_end(w, nodes, from);
        let group = nodes.get(from..end).unwrap_or(&[]);
        reset_lines(w);
        blocks::write_blocks(w, group, "", 0, false);
        if w.lines.is_empty() {
            return end;
        }
        if repeated_repair(w, group, &mut history) {
            return end;
        }
        let f = first && !w.blank_first;
        if !settle(w, doc, group, f) && !exhausted(w) {
            if w.sp.losses.len() == reported {
                report_unread(w, group);
            }
            prune_group_escapes(w, group, first && !w.blank_first);
            return end;
        }
    }
}

fn refuse_repair(w: &mut Writer<'_>, node: &Value) {
    w.sp.losses.push(state::Loss {
        code: "writer-refused",
        message: "the writer could not complete its repair for this block".into(),
        node: Some(id(node)),
        step: None,
    });
}

fn local_id(ids: &mut HashMap<usize, usize>, n: &Value) -> usize {
    let next = ids.len();
    *ids.entry(id(n)).or_insert(next)
}

/// C2-W-133: the repair state of this group.
fn repair_key(w: &Writer<'_>, ids: &mut HashMap<usize, usize>, group: &[Value]) -> String {
    use std::fmt::Write as _;
    let mut key = String::new();
    let _ = write!(
        key,
        "{:?}|{}|{}|",
        w.lines,
        w.blank_first,
        w.sp.losses.len()
    );
    let mut pending: Vec<&Value> = group.iter().collect();
    while let Some(n) = pending.pop() {
        let nid = local_id(ids, n);
        let escapes = if ty(n) == "text" {
            w.sp.escapes.get(&id(n))
        } else {
            None
        };
        let attempt = w.sp.attempts.get(&id(n));
        let attempt_escapes: Option<Vec<(usize, usize)>> = attempt.map(|a| {
            a.escapes
                .iter()
                .map(|(t, at)| (local_id(ids, t), *at))
                .collect()
        });
        let _ = write!(
            key,
            "[{nid},{escapes:?},{:?},{:?},{attempt_escapes:?},{}]",
            w.forms.get(&id(n)),
            attempt.map(|a| a.step),
            w.is_lost(n)
        );
        node::for_each_slot(n, false, &mut |kids, _, _| pending.extend(kids.iter()));
    }
    key
}

fn repeated_repair(w: &mut Writer<'_>, group: &[Value], h: &mut History) -> bool {
    let chars: usize = w.lines.iter().map(|l| l.len() + 1).sum();
    if h.rounds == 0 {
        h.remaining = h
            .remaining
            .max(i64::try_from(chars * MIN_REPAIR_ROUNDS).unwrap_or(i64::MAX));
        w.work_limit = w
            .work_chars
            .saturating_add(WORK_FLOOR.max(chars.saturating_mul(WORK_FACTOR)));
    }
    h.rounds += 1;
    h.remaining -= i64::try_from(chars).unwrap_or(i64::MAX);
    let refuse = h.rounds > MAX_ROUNDS || h.remaining < 0 || exhausted(w) || {
        let key = repair_key(w, &mut h.ids, group);
        if h.seen.contains(&key) {
            true
        } else {
            h.recent.push_back(key.clone());
            h.seen.insert(key);
            if h.recent.len() > 16 {
                if let Some(old) = h.recent.pop_front() {
                    h.seen.remove(&old);
                }
            }
            false
        }
    };
    if refuse {
        if let Some(host) = group.first() {
            refuse_repair(w, host);
        }
    }
    refuse
}

#[cfg(test)]
mod repair_limit_test {
    use super::*;

    #[test]
    fn exhausted_repair_budget_reports_writer_refused() {
        let pool = typed_arena::Arena::new();
        let mut writer = Writer::new(&pool, false);
        writer.lines.push("x".into());
        let group = [
            serde_json::json!({"type": "paragraph", "children": [{"type": "text", "value": "x"}]}),
        ];
        let mut history = History {
            ids: HashMap::new(),
            recent: std::collections::VecDeque::new(),
            seen: HashSet::new(),
            rounds: 1,
            remaining: 0,
        };

        assert!(repeated_repair(&mut writer, &group, &mut history));
        assert_eq!(writer.sp.losses[0].code, "writer-refused");
    }
}

/// The text nodes below `nodes` in the order they are written: a title before the children.
fn text_order(nodes: &[Value], order: &mut HashMap<usize, usize>) {
    for n in nodes {
        if ty(n) == "text" {
            let next = order.len();
            order.insert(id(n), next);
        }
        node::for_each_slot(n, true, &mut |kids, _, _| text_order(kids, order));
    }
}

fn rank(order: &HashMap<usize, usize>, n: &Value) -> i64 {
    order
        .get(&id(n))
        .map_or(-1, |&r| i64::try_from(r).unwrap_or(i64::MAX))
}

fn leftmost<'t>(group: &[Value], escapes: &[(N<'t>, usize)]) -> Option<(N<'t>, usize)> {
    let mut order = HashMap::new();
    text_order(group, &mut order);
    let mut best = *escapes.first()?;
    for &e in escapes {
        let (r, b) = (rank(&order, e.0), rank(&order, best.0));
        if r < b || (r == b && e.1 < best.1) {
            best = e;
        }
    }
    Some(best)
}

fn defer_escapes(w: &mut Writer<'_>, deferred: &[(usize, usize)]) {
    for attempt in w.sp.attempts.values_mut() {
        let before = attempt.escapes.len();
        attempt
            .escapes
            .retain(|(n, at)| !deferred.contains(&(id(n), *at)));
        if attempt.escapes.len() != before {
            attempt.step = attempt.step.saturating_sub(1);
        }
    }
}

fn group_tree<'t>(
    w: &mut Writer<'t>,
    group: &'t [Value],
    first: bool,
    escapes: HashMap<usize, Vec<usize>>,
) -> Value {
    w.sp.escapes = escapes;
    w.lines.clear();
    w.lead = None;
    w.clear_starts();
    blocks::write_blocks(w, group, "", 0, false);
    let src = group_source(w, first);
    parse_chunk(w, &src)
}

/// C2-W-088, C2-W-091: whether the test asks for the escape `at` of `node`.
fn is_asked<'t>(
    w: &mut Writer<'t>,
    group: &'t [Value],
    first: bool,
    node: &Value,
    at: usize,
) -> bool {
    let placed = w.sp.escapes.clone();
    let lines = w.lines.clone();
    let lead = w.lead.clone();
    let starts = (w.starts.clone(), w.start_index.clone());
    let mut order = HashMap::new();
    text_order(group, &mut order);
    let own_rank = rank(&order, node);
    let mut left: HashMap<usize, Vec<usize>> = HashMap::new();
    for (text_id, offsets) in &placed {
        let r = order
            .get(text_id)
            .map_or(-1, |&r| i64::try_from(r).unwrap_or(i64::MAX));
        if r < own_rank {
            left.insert(*text_id, offsets.clone());
        } else if *text_id == id(node) {
            left.insert(
                *text_id,
                offsets.iter().copied().filter(|&o| o < at).collect(),
            );
        }
    }
    let mut escaped = left.clone();
    let list = escaped.entry(id(node)).or_default();
    list.push(at);
    list.sort_unstable();
    let without = group_tree(w, group, first, left);
    let with = group_tree(w, group, first, escaped);
    let asked = !node::trees_equal(&with, &without);
    w.sp.escapes = placed;
    w.lines = lines;
    w.lead = lead;
    w.starts = starts.0;
    w.start_index = starts.1;
    asked
}

fn compare_group<'t>(w: &mut Writer<'t>, doc: N<'t>, group: &'t [Value], first: bool) -> Check<'t> {
    let source = group_source(w, first);
    let parsed = parse_chunk(w, &source);
    let mut c = Check {
        source: Some(source),
        ..Check::default()
    };
    compare_blocks(w, &mut c, group, children(&parsed), doc);
    escape_openers(w, &mut c, &parsed, if first { 1 } else { 2 });
    w.parsed = Some(parsed);
    c
}

/// One round of the test of C2-W-089. False when the group reads back as its blocks.
fn settle<'t>(w: &mut Writer<'t>, doc: N<'t>, group: &'t [Value], first: bool) -> bool {
    let mut c = compare_group(w, doc, group, first);
    if drop_asked_trials(w, &mut c, group, first) && c.changes() == 0 {
        return true;
    }
    if c.escapes.len() > 1 && c.escapes.len() <= MAX_ORDERED_ESCAPES {
        keep_leftmost_escape(w, &mut c, group);
    }
    extend_chains(w, &mut c);
    if c.changes() == 0 && !c.losses.is_empty() && first && !w.blank_first {
        // C2-W-005: what only line 1 reads differently is written below one blank line.
        w.blank_first = compare_group(w, doc, group, false).losses.is_empty();
        if w.blank_first {
            return true;
        }
    }
    apply_changes(w, &c)
}

/// Drops the escapes of trials that were already asked; true when any was dropped.
fn drop_asked_trials<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    group: &'t [Value],
    first: bool,
) -> bool {
    let mut dropped = false;
    let trials = if c.trials.len() <= MAX_ORDERED_ESCAPES {
        c.trials.clone()
    } else {
        Vec::new()
    };
    for (n, at) in trials {
        if is_asked(w, group, first, n, at) {
            continue;
        }
        if let Some(p) = c
            .escapes
            .iter()
            .position(|(e, a)| id(e) == id(n) && *a == at)
        {
            c.escapes.remove(p);
        }
        dropped = true;
    }
    dropped
}

fn keep_leftmost_escape<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, group: &'t [Value]) {
    let Some(best) = leftmost(group, &c.escapes) else {
        return;
    };
    let deferred: Vec<(usize, usize)> = c
        .escapes
        .iter()
        .filter(|(n, at)| !(id(n) == id(best.0) && *at == best.1))
        .map(|(n, at)| (id(n), *at))
        .collect();
    defer_escapes(w, &deferred);
    c.escapes = vec![best];
}

const LINE_LEAVES: &[&str] = &[
    "code",
    "literal_inline",
    "math",
    "raw_inline",
    "comment",
    "critic_comment",
];

/// Whether a line of `value` after the first starts as a block opener can.
fn has_opener_line(value: &str) -> bool {
    for line in value.split('\n').skip(1) {
        let t = line.trim_start_matches([' ', '\t']);
        let Some(&first) = t.as_bytes().first() else {
            continue;
        };
        if !first.is_ascii_alphanumeric() {
            return true;
        }
        let end = t.bytes().take_while(u8::is_ascii_alphanumeric).count();
        if matches!(t.as_bytes().get(end), Some(b'.' | b')')) {
            return true;
        }
    }
    false
}

/// C2-W-016: a lost node holds a leaf whose later line would open a block.
fn refuse_leaf_lines(w: &mut Writer<'_>, n: &Value, depth: usize) {
    if depth > 200 || matches!(ty(n), "paragraph" | "list_item") {
        return;
    }
    if LINE_LEAVES.contains(&ty(n)) {
        if has_opener_line(text(n)) {
            lose(w, n, Some("value"));
        }
        return;
    }
    for c in children(n) {
        refuse_leaf_lines(w, c, depth + 1);
    }
}

fn apply_changes(w: &mut Writer<'_>, c: &Check<'_>) -> bool {
    if c.forms.iter().any(|n| !w.forms.contains_key(&id(n))) {
        w.sp.escapes.clear();
        w.sp.attempts.clear();
        for n in &c.forms {
            *w.forms.entry(id(n)).or_insert(0) += 1;
        }
        return true;
    }
    for (n, at) in &c.retracted {
        if let Some(list) = w.sp.escapes.get_mut(&id(n)) {
            if let Some(p) = list.iter().position(|x| x == at) {
                list.remove(p);
            }
        }
    }
    for (n, at) in &c.escapes {
        w.add_escape(n, *at);
    }
    for n in &c.forms {
        *w.forms.entry(id(n)).or_insert(0) += 1;
    }
    if c.changes() > 0 {
        return true;
    }
    for (n, _) in &c.losses {
        refuse_leaf_lines(w, n, 0);
    }
    for (n, field) in &c.losses {
        lose(w, n, field.as_deref());
    }
    false
}

/// C2-W-002: whether `actual` is `expected` for the readback check (C2-A-193).
fn writer_equal(expected: &Value, actual: &Value) -> bool {
    node::trees_equal(expected, actual)
        || extension::source_core_equivalent(expected, Some(actual))
        || node::trees_equal(&node::without_standard_data(expected), actual)
        || extension::with_core_inlines(expected).is_some_and(|e| node::trees_equal(&e, actual))
}

/// C2-W-011: a block that the rounds let pass and that reads back as another tree is reported.
fn report_unread(w: &mut Writer<'_>, group: &[Value]) {
    let parsed: Vec<Value> = w
        .parsed
        .as_ref()
        .map(|p| children(p).to_vec())
        .unwrap_or_default();
    for at in 0..group.len().max(parsed.len()) {
        if let (Some(block), Some(e)) = (parsed.get(at), group.get(at)) {
            if writer_equal(e, block) {
                continue;
            }
        }
        if let Some(n) = group.get(at.min(group.len().saturating_sub(1))) {
            lose(w, n, None);
        }
        return;
    }
}

fn star_after_break(nodes: &[Value], byte: usize, parent: Option<&Value>) -> bool {
    for (index, n) in nodes.iter().enumerate() {
        let Some((start, end)) = span(n) else {
            continue;
        };
        if byte < start || byte >= end {
            continue;
        }
        if ty(n) == "text" {
            return parent.is_some_and(|p| matches!(ty(p), "emphasis" | "strong"))
                && index
                    .checked_sub(1)
                    .and_then(|i| nodes.get(i))
                    .is_some_and(|b| ty(b) == "soft_break");
        }
        if star_after_break(children(n), byte, Some(n)) {
            return true;
        }
    }
    false
}

fn prunable_opener(w: &Writer<'_>, source: &str, at: usize) -> bool {
    let b = source.as_bytes();
    let next = b.get(at + 1).copied();
    if next == Some(b'*') {
        return w
            .parsed
            .as_ref()
            .is_some_and(|p| star_after_break(children(p), at, None));
    }
    if next == Some(b'[') {
        let rest = source.get(at + 2..).unwrap_or("");
        if rest.starts_with("^]") {
            return true;
        }
        if !rest.starts_with("----") {
            return false;
        }
        let close = rest.find(']');
        let line_end = rest.find('\n');
        return at.checked_sub(1).and_then(|p| b.get(p)) != Some(&b'^')
            || close.is_none()
            || matches!((line_end, close), (Some(l), Some(c)) if c > l);
    }
    if next != Some(b'{') {
        return false;
    }
    match b.get(at + 2) {
        Some(b'/' | b'*') => true,
        Some(b'#') => {
            let rest = source.get(at + 3..).unwrap_or("");
            match rest.find('}') {
                Some(close) => rest.find("\\-").is_none_or(|d| d > close),
                None => false,
            }
        }
        _ => false,
    }
}

/// Whether the byte `at` of `source` lies in a verbatim reference.
fn in_raw(source: &str, at: usize, raws: &[String]) -> bool {
    raws.iter().filter(|r| !r.is_empty()).any(|r| {
        source
            .match_indices(r.as_str())
            .any(|(start, m)| start <= at && at < start + m.len())
    })
}

/// C2-W-088, C2-W-089: test candidate escapes within the group that wrote them.
fn prune_group_escapes(w: &mut Writer<'_>, group: &[Value], first: bool) {
    let mut source = group_source(w, first);
    let raws = std::mem::take(&mut w.raw_refs);
    let mut at = source.find('\\');
    while let Some(p) = at {
        if exhausted(w) {
            if let Some(host) = group.first() {
                refuse_repair(w, host);
            }
            break;
        }
        if prunable_opener(w, &source, p) && !in_raw(&source, p, &raws) {
            let candidate = format!(
                "{}{}",
                source.get(..p).unwrap_or(""),
                source.get(p + 1..).unwrap_or("")
            );
            let parsed = parse_chunk(w, &candidate);
            let kids = children(&parsed);
            if kids.len() == group.len()
                && kids.iter().zip(group).all(|(n, e)| node::trees_equal(n, e))
            {
                source = candidate;
                w.parsed = Some(parsed);
                at = source.get(p..).and_then(|r| r.find('\\')).map(|q| q + p);
                continue;
            }
        }
        at = source
            .get(p + 1..)
            .and_then(|r| r.find('\\'))
            .map(|q| q + p + 1);
    }
    let body = source
        .get(usize::from(!first)..source.len().saturating_sub(1))
        .unwrap_or("");
    w.lines = body.split('\n').map(str::to_string).collect();
}
