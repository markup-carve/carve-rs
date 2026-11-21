//! Rule selection and reporting from the pinned 0.2 lint model, compiled once per process.
#![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

use super::data_interpreter::{
    Eval, Expr, F, Facts, Pattern, Scope, Val, entry_quick_tests, field_expr, held_literals, holds,
    pattern, quick_rejects, quick_tests,
};
use super::data_model::{LineRec, Node, Records, Subject};
use super::pairs;
use super::{Holder, LintOptions, Range, active_catalog, lines::Line};
use crate::diagnostic::Diagnostic;
use crate::source::Source;
use crate::view::V;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;
use std::sync::OnceLock;

#[allow(clippy::expect_used)] // The pinned model is a checked build input.
fn model() -> &'static Value {
    static MODEL: OnceLock<Value> = OnceLock::new();
    MODEL.get_or_init(|| {
        serde_json::from_str(include_str!("../../spec-data/lint-model.json"))
            .expect("pinned lint model")
    })
}

thread_local! {
    static DATA_MODE: Cell<Option<bool>> = const { Cell::new(None) };
}

fn read_mode() -> bool {
    std::env::var("CARVE_LINT_RULES").is_ok_and(|value| value == "data")
}

/// Whether the rules of the model run from data; read once per `lint` call.
pub(super) fn data_mode() -> bool {
    DATA_MODE.with(Cell::get).unwrap_or_else(read_mode)
}

/// Holds the switch for one `lint` call and restores the outer value when dropped.
pub(super) struct ModeScope(Option<bool>);

pub(super) fn enter() -> ModeScope {
    ModeScope(DATA_MODE.with(|mode| mode.replace(Some(read_mode()))))
}

impl Drop for ModeScope {
    fn drop(&mut self) {
        DATA_MODE.with(|mode| mode.set(self.0));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Over {
    Line,
    Block,
    Inline,
}

struct Entry {
    when: Option<Expr>,
    /// The quick tests of `when` that run before anything stateful.
    quick: Vec<usize>,
    message: usize,
    start: Option<&'static Expr>,
    end: Option<&'static Expr>,
    own_start: Option<Expr>,
    own_end: Option<Expr>,
    values: Vec<(&'static str, Expr)>,
}

struct Each {
    subject: Expr,
    pattern: Option<Pattern>,
    /// `pairs`: the mark of the pair scan and whether it is strict (C2-W-147).
    pairs: Option<(&'static str, bool)>,
    base: Option<Expr>,
}

struct Rule {
    id: &'static str,
    over: Over,
    types: Option<Vec<&'static str>>,
    verbatim: Vec<&'static str>,
    lets: Vec<Expr>,
    when: Option<Expr>,
    each: Option<Each>,
    range: Option<(Expr, Expr)>,
    entries: Vec<Entry>,
    /// The quick tests of `when`, by index into its top-level `all`.
    quick: Vec<usize>,
}

struct Compiled {
    rules: Vec<Rule>,
    ids: HashSet<&'static str>,
    verbatim_leaves: Vec<&'static str>,
    /// Every inline rule over `text` first requires a `[` in its text.
    text_needs_bracket: bool,
    /// The types of the records whose rules read their `node` (C2-W-140), with the depth they read.
    node_types: HashMap<&'static str, usize>,
    /// The keys those rules read.
    node_keys: HashSet<&'static str>,
}

fn names(value: &'static Value) -> Vec<&'static str> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

fn compile_rule(rule: &'static Value) -> Rule {
    let let_names: Vec<&'static str> = rule
        .get("let")
        .and_then(Value::as_object)
        .map(|lets| lets.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let scope = Scope {
        model: model(),
        lets: &let_names,
    };
    let lets = let_names
        .iter()
        .map(|name| scope.compile(&rule["let"][*name]))
        .collect();
    let over = match rule["over"].as_str() {
        Some("block") => Over::Block,
        Some("inline") => Over::Inline,
        _ => Over::Line,
    };
    let values = |object: &'static Value| -> Vec<(&'static str, &'static Value)> {
        object
            .as_object()
            .into_iter()
            .flatten()
            .map(|(key, value)| (key.as_str(), value))
            .collect()
    };
    let shared = values(&rule["values"]);
    let entries = rule["report"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|entry| {
            let mut merged = shared.clone();
            for (key, value) in values(&entry["values"]) {
                if let Some(slot) = merged.iter_mut().find(|(name, _)| *name == key) {
                    slot.1 = value;
                } else {
                    merged.push((key, value));
                }
            }
            let when = entry.get("when").map(|when| scope.compile(when));
            Entry {
                quick: when.as_ref().map(entry_quick_tests).unwrap_or_default(),
                when,
                message: entry["message"].as_u64().unwrap_or(0) as usize,
                start: None,
                end: None,
                own_start: entry.get("start").map(|start| scope.compile(start)),
                own_end: entry.get("end").map(|end| scope.compile(end)),
                values: merged
                    .into_iter()
                    .map(|(key, value)| (key, scope.compile(value)))
                    .collect(),
            }
        })
        .collect();
    let each = rule.get("each").and_then(|each| {
        let pairs = each["pairs"].as_array();
        let items = pairs.or_else(|| each["matches"].as_array())?;
        let subject = items.first()?;
        let base = match subject.get("f").and_then(Value::as_str) {
            Some("text") => Some(field_expr("start")),
            Some("body") => Some(field_expr("body_start")),
            _ => None,
        };
        Some(Each {
            subject: scope.compile(subject),
            pattern: items
                .get(1)
                .and_then(Value::as_str)
                .filter(|_| pairs.is_none())
                .and_then(|text| pattern(text, "")),
            pairs: pairs.and_then(|items| {
                let mark = items.get(1)?.as_str()?;
                Some((mark, items.get(2).and_then(Value::as_str) == Some("strict")))
            }),
            base,
        })
    });
    let when = rule.get("when").map(|when| scope.compile(when));
    Rule {
        id: rule["id"].as_str().unwrap_or(""),
        over,
        types: rule.get("types").map(names),
        verbatim: names(&rule["verbatim"]),
        lets,
        each,
        range: rule
            .get("range")
            .map(|range| (scope.compile(&range["start"]), scope.compile(&range["end"]))),
        entries,
        quick: when.as_ref().map(quick_tests).unwrap_or_default(),
        when,
    }
}

fn compiled() -> &'static Compiled {
    static COMPILED: OnceLock<Compiled> = OnceLock::new();
    COMPILED.get_or_init(|| {
        let mut rules: Vec<Rule> = model()["rules"]
            .as_array()
            .into_iter()
            .flatten()
            .map(compile_rule)
            .collect();
        // An entry without its own range reads the rule's; both live in the static rule list.
        for rule in &mut rules {
            let range: Option<&'static (Expr, Expr)> =
                rule.range.take().map(|range| &*Box::leak(Box::new(range)));
            for entry in &mut rule.entries {
                entry.start = range.map(|(start, _)| start);
                entry.end = range.map(|(_, end)| end);
            }
        }
        let text_rules: Vec<&Value> = model()["rules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|rule| {
                rule["over"] == "inline"
                    && rule["types"]
                        .as_array()
                        .is_some_and(|types| types.iter().any(|item| item == "text"))
            })
            .collect();
        let text_needs_bracket = !text_rules.is_empty()
            && text_rules.iter().all(|rule| {
                rule["when"]["contains"].as_array().is_some_and(|args| {
                    args.first().is_some_and(|value| value["f"] == "text")
                        && args.get(1).is_some_and(|value| value == "[")
                })
            });
        let mut node_types: HashMap<&'static str, usize> = HashMap::new();
        let mut node_keys = HashSet::new();
        for rule in model()["rules"].as_array().into_iter().flatten() {
            let depth = node_reads(rule, &mut node_keys);
            if depth > 0 {
                for kind in names(&rule["types"]) {
                    let known = node_types.entry(kind).or_default();
                    *known = (*known).max(depth);
                }
            }
        }
        Compiled {
            node_types,
            node_keys,
            ids: rules.iter().map(|rule| rule.id).collect(),
            rules,
            verbatim_leaves: names(&model()["verbatim_leaves"]),
            text_needs_bracket,
        }
    })
}

/// How deep a rule reads into its `node`, counting list items as a level, and the keys it reads;
/// 0 for a rule that reads no `node` (C2-W-140).
fn node_reads(value: &'static Value, keys: &mut HashSet<&'static str>) -> usize {
    let path_depth = |text: &'static str, keys: &mut HashSet<&'static str>| {
        let parts: Vec<&'static str> = text.split('.').collect();
        keys.extend(
            parts
                .iter()
                .skip(1)
                .copied()
                .filter(|part| part.parse::<usize>().is_err()),
        );
        parts.len()
    };
    match value {
        Value::String(text) if text == "node" || text.starts_with("node.") => {
            path_depth(text, keys)
        }
        Value::Object(map)
            if map
                .get("in")
                .and_then(Value::as_str)
                .is_some_and(|list| list.starts_with("node.")) =>
        {
            let list = path_depth(map["in"].as_str().unwrap_or(""), keys);
            let mut item = 0;
            item_reads(&map["where"], keys, &mut item);
            list + item
        }
        Value::Array(items) => items
            .iter()
            .map(|item| node_reads(item, keys))
            .max()
            .unwrap_or(0),
        Value::Object(map) => map
            .values()
            .map(|item| node_reads(item, keys))
            .max()
            .unwrap_or(0),
        _ => 0,
    }
}

/// The deepest `item.…` path under a `some` or `count` over a node list, the item itself counted.
fn item_reads(value: &'static Value, keys: &mut HashSet<&'static str>, depth: &mut usize) {
    match value {
        Value::String(text) if text.starts_with("item.") => {
            let parts: Vec<&'static str> = text.split('.').collect();
            keys.extend(
                parts
                    .iter()
                    .skip(1)
                    .copied()
                    .filter(|part| part.parse::<usize>().is_err()),
            );
            *depth = (*depth).max(parts.len());
        }
        Value::Array(items) => items.iter().for_each(|item| item_reads(item, keys, depth)),
        Value::Object(map) => map.values().for_each(|item| item_reads(item, keys, depth)),
        _ => {}
    }
}

/// The nodes a `node` field can read, by range, cut to the depth and keys their rules read.
pub(super) fn nodes(ranges: &[Range<'_>], values: &[V<'_>]) -> Vec<Option<Value>> {
    let compiled = compiled();
    let keep = |key: &str| compiled.node_keys.contains(key);
    ranges
        .iter()
        .zip(values)
        .map(|(range, value)| {
            compiled
                .node_types
                .get(range.kind)
                .map(|depth| value.to_value_within(*depth, &keep))
        })
        .collect()
}

pub(super) fn modeled_ids() -> &'static HashSet<&'static str> {
    &compiled().ids
}

/// Whether the model states every one of these rules and runs them, so their hand-written scan is skipped;
/// the membership is read once per call site.
macro_rules! replaced {
    ($($code:literal),+ $(,)?) => {{
        static ALL: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        $crate::lint::data_rules::data_mode()
            && *ALL.get_or_init(|| {
                [$($code),+]
                    .iter()
                    .all(|code| $crate::lint::data_rules::modeled_ids().contains(code))
            })
    }};
}

fn report(
    rule: &'static Rule,
    open: u64,
    eval: &mut Eval<'_, '_>,
    out: &mut Vec<Diagnostic>,
    path: &dyn Fn() -> String,
) {
    for (at, entry) in rule.entries.iter().enumerate() {
        if at < 64 && open & (1 << at) == 0 {
            continue;
        }
        if !entry.when.as_ref().is_none_or(|when| eval.test(when)) {
            continue;
        }
        let start = eval.offset(entry.own_start.as_ref().or(entry.start));
        let end = eval.offset(entry.own_end.as_ref().or(entry.end));
        let source = eval.records.positions();
        if start > end || end > source.text().len() {
            return;
        }
        let values: Vec<(&str, std::borrow::Cow<'_, str>)> = entry
            .values
            .iter()
            .map(|(key, expression)| (*key, eval.string(expression)))
            .collect();
        let refs: Vec<(&str, &str)> = values
            .iter()
            .map(|(key, value)| (*key, value.as_ref()))
            .collect();
        let mut diagnostic = Diagnostic::nth(
            rule.id,
            entry.message,
            &refs,
            None,
            Some(source.pos(start, end)),
        );
        diagnostic.path = Some(path());
        out.push(diagnostic);
        return;
    }
}

fn run_rule<'a>(
    (rule, open): (&'static Rule, u64),
    records: &Records<'a>,
    subject: Subject<'_, 'a>,
    line_index: usize,
    out: &mut Vec<Diagnostic>,
    path: &dyn Fn() -> String,
) {
    if let Some(when) = &rule.when
        && quick_rejects(when, &rule.quick, records, &subject)
    {
        return;
    }
    // Every open entry failing a quick test means no report, whatever the rule's own test says.
    if rule.entries.iter().enumerate().all(|(at, entry)| {
        (at < 64 && open & (1 << at) == 0)
            || entry
                .when
                .as_ref()
                .is_some_and(|when| quick_rejects(when, &entry.quick, records, &subject))
    }) {
        return;
    }
    let line = &records.lines[line_index];
    let mut eval = Eval {
        records,
        subject,
        line: line.text,
        line_start: line.start,
        lets: &rule.lets,
        vars: Vec::new(),
        item: None,
        spans: Vec::new(),
        base: 0,
    };
    if !rule.when.as_ref().is_none_or(|when| eval.test(when)) {
        return;
    }
    let Some(each) = &rule.each else {
        report(rule, open, &mut eval, out, path);
        return;
    };
    let subject = eval.value(&each.subject);
    let Val::Str(text) = subject else {
        return;
    };
    let base = eval.offset(each.base.as_ref());
    if let Some((mark, strict)) = each.pairs {
        for pair in pairs::scan(&text, mark, strict) {
            eval.spans.clear();
            eval.spans.extend(pair);
            eval.base = base;
            report(rule, open, &mut eval, out, path);
        }
        return;
    }
    let Some(pattern) = &each.pattern else {
        return;
    };
    for found in pattern.re.captures_iter(&text) {
        eval.spans.clear();
        for &(name, index) in &pattern.groups {
            if let Some(span) = found.get(index) {
                eval.spans.push((name, span.start(), span.end()));
            }
        }
        eval.base = base;
        report(rule, open, &mut eval, out, path);
    }
}

/// A multiplicative hash for the small integer keys of the line buckets.
#[derive(Default)]
struct Mix(u64);

impl Hasher for Mix {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let mut word = [0; 8];
            word.copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
        let mut tail = [0; 8];
        tail[..chunks.remainder().len()].copy_from_slice(chunks.remainder());
        self.write_u64(u64::from_le_bytes(tail) ^ (bytes.len() as u64) << 56);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.write_u64(u64::from(value));
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

/// The address of a type name: the parse hands out the same `&'static str` for a type, so this finds
/// a known bucket without comparing text; a second address for the same text only adds an alias.
fn address(text: &str) -> (usize, usize) {
    (text.as_ptr() as usize, text.len())
}

fn type_matches(rule: &Rule, kind: &str) -> bool {
    rule.types
        .as_ref()
        .is_none_or(|types| types.contains(&kind))
}

/// Runs the active rules of the model over the records of one parse, lines first, then nodes in tree order.
pub(super) fn run<'a>(
    source: &'a str,
    positions: &'a Source<'a>,
    ranges: &'a [Range<'a>],
    holders: &'a [Holder],
    nodes: &'a [Option<Value>],
    model: &'a [Line],
    options: &LintOptions,
) -> Vec<Diagnostic> {
    let active: Vec<usize> = compiled()
        .rules
        .iter()
        .enumerate()
        .filter(|(_, rule)| active_catalog(rule.id, options))
        .map(|(at, _)| at)
        .collect();
    let mut out = Vec::new();
    if active.is_empty() {
        return out;
    }
    let records = Records::new(source, positions, ranges, holders, nodes, model);
    CACHE.with(|cache| {
        // A nested lint (none today) gets buckets of its own instead of the shared ones.
        let mut own;
        let mut shared = cache.try_borrow_mut();
        let cache = if let Ok(cache) = shared.as_deref_mut() {
            cache
        } else {
            own = Cache::default();
            &mut own
        };
        if cache.active != active {
            *cache = Cache::new(active);
        }
        run_lines(&records, cache, &mut out);
        run_nodes(&records, cache, &mut out);
    });
    out
}

/// The rules of a bucket with the report entries its facts leave open (bit `i` for entry `i`; entries
/// past 64 always run), shared between the calls that meet the same facts.
type Bucket = Rc<[(&'static Rule, u64)]>;

/// The rules the facts do not rule out, each with its open report entries.
fn open_rules(
    rules: &[&'static Rule],
    facts: &Facts<'_>,
    over_line: bool,
    keep: impl Fn(&Rule) -> bool,
) -> Vec<(&'static Rule, u64)> {
    rules
        .iter()
        .copied()
        .filter(|rule| {
            keep(rule)
                && rule
                    .when
                    .as_ref()
                    .is_none_or(|when| facts.decide(when, over_line) != Some(false))
        })
        .map(|rule| {
            let mut open = u64::MAX;
            for (at, entry) in rule.entries.iter().enumerate().take(64) {
                if entry
                    .when
                    .as_ref()
                    .is_some_and(|when| facts.skips(when, over_line))
                {
                    open &= !(1 << at);
                }
            }
            (rule, open)
        })
        .filter(|(_, open)| *open != 0)
        .collect()
}
type Small<K, V> = HashMap<K, V, BuildHasherDefault<Mix>>;

/// The active rules of the last calls and their buckets, keyed by the facts that decide them; a call
/// with other active rules starts over.
#[derive(Default)]
struct Cache {
    active: Vec<usize>,
    line_rules: Vec<&'static Rule>,
    block_rules: Vec<&'static Rule>,
    inline_rules: Vec<&'static Rule>,
    literals: Vec<(bool, &'static str)>,
    lines: Small<String, Small<u64, Bucket>>,
    blocks: Small<String, Small<String, [Option<Bucket>; 8]>>,
    inlines: Small<String, Small<u16, Bucket>>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

impl Cache {
    fn new(active: Vec<usize>) -> Self {
        let rules = &compiled().rules;
        let over = |kind: Over| -> Vec<&'static Rule> {
            active
                .iter()
                .map(|at| &rules[*at])
                .filter(|rule| rule.over == kind)
                .collect()
        };
        let line_rules = over(Over::Line);
        let mut literals = Vec::new();
        for rule in &line_rules {
            if let Some(when) = &rule.when {
                held_literals(when, &mut literals);
            }
        }
        // The key keeps 31 bits for literals; a literal past them is simply not decided.
        literals.truncate(31);
        Self {
            line_rules,
            block_rules: over(Over::Block),
            inline_rules: over(Over::Inline),
            literals,
            active,
            ..Self::default()
        }
    }

    fn line(
        &mut self,
        leaf: &str,
        key: u64,
        make: impl FnOnce(&Self) -> Vec<(&'static Rule, u64)>,
    ) -> Bucket {
        if let Some(bucket) = self.lines.get(leaf).and_then(|known| known.get(&key)) {
            return bucket.clone();
        }
        let bucket: Bucket = make(self).into();
        self.lines
            .entry(leaf.to_owned())
            .or_default()
            .insert(key, bucket.clone());
        bucket
    }

    fn block(&mut self, key: BlockKey<'_>) -> Bucket {
        let (kind, parent, attributed, image_line, first_child) = key;
        let slot =
            usize::from(attributed) * 4 + usize::from(image_line) * 2 + usize::from(first_child);
        if let Some(bucket) = self
            .blocks
            .get(kind)
            .and_then(|known| known.get(parent))
            .and_then(|known| known[slot].as_ref())
        {
            return bucket.clone();
        }
        let bucket: Bucket = block_bucket(&self.block_rules, key).into();
        self.blocks
            .entry(kind.to_owned())
            .or_default()
            .entry(parent.to_owned())
            .or_default()[slot] = Some(bucket.clone());
        bucket
    }

    fn inline(&mut self, key: InlineKey<'_>) -> Bucket {
        let (kind, first) = key;
        let slot = first.map_or(256, u16::from);
        if let Some(bucket) = self.inlines.get(kind).and_then(|known| known.get(&slot)) {
            return bucket.clone();
        }
        let bucket: Bucket = inline_bucket(&self.inline_rules, key).into();
        self.inlines
            .entry(kind.to_owned())
            .or_default()
            .insert(slot, bucket.clone());
        bucket
    }
}

/// The line rules a bucket of lines needs: those its facts do not rule out.
fn line_bucket(
    rules: &[&'static Rule],
    literals: &[(bool, &'static str)],
    line: &LineRec<'_>,
    first: bool,
    prev_same: bool,
    held: u64,
) -> Vec<(&'static Rule, u64)> {
    let leaf = line.leaf_kind;
    let verbatim = compiled().verbatim_leaves.contains(&leaf);
    let holds: Vec<(bool, &'static str, bool)> = literals
        .iter()
        .enumerate()
        .map(|(bit, (on_body, needle))| (*on_body, *needle, held & (1 << bit) != 0))
        .collect();
    let facts = Facts {
        kind: leaf,
        parent: None,
        text_first: Some(line.text.as_bytes().first().copied()),
        body_first: Some(line.body().as_bytes().first().copied()),
        flags: &[(F::LeafFirst, first), (F::PrevSameLeaf, prev_same)],
        first_child: None,
        holds: &holds,
    };
    open_rules(rules, &facts, true, |rule| {
        !verbatim || rule.verbatim.contains(&leaf)
    })
}

fn run_lines(records: &Records<'_>, cache: &mut Cache, out: &mut Vec<Diagnostic>) {
    let mut leaves: Vec<&str> = Vec::new();
    let mut by_key: Small<u64, usize> = HashMap::default();
    let mut buckets: Vec<Bucket> = Vec::new();
    // Neighboring lines often share their facts: the last key is checked before the map.
    let mut last: Option<(u64, usize)> = None;
    for (index, line) in records.lines.iter().enumerate() {
        let leaf = line.leaf_kind;
        let at = leaves
            .iter()
            // A second address for the same leaf type only adds a bucket with the same rules.
            .position(|known| std::ptr::eq(*known, leaf))
            .unwrap_or_else(|| {
                leaves.push(leaf);
                leaves.len() - 1
            });
        let text = line.text.as_bytes().first().copied();
        let body = line.body().as_bytes().first().copied();
        let first = records.leaf_first(index);
        let prev_same = records.prev_same_leaf(index);
        let mut held = 0u64;
        for (bit, (on_body, needle)) in cache.literals.iter().enumerate() {
            if holds(if *on_body { line.body() } else { line.text }, needle) {
                held |= 1 << bit;
            }
        }
        // Bits: 31 literals, 9 each for the first byte of body and text, then the two flags.
        let facts = (u64::from(prev_same) << 50)
            | (u64::from(first) << 49)
            | (u64::from(text.map_or(256, u16::from)) << 40)
            | (u64::from(body.map_or(256, u16::from)) << 31)
            | held;
        let key = ((at as u64) << 51) | facts;
        let found = match last {
            Some((known, found)) if known == key => found,
            _ => *by_key.entry(key).or_insert_with(|| {
                buckets.push(cache.line(leaf, facts, |cache| {
                    line_bucket(
                        &cache.line_rules,
                        &cache.literals,
                        line,
                        first,
                        prev_same,
                        held,
                    )
                }));
                buckets.len() - 1
            }),
        };
        last = Some((key, found));
        let path = || records.lines[index].path.to_owned();
        for rule in buckets[found].iter() {
            run_rule(*rule, records, Subject::Line(index), index, out, &path);
        }
    }
}

/// What a block's bucket is keyed by: its type, its parent's type, its `attributed` and `image_line`,
/// and whether it is the first child.
type BlockKey<'a> = (&'a str, &'a str, bool, bool, bool);
/// A block key by the addresses of its type names.
type BlockAlias = ((usize, usize), (usize, usize), bool, bool, bool);
/// What an inline's bucket is keyed by: its type and the first byte of its text.
type InlineKey<'a> = (&'a str, Option<u8>);

fn block_bucket(rules: &[&'static Rule], key: BlockKey<'_>) -> Vec<(&'static Rule, u64)> {
    let (kind, parent, attributed, image_line, first_child) = key;
    let facts = Facts {
        kind,
        parent: Some(parent),
        text_first: None,
        body_first: None,
        flags: &[(F::Attributed, attributed), (F::ImageLine, image_line)],
        first_child: Some(first_child),
        holds: &[],
    };
    open_rules(rules, &facts, false, |rule| type_matches(rule, kind))
}

fn inline_bucket(rules: &[&'static Rule], key: InlineKey<'_>) -> Vec<(&'static Rule, u64)> {
    let (kind, first) = key;
    let facts = Facts {
        kind,
        parent: None,
        text_first: Some(first),
        body_first: None,
        flags: &[],
        first_child: None,
        holds: &[],
    };
    open_rules(rules, &facts, false, |rule| {
        rule.types
            .as_ref()
            .is_some_and(|types| types.contains(&kind))
    })
}

fn run_nodes<'a>(records: &Records<'a>, cache: &mut Cache, out: &mut Vec<Diagnostic>) {
    let text_needs_bracket = compiled().text_needs_bracket;
    let mut block_alias: Small<BlockAlias, usize> = HashMap::default();
    let mut inline_alias: Small<((usize, usize), u16), usize> = HashMap::default();
    let buckets: RefCell<Vec<Bucket>> = RefCell::new(Vec::new());
    let add = |bucket: Bucket| {
        let mut buckets = buckets.borrow_mut();
        buckets.push(bucket);
        buckets.len() - 1
    };
    let mut found_block = 0;
    let mut found_inline = 0;
    let wants_runs = cache
        .inline_rules
        .iter()
        .any(|rule| type_matches(rule, "text_run") && rule.types.is_some());
    let cache = RefCell::new(cache);
    for (at, range) in records.ranges.iter().enumerate() {
        let mut block = |kind: &'a str, parent: &'a str| {
            let key = (
                kind,
                parent,
                range.has_attrs,
                range.image_line,
                range.index == 0,
            );
            found_block = *block_alias
                .entry((address(kind), address(parent), key.2, key.3, key.4))
                .or_insert_with(|| add(cache.borrow_mut().block(key)));
            !buckets.borrow()[found_block].is_empty()
        };
        let mut inline = |kind: &'a str, text: &'a str| {
            let first = text.as_bytes().first().copied();
            found_inline = *inline_alias
                .entry((address(kind), first.map_or(256, u16::from)))
                .or_insert_with(|| add(cache.borrow_mut().inline((kind, first))));
            !buckets.borrow()[found_inline].is_empty()
                && (kind != "text" || !text_needs_bracket || text.contains('['))
        };
        match records.node(at, &mut block, &mut inline) {
            Node::Skip => {}
            Node::Block(record) => {
                let path = || records.path_of(record.range);
                for rule in buckets.borrow()[found_block].iter() {
                    run_rule(
                        *rule,
                        records,
                        Subject::Block(&record),
                        record.line_index,
                        out,
                        &path,
                    );
                }
            }
            Node::Inline(record) => {
                let path = || records.path_of(record.range);
                for rule in buckets.borrow()[found_inline].iter() {
                    run_rule(
                        *rule,
                        records,
                        Subject::Inline(&record),
                        record.line_index,
                        out,
                        &path,
                    );
                }
            }
        }
        if wants_runs && let Some(record) = records.text_run(at) {
            let first = record.first_byte();
            let bucket = *inline_alias
                .entry((address("text_run"), first.map_or(256, u16::from)))
                .or_insert_with(|| add(cache.borrow_mut().inline(("text_run", first))));
            let path = || records.path_of(record.range);
            for rule in buckets.borrow()[bucket].iter() {
                run_rule(
                    *rule,
                    records,
                    Subject::Inline(&record),
                    record.line_index,
                    out,
                    &path,
                );
            }
        }
    }
}
