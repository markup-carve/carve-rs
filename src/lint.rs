//! Source lint under C2-W-103 to C2-W-109.

use crate::diagnostic::Diagnostic;
use crate::source::Source;
use serde_json::Value;
use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

mod controls;
mod data_attrs;
mod data_interpreter;
mod data_model;
mod data_rules;
mod includes;
mod lines;
mod nodes;
mod pairs;
use crate::view::V;
mod walk;

/// Rule switches and platform selection for a lint run.
#[derive(Debug, Clone, Default)]
pub struct LintOptions {
    /// Heading id policy used to resolve fragment targets.
    pub heading_ids: crate::options::HeadingIds,
    /// Whether raw HTML blocks contribute rendered anchors.
    pub raw_html: bool,
    /// A rule id mapped to `on` or `off`.
    pub rules: HashMap<String, String>,
    /// Platform names, currently `github`.
    pub platforms: Vec<String>,
    /// Registered extensions that may contribute generated IDs.
    pub extensions: Vec<String>,
    /// Include expansion for lint of an entry document (C2-N-085).
    pub includes: Option<crate::include::Includes>,
}

fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../spec-data/lint-rules.json")).unwrap_or(Value::Null)
    })
}

/// What `active` reads of a catalog row, indexed by rule id once.
struct Row {
    usable: bool,
    default_on: bool,
    modeled: bool,
}

fn rows() -> &'static HashMap<&'static str, Row> {
    static ROWS: OnceLock<HashMap<&'static str, Row>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let mut rows = HashMap::new();
        for row in catalog()["rules"].as_array().into_iter().flatten() {
            let Some(id) = row["id"].as_str() else {
                continue;
            };
            rows.entry(id).or_insert_with(|| Row {
                usable: (row["channel"] == "lint" || row["channel"].is_null())
                    && !matches!(row["status"].as_str(), Some("obsolete" | "void")),
                default_on: row["default"].as_str().unwrap_or("on") == "on",
                modeled: data_rules::modeled_ids().contains(id),
            });
        }
        rows
    })
}

fn active(code: &str, opts: &LintOptions) -> bool {
    let Some(row) = rows().get(code) else {
        return false;
    };
    !row.modeled && row_active(code, row, opts)
}

fn active_catalog(code: &str, opts: &LintOptions) -> bool {
    rows()
        .get(code)
        .is_some_and(|row| row_active(code, row, opts))
}

fn row_active(code: &str, row: &Row, opts: &LintOptions) -> bool {
    if !row.usable {
        return false;
    }
    if code.starts_with("platform-") && !opts.platforms.iter().any(|p| p == "github") {
        return false;
    }
    opts.rules
        .get(code)
        .map_or(row.default_on, |setting| setting == "on")
}

/// Runs enabled lint rules and returns catalog diagnostics in source order.
#[must_use]
pub fn lint(source: &str, opts: &LintOptions) -> Vec<Diagnostic> {
    let includes = opts.includes.is_some() && source.contains("{{");
    let parse = crate::Options {
        includes: opts.includes.clone().filter(|_| includes),
        heading_ids: opts.heading_ids,
        raw_html: opts.raw_html,
        ..crate::Options::default()
    };
    let same = !includes && opts.heading_ids == crate::options::HeadingIds::default();
    let (mut out, diagnostics) = crate::with_lint_tree(
        source,
        &crate::Options::default(),
        &parse,
        same,
        |tree, arena| lint_core(source, opts, tree, arena),
    );
    out.extend(
        diagnostics
            .into_iter()
            .filter(|diagnostic| !data_rules::modeled_ids().contains(diagnostic.code)),
    );
    if includes {
        includes::scan(source, opts, &mut out);
    }
    out
}

fn lint_core<'a>(
    source: &str,
    opts: &LintOptions,
    tree: Option<V<'a>>,
    arena: &'a crate::Arena,
) -> Vec<Diagnostic> {
    let parsed = Source::new(source);
    let walk::Walk {
        ranges,
        items,
        comments,
        holders,
        values,
    } = tree.map_or_else(walk::Walk::empty, |tree| {
        walk::Walk::new(tree, arena, source)
    });
    let positions = Ranges::new(source, ranges, holders);
    let mut out = Vec::new();
    let line_model = lines::lines(source, &positions.all);
    if tree.is_some() {
        nodes::scan(source, opts, items, &mut out);
    }
    referenced_footnote_platform_tokens(source, opts, &positions, &mut out);
    if let Some(tree) = tree {
        out.retain(|finding| !data_rules::modeled_ids().contains(finding.code));
        let mut positions = positions;
        let mut joined = Vec::new();
        joined_rows(source, tree, &mut positions, &values, &mut joined);
        let nodes = TreeNodes {
            values: &values,
            joined,
            made: std::cell::RefCell::new(Vec::new()),
        };
        out.extend(data_rules::run(
            source,
            &parsed,
            &positions.all,
            &positions.holders,
            &nodes,
            &values,
            tree,
            &line_model,
            opts,
        ));
    }
    let invalid_tables: Vec<_> = out
        .iter()
        .filter(|finding| finding.code == "table-attribute-invalid")
        .filter_map(|finding| finding.path.as_deref())
        .map(str::to_string)
        .collect();
    out.retain(|finding| {
        !matches!(
            finding.code,
            "table-column-arity" | "table-column-overlap" | "table-width-total"
        ) || !invalid_tables.iter().any(|path| {
            finding
                .path
                .as_deref()
                .is_some_and(|actual| actual.starts_with(path))
        })
    });
    out.sort_by_key(|d| d.pos.map_or(0, |p| p.start_offset));
    controls::filter(source, out, &comments)
}

fn referenced_footnote_platform_tokens(
    source: &str,
    opts: &LintOptions,
    ranges: &Ranges<'_>,
    output: &mut Vec<Diagnostic>,
) {
    if !opts.platforms.iter().any(|platform| platform == "github") {
        return;
    }
    let positions = Source::new(source);
    let references: HashSet<&str> = ranges
        .all
        .iter()
        .filter_map(|range| range.footnote_key)
        .collect();
    let mut offset = 0;
    for piece in source.split_inclusive('\n') {
        let line = piece.trim_end_matches('\n');
        if let Some(rest) = line.strip_prefix("[^")
            && let Some((label, body)) = rest.split_once("]:")
            && references.contains(label)
            && !opaque_definition(ranges, offset)
        {
            let body_start = offset + 2 + label.len() + 2;
            let trimmed = body.trim_start();
            let start = body_start + body.len() - trimmed.len();
            let (code, token) = if let Some(after_hash) = trimmed.strip_prefix('#') {
                (
                    "platform-issue-reference",
                    trimmed
                        .get(..1 + after_hash.bytes().take_while(u8::is_ascii_digit).count())
                        .unwrap_or(""),
                )
            } else if trimmed.starts_with('@') {
                (
                    "platform-mention-token",
                    trimmed.split_whitespace().next().unwrap_or(""),
                )
            } else {
                offset += piece.len();
                continue;
            };
            if token.len() > 1 && active(code, opts) {
                let mut diagnostic = Diagnostic::new(
                    code,
                    &[("host", "GitHub"), ("text", token), ("what", "a mention")],
                    None,
                    Some(positions.pos(start, start + token.len())),
                );
                diagnostic.path = Some(String::new());
                output.push(diagnostic);
            }
        }
        offset += piece.len();
    }
}

fn opaque_definition(ranges: &Ranges<'_>, offset: usize) -> bool {
    ranges.around(offset).any(|range| {
        ["code_block", "raw_block", "frontmatter", "comment"].contains(&range.kind)
            && range.start <= offset
            && offset < range.end
    })
}

/// Drops every finding that a later finding of the same code at `replacing`
/// overlaps: one pass in place of a scan of all findings per emit.
fn drop_replaced(output: &mut Vec<Diagnostic>, replacing: &[usize]) {
    let mut later: HashMap<&'static str, Vec<(u32, u32, usize)>> = HashMap::new();
    for &at in replacing {
        if let Some(finding) = output.get(at)
            && let Some(pos) = finding.pos
        {
            later
                .entry(finding.code)
                .or_default()
                .push((pos.start_offset, pos.end_offset, at));
        }
    }
    if later.is_empty() {
        return;
    }
    let mut dropped = vec![false; output.len()];
    for (code, mut spans) in later {
        // A Fenwick tree over the starts of the replacing findings holds the
        // furthest end; filled from the last finding back, a query sees only later ones.
        let mut starts: Vec<u32> = spans.iter().map(|span| span.0).collect();
        starts.sort_unstable();
        starts.dedup();
        let mut furthest = vec![0u32; starts.len() + 1];
        spans.sort_unstable_by_key(|span| span.2);
        let mut spans = spans.into_iter().rev().peekable();
        for (at, finding) in output.iter().enumerate().rev() {
            if finding.code == code
                && let Some(pos) = finding.pos
            {
                let mut slot = starts.partition_point(|start| *start < pos.end_offset);
                while slot > 0 {
                    if furthest
                        .get(slot)
                        .is_some_and(|end| *end > pos.start_offset)
                    {
                        if let Some(flag) = dropped.get_mut(at) {
                            *flag = true;
                        }
                        break;
                    }
                    slot &= slot - 1;
                }
            }
            while let Some((start, end, _)) = spans.next_if(|span| span.2 == at) {
                let mut slot = starts.partition_point(|s| *s < start) + 1;
                while let Some(slot_end) = furthest.get_mut(slot) {
                    *slot_end = (*slot_end).max(end);
                    slot += slot & slot.wrapping_neg();
                }
            }
        }
    }
    let mut flags = dropped.into_iter();
    output.retain(|_| !flags.next().unwrap_or(false));
}

/// The node ranges, with the ranges that touch each source line, so a lookup
/// at one offset reads the ranges around it instead of all of them.
struct Ranges<'a> {
    all: Vec<Range<'a>>,
    holders: Vec<Holder>,
    source: &'a str,
    index: OnceCell<RangeIndex>,
}

struct RangeIndex {
    line_starts: Vec<usize>,
    by_line: Vec<Vec<usize>>,
}

impl<'a> Ranges<'a> {
    const fn new(source: &'a str, all: Vec<Range<'a>>, holders: Vec<Holder>) -> Self {
        Self {
            all,
            holders,
            source,
            index: OnceCell::new(),
        }
    }

    fn index(&self) -> &RangeIndex {
        self.index.get_or_init(|| {
            let mut line_starts = vec![0];
            line_starts.extend(self.source.match_indices('\n').map(|(at, _)| at + 1));
            let line_of = |at: usize| line_starts.partition_point(|s| *s <= at).saturating_sub(1);
            let mut by_line = vec![Vec::new(); line_starts.len()];
            for (at, range) in self.all.iter().enumerate() {
                let lines = line_of(range.start)..=line_of(range.end.max(range.start));
                for line in by_line.get_mut(lines).into_iter().flatten() {
                    line.push(at);
                }
            }
            RangeIndex {
                line_starts,
                by_line,
            }
        })
    }

    /// The ranges on the line of `at`, in tree order: every range that holds `at` is among them.
    fn around(&self, at: usize) -> impl Iterator<Item = &Range<'a>> {
        let index = self.index();
        let line = index
            .line_starts
            .partition_point(|s| *s <= at)
            .saturating_sub(1);
        index
            .by_line
            .get(line)
            .into_iter()
            .flatten()
            .filter_map(|at| self.all.get(*at))
    }
}

#[allow(clippy::struct_excessive_bools)] // Independent facts of a node, read by separate rules.
struct Range<'a> {
    start: usize,
    end: usize,
    path: LazyPath<'a>,
    kind: &'a str,
    start_line: usize,
    end_line: usize,
    has_attrs: bool,
    footnote_key: Option<&'a str>,
    /// A comment written as a block.
    form_block: bool,
    /// A paragraph whose only child is an image.
    image_line: bool,
    /// The holder whose `children` hold this node; `None` for the document's own children.
    holder: Option<u32>,
    /// The node's index in its `children` list.
    index: u32,
    /// The range of the item before it in the same `children` list.
    prev: Option<u32>,
    /// A table row without `pos` that a continuation row joined, placed by its source layout entry
    /// for the data rules (C2-W-150).
    joined: bool,
}

/// A node that holds `children`: its range, if it has one, and the holder that holds it.
struct Holder {
    range: Option<u32>,
    up: Option<u32>,
    /// The `title` list of its range's node: inline content whose leaf is that node, as carve-js reads it.
    title: bool,
}

/// A node's pointer, written out only when a rule reads it.
struct LazyPath<'a> {
    node: Option<&'a walk::PathNode<'a>>,
    /// The pointer text, built on first use by the data rules.
    text: OnceCell<String>,
}

impl LazyPath<'_> {
    fn len(&self) -> usize {
        self.node.map_or(0, |node| node.len)
    }

    /// Whether this is the document's own path, without building the text.
    const fn is_empty(&self) -> bool {
        self.node.is_none()
    }

    fn as_str(&self) -> &str {
        self.text.get_or_init(|| self.text())
    }

    fn text(&self) -> String {
        walk::path_text(self.node)
    }
}

impl PartialEq<&str> for LazyPath<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.len() == other.len() && self.text() == *other
    }
}

/// The tree nodes of the ranges, the rows of `joined_rows` after those of the walk, and the values
/// the data rules have stepped into, by handle.
struct TreeNodes<'v, 't> {
    values: &'v [V<'t>],
    joined: Vec<V<'t>>,
    made: std::cell::RefCell<Vec<V<'t>>>,
}

impl<'t> TreeNodes<'_, 't> {
    fn handle(&self, node: V<'t>) -> Option<u32> {
        let mut made = self.made.borrow_mut();
        made.push(node);
        u32::try_from(made.len() - 1).ok()
    }
}

impl data_model::NodeTree for TreeNodes<'_, '_> {
    fn root(&self, range: u32) -> Option<u32> {
        let at = range as usize;
        let node = self
            .values
            .get(at)
            .or_else(|| self.joined.get(at.wrapping_sub(self.values.len())))
            .copied()
            .filter(|node| node.is_object())?;
        self.handle(node)
    }

    fn step(&self, node: u32, step: data_model::Step) -> data_model::Leaf<'_> {
        use data_model::Leaf;
        let Some(value) = self.made.borrow().get(node as usize).copied() else {
            return Leaf::Absent;
        };
        let found = match step {
            data_model::Step::Key(key) => value.get(key),
            data_model::Step::Index(index) => value.as_array().and_then(|items| items.get(index)),
        };
        let Some(found) = found.filter(|found| !found.is_null()) else {
            return Leaf::Absent;
        };
        if let Some(text) = found.as_str() {
            Leaf::Str(text)
        } else if let Some(b) = found.as_bool() {
            Leaf::Bool(b)
        } else if found.is_number() {
            Leaf::Num(found.as_i64().unwrap_or(0))
        } else {
            self.handle(found).map_or(Leaf::Absent, Leaf::Node)
        }
    }

    fn len(&self, node: u32) -> Option<usize> {
        let value = self.made.borrow().get(node as usize).copied()?;
        value.as_array().map(crate::view::Arr::len)
    }
}

/// C2-W-150: a table row that a continuation row joined omits `pos` (C2-A-158) and still has a block
/// record, placed where its source layout entry is.
fn joined_rows<'t>(
    source: &str,
    tree: V<'_>,
    positions: &mut Ranges<'_>,
    values: &[V<'t>],
    joined: &mut Vec<V<'t>>,
) {
    let rows = pos_less_rows(&positions.all, values);
    if rows.is_empty() {
        return;
    }
    let entries = crate::api::layout::entries(source, &tree.to_value());
    let spans: HashMap<&str, (usize, usize)> = entries
        .iter()
        .filter_map(|entry| {
            let byte = |key: &str| {
                entry
                    .get(key)
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|at| usize::try_from(at).ok())
            };
            Some((
                entry.get("path")?.as_str()?,
                (byte("startByte")?, byte("endByte")?),
            ))
        })
        .collect();
    let breaks: Vec<usize> = memchr_lines(source);
    let line_of = |at: usize| breaks.partition_point(|&brk| brk < at) + 1;
    let mut index = JoinIndex::new(positions);
    for (table, group, row_at, row) in rows {
        let Some(table_path) = positions
            .all
            .get(table)
            .map(|range| range.path.as_str().to_owned())
        else {
            continue;
        };
        let group_path = format!("{table_path}/children/{group}");
        let path = format!("{group_path}/children/{row_at}");
        let (Some(&(start, end)), Some(&parent), Ok(at), Ok(made)) = (
            spans.get(path.as_str()),
            index.groups.get(group_path.as_str()),
            u32::try_from(row_at),
            u32::try_from(positions.all.len()),
        ) else {
            continue;
        };
        let prev = at
            .checked_sub(1)
            .and_then(|before| index.siblings.get(&(parent, before)).copied())
            .and_then(|before| u32::try_from(before).ok());
        let next = index.siblings.get(&(parent, at + 1)).copied();
        let (node, holder) = index.cells.get(&(parent, at)).copied().unzip();
        let node = node.flatten();
        let text = OnceCell::new();
        if node.is_none() {
            let _ = text.set(path);
        }
        positions.all.push(Range {
            start,
            end,
            path: LazyPath { node, text },
            kind: "table_row",
            start_line: line_of(start),
            end_line: line_of(end.saturating_sub(1).max(start)),
            has_attrs: false,
            footnote_key: None,
            form_block: false,
            image_line: false,
            holder: Some(parent),
            index: at,
            prev,
            joined: true,
        });
        index.siblings.insert((parent, at), made as usize);
        joined.push(row);
        if let Some(holder) = holder.and_then(|cells| positions.holders.get_mut(cells as usize)) {
            holder.range = Some(made);
        }
        if let Some(next) = next.and_then(|after| positions.all.get_mut(after)) {
            next.prev = Some(made);
        }
    }
}

/// The offsets of the line breaks of a source.
fn memchr_lines(source: &str) -> Vec<usize> {
    source
        .bytes()
        .enumerate()
        .filter_map(|(at, byte)| (byte == b'\n').then_some(at))
        .collect()
}

/// What `joined_rows` looks up, indexed once per call.
struct JoinIndex<'a> {
    /// The holder of the children of each table group, by the group's path.
    groups: HashMap<String, u32>,
    /// The range of each item of a `children` list, by holder and index.
    siblings: HashMap<(u32, u32), usize>,
    /// For a row without a range: the path node of the row and the holder of its cells, read off a
    /// cell of it that has a range, by the group's holder and the row's index.
    cells: HashMap<(u32, u32), (Option<&'a walk::PathNode<'a>>, u32)>,
}

impl<'a> JoinIndex<'a> {
    fn new(positions: &Ranges<'a>) -> Self {
        let holder_of: HashMap<u32, u32> = positions
            .holders
            .iter()
            .enumerate()
            .filter_map(|(at, holder)| Some((holder.range?, u32::try_from(at).ok()?)))
            .collect();
        let mut groups = HashMap::new();
        let mut siblings = HashMap::new();
        let mut cells = HashMap::new();
        for (at, range) in positions.all.iter().enumerate() {
            let Ok(own) = u32::try_from(at) else {
                break;
            };
            if let Some(parent) = range.holder {
                siblings.insert((parent, range.index), at);
            }
            if matches!(range.kind, "table_head" | "table_body" | "table_foot")
                && let Some(&holder) = holder_of.get(&own)
            {
                groups.insert(range.path.as_str().to_owned(), holder);
            }
            let Some(cell_holder) = range.holder.filter(|_| range.kind == "table_cell") else {
                continue;
            };
            let Some(row_holder) = positions.holders.get(cell_holder as usize) else {
                continue;
            };
            let row = range.path.node.and_then(|node| node.previous?.previous);
            if let (None, Some(group), Some(walk::Segment::Index(index))) = (
                row_holder.range,
                row_holder.up,
                row.map(|node| node.segment),
            ) && let Ok(index) = u32::try_from(index)
            {
                cells.entry((group, index)).or_insert((row, cell_holder));
            }
        }
        Self {
            groups,
            siblings,
            cells,
        }
    }
}

/// The table rows without `pos`: the table's range, the group and row indexes, and the row.
fn pos_less_rows<'t>(ranges: &[Range<'_>], values: &[V<'t>]) -> Vec<(usize, usize, usize, V<'t>)> {
    let mut rows = Vec::new();
    for (table, (range, value)) in ranges.iter().zip(values).enumerate() {
        if range.kind != "table" {
            continue;
        }
        let groups = value.at("children");
        let groups = groups
            .as_array()
            .into_iter()
            .flat_map(crate::view::Arr::iter);
        for (group, rows_of) in groups.enumerate() {
            let list = rows_of.at("children");
            let list = list.as_array().into_iter().flat_map(crate::view::Arr::iter);
            for (index, row) in list.enumerate() {
                if row.ty() == "table_row" && row.pos().is_none() {
                    rows.push((table, group, index, row));
                }
            }
        }
    }
    rows
}

/// Records item `at` of a `children` list, made as range `made`, as the previous sibling of item `at + 1`.
fn link_prev(slot: &mut Option<(u32, u32)>, at: u32, made: u32, out: &mut [Range<'_>]) {
    if let Some((later, range)) = *slot
        && later == at + 1
        && let Some(next) = out.get_mut(range as usize)
    {
        next.prev = Some(made);
    }
    *slot = Some((at, made));
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, LintOptions, drop_replaced, walk};
    use crate::ast::Pos;
    use crate::include::{Cause, Includes};
    use std::sync::Arc;

    #[test]
    fn configured_resolver_without_include_opener_parses_once() {
        let options = LintOptions {
            includes: Some(Includes::new(Arc::new(|_| Err(Cause::NotFound)))),
            ..LintOptions::default()
        };
        crate::PREPARE_COUNT.with(|count| count.set(0));
        let _ = super::lint("| a | b |\n", &options);
        crate::PREPARE_COUNT.with(|count| assert_eq!(count.get(), 1));
    }

    #[test]
    fn data_walk_does_not_prepare_hand_records_for_clean_tables_at_scale() {
        for rows in [32, 512] {
            let source = format!("|= A |= B |\n{}", "| a | b |\n".repeat(rows));
            let opts = crate::Options::default();
            crate::with_lint_tree(&source, &opts, &opts, true, |tree, arena| {
                let checked = tree.map(|tree| {
                    let walk = walk::Walk::new(tree, arena, &source);
                    assert!(
                        walk.items.is_empty(),
                        "{rows} rows prepared hand-node items: {:?}",
                        walk.items
                            .iter()
                            .map(|item| item.node.ty().as_str().unwrap_or("").to_string())
                            .collect::<Vec<_>>()
                    );
                });
                assert!(checked.is_some(), "parsed table");
            });
        }
    }

    fn finding(code: &'static str, start: u32, end: u32) -> Diagnostic {
        let pos = Pos {
            start_offset: start,
            end_offset: end,
            ..Pos::default()
        };
        Diagnostic::new(code, &[], None, Some(pos))
    }

    /// The one-pass filter keeps exactly what a `retain` before each replacing push kept.
    #[test]
    fn drop_replaced_matches_retain_per_emit() {
        let mut seed = 7u64;
        let mut next = |bound: u64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            u32::try_from((seed >> 33) % bound).unwrap_or(0)
        };
        for _ in 0..500 {
            let mut expected: Vec<Diagnostic> = Vec::new();
            let mut output: Vec<Diagnostic> = Vec::new();
            let mut replacing = Vec::new();
            for _ in 0..next(40) {
                let code = match next(3) {
                    0 => "a",
                    1 => "b",
                    _ => "c",
                };
                let start = next(60);
                let item = finding(code, start, start + next(6));
                if next(4) == 0 {
                    expected.push(item.clone());
                } else {
                    let pos = item.pos.unwrap_or_default();
                    expected.retain(|d| {
                        !(d.code == code
                            && d.pos.is_some_and(|p| {
                                p.start_offset < pos.end_offset && pos.start_offset < p.end_offset
                            }))
                    });
                    expected.push(item.clone());
                    replacing.push(output.len());
                }
                output.push(item);
            }
            drop_replaced(&mut output, &replacing);
            assert_eq!(output, expected);
        }
    }
}
