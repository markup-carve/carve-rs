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
mod controls;
mod extension;
mod facts;
mod inline;
mod node;
mod pairs;
mod spans;
mod state;
mod table;
mod trim;
mod width_data;

pub(crate) use blocks::include_spelling;
pub(crate) use table::display_width;

use crate::api::{Entry, Range};
use crate::hash::{AddrMap, AddrSet};
use crate::view::{Arr, V};
use check::{Check, Chunk, compare_blocks, escape_openers, exhausted, extend_chains};
use node::{N, children, id, span, text, ty};
use serde_json::Value;
use state::{GroupCertificate, Writer, lose};
use std::collections::HashSet;

/// The link keys of a document, shared by the nested writes of its source bodies.
pub(crate) type LinkKeys = std::sync::Arc<HashSet<String>>;

/// The keys of the link reference definitions among `nodes`, as the writer reads them.
pub(crate) fn link_keys<'n>(nodes: impl IntoIterator<Item = &'n Value>) -> LinkKeys {
    std::sync::Arc::new(
        nodes
            .into_iter()
            .filter(|n| ty(*n) == "link_reference_definition")
            .filter_map(|n| node::s(n, "key"))
            .map(str::to_string)
            .collect(),
    )
}

/// How one write runs.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// C2-W-068.
    pub align_tables: bool,
    /// The standard extensions the write enables, by name.
    pub extensions: Vec<String>,
    /// The sidecar facts; `None` writes the defaults of C2-W-083 (C2-W-102, C2-W-127).
    pub layout: Option<Vec<facts::Entry>>,
    /// The source of the tree, for C2-W-144; `None` for a supplied tree.
    pub source: Option<String>,
    /// The link keys of the document around a nested write, in place of its definitions.
    pub link_keys: Option<LinkKeys>,
}

impl Settings {
    /// The facts of a sidecar JSON value (C2-L-009).
    #[must_use]
    pub fn with_layout(mut self, layout: Option<&Value>) -> Self {
        self.layout = layout.map(|l| facts::from_layout(V::Json(l)));
        // C2-L-056: a base64 source holds bytes, not the text the positions count.
        self.source = layout
            .filter(|l| l.get("encoding").and_then(Value::as_str) != Some("base64"))
            .and_then(|l| l.get("source"))
            .and_then(Value::as_str)
            .map(str::to_string);
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
    let pool = typed_arena::Arena::new();
    format_view(V::Json(doc), &pool, src, settings)
}

/// [`format_tree`] of a view; `pool` holds the nodes the writer makes and outlives the tree.
pub(crate) fn format_view<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    src: Option<&str>,
    settings: &Settings,
) -> (String, Vec<Entry>) {
    let mut settings = settings.clone();
    if let Some(src) = src {
        settings.layout = Some(facts::from_source(doc, src));
    }
    write_trimmed(doc, pool, &settings, src.map(str::len))
}

/// The nodes the writer makes while it writes a tree (C2-W-089 stand-ins).
pub(crate) type Pool = typed_arena::Arena<Value>;

/// `format` of a body with every default, or `None` where the writer reports a loss.
pub(crate) fn format_plain(body: &str) -> Option<String> {
    let doc = parse_plain(body)?;
    let (out, losses) = format_tree(&doc, Some(body), &Settings::default());
    (!has_loss(&losses)).then_some(out)
}

/// [`format_plain`] in the context of a document with the link keys `keys`:
/// an escape or a shortcut reference in `body` is read as it reads there.
pub(crate) fn format_in_context(body: &str, keys: &LinkKeys) -> Option<String> {
    if keys.is_empty() {
        return format_plain(body);
    }
    let doc = crate::tree_value_in_context(body, keys)?;
    let settings = Settings {
        link_keys: Some(std::sync::Arc::clone(keys)),
        ..Settings::default()
    };
    let (out, losses) = format_tree(&doc, Some(body), &settings);
    (!has_loss(&losses)).then_some(out)
}

pub(crate) fn parse_plain(body: &str) -> Option<Value> {
    crate::api::writer::tree_value(&crate::tree_json(body, &crate::Options::default())).ok()
}

#[cfg(test)]
thread_local! {
    static REPAIR_PARSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PAIR_SCANS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static WRITTEN_BLOCKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod nested_write_test {
    use super::WRITTEN_BLOCKS;

    /// The top-level blocks every write of a format with `carve.tabs` takes,
    /// nested source-body writes included.
    fn blocks_written(groups: usize) -> usize {
        let unit = |k: usize| {
            format!(
                ":::: tabs\n::: tab \"T0\"\nsee [x{k}][r{k}]\n:::\n\n::: tab \"T1\"\nb\n:::\n::::\n\n[r{k}]: http://x/{k}\n"
            )
        };
        let src: String = (0..groups).map(unit).collect::<Vec<_>>().join("\n");
        let opts = crate::api::WriteOptions {
            extensions: vec!["carve.tabs".into()],
            ..crate::api::WriteOptions::default()
        };
        let before = WRITTEN_BLOCKS.with(std::cell::Cell::get);
        let output = crate::api::Carve::new()
            .format_with_diagnostics(src.as_str(), &opts)
            .map_or_else(|e| e.to_string(), |w| w.output);
        assert_eq!(output, src);
        WRITTEN_BLOCKS.with(std::cell::Cell::get) - before
    }

    #[test]
    fn source_bodies_do_not_copy_every_definition() {
        let (small, large) = (blocks_written(8), blocks_written(32));
        assert!(
            large <= small * 5,
            "8 groups write {small} blocks, 32 groups {large}"
        );
    }
}

#[cfg(test)]
mod plain_table_certificate_test {
    use super::*;

    fn format_typed(src: &str) -> String {
        let arena = crate::Arena::for_input(src);
        let doc =
            crate::parse_resolved_in(src, &crate::Options::default(), &arena).unwrap_or_default();
        let pool = Pool::new();
        let (out, losses) = format_view(V::Doc(&doc), &pool, Some(src), &Settings::default());
        assert_eq!(losses.len(), 0);
        out
    }

    #[test]
    fn ordinary_rows_need_no_repair_parse() {
        let before = REPAIR_PARSES.with(std::cell::Cell::get);
        let pairs_before = PAIR_SCANS.with(std::cell::Cell::get);
        let output = format_typed("|alpha|beta|\n|7|x|\n");
        assert_eq!(output, "| alpha | beta |\n| 7 | x |\n");
        assert_eq!(REPAIR_PARSES.with(std::cell::Cell::get) - before, 0);
        assert_eq!(PAIR_SCANS.with(std::cell::Cell::get) - pairs_before, 0);
    }

    #[test]
    fn marked_cells_keep_the_repair_parse() {
        let before = REPAIR_PARSES.with(std::cell::Cell::get);
        let output = format_typed("|*alpha*|beta|\n");
        assert_eq!(output, "| *alpha* | beta |\n");
        assert!(REPAIR_PARSES.with(std::cell::Cell::get) > before);
    }

    #[test]
    fn explicit_empty_text_keeps_the_repair_parse() {
        use crate::ast::{
            CellParagraph, Document, Node, Table, TableBody, TableCell, TableColumn, TableRow, Text,
        };

        let arena = crate::Arena::new();
        let text = Node::Text(arena.alloc(Text::default()));
        let paragraph = Node::CellParagraph(arena.alloc(CellParagraph {
            children: arena.one(text),
            ..CellParagraph::default()
        }));
        let cell = Node::TableCell(arena.alloc(TableCell {
            children: arena.one(paragraph),
            ..TableCell::default()
        }));
        let row = Node::TableRow(arena.alloc(TableRow {
            children: arena.one(cell),
            ..TableRow::default()
        }));
        let body = Node::TableBody(arena.alloc(TableBody {
            children: arena.one(row),
            ..TableBody::default()
        }));
        let table = Node::Table(arena.alloc(Table {
            children: arena.one(body),
            columns: arena.one(Node::TableColumn(arena.alloc(TableColumn::default()))),
            ..Table::default()
        }));
        let doc = Document {
            ast_version: "2.0",
            children: arena.one(table),
            ..Document::default()
        };
        let pool = Pool::new();
        let before = REPAIR_PARSES.with(std::cell::Cell::get);
        let _ = format_view(V::Doc(&doc), &pool, None, &Settings::default());
        assert!(REPAIR_PARSES.with(std::cell::Cell::get) > before);
    }

    #[test]
    fn zero_cell_row_keeps_its_closing_pipe() {
        let mut doc = parse_plain("||\n").unwrap_or_default();
        if let Some(row_children) = doc.pointer_mut("/children/0/children/0/children/0/children") {
            *row_children = serde_json::json!([]);
        }
        let (out, losses) = write_value(&doc, &Settings::default());
        assert_eq!(out, "||\n");
        assert!(losses.iter().any(|e| e.code == "structure-unspellable"));
    }
}

/// Writes the tree `doc` (its JSON value, read and checked).
#[must_use]
pub fn write_value(doc: &Value, settings: &Settings) -> (String, Vec<Entry>) {
    let pool = Pool::new();
    write_view(V::Json(doc), &pool, settings)
}

/// [`write_value`] of a view; `pool` outlives the tree.
pub(crate) fn write_view<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    settings: &Settings,
) -> (String, Vec<Entry>) {
    write_trimmed(doc, pool, settings, None)
}

/// [`write_view`] with the length of the source the tree was read from, which sizes the repair budget.
fn write_trimmed<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    settings: &Settings,
    source_len: Option<usize>,
) -> (String, Vec<Entry>) {
    // A parse gives no such spaces, so only a supplied JSON tree is trimmed.
    let recorded = |at: &str| {
        settings
            .layout
            .iter()
            .flatten()
            .find(|entry| entry.path == at)
            .and_then(|entry| entry.facts.source_format.as_deref())
    };
    let delimited = |table: &Value, at: &str| table::writes_delimited(V::Json(table), recorded(at));
    if let V::Json(value) = doc
        && let Some(trimmed) = trim::trim(value, &delimited)
    {
        let pool = Pool::new();
        let moved;
        let settings = if let Some(layout) = &settings.layout {
            moved = Settings {
                layout: Some(trimmed.forward_layout(layout)),
                ..settings.clone()
            };
            &moved
        } else {
            settings
        };
        let (source, mut entries) = write_sized(V::Json(&trimmed.doc), &pool, settings, source_len);
        for entry in &mut entries {
            if let Some(path) = &entry.path {
                entry.path = Some(trimmed.remap(path));
            }
        }
        return (source, entries);
    }
    write_sized(doc, pool, settings, source_len)
}

/// [`write_trimmed`] after the trim.
fn write_sized<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    settings: &Settings,
    source_len: Option<usize>,
) -> (String, Vec<Entry>) {
    let budget = repair_budget(source_len.unwrap_or_else(|| written_estimate(doc)));
    let Some(marks) = spans::add(doc) else {
        return write_tree(doc, pool, settings, budget);
    };
    let (source, mut entries) = write_tree(V::Json(&marks.doc), pool, settings, budget);
    for entry in &mut entries {
        if let Some(path) = &entry.path {
            entry.path = Some(marks.remap(path));
        }
    }
    for at in &marks.unspelled {
        let path = format!("{at}/rowspan");
        if !entries
            .iter()
            .any(|e| e.path.as_deref() == Some(path.as_str()))
        {
            let message =
                "Carve source cannot spell field rowspan of a table_cell node here".to_string();
            entries.push(loss_entry("field-unspellable", message, path, None));
        }
    }
    (source, entries)
}

fn write_tree<'t>(
    doc: V<'t>,
    pool: &'t Pool,
    settings: &Settings,
    budget: usize,
) -> (String, Vec<Entry>) {
    let mut w = Writer::new(pool, settings.align_tables);
    w.work_limit = budget;
    w.extensions.clone_from(&settings.extensions);
    if settings.extensions.iter().any(|e| e == "carve.tabs") {
        w.matchers.push(state::Matcher {
            form: "div".into(),
            name: "tabs".into(),
        });
    }
    w.link_keys = match &settings.link_keys {
        Some(keys) => std::sync::Arc::clone(keys),
        None => std::sync::Arc::new(
            children(doc)
                .iter()
                .filter(|n| ty(n) == "link_reference_definition")
                .filter_map(|n| node::s(n, "key"))
                .map(str::to_string)
                .collect(),
        ),
    };
    #[cfg(test)]
    WRITTEN_BLOCKS.with(|count| count.set(count.get() + children(doc).len()));
    if let Some(entries) = &settings.layout {
        facts::read(&mut w, doc, entries);
    }
    read_abbreviations(&mut w, doc);
    let written = write_document(&mut w, doc);
    let source = controls::keep_above(&w, written, settings.source.as_deref());
    let mut entries = locate(doc, &w);
    root_metadata_losses(doc, &mut entries);
    (source, entries)
}

/// C2-B-085: the first definition of a term wins.
fn read_abbreviations(w: &mut Writer<'_>, doc: V<'_>) {
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

fn locate(doc: V<'_>, w: &Writer<'_>) -> Vec<Entry> {
    let wanted: AddrSet<usize> = w.sp.losses.iter().filter_map(|l| l.node).collect();
    let found = node::pointers(doc, &wanted);
    let mut nodes: AddrMap<usize, V<'_>> = AddrMap::default();
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
                .and_then(|n| nodes.get(&n).copied())
                .and_then(span)
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

fn root_metadata_losses(doc: V<'_>, out: &mut Vec<Entry>) {
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

fn last_block<'t>(nodes: Arr<'t>, from: usize, to: usize, list: Option<N<'t>>) -> Option<N<'t>> {
    nodes
        .slice(from, to)
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
        // C2-W-146: test the written text with the five pair rules and reparse each escape round.
        for _ in 0..16 {
            if w.group_certificate == GroupCertificate::PlainRows {
                break;
            }
            #[cfg(test)]
            PAIR_SCANS.with(|count| count.set(count.get() + 1));
            let asked = pairs::asked(w, nodes.slice(from, at).unwrap_or_else(Arr::empty));
            if asked.is_empty() {
                break;
            }
            let saved_lines = w.lines.clone();
            let saved_escapes = w.sp.escapes.clone();
            let saved_losses = w.sp.losses.len();
            pairs::apply(w, &asked);
            let next = repair_group(w, doc, from, out.is_empty());
            if w.sp.losses.len() > saved_losses {
                w.lines = saved_lines;
                w.sp.escapes = saved_escapes;
                w.sp.losses.truncate(saved_losses);
                break;
            }
            at = next;
        }
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
        w.written_chars += blanks + w.lines.iter().map(|l| l.len() + 1).sum::<usize>();
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
fn joins_group(w: &mut Writer<'_>, before: V<'_>, node: V<'_>) -> bool {
    if ty(node) == "paragraph" {
        return ty(before) == "definition_list";
    }
    ty(node) == "comment" && blocks::blank_lines_between(w, before, None, node, false) == 0
}

/// `prior` is the last block of the group before `at` that is no comment, and `following` the block behind the
/// comment lines after `at`.
fn is_bridged<'a>(
    w: &Writer<'_>,
    nodes: Arr<'a>,
    prior: Option<V<'a>>,
    at: usize,
    following: Option<V<'a>>,
) -> bool {
    let Some(prior) = prior else {
        return false;
    };
    if !matches!(ty(prior), "list" | "definition_list" | "footnote") {
        return false;
    }
    let node = nodes.get(at);
    let following = if node.is_some_and(|n| ty(n) == "comment") {
        following
    } else {
        node
    };
    !blocks::following_attributes(w, following).is_empty()
}

fn group_end(w: &mut Writer<'_>, nodes: Arr<'_>, from: usize) -> usize {
    let mut end = from + 1;
    // Kept as the group grows, so a run of comments is walked once rather than once per comment.
    let mut prior = nodes.get(from).filter(|n| ty(*n) != "comment");
    let mut after = 0;
    while let (Some(before), Some(node)) = (nodes.get(end - 1), nodes.get(end)) {
        if after <= end {
            after = blocks::comment_lines_end(nodes, end);
        }
        let bridged = is_bridged(w, nodes, prior, end, nodes.get(after));
        let hoisted = ty(node) == "comment" && bridged;
        let belongs = ty(before) == "comment" && bridged;
        if !hoisted && !belongs && !joins_group(w, before, node) {
            break;
        }
        if ty(node) != "comment" {
            prior = Some(node);
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
    w.group_certificate = GroupCertificate::None;
    let reported = w.sp.losses.len();
    repair_rounds(w, doc, from, first, reported)
}

/// C2-W-133: one repair budget for the whole document, `len` the characters of the document as written.
fn repair_budget(len: usize) -> usize {
    WORK_FLOOR.max(len.saturating_mul(WORK_FACTOR))
}

/// The length of a supplied tree as written, over-estimated: every string twice, a few characters per value.
fn written_estimate(v: V<'_>) -> usize {
    if let Some(s) = v.as_str() {
        return s.len().saturating_mul(2).saturating_add(2);
    }
    let mut sum: usize = 2;
    if let Some(items) = v.as_array() {
        for item in items {
            sum = sum.saturating_add(written_estimate(item));
        }
    } else if v.is_object() {
        v.each(&mut |_, value| sum = sum.saturating_add(written_estimate(value)));
    }
    sum
}

const MAX_REPAIR_CHARS: usize = 20_000_000;
const MIN_REPAIR_ROUNDS: usize = 32;
const WORK_FACTOR: usize = 1000;
const WORK_FLOOR: usize = 24_000_000;
const MAX_ROUNDS: usize = 8192;
const MAX_ORDERED_ESCAPES: usize = 4;

struct History {
    ids: AddrMap<usize, usize>,
    recent: std::collections::VecDeque<RepairKey>,
    seen: HashSet<RepairKey>,
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
        ids: AddrMap::default(),
        recent: std::collections::VecDeque::new(),
        seen: HashSet::new(),
        rounds: 0,
        remaining: i64::try_from(MAX_REPAIR_CHARS).unwrap_or(i64::MAX),
    };
    loop {
        let end = group_end(w, nodes, from);
        let group = nodes.slice(from, end).unwrap_or_else(Arr::empty);
        reset_lines(w);
        blocks::write_blocks(w, group, "", 0, false);
        if w.lines.is_empty() {
            return end;
        }
        if table::certifies_plain_rows(w, group) {
            w.group_certificate = GroupCertificate::PlainRows;
            return end;
        }
        if repeated_repair(w, group, &mut history) {
            return end;
        }
        let f = first && !w.blank_first;
        let chunks = typed_arena::Arena::new();
        let mut round = Round {
            chunks: &chunks,
            last: None,
            source: None,
            equal: false,
        };
        if !settle(w, doc, group, f, &mut round) && !exhausted(w) {
            if w.sp.losses.len() == reported && !round.equal {
                report_unread(w, group, round.last);
            }
            prune_group_escapes(w, group, first && !w.blank_first, &round);
            return end;
        }
    }
}

fn refuse_repair(w: &mut Writer<'_>, node: V<'_>) {
    w.sp.losses.push(state::Loss {
        code: "writer-refused",
        message: "the writer could not complete its repair for this block".into(),
        node: Some(id(node)),
        step: None,
    });
}

fn local_id(ids: &mut AddrMap<usize, usize>, n: V<'_>) -> usize {
    let next = ids.len();
    *ids.entry(id(n)).or_insert(next)
}

/// C2-W-133: the repair state of a group, compared exactly: the written lines and
/// a flat record of the per-node state with every optional part length-prefixed.
#[derive(Clone, PartialEq, Eq, Hash)]
struct RepairKey {
    lines: Vec<String>,
    state: Vec<usize>,
}

fn push_opt(state: &mut Vec<usize>, v: Option<usize>) {
    match v {
        Some(v) => state.extend([1, v]),
        None => state.push(0),
    }
}

fn repair_key(w: &Writer<'_>, ids: &mut AddrMap<usize, usize>, group: Arr<'_>) -> RepairKey {
    let mut state = vec![usize::from(w.blank_first), w.sp.losses.len()];
    let mut pending: Vec<V<'_>> = group.iter().collect();
    while let Some(n) = pending.pop() {
        state.push(local_id(ids, n));
        let escapes = if ty(n) == "text" {
            w.sp.escapes.get(&id(n))
        } else {
            None
        };
        push_opt(&mut state, escapes.map(Vec::len));
        state.extend(escapes.into_iter().flatten());
        push_opt(
            &mut state,
            w.forms
                .get(&id(n))
                .map(|&f| usize::try_from(f).unwrap_or(usize::MAX)),
        );
        let attempt = w.sp.attempts.get(&id(n));
        push_opt(&mut state, attempt.map(|a| a.step));
        push_opt(&mut state, attempt.map(|a| a.escapes.len()));
        for (t, at) in attempt.map_or(&[][..], |a| a.escapes.as_slice()) {
            state.extend([local_id(ids, *t), *at]);
        }
        state.push(usize::from(w.is_lost(n)));
        node::for_each_slot(n, false, &mut |kids, _, _| pending.extend(kids.iter()));
    }
    RepairKey {
        lines: w.lines.clone(),
        state,
    }
}

fn repeated_repair(w: &mut Writer<'_>, group: Arr<'_>, h: &mut History) -> bool {
    let chars: usize = w.lines.iter().map(|l| l.len() + 1).sum();
    if h.rounds == 0 {
        // C2-W-133 sizes the budget by the document as written; the source only bounds it from below.
        w.work_limit = w.work_limit.max(repair_budget(w.written_chars + chars));
        h.remaining = h
            .remaining
            .max(i64::try_from(chars * MIN_REPAIR_ROUNDS).unwrap_or(i64::MAX));
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

/// The text nodes below `nodes` in the order they are written: a title before the children.
fn text_order(nodes: Arr<'_>, order: &mut AddrMap<usize, usize>) {
    for n in nodes {
        if ty(n) == "text" {
            let next = order.len();
            order.insert(id(n), next);
        }
        node::for_each_slot(n, true, &mut |kids, _, _| text_order(kids, order));
    }
}

fn rank(order: &AddrMap<usize, usize>, n: V<'_>) -> i64 {
    order
        .get(&id(n))
        .map_or(-1, |&r| i64::try_from(r).unwrap_or(i64::MAX))
}

fn leftmost<'t>(group: Arr<'_>, escapes: &[(N<'t>, usize)]) -> Option<(N<'t>, usize)> {
    let mut order = AddrMap::default();
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

/// The group written with `escapes`, for a parse.
fn group_chunk<'t>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    escapes: AddrMap<usize, Vec<usize>>,
) -> Chunk {
    w.sp.escapes = escapes;
    written_chunk(w, group, first)
}

/// The group written with the escapes `w` holds, for a parse.
fn written_chunk<'t>(w: &mut Writer<'t>, group: Arr<'t>, first: bool) -> Chunk {
    w.lines.clear();
    w.lead = None;
    w.clear_starts();
    blocks::write_blocks(w, group, "", 0, false);
    Chunk::new(group_source(w, first))
}

/// C2-W-088, C2-W-091: whether the test asks for the escape `at` of `node`.
/// `read` is the group's last read-back source and its parse, reused where the text without the escape is the same.
fn is_asked<'t>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    (node, at): (V<'_>, usize),
    read: Option<(&str, V<'_>)>,
) -> bool {
    let lines = w.lines.clone();
    let lead = w.lead.clone();
    let starts = (w.starts.clone(), w.start_index.clone());
    let mut order = AddrMap::default();
    text_order(group, &mut order);
    let own_rank = rank(&order, node);
    let asked = if own_rank < 0 {
        asked_outside(w, group, first, (node, at), &order, read)
    } else {
        asked_in_group(w, group, first, (node, at), (&order, own_rank), read)
    };
    w.lines = lines;
    w.lead = lead;
    w.starts = starts.0;
    w.start_index = starts.1;
    asked
}

/// [`is_asked`] for a node of the group: the escapes of the text behind it are set aside in place, so the
/// work is that of the group and not of every escape the document holds.
fn asked_in_group<'t>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    (node, at): (V<'_>, usize),
    (order, own_rank): (&AddrMap<usize, usize>, i64),
    read: Option<(&str, V<'_>)>,
) -> bool {
    let mut behind = Vec::new();
    for (text_id, &r) in order {
        if i64::try_from(r).unwrap_or(i64::MAX) > own_rank
            && let Some(offsets) = w.sp.escapes.remove(text_id)
        {
            behind.push((*text_id, offsets));
        }
    }
    let own = w.sp.escapes.get(&id(node)).cloned();
    if let Some(offsets) = &own {
        let before = offsets.iter().copied().filter(|&o| o < at).collect();
        w.sp.escapes.insert(id(node), before);
    }
    let without = written_chunk(w, group, first);
    let list = w.sp.escapes.entry(id(node)).or_default();
    list.push(at);
    list.sort_unstable();
    let with = written_chunk(w, group, first);
    let with = with.parse(w);
    let asked = match read {
        Some((text, parsed)) if text == without.text() => !node::trees_equal(with, parsed),
        _ => !node::trees_equal(with, without.parse(w)),
    };
    match own {
        Some(offsets) => w.sp.escapes.insert(id(node), offsets),
        None => w.sp.escapes.remove(&id(node)),
    };
    w.sp.escapes.extend(behind);
    asked
}

/// [`is_asked`] for a node outside the order of the group: only its own escapes before `at` stay.
fn asked_outside<'t>(
    w: &mut Writer<'t>,
    group: Arr<'t>,
    first: bool,
    (node, at): (V<'_>, usize),
    order: &AddrMap<usize, usize>,
    read: Option<(&str, V<'_>)>,
) -> bool {
    let own_rank = -1;
    let placed = w.sp.escapes.clone();
    let mut left: AddrMap<usize, Vec<usize>> = AddrMap::default();
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
    let without = group_chunk(w, group, first, left);
    let with = group_chunk(w, group, first, escaped);
    let with = with.parse(w);
    let asked = match read {
        Some((text, parsed)) if text == without.text() => !node::trees_equal(with, parsed),
        _ => !node::trees_equal(with, without.parse(w)),
    };
    w.sp.escapes = placed;
    asked
}

/// The parses of one repair round: the last one is read by the loss report and the prune.
struct Round<'s> {
    chunks: &'s typed_arena::Arena<Chunk>,
    last: Option<V<'s>>,
    /// The source `last` was parsed from.
    source: Option<&'s str>,
    /// The last read-back parse was already proved equal to the group.
    equal: bool,
}

fn compare_group<'t, 's>(
    w: &mut Writer<'t>,
    doc: N<'t>,
    group: Arr<'t>,
    first: bool,
    round: &mut Round<'s>,
) -> Check<'t> {
    #[cfg(test)]
    REPAIR_PARSES.with(|count| count.set(count.get() + 1));
    let chunk: &'s Chunk = round.chunks.alloc(Chunk::new(group_source(w, first)));
    let parsed = chunk.parse(w);
    round.last = Some(parsed);
    round.source = Some(chunk.text());
    let mut c = Check {
        source: Some(chunk.text().to_string()),
        ..Check::default()
    };
    let actual = children(parsed);
    round.equal = group.len() == actual.len()
        && group
            .iter()
            .zip(actual)
            .all(|(expected, read_back)| node::trees_equal(expected, read_back));
    if !round.equal {
        compare_blocks(w, &mut c, group, actual, doc);
    }
    escape_openers(w, &mut c, parsed, if first { 1 } else { 2 });
    c
}

/// One round of the test of C2-W-089. False when the group reads back as its blocks.
fn settle<'t>(
    w: &mut Writer<'t>,
    doc: N<'t>,
    group: Arr<'t>,
    first: bool,
    round: &mut Round<'_>,
) -> bool {
    let mut c = compare_group(w, doc, group, first, round);
    let read = round.source.zip(round.last);
    let joint_braces = joint_brace_repair(w, &c, group, first);
    let dropped = !joint_braces && drop_asked_trials(w, &mut c, group, first, read);
    if dropped && c.changes() == 0 {
        return true;
    }
    if !joint_braces && c.escapes.len() > 1 && c.escapes.len() <= MAX_ORDERED_ESCAPES {
        keep_leftmost_escape(w, &mut c, group);
    }
    extend_chains(w, &mut c);
    if c.changes() == 0 && !c.losses.is_empty() && first && !w.blank_first {
        // C2-W-005: what only line 1 reads differently is written below one blank line.
        w.blank_first = compare_group(w, doc, group, false, round).losses.is_empty();
        if w.blank_first {
            return true;
        }
    }
    if c.changes() == 0 && !c.losses.is_empty() && braces_unpaired(w, group) {
        return true;
    }
    apply_changes(w, &c)
}

/// Two brace escapes can be needed together even though neither changes the
/// parse alone. Keep that repair when its combined spelling reads back as the
/// group; the ordinary per-escape test handles every other candidate.
fn joint_brace_repair<'t>(w: &mut Writer<'t>, c: &Check<'t>, group: Arr<'t>, first: bool) -> bool {
    let Some((first_node, _)) = c.trials.first().copied() else {
        return false;
    };
    if c.trials.len() < 2
        || c.trials.len() != c.escapes.len()
        || c.trials
            .iter()
            .any(|(n, at)| id(*n) != id(first_node) || text(*n).as_bytes().get(*at) != Some(&b'{'))
    {
        return false;
    }
    let saved = w.sp.escapes.clone();
    let lines = w.lines.clone();
    let lead = w.lead.clone();
    let starts = (w.starts.clone(), w.start_index.clone());
    for (n, at) in &c.escapes {
        w.add_escape(n, *at);
    }
    let chunk = written_chunk(w, group, first);
    let parsed = chunk.parse(w);
    let actual = children(parsed);
    let equal =
        group.len() == actual.len() && group.iter().zip(actual).all(|(e, a)| writer_equal(e, a));
    w.sp.escapes = saved;
    w.lines = lines;
    w.lead = lead;
    w.starts = starts.0;
    w.start_index = starts.1;
    equal
}

/// Drops the escapes of trials that were already asked; true when any was dropped.
fn drop_asked_trials<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    group: Arr<'t>,
    first: bool,
    read: Option<(&str, V<'_>)>,
) -> bool {
    let mut dropped = false;
    let trials = if c.trials.len() <= MAX_ORDERED_ESCAPES {
        c.trials.clone()
    } else {
        Vec::new()
    };
    for (n, at) in trials {
        if is_asked(w, group, first, (n, at), read) {
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

fn keep_leftmost_escape<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, group: Arr<'t>) {
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
fn refuse_leaf_lines(w: &mut Writer<'_>, n: V<'_>, depth: usize) {
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

/// C2-W-069, C2-W-117: the comparison of a group stops at a loss, so the bare pairs behind it that cannot
/// pair are braced here; true when one was.
fn braces_unpaired<'t>(w: &mut Writer<'t>, group: Arr<'t>) -> bool {
    let forms = w.forms.len();
    reset_lines(w);
    w.strict_pairs = true;
    blocks::write_blocks(w, group, "", 0, false);
    w.strict_pairs = false;
    w.forms.len() > forms
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
        refuse_leaf_lines(w, *n, 0);
    }
    for (n, field) in &c.losses {
        lose(w, n, field.as_deref());
    }
    false
}

/// C2-W-002: whether `actual` is `expected` for the readback check (C2-A-193).
fn writer_equal(expected: V<'_>, actual: V<'_>) -> bool {
    node::trees_equal(expected, actual)
        || extension::source_core_equivalent(expected, Some(actual))
        || node::trees_equal(&node::without_standard_data(expected), actual)
        || extension::with_core_inlines(expected).is_some_and(|e| node::trees_equal(&e, actual))
}

/// C2-W-011: a block that the rounds let pass and that reads back as another tree is reported.
/// `parsed` is the last read-back parse of the group.
fn report_unread(w: &mut Writer<'_>, group: Arr<'_>, parsed: Option<V<'_>>) {
    let parsed: Arr<'_> = parsed.map_or_else(Arr::empty, children);
    let unread = (0..group.len().max(parsed.len())).find(|&at| {
        !matches!((parsed.get(at), group.get(at)), (Some(block), Some(e)) if writer_equal(e, block))
    });
    if let Some(n) = unread.and_then(|at| group.get(at.min(group.len().saturating_sub(1)))) {
        lose(w, n, None);
    }
}

fn star_after_break(nodes: Arr<'_>, byte: usize, parent: Option<V<'_>>) -> bool {
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

/// `parsed` is the read-back parse of `source`, asked for only before a star.
fn prunable_opener<'p>(source: &str, at: usize, parsed: &mut dyn FnMut() -> Option<V<'p>>) -> bool {
    let b = source.as_bytes();
    let next = b.get(at + 1).copied();
    if next == Some(b'*') {
        return parsed().is_some_and(|p| star_after_break(children(p), at, None));
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
/// `round.last` is the last read-back parse of the group, the parse of its source as written.
fn prune_group_escapes(w: &mut Writer<'_>, group: Arr<'_>, first: bool, round: &Round<'_>) {
    let mut source = group_source(w, first);
    let raws = std::mem::take(&mut w.raw_refs);
    let mut at = source.find('\\');
    // The parse of `source` as it stands, read again (uncounted) after a prune only where a star asks.
    let mut parsed = round.last;
    let mut parsed_source: Option<String> = None;
    while let Some(p) = at {
        if exhausted(w) {
            if let Some(host) = group.first() {
                refuse_repair(w, host);
            }
            break;
        }
        let mut current = || {
            if parsed.is_none() {
                let chunk: &Chunk = round.chunks.alloc(Chunk::new(parsed_source.take()?));
                parsed = Some(chunk.read(w, Some(&w.link_keys)));
            }
            parsed
        };
        if prunable_opener(&source, p, &mut current) && !in_raw(&source, p, &raws) {
            let candidate = format!(
                "{}{}",
                source.get(..p).unwrap_or(""),
                source.get(p + 1..).unwrap_or("")
            );
            let chunk = Chunk::new(candidate);
            let kids = children(chunk.parse(w));
            if kids.len() == group.len()
                && kids.iter().zip(group).all(|(n, e)| node::trees_equal(n, e))
            {
                source = chunk.text().to_string();
                parsed = None;
                parsed_source = Some(source.clone());
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
            ids: AddrMap::default(),
            recent: std::collections::VecDeque::new(),
            seen: HashSet::new(),
            rounds: 1,
            remaining: 0,
        };

        assert!(repeated_repair(
            &mut writer,
            Arr::Json(&group),
            &mut history
        ));
        assert_eq!(
            writer.sp.losses.first().map(|loss| loss.code),
            Some("writer-refused")
        );
    }

    #[test]
    fn budget_grows_with_the_written_length() {
        let pool = typed_arena::Arena::new();
        let mut writer = Writer::new(&pool, false);
        writer.work_limit = repair_budget(10);
        writer.written_chars = 40_000;
        writer.lines.push("xyz".into());
        let group = [
            serde_json::json!({"type": "paragraph", "children": [{"type": "text", "value": "xyz"}]}),
        ];
        let mut history = History {
            ids: AddrMap::default(),
            recent: std::collections::VecDeque::new(),
            seen: HashSet::new(),
            rounds: 0,
            remaining: i64::try_from(MAX_REPAIR_CHARS).unwrap_or(i64::MAX),
        };

        assert!(!repeated_repair(
            &mut writer,
            Arr::Json(&group),
            &mut history
        ));
        assert_eq!(writer.work_limit, 40_004 * WORK_FACTOR);
    }
}
