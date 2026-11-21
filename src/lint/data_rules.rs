//! Rule selection and reporting from the pinned 0.2 lint model, compiled once per process.
#![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

use super::data_interpreter::{
    Eval, Expr, F, Facts, Pattern, Scope, Val, field_expr, held_literals, pattern, truth,
};
use super::data_model::{LineRec, Node, Records, Subject};
use super::{Holder, LintOptions, Range, active_catalog, lines::Line};
use crate::diagnostic::Diagnostic;
use crate::source::Source;
use serde_json::Value;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};
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
}

struct Compiled {
    rules: Vec<Rule>,
    ids: HashSet<&'static str>,
    verbatim_leaves: Vec<&'static str>,
    /// Every inline rule over `text` first requires a `[` in its text.
    text_needs_bracket: bool,
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
            Entry {
                when: entry.get("when").map(|when| scope.compile(when)),
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
        let items = each["matches"].as_array()?;
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
                .and_then(|text| pattern(text, "")),
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
        when,
        each,
        range: rule
            .get("range")
            .map(|range| (scope.compile(&range["start"]), scope.compile(&range["end"]))),
        entries,
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
        Compiled {
            ids: rules.iter().map(|rule| rule.id).collect(),
            rules,
            verbatim_leaves: names(&model()["verbatim_leaves"]),
            text_needs_bracket,
        }
    })
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
    eval: &mut Eval<'_, '_>,
    out: &mut Vec<Diagnostic>,
    path: &dyn Fn() -> String,
) {
    for entry in &rule.entries {
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
    rule: &'static Rule,
    records: &Records<'a>,
    subject: Subject<'_, 'a>,
    line_index: usize,
    out: &mut Vec<Diagnostic>,
    path: &dyn Fn() -> String,
) {
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
        report(rule, &mut eval, out, path);
        return;
    };
    let subject = eval.value(&each.subject);
    let Val::Str(text) = subject else {
        return;
    };
    let Some(pattern) = &each.pattern else {
        return;
    };
    let base = eval.offset(each.base.as_ref());
    for found in pattern.re.captures_iter(&text) {
        eval.spans = pattern
            .groups
            .iter()
            .filter_map(|&name| {
                found
                    .name(name)
                    .map(|span| (name, span.start(), span.end()))
            })
            .collect();
        eval.base = base;
        report(rule, &mut eval, out, path);
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
        for byte in bytes {
            self.write_u64(u64::from(*byte));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
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
    ranges: &'a [Range],
    holders: &'a [Holder],
    model: &'a [Line],
    options: &LintOptions,
) -> Vec<Diagnostic> {
    let active: Vec<&'static Rule> = compiled()
        .rules
        .iter()
        .filter(|rule| active_catalog(rule.id, options))
        .collect();
    let mut out = Vec::new();
    if active.is_empty() {
        return out;
    }
    let records = Records::new(source, positions, ranges, holders, model);
    let over = |kind: Over| -> Vec<&'static Rule> {
        active
            .iter()
            .copied()
            .filter(|rule| rule.over == kind)
            .collect()
    };
    run_lines(&records, &over(Over::Line), &mut out);
    run_nodes(&records, &over(Over::Block), &over(Over::Inline), &mut out);
    out
}

/// The line rules a bucket of lines needs: those its facts do not rule out.
fn line_bucket(
    rules: &[&'static Rule],
    literals: &[(bool, &'static str)],
    line: &LineRec<'_>,
    first: bool,
    held: u64,
) -> Vec<&'static Rule> {
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
        flags: &[(F::LeafFirst, first)],
        holds: &holds,
    };
    rules
        .iter()
        .copied()
        .filter(|rule| {
            (!verbatim || rule.verbatim.contains(&leaf))
                && rule
                    .when
                    .as_ref()
                    .is_none_or(|when| facts.decide(when, true) != Some(false))
        })
        .collect()
}

fn run_lines(records: &Records<'_>, rules: &[&'static Rule], out: &mut Vec<Diagnostic>) {
    let mut literals = Vec::new();
    for rule in rules {
        if let Some(when) = &rule.when {
            held_literals(when, &mut literals);
        }
    }
    // The key keeps 31 bits for literals; a literal past them is simply not decided.
    literals.truncate(31);
    let mut leaves: Vec<&str> = Vec::new();
    let mut by_key: HashMap<u64, Vec<&'static Rule>, BuildHasherDefault<Mix>> = HashMap::default();
    for (index, line) in records.lines.iter().enumerate() {
        let leaf = line.leaf_kind;
        let at = leaves
            .iter()
            .position(|known| *known == leaf)
            .unwrap_or_else(|| {
                leaves.push(leaf);
                leaves.len() - 1
            });
        let text = line.text.as_bytes().first().copied();
        let body = line.body().as_bytes().first().copied();
        let first = truth(&records.subject_field(&Subject::Line(index), F::LeafFirst));
        let mut held = 0u64;
        for (bit, (on_body, needle)) in literals.iter().enumerate() {
            if (if *on_body { line.body() } else { line.text }).contains(needle) {
                held |= 1 << bit;
            }
        }
        let key = ((at as u64) << 50)
            | (u64::from(first) << 49)
            | (u64::from(text.map_or(256, u16::from)) << 40)
            | (u64::from(body.map_or(256, u16::from)) << 31)
            | held;
        let bucket = by_key
            .entry(key)
            .or_insert_with(|| line_bucket(rules, &literals, line, first, held));
        let path = || records.lines[index].path.to_owned();
        for rule in bucket.iter() {
            run_rule(rule, records, Subject::Line(index), index, out, &path);
        }
    }
}

/// What a block's bucket is keyed by: its type, its parent's type, and its `attributed` and `image_line`.
type BlockKey<'a> = (&'a str, &'a str, bool, bool);
/// What an inline's bucket is keyed by: its type and the first byte of its text.
type InlineKey<'a> = (&'a str, Option<u8>);

fn block_bucket(rules: &[&'static Rule], key: BlockKey<'_>) -> Vec<&'static Rule> {
    let (kind, parent, attributed, image_line) = key;
    let facts = Facts {
        kind,
        parent: Some(parent),
        text_first: None,
        body_first: None,
        flags: &[(F::Attributed, attributed), (F::ImageLine, image_line)],
        holds: &[],
    };
    rules
        .iter()
        .copied()
        .filter(|rule| {
            type_matches(rule, kind)
                && rule
                    .when
                    .as_ref()
                    .is_none_or(|when| facts.decide(when, false) != Some(false))
        })
        .collect()
}

fn inline_bucket(rules: &[&'static Rule], key: InlineKey<'_>) -> Vec<&'static Rule> {
    let (kind, first) = key;
    let facts = Facts {
        kind,
        parent: None,
        text_first: Some(first),
        body_first: None,
        flags: &[],
        holds: &[],
    };
    rules
        .iter()
        .copied()
        .filter(|rule| {
            rule.types
                .as_ref()
                .is_some_and(|types| types.contains(&kind))
                && rule
                    .when
                    .as_ref()
                    .is_none_or(|when| facts.decide(when, false) != Some(false))
        })
        .collect()
}

fn run_nodes<'a>(
    records: &Records<'a>,
    block_rules: &[&'static Rule],
    inline_rules: &[&'static Rule],
    out: &mut Vec<Diagnostic>,
) {
    let text_needs_bracket = compiled().text_needs_bracket;
    let mut by_block: Vec<(BlockKey<'a>, Vec<&'static Rule>)> = Vec::new();
    let mut by_inline: Vec<(InlineKey<'a>, Vec<&'static Rule>)> = Vec::new();
    let mut found_block = 0;
    let mut found_inline = 0;
    for (at, range) in records.ranges.iter().enumerate() {
        let mut block = |kind: &'a str, parent: &'a str| {
            let key = (kind, parent, range.has_attrs, range.image_line);
            found_block = by_block
                .iter()
                .position(|(known, _)| *known == key)
                .unwrap_or_else(|| {
                    by_block.push((key, block_bucket(block_rules, key)));
                    by_block.len() - 1
                });
            !by_block[found_block].1.is_empty()
        };
        let mut inline = |kind: &'a str, text: &'a str| {
            let key = (kind, text.as_bytes().first().copied());
            found_inline = by_inline
                .iter()
                .position(|(known, _)| *known == key)
                .unwrap_or_else(|| {
                    by_inline.push((key, inline_bucket(inline_rules, key)));
                    by_inline.len() - 1
                });
            !by_inline[found_inline].1.is_empty()
                && (kind != "text" || !text_needs_bracket || text.contains('['))
        };
        match records.node(at, &mut block, &mut inline) {
            Node::Skip => {}
            Node::Block(record) => {
                let path = || records.path_of(record.range);
                for rule in &by_block[found_block].1 {
                    run_rule(
                        rule,
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
                for rule in &by_inline[found_inline].1 {
                    run_rule(
                        rule,
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
}
