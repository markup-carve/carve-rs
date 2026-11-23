//! The escaping test of the writer (C2-W-088 to C2-W-091): the written blocks
//! are parsed again and compared with the tree they were written from. Where
//! the parse reads a construct that the tree does not hold, the opener of that
//! construct takes a backslash; where a construct of the tree did not form, it
//! takes its other spelling (C2-W-005). What neither repairs is a loss.

use super::inline::{
    IN_BRACKET, IN_CELL, IN_EMPHASIS, IN_STRONG, first_from, spelled_end, write_inlines,
};
use super::kinds::KindBlocks;
use super::node::{
    Fields, N, caption, children, has, id, is_break, is_inline_parent, result_fields, s,
    skipped_in, span, text, title, ty,
};
use super::state::{Attempt, PLACE_COLUMNS, ProbeResult, Spelling, Writer};
use crate::hash::{AddrMap, AddrSet};
use crate::view::{Arr, V};
use serde_json::Value;
use std::collections::HashSet;

fn refs(nodes: Arr<'_>) -> Vec<V<'_>> {
    nodes.iter().collect()
}

/// Written text for the read-back check and the arena its parse lives in: the
/// tree is read through a view and dropped with the arena, never built as JSON.
pub struct Chunk {
    text: String,
    arena: crate::Arena,
}

impl Chunk {
    pub fn new(text: String) -> Self {
        let arena = crate::Arena::for_input(&text);
        Self { text, arena }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The tree with the definitions of the document `w` writes.
    pub fn parse<'c>(&'c self, w: &mut Writer<'_>) -> V<'c> {
        let keys = std::sync::Arc::clone(&w.link_keys);
        self.parse_keys(w, Some(&keys))
    }

    /// The pre-resolve tree with the link keys `keys` and the abbreviations of
    /// the document in scope. Each call counts toward the repair limit.
    pub fn parse_keys<'c>(&'c self, w: &mut Writer<'_>, keys: Option<&HashSet<String>>) -> V<'c> {
        w.parses += 1;
        w.work_chars = w.work_chars.saturating_add(self.text.len());
        self.read(w, keys)
    }

    /// The tree as [`Self::parse_keys`] reads it, counted toward no limit.
    pub fn read<'c>(&'c self, w: &Writer<'_>, keys: Option<&HashSet<String>>) -> V<'c> {
        #[cfg(test)]
        super::READ_CHARS.with(|count| count.set(count.get() + self.text.len()));
        let source = crate::source::Source::new(&self.text);
        let out = crate::block::scan(&source);
        let (doc, _) =
            crate::tree::build_layered(&source, &self.arena, out.events, keys, &w.abbreviations);
        if w.extensions.is_empty() {
            return V::Doc(self.arena.alloc(doc));
        }
        let mut v = serde_json::to_value(&doc).unwrap_or_default();
        if let Some(m) = v.as_object_mut() {
            m.insert("type".into(), "document".into());
        }
        let outer = keys.map(|k| {
            if std::ptr::eq(k, std::sync::Arc::as_ptr(&w.link_keys)) {
                std::sync::Arc::clone(&w.link_keys)
            } else {
                std::sync::Arc::new(k.clone())
            }
        });
        super::extension::apply_claims(&mut v, &w.extensions, &self.text, outer.as_ref());
        V::Json(self.arena.json(v))
    }

    /// The tree of the whole written document, for the check of C2-W-144; it is
    /// no repair parse of a group, so it counts toward no limit.
    pub fn read_written<'c>(&'c self, w: &Writer<'_>) -> V<'c> {
        self.read(w, Some(&w.link_keys))
    }
}

/// C2-W-133: whether the repairs of the document have parsed more than its budget.
pub fn exhausted(w: &Writer<'_>) -> bool {
    w.work_chars > w.work_limit
}

/// What a construct of the parse was, as far as the chains of one round read it.
#[derive(Debug, Clone)]
pub struct Built {
    kind: String,
    start: Option<usize>,
    holds_closer: bool,
}

/// One comparison of written blocks with their parse: what it asks the next write to change.
#[derive(Default)]
pub struct Check<'t> {
    pub escapes: Vec<(N<'t>, usize)>,
    pub retracted: Vec<(N<'t>, usize)>,
    pub trials: Vec<(N<'t>, usize)>,
    pub marks: AddrMap<usize, AddrMap<usize, bool>>,
    pub forms: Vec<N<'t>>,
    pub respelled: AddrSet<usize>,
    pub misaligned: bool,
    pub losses: Vec<(N<'t>, Option<String>)>,
    pub flags: u32,
    pub span_marks: bool,
    pub owner: Option<N<'t>>,
    pub chains: Vec<(N<'t>, usize, Built)>,
    pub source: Option<String>,
    /// C2-W-088: the escapes that make a line opener text, by node and offset.
    pub line_escapes: HashSet<(usize, usize)>,
    /// Escapes of a closer that ended a node early (C2-W-125 case five): a
    /// probe round gives them no kind (C2-W-088, CC-1568).
    pub closers: HashSet<(usize, usize)>,
}

impl Check<'_> {
    pub fn changes(&self) -> usize {
        self.escapes.len() + self.forms.len() + self.retracted.len()
    }
}

fn same_value(a: V<'_>, b: Option<V<'_>>) -> bool {
    let Some(b) = b else { return false };
    match (a.as_array(), b.as_array()) {
        (Some(x), Some(y)) => {
            return x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_value(p, Some(q)));
        }
        (None, None) => {}
        _ => return false,
    }
    match (a.is_object(), b.is_object()) {
        (true, true) => same_fields(a, b),
        (false, false) => super::node::equal_scalars(a, b),
        _ => false,
    }
}

fn same_fields(l: V<'_>, r: V<'_>) -> bool {
    let (lf, rf) = (result_fields(l), result_fields(r));
    let mut same = true;
    l.each(&mut |k, v| {
        same = same && (skipped_in(l, lf, k, false) || same_value(v, r.get(k)));
    });
    r.each(&mut |k, _| {
        same = same && (skipped_in(r, rf, k, false) || l.get(k).is_some());
    });
    same
}

fn is_compared(c: &Check<'_>, node: V<'_>, fields: Option<&Fields>, name: &str) -> bool {
    if matches!(name, "children" | "caption") {
        return false;
    }
    if name == "data"
        && node.get("data").filter(|d| d.is_object()).is_some_and(|d| {
            let mut standard = true;
            d.each(&mut |k, _| {
                standard =
                    standard && matches!(k, "carve.source" | "carve.style" | "carve.annotation");
            });
            standard
        })
    {
        return false;
    }
    if name == "title" {
        return title(node).is_none();
    }
    !node.is_object() || !skipped_in(node, fields, name, c.span_marks)
}

/// C2-W-136: the `attrs` of `e` without the keys the writer reported and did not write.
fn written_attrs(w: &Writer<'_>, e: V<'_>) -> Option<Value> {
    let a = e.get("attrs")?;
    let Some(keys) = w.sp.lost_keys.get(&id(e)) else {
        return Some(a.to_value());
    };
    let mut rest = a.to_value().as_object()?.clone();
    let kept: Vec<Value> = super::node::key_values(e)
        .into_iter()
        .filter(|(k, _)| !keys.iter().any(|x| x == k))
        .map(|(k, v)| serde_json::json!([k, v]))
        .collect();
    let empty = kept.is_empty();
    if empty {
        rest.remove("keyValues");
    } else {
        rest.insert("keyValues".into(), Value::Array(kept));
    }
    let no_classes = super::node::classes(e).is_empty();
    if rest.get("id").is_none() && no_classes && empty {
        return None;
    }
    Some(Value::Object(rest))
}

/// Whether the field `name` of `e` reads back as `other`, the same field of the parse.
fn field_differs(w: &Writer<'_>, e: V<'_>, name: &str, field: V<'_>, other: Option<V<'_>>) -> bool {
    if name == "attrs" && w.sp.lost_keys.contains_key(&id(e)) {
        return written_attrs(w, e).is_some_and(|v| !same_value(V::Json(&v), other));
    }
    !same_value(field, other)
}

/// [`differing_field`] of two typed nodes, their fields zipped; `None` for two node types.
fn differing_node_field(
    w: &Writer<'_>,
    c: &Check<'_>,
    e: V<'_>,
    a: V<'_>,
) -> Option<Option<&'static str>> {
    let (V::Node(x), V::Node(y)) = (e, a) else {
        return None;
    };
    // Two nodes of one type share their result fields.
    let fields = result_fields(e);
    let (mut first, mut extra) = (None, None);
    let zipped = x.view_zip(y, &mut |name, field, other| match field {
        Some(field) => {
            if first.is_none()
                && is_compared(c, e, fields, name)
                && field_differs(w, e, name, field, other)
            {
                first = Some(name);
            }
        }
        None => {
            if extra.is_none() && is_compared(c, a, fields, name) {
                extra = Some(name);
            }
        }
    });
    zipped.then(|| first.or(extra))
}

/// The name of the first field in which `e` and `a` differ.
fn differing_field(w: &Writer<'_>, c: &Check<'_>, e: V<'_>, a: V<'_>) -> Option<String> {
    if !(e.is_object() && a.is_object()) {
        return None;
    }
    if let Some(name) = differing_node_field(w, c, e, a) {
        return name.map(str::to_string);
    }
    let (ef, af) = (result_fields(e), result_fields(a));
    let mut first = None;
    e.each(&mut |name, field| {
        if first.is_some() || !is_compared(c, e, ef, name) {
            return;
        }
        if field_differs(w, e, name, field, a.get(name)) {
            first = Some(name);
        }
    });
    if let Some(name) = first {
        return Some(name.to_string());
    }
    let mut extra = None;
    a.each(&mut |name, _| {
        if extra.is_none() && e.get(name).is_none() && is_compared(c, a, af, name) {
            extra = Some(name.to_string());
        }
    });
    extra
}

fn change_form<'t>(c: &mut Check<'t>, node: N<'t>) {
    c.forms.push(node);
    c.respelled.insert(id(node));
}

fn mark(c: &mut Check<'_>, node: V<'_>, at: usize, escaped: bool) {
    c.marks.entry(id(node)).or_default().insert(at, escaped);
}

fn is_escaped(w: &Writer<'_>, c: &Check<'_>, node: V<'_>, at: usize) -> bool {
    if let Some(&asked) = c.marks.get(&id(node)).and_then(|m| m.get(&at)) {
        return asked;
    }
    let written = w.escapes_of(node);
    written.get(first_from(written, at)) == Some(&at)
}

/// Asks for a backslash before the byte `at` of the text node. False where that character takes none.
pub fn escape_at<'t>(w: &Writer<'t>, c: &mut Check<'t>, node: N<'t>, at: usize) -> bool {
    let b = text(node).as_bytes();
    if !b.get(at).is_some_and(u8::is_ascii_punctuation) || is_escaped(w, c, node, at) {
        return false;
    }
    c.escapes.push((node, at));
    mark(c, node, at, true);
    true
}

fn escape_at_opt<'t>(w: &Writer<'t>, c: &mut Check<'t>, node: N<'t>, at: Option<usize>) -> bool {
    at.is_some_and(|at| escape_at(w, c, node, at))
}

/// The offsets at which a line that reads as a block opener can be made text, best first (C2-W-092).
fn opener_offsets(value: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    if value.starts_with("--") {
        offsets.push(1);
    }
    offsets.push(0);
    let b = value.as_bytes();
    let end = b.iter().take_while(|c| c.is_ascii_alphanumeric()).count();
    if end > 0 && matches!(b.get(end), Some(b'.' | b')')) {
        offsets.push(end);
    }
    offsets
}

/// The line that starts at `nodes[from]` opened a block: escapes its opener (C2-W-090, C2-W-092).
fn escape_line_start<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    nodes: &[N<'t>],
    from: usize,
    owner: N<'t>,
) {
    c.misaligned = true;
    let first = nodes.get(from).copied();
    let before = c.escapes.len();
    if escape_run_opener(w, c, nodes, from) {
        for (n, at) in c.escapes.get(before..).unwrap_or(&[]) {
            c.line_escapes.insert((id(*n), *at));
        }
        return;
    }
    let mut at = from + 1;
    while let Some(&n) = nodes.get(at) {
        if is_break(Some(n)) {
            break;
        }
        if ty(n) == "text" && escape_at(w, c, n, 0) {
            return;
        }
        at += 1;
    }
    match first {
        Some(f) if has_other_form(w, c, f) => change_form(c, f),
        _ => c.losses.push((first.unwrap_or(owner), None)),
    }
}

fn has_other_form(w: &Writer<'_>, c: &Check<'_>, node: V<'_>) -> bool {
    if w.form(node) > 0 || c.respelled.contains(&id(node)) {
        return false;
    }
    if matches!(ty(node), "emphasis" | "strong" | "comment") {
        return true;
    }
    has(node, "key") && w.fact(node).is_some_and(|f| f.raw_ref.is_some())
}

/// The last text node below `node`, or `node` itself where it is one.
fn last_text(node: Option<V<'_>>) -> Option<V<'_>> {
    let mut at = node?;
    loop {
        if ty(at) == "text" {
            return Some(at);
        }
        at = children(at).last()?;
    }
}

fn ends_line_with_cr(value: &str, at: usize) -> bool {
    let b = value.as_bytes();
    let mut k = at;
    while matches!(b.get(k), Some(b' ' | b'\t')) {
        k += 1;
    }
    b.get(k) == Some(&b'\r')
}

const JOINING: &[u8] = b"\\!^$`";

fn closing_mark(t: &str) -> Option<u8> {
    Some(match t {
        "emphasis" => b'/',
        "strong" => b'*',
        "underline" => b'_',
        "strike" => b'~',
        "highlight" => b'=',
        "superscript" => b'^',
        "subscript" => b',',
        "insert" => b'+',
        "delete" => b'-',
        "span" | "link" | "inline_footnote" | "inline_extension" => b']',
        _ => return None,
    })
}

fn escape_end<'t>(w: &Writer<'t>, c: &mut Check<'t>, t: Option<N<'t>>, braces: bool) -> bool {
    let Some(t) = t else { return false };
    let value = text(t);
    if let Some(&last) = value.as_bytes().last() {
        if JOINING.contains(&last) && escape_at(w, c, t, value.len() - 1) {
            return true;
        }
    }
    braces && escape_at_opt(w, c, t, value.rfind('{'))
}

fn escape_in_content<'t>(w: &Writer<'t>, c: &mut Check<'t>, e: N<'t>) -> bool {
    let Some(m) = closing_mark(ty(e)) else {
        return false;
    };
    if !has(e, "children") {
        return false;
    }
    let kids = refs(children(e));
    escape_closer(w, c, &kids, 0, 0, m) || (m == b']' && escape_mark(w, c, &kids, 0, 0, b'['))
}

fn joins_next(es: &[N<'_>], i: usize) -> bool {
    let value = i
        .checked_sub(1)
        .and_then(|p| es.get(p))
        .and_then(|n| last_text(Some(*n)))
        .map_or("", text);
    value.as_bytes().last().is_some_and(|b| JOINING.contains(b))
}

fn escape_owner_mark<'t>(w: &Writer<'t>, c: &mut Check<'t>, e: N<'t>) -> bool {
    let Some(m) = c.owner.and_then(|o| closing_mark(ty(o))) else {
        return false;
    };
    if !has(e, "children") {
        return false;
    }
    let kids = refs(children(e));
    escape_closer(w, c, &kids, 0, 0, m) || (m == b']' && escape_mark(w, c, &kids, 0, 0, b'['))
}

fn escape_before<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, es: &[N<'t>], i: usize) -> bool {
    let before = i
        .checked_sub(1)
        .and_then(|p| es.get(p))
        .and_then(|n| last_text(Some(*n)));
    let Some(&e) = es.get(i) else { return false };
    escape_end(w, c, before, true) || escape_name_sigil(w, c, before, e)
}

fn is_extension_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn escape_name_sigil<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    t: Option<N<'t>>,
    e: N<'t>,
) -> bool {
    let Some(t) = t else { return false };
    let flags = c.flags;
    if !write_inlines(w, &[e], flags).starts_with('[') {
        return false;
    }
    let b = text(t).as_bytes();
    let mut start = b.len();
    while start > 0 && b.get(start - 1).is_some_and(|&x| is_extension_name_char(x)) {
        start -= 1;
    }
    while start < b.len()
        && b.get(start)
            .is_some_and(|&x| x == b'-' || x.is_ascii_digit())
    {
        start += 1;
    }
    start < b.len() && start > 0 && b.get(start - 1) == Some(&b':') && escape_at(w, c, t, start - 1)
}

fn escape_content_start<'t>(w: &Writer<'t>, c: &mut Check<'t>, e: N<'t>) -> bool {
    let Some(m) = closing_mark(ty(e)) else {
        return false;
    };
    let Some(first) = children(e).first() else {
        return false;
    };
    m != b']'
        && ty(first) == "text"
        && text(first).as_bytes().first() == Some(&m)
        && escape_at(w, c, first, 0)
}

fn escape_footnote_brace<'t>(w: &Writer<'t>, c: &mut Check<'t>, e: N<'t>) -> bool {
    if ty(e) != "inline_footnote" {
        return false;
    }
    let kids = children(e);
    let (Some(first), Some(last)) = (kids.first(), kids.last()) else {
        return false;
    };
    if ty(first) != "text" || ty(last) != "comment" || s(last, "form") != Some("line") {
        return false;
    }
    if !kids.iter().any(|n| matches!(ty(n), "strong" | "emphasis")) {
        return false;
    }
    let b = text(first).as_bytes();
    let brace = b
        .windows(2)
        .position(|p| p.first() == Some(&b'{') && matches!(p.get(1), Some(b'*' | b'/')));
    escape_at_opt(w, c, first, brace)
}

fn text_at<'t>(nodes: &[N<'t>], at: usize) -> Option<N<'t>> {
    nodes.get(at).copied().filter(|n| ty(n) == "text")
}

fn escape_after<'t>(w: &Writer<'t>, c: &mut Check<'t>, es: &[N<'t>], i: usize) -> bool {
    text_at(es, i + 1).is_some_and(|n| escape_at(w, c, n, 0))
}

fn escape_inside<'t>(w: &Writer<'t>, c: &mut Check<'t>, es: &[N<'t>], i: usize) -> bool {
    let t = es
        .get(i)
        .copied()
        .filter(|e| has(e, "children"))
        .and_then(|e| last_text(Some(e)));
    escape_end(w, c, t, true)
}

fn escape_closer_before_brace<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    es: &[N<'t>],
    i: usize,
) -> bool {
    let (Some(&e), Some(&next)) = (es.get(i), es.get(i + 1)) else {
        return false;
    };
    if ty(next) != "text" || !text(next).starts_with('}') {
        return false;
    }
    if !last_text(Some(e)).is_some_and(|t| text(t).contains('{')) {
        return false;
    }
    escape_in_content(w, c, e)
}

const RESPELL_STEPS: usize = 15;

fn respell_step<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    es: &[N<'t>],
    i: usize,
    step: usize,
    attempt: &mut Attempt<'t>,
) -> bool {
    let Some(&e) = es.get(i) else { return false };
    match step {
        0 | 11 => escape_before(w, c, es, i),
        1 => escape_content_start(w, c, e),
        2 => escape_footnote_brace(w, c, e),
        3 => escape_content_construct(w, c, e),
        4 => escape_across_closer(w, c, es, i),
        5 | 12 => escape_inside(w, c, es, i),
        6 => escape_closer_before_brace(w, c, es, i),
        7 => escape_after(w, c, es, i),
        8 => escape_content_line(w, c, e),
        _ => late_respell_step(w, c, e, step, attempt),
    }
}

fn late_respell_step<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    step: usize,
    attempt: &mut Attempt<'t>,
) -> bool {
    match step {
        9 => escape_owner_mark(w, c, e),
        10 => {
            if !has_other_form(w, c, e) {
                return false;
            }
            for (n, at) in attempt.escapes.drain(..) {
                c.retracted.push((n, at));
                mark(c, n, at, false);
            }
            change_form(c, e);
            true
        }
        13 => escape_in_content(w, c, e),
        14 => escape_lone_bracket(w, c, e),
        _ => false,
    }
}

/// The node `es[i]` did not read back: the ways to make it form are tried one per round.
fn respell<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, es: &[N<'t>], i: usize) -> bool {
    let Some(&e) = es.get(i) else { return false };
    if w.is_lost(e) {
        return false;
    }
    let mut attempt = w.sp.attempts.remove(&id(e)).unwrap_or_default();
    let before = c.escapes.len();
    while attempt.step < RESPELL_STEPS {
        let step = attempt.step;
        attempt.step += 1;
        if !respell_step(w, c, es, i, step, &mut attempt) {
            continue;
        }
        for &escape in c.escapes.get(before..).unwrap_or(&[]) {
            attempt.escapes.push(escape);
            if step != 14 {
                c.trials.push(escape);
            }
        }
        w.sp.attempts.insert(id(e), attempt);
        return true;
    }
    w.sp.attempts.insert(id(e), attempt);
    c.losses.push((e, None));
    false
}

fn texts_below(node: V<'_>) -> Vec<V<'_>> {
    let mut out = Vec::new();
    for c in children(node) {
        if ty(c) == "text" {
            out.push(c);
        } else {
            out.extend(texts_below(c));
        }
    }
    out
}

fn escape_lone_bracket<'t>(w: &Writer<'t>, c: &mut Check<'t>, e: N<'t>) -> bool {
    if !matches!(ty(e), "span" | "inline_footnote" | "inline_extension") || !has(e, "children") {
        return false;
    }
    !lone_brackets(w, c, e).is_empty() || escape_mark(w, c, &refs(children(e)), 0, 0, b'[')
}

/// C2-W-125 case five: escapes each bracket of the content of `e` without a partner there, the `[` that
/// reads the node on and the `]` that ends it early. Returns the escapes, each with whether it stands
/// inside a mark of the content.
fn lone_brackets<'t>(w: &Writer<'t>, c: &mut Check<'t>, e: N<'t>) -> Vec<(N<'t>, usize, bool)> {
    let mut lone = scan_lone_brackets(w, c, e);
    lone.retain(|&(t, at, _)| escape_at(w, c, t, at));
    lone
}

fn has_lone_bracket(w: &Writer<'_>, c: &Check<'_>, e: V<'_>) -> bool {
    !scan_lone_brackets(w, c, e).is_empty()
}

fn scan_lone_brackets<'t>(w: &Writer<'_>, c: &Check<'_>, e: N<'t>) -> Vec<(N<'t>, usize, bool)> {
    let mut open: Vec<(N<'t>, usize, bool)> = Vec::new();
    let mut lone: Vec<(N<'t>, usize, bool)> = Vec::new();
    let mut content: Vec<(N<'t>, bool)> = children(e).iter().rev().map(|n| (n, false)).collect();
    while let Some((n, nested)) = content.pop() {
        if ty(n) == "text" {
            for (at, &b) in text(n).as_bytes().iter().enumerate() {
                if (b != b'[' && b != b']') || is_escaped(w, c, n, at) {
                    continue;
                }
                if b == b'[' {
                    open.push((n, at, nested));
                } else if open.pop().is_none() {
                    lone.push((n, at, nested));
                }
            }
        } else if closing_mark(ty(n)) != Some(b']') {
            content.extend(children(n).iter().rev().map(|k| (k, true)));
        }
    }
    lone.extend(open);
    lone
}

fn content_flags(c: &Check<'_>, e: V<'_>) -> u32 {
    c.flags
        | match ty(e) {
            "emphasis" => IN_EMPHASIS,
            "strong" => IN_STRONG,
            "link" => IN_BRACKET,
            _ => 0,
        }
}

const MAX_PROBE_ROUNDS: usize = 32;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Site {
    Content,
    Across,
}

/// What a probe takes over into the comparison of its round.
enum Takes<'t> {
    All,
    Across {
        closer: usize,
        word: usize,
        own: AddrSet<usize>,
        mark: Option<u8>,
    },
    #[allow(dead_code)]
    Never(std::marker::PhantomData<&'t ()>),
}

impl Takes<'_> {
    fn takes(&self, node: V<'_>, at: usize) -> bool {
        match self {
            Self::All => true,
            Self::Across {
                closer,
                word,
                own,
                mark,
            } => {
                id(node) != *closer
                    && id(node) != *word
                    && !(own.contains(&id(node))
                        && mark.is_some_and(|m| text(node).as_bytes().get(at) == Some(&m)))
            }
            Self::Never(_) => false,
        }
    }
}

fn node_state(w: &Writer<'_>, c: &Check<'_>, node: V<'_>) -> Option<String> {
    let escapes = (ty(node) == "text")
        .then(|| w.sp.escapes.get(&id(node)))
        .flatten();
    let marks = (ty(node) == "text")
        .then(|| c.marks.get(&id(node)))
        .flatten();
    let form = w.forms.get(&id(node));
    let keys = w.sp.lost_keys.get(&id(node));
    let lost = w.is_lost(node);
    if escapes.is_none() && marks.is_none() && form.is_none() && keys.is_none() && !lost {
        return None;
    }
    let mut marks: Vec<(usize, bool)> = marks
        .map(|m| m.iter().map(|(a, b)| (*a, *b)).collect())
        .unwrap_or_default();
    marks.sort_unstable();
    Some(format!("{escapes:?}{marks:?}{form:?}{keys:?}{lost}"))
}

fn probe_key<'t>(
    w: &mut Writer<'t>,
    c: &Check<'t>,
    e: N<'t>,
    expected: &[N<'t>],
    flags: u32,
    site: Site,
) -> String {
    let mut key = format!(
        "{}{}|{flags}|{}|{:?}",
        u8::from(site == Site::Across),
        u8::from(w.kind_escapes),
        w.node_id(e),
        node_state(w, c, e)
    );
    for n in expected {
        let nid = w.node_id(n);
        key.push(',');
        key.push_str(&nid.to_string());
    }
    let mut pending: Vec<V<'_>> = expected.to_vec();
    while let Some(n) = pending.pop() {
        pending.extend(children(n));
        if let Some(state) = node_state(w, c, n) {
            let nid = w.node_id(n);
            key.push(';');
            key.push_str(&nid.to_string());
            key.push(':');
            key.push_str(&state);
        }
    }
    key
}

fn probe_escapes<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    expected: &[N<'t>],
    flags: u32,
    site: Site,
    takes: &Takes<'t>,
) -> Option<bool> {
    let key = probe_key(w, c, e, expected, flags, site);
    if let Some(known) = w.probes.get(&key).cloned() {
        for (n, at) in known.taken {
            escape_at(w, c, n, at);
        }
        return known.found;
    }
    let before = c.escapes.len();
    let found = run_probe(w, c, e, expected, flags, takes);
    if !exhausted(w) {
        let asked = c.escapes.get(before..).unwrap_or(&[]).to_vec();
        w.probes.insert(
            key,
            ProbeResult {
                found,
                taken: asked,
            },
        );
    }
    found
}

fn probe_spelling<'t>(w: &Writer<'t>, expected: &[N<'t>]) -> Spelling<'t> {
    let mut escapes = AddrMap::default();
    for n in expected {
        let texts = if ty(n) == "text" {
            vec![*n]
        } else {
            texts_below(*n)
        };
        for t in texts {
            if let Some(own) = w.sp.escapes.get(&id(t)) {
                escapes.insert(id(t), own.clone());
            }
        }
    }
    Spelling {
        escapes,
        attempts: AddrMap::default(),
        losses: Vec::new(),
        lost: w.sp.lost.clone(),
        lost_keys: w.sp.lost_keys.clone(),
    }
}

fn run_probe<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    expected: &[N<'t>],
    flags: u32,
    takes: &Takes<'t>,
) -> Option<bool> {
    let mut probe = probe_spelling(w, expected);
    let mut found = false;
    let mut kinds = ProbeKinds {
        blocks: None,
        partners: None,
    };
    for round in 0..MAX_PROBE_ROUNDS {
        if exhausted(w) {
            return Some(found);
        }
        std::mem::swap(&mut w.sp, &mut probe);
        let result = probe_round(w, e, expected, flags, &mut kinds);
        std::mem::swap(&mut w.sp, &mut probe);
        let Some((asked, lossy)) = result else {
            return if round == 0 { None } else { Some(found) };
        };
        let placed: Vec<(N<'t>, usize)> = asked
            .into_iter()
            .filter(|&(n, at)| takes.takes(n, at) && escape_at(w, c, n, at))
            .collect();
        if placed.is_empty() && lossy {
            return if round == 0 { None } else { Some(found) };
        }
        if placed.is_empty() {
            return Some(found);
        }
        found = true;
        // One sort per node and round: one per escape made a node with many escapes quadratic.
        let mut touched: AddrSet<usize> = AddrSet::default();
        for (n, at) in placed {
            probe.escapes.entry(id(n)).or_default().push(at);
            touched.insert(id(n));
        }
        for n in touched {
            if let Some(list) = probe.escapes.get_mut(&n) {
                #[cfg(test)]
                super::PROBE_SORTED.with(|c| c.set(c.get() + list.len()));
                list.sort_unstable();
            }
        }
    }
    Some(found)
}

/// The kind blocks and bracket partners of the probed inlines, kept across the rounds of one probe.
struct ProbeKinds<'t> {
    blocks: Option<KindBlocks<'t>>,
    partners: Option<Partners<'t>>,
}

/// One round of a probe, in the spelling of the probe: the escapes it asked for and whether it found losses.
fn probe_round<'t>(
    w: &mut Writer<'t>,
    e: N<'t>,
    expected: &[N<'t>],
    flags: u32,
    kinds: &mut ProbeKinds<'t>,
) -> Option<(Vec<(N<'t>, usize)>, bool)> {
    let chunk = Chunk::new(format!("{}\n", write_inlines(w, expected, flags)));
    let parsed = chunk.parse(w);
    let source = chunk.text().to_string();
    let block = children(parsed).single()?;
    if ty(block) != "paragraph" {
        return None;
    }
    let mut probe = Check {
        flags,
        source: Some(source),
        ..Check::default()
    };
    compare_inlines(w, &mut probe, expected, children(block), e);
    if w.kind_escapes {
        escape_probe_kinds(w, &mut probe, expected, &mut kinds.blocks);
        // Every bracketed node writes a `]`: without one there is no partner to escape.
        if chunk.text().contains(']') {
            let partners = kinds
                .partners
                .get_or_insert_with(|| partner_closers(expected));
            escape_partner_closers(w, &mut probe, partners);
        }
    }
    extend_chains(w, &mut probe);
    Some((probe.escapes, !probe.losses.is_empty()))
}

/// C2-W-088: a probe round escapes every opener of each kind its parse reports
/// in the probed inlines, read as one leaf block, as a round of the group does.
/// A C2-W-125 trial or closer repair gives no kind (CC-1568).
fn escape_probe_kinds<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    expected: &[N<'t>],
    blocks: &mut Option<KindBlocks<'t>>,
) {
    let reported: Vec<(N<'t>, usize)> = c
        .escapes
        .iter()
        .filter(|&&(n, at)| {
            !c.closers.contains(&(id(n), at))
                && !c.trials.iter().any(|&(t, a)| a == at && id(t) == id(n))
        })
        .copied()
        .collect();
    if reported.is_empty() {
        return;
    }
    let blocks = blocks.get_or_insert_with(|| KindBlocks::content(expected));
    let mut done = HashSet::new();
    for (n, at) in reported {
        let line = c.line_escapes.contains(&(id(n), at));
        let Some(key) = blocks.key(n, at, line) else {
            continue;
        };
        if done.insert(key.clone()) {
            for (m, offset) in blocks.openers(0, &key) {
                escape_at(w, c, m, offset);
            }
        }
    }
}

/// Each `]` of a bracketed content with the `[` of that content it closes.
pub type Partners<'t> = Vec<((N<'t>, usize), (N<'t>, usize))>;

/// The bracket partners in the content of each bracketed node below `nodes`.
pub fn partner_closers<'t>(nodes: &[N<'t>]) -> Partners<'t> {
    let mut partners = Vec::new();
    let mut pending: Vec<N<'t>> = nodes.to_vec();
    while let Some(node) = pending.pop() {
        if !has(node, "children") {
            continue;
        }
        if closing_mark(ty(node)) != Some(b']') {
            pending.extend(children(node).iter());
            continue;
        }
        let mut open: Vec<(N<'t>, usize)> = Vec::new();
        let mut content: Vec<N<'t>> = children(node).iter().collect();
        content.reverse();
        while let Some(n) = content.pop() {
            if closing_mark(ty(n)) == Some(b']') {
                pending.push(n);
            } else if ty(n) == "text" {
                for (at, &b) in text(n).as_bytes().iter().enumerate() {
                    match b {
                        b'[' => open.push((n, at)),
                        b']' => {
                            if let Some(partner) = open.pop() {
                                partners.push(((n, at), partner));
                            }
                        }
                        _ => {}
                    }
                }
            } else if has(n, "children") {
                content.extend(children(n).iter().rev());
            }
        }
    }
    partners
}

/// C2-W-125 case five: a `[` escaped in the content of a bracketed node leaves
/// the `]` it paired with bare, which would end that node early; that `]` takes
/// its backslash in the same round.
pub fn escape_partner_closers<'t>(w: &Writer<'t>, c: &mut Check<'t>, partners: &Partners<'t>) {
    for &((n, at), (p, pat)) in partners {
        if is_escaped(w, c, p, pat) {
            escape_at(w, c, n, at);
        }
    }
}

fn escape_content_construct<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, e: N<'t>) -> bool {
    let kids = children(e);
    if !has(e, "children") || kids.is_empty() {
        return false;
    }
    let word = w.stand_in(e, "word", "x");
    let mut flat: Vec<N<'t>> = Vec::with_capacity(kids.len() + 1);
    flat.push(word);
    for n in kids {
        flat.push(if ty(n) == "soft_break" {
            w.stand_in(n, "space", " ")
        } else {
            n
        });
    }
    let mut with_word = vec![word];
    with_word.extend(kids.iter());
    let variants: Vec<Vec<N<'t>>> = if is_break(kids.first()) {
        vec![with_word, flat]
    } else {
        vec![refs(kids), with_word, flat]
    };
    let flags = content_flags(c, e);
    for expected in variants {
        if let Some(found) = probe_escapes(w, c, e, &expected, flags, Site::Content, &Takes::All) {
            return found;
        }
    }
    false
}

const MAX_ACROSS: usize = 16;

fn is_paired_mark(n: V<'_>) -> bool {
    matches!(
        ty(n),
        "emphasis"
            | "strong"
            | "underline"
            | "strike"
            | "highlight"
            | "superscript"
            | "subscript"
            | "insert"
            | "delete"
    ) && has(n, "children")
}

fn opens_across(value: &str) -> bool {
    value.contains(['[', '`', '<', '{', '$', ':'])
}

fn escape_across_closer<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, es: &[N<'t>], i: usize) -> bool {
    let Some(&e) = es.get(i) else { return false };
    if !is_paired_mark(e) || !texts_below(e).iter().any(|t| opens_across(text(t))) {
        return false;
    }
    let flags = content_flags(c, e);
    let outer = c.flags;
    let spelled = write_inlines(w, &[e], outer);
    let content = write_inlines(w, &refs(children(e)), flags);
    let tail = (spelled.len().saturating_sub(content.len())) / 2;
    let closer_text = spelled
        .get(spelled.len().saturating_sub(tail)..)
        .unwrap_or("")
        .to_string();
    let closer = w.stand_in(e, "closer", &closer_text);
    let word = w.stand_in(e, "word", "x");
    let end = es.len().min(i + 1 + MAX_ACROSS);
    let mut inlines: Vec<N<'t>> = refs(children(e));
    inlines.push(closer);
    inlines.extend(es.get(i + 1..end).unwrap_or(&[]).iter().copied());
    let takes = Takes::Across {
        closer: id(closer),
        word: id(word),
        own: texts_below(e).into_iter().map(id).collect(),
        mark: closing_mark(ty(e)),
    };
    if probe_escapes(w, c, e, &inlines, flags, Site::Across, &takes) == Some(true) {
        return true;
    }
    let mut behind = vec![word];
    behind.extend(inlines);
    probe_escapes(w, c, e, &behind, flags, Site::Across, &takes) == Some(true)
}

fn escape_content_line<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, e: N<'t>) -> bool {
    if !has(e, "children") {
        return false;
    }
    let nodes = refs(children(e));
    for (at, &node) in nodes.iter().enumerate() {
        if has(node, "children") && escape_content_line(w, c, node) {
            return true;
        }
        let Some(&next) = nodes.get(at + 1) else {
            continue;
        };
        if !is_break(Some(node))
            || ty(next) != "text"
            || !has_opener_shape(&nodes, at + 1)
            || !opens_block_alone(w, c, &nodes, at + 1)
        {
            continue;
        }
        if escape_run_opener(w, c, &nodes, at + 1) {
            return true;
        }
    }
    false
}

fn opens_block_alone<'t>(w: &mut Writer<'t>, c: &Check<'t>, nodes: &[N<'t>], from: usize) -> bool {
    let mut end = from;
    while nodes.get(end).is_some_and(|n| !is_break(Some(*n))) {
        end += 1;
    }
    let line = write_inlines(w, nodes.get(from..end).unwrap_or(&[]), c.flags);
    if let Some(&known) = w.line_opens.get(&line) {
        return known;
    }
    if exhausted(w) {
        return false;
    }
    let chunk = Chunk::new(line.clone());
    let opens = children(chunk.parse(w))
        .single()
        .is_none_or(|p| ty(p) != "paragraph");
    w.line_opens.insert(line, opens);
    opens
}

fn opener_shift(a: V<'_>) -> usize {
    match ty(a) {
        "image" | "literal_inline" | "inline_footnote" => 1,
        "math" => {
            if super::node::flag(a, "display") {
                2
            } else {
                1
            }
        }
        "smart_punctuation" => {
            let source = s(a, "source").unwrap_or("");
            if source.starts_with("{-") {
                2
            } else {
                usize::from(source.starts_with('-'))
            }
        }
        _ => 0,
    }
}

/// The first byte of the opener of `a`, where it is known.
fn opener_byte(a: V<'_>) -> Option<u8> {
    match ty(a) {
        "image" => Some(b'!'),
        "math" => Some(b'$'),
        "smart_punctuation" => s(a, "source").and_then(|x| x.bytes().next()),
        _ => None,
    }
}

fn is_chain_opener(a: V<'_>) -> bool {
    match ty(a) {
        "footnote_ref" => true,
        "link" | "image" | "span" | "inline_footnote" => {
            !has(a, "key") || matches!(s(a, "refForm"), Some("full" | "collapsed" | "shortcut"))
        }
        _ => false,
    }
}

fn holds_closer(node: V<'_>) -> bool {
    if ty(node) == "image" && s(node, "alt").is_some_and(|a| a.contains(']')) {
        return true;
    }
    children(node).iter().any(|c| {
        if ty(c) == "text" {
            text(c).contains(']')
        } else {
            holds_closer(c)
        }
    })
}

const SPAN_TYPES: &[&str] = &["code", "literal_inline", "math"];

/// The text node and offset `at` bytes into the run of text nodes starting at `es[i]`.
fn run_offset<'t>(es: &[N<'t>], i: usize, mut at: usize) -> Option<(N<'t>, usize)> {
    let mut k = i;
    while let Some(n) = text_at(es, k) {
        let len = text(n).len();
        if at < len {
            return Some((n, at));
        }
        at -= len;
        k += 1;
    }
    None
}

/// C2-W-090, C2-W-099: a dash takes the escape of the bare hyphen run it
/// stands in; `None` when that escape is already asked.
fn dash_run_start(
    w: &Writer<'_>,
    c: &Check<'_>,
    e: N<'_>,
    start: usize,
    a: V<'_>,
) -> Option<usize> {
    if ty(a) != "smart_punctuation" || !s(a, "source").is_some_and(|x| x.starts_with('-')) {
        return Some(start);
    }
    let b = text(e).as_bytes();
    let mut start = start;
    while start > 0 && b.get(start - 1) == Some(&b'-') && !is_escaped(w, c, e, start - 1) {
        start -= 1;
    }
    (!is_escaped(w, c, e, start + 1)).then_some(start)
}

fn escape_opener<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    es: &[N<'t>],
    i: usize,
    at: usize,
    a: V<'_>,
) {
    let b = text(e).as_bytes();
    let mut start = at;
    while matches!(b.get(start), Some(b' ' | b'\t')) {
        start += 1;
    }
    let Some(start) = dash_run_start(w, c, e, start, a) else {
        return;
    };
    let shifted = start + opener_shift(a);
    // The opener's second character can sit in the next text node (C2-W-099).
    if shifted >= b.len()
        && b.get(start).copied() == opener_byte(a)
        && run_offset(es, i, shifted).is_some_and(|(n, k)| escape_at(w, c, n, k))
    {
        return;
    }
    if escape_at(w, c, e, shifted) {
        let opener = b.get(shifted).copied();
        if (opener == Some(b'[') && is_chain_opener(a))
            || (opener == Some(b'`') && SPAN_TYPES.contains(&ty(a)))
        {
            c.chains.push((
                e,
                shifted,
                Built {
                    kind: ty(a).to_string(),
                    start: span(a).map(|p| p.0),
                    holds_closer: holds_closer(a),
                },
            ));
        }
    } else if !escape_at(w, c, e, start) {
        c.losses.push((e, Some("value".into())));
    }
}

fn sigil_of(b: &[u8], at: usize) -> u8 {
    match at.checked_sub(1).and_then(|p| b.get(p)) {
        Some(&x @ (b'!' | b'^')) => x,
        _ => 0,
    }
}

fn bracket_run(b: &[u8], at: usize) -> (usize, usize) {
    let in_run = |k: usize| {
        b.get(k) == Some(&b'[')
            || (matches!(b.get(k), Some(b'!' | b'^')) && b.get(k + 1) == Some(&b'['))
    };
    let mut start = at;
    while start > 0 && in_run(start - 1) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < b.len() && in_run(end) {
        end += 1;
    }
    (start, end)
}

fn escape_footnote_brackets<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    node: N<'t>,
    at: usize,
    end: usize,
) {
    let b = text(node).as_bytes();
    for k in at + 1..end {
        if b.get(k) != Some(&b'[') {
            continue;
        }
        if b.get(k + 1) != Some(&b'^') || k + 2 >= end {
            return;
        }
        escape_at(w, c, node, k);
    }
}

fn escape_brackets<'t>(w: &Writer<'t>, c: &mut Check<'t>, node: N<'t>, at: usize, a: &Built) {
    let b = text(node).as_bytes();
    let (start, end) = bracket_run(b, at);
    if !b.get(end..).is_some_and(|r| r.contains(&b']')) {
        return;
    }
    if a.kind == "footnote_ref" {
        escape_footnote_brackets(w, c, node, at, end);
        return;
    }
    let sigil = sigil_of(b, at);
    let opens =
        |k: usize| sigil_of(b, k) == sigil && (sigil == b'^' || b.get(k + 1) != Some(&b'^'));
    let mut k = at;
    while k > start {
        k -= 1;
        if b.get(k) != Some(&b'[') {
            continue;
        }
        if !opens(k) {
            break;
        }
        escape_at(w, c, node, k);
    }
    if a.holds_closer {
        return;
    }
    for k in at + 1..end {
        if b.get(k) != Some(&b'[') {
            continue;
        }
        if !opens(k) {
            return;
        }
        escape_at(w, c, node, k);
    }
}

fn escape_following_runs<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    node: N<'t>,
    from: usize,
    width: usize,
) {
    let b = text(node).as_bytes();
    let mut starts = Vec::new();
    let mut start = b
        .get(from..)
        .and_then(|r| r.iter().position(|&x| x == b'`'))
        .map(|p| p + from);
    while let Some(st) = start {
        let mut end = st + 1;
        while b.get(end) == Some(&b'`') {
            end += 1;
        }
        if end - st != width {
            return;
        }
        starts.push(st);
        start = b
            .get(end..)
            .and_then(|r| r.iter().position(|&x| x == b'`'))
            .map(|p| p + end);
    }
    for &st in starts.iter().take(starts.len().saturating_sub(1)) {
        escape_at(w, c, node, st);
    }
}

fn last_bare_backtick(source: &str) -> Option<usize> {
    let mut slashes = 0;
    let mut last = None;
    for (at, &b) in source.as_bytes().iter().enumerate() {
        if b == b'`' && slashes % 2 == 0 {
            last = Some(at);
        }
        slashes = if b == b'\\' { slashes + 1 } else { 0 };
    }
    last
}

fn escape_backticks<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    node: N<'t>,
    at: usize,
    a: &Built,
    last_bare: Option<usize>,
) {
    let b = text(node).as_bytes();
    let mut run = at;
    while b.get(run) == Some(&b'`') {
        run += 1;
    }
    let Some(source) = c.source.clone() else {
        return;
    };
    let Some(pos) = a.start else {
        escape_following_runs(w, c, node, run, run - at);
        return;
    };
    let src = source.as_bytes();
    let Some(mut start) = src
        .get(pos.min(src.len())..)
        .and_then(|r| r.iter().position(|&x| x == b'`'))
        .map(|p| p + pos)
    else {
        return;
    };
    let end = start + run - at;
    while start < end && src.get(start) == Some(&b'`') {
        start += 1;
    }
    if start != end || src.get(end) == Some(&b'`') {
        return;
    }
    if last_bare.is_some_and(|l| l >= end) {
        return;
    }
    for k in at + 1..run {
        escape_at(w, c, node, k);
    }
}

/// Extends the escapes of the chains that this round places by those the procedure would place behind each.
pub fn extend_chains<'t>(w: &Writer<'t>, c: &mut Check<'t>) {
    if c.chains.is_empty() {
        return;
    }
    let placed: AddrSet<(usize, usize)> = c.escapes.iter().map(|(n, at)| (id(n), *at)).collect();
    let last_bare = c.source.as_deref().and_then(last_bare_backtick);
    let chains = std::mem::take(&mut c.chains);
    for (node, at, a) in &chains {
        if !placed.contains(&(id(node), *at)) {
            continue;
        }
        if text(node).as_bytes().get(*at) == Some(&b'`') {
            escape_backticks(w, c, *node, *at, a, last_bare);
        } else {
            escape_brackets(w, c, *node, *at, a);
        }
    }
    c.chains = chains;
}

#[derive(Default)]
struct Cursor {
    i: usize,
    vi: usize,
    j: usize,
    aj: usize,
    anchor: Option<usize>,
    anchor_byte: usize,
}

fn escape_mark<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    nodes: &[N<'t>],
    start: usize,
    from: usize,
    m: u8,
) -> bool {
    for (at, &node) in nodes.iter().enumerate().skip(start) {
        if ty(node) != "text" {
            if has(node, "children") && escape_mark(w, c, &refs(children(node)), 0, 0, m) {
                return true;
            }
            continue;
        }
        let b = text(node).as_bytes();
        let begin = if at == start { from } else { 0 };
        for (k, &x) in b.iter().enumerate().skip(begin) {
            if x == m && escape_at(w, c, node, k) {
                return true;
            }
        }
    }
    false
}

/// `escape_mark` for the closing mark of a node that ended early; the escape is kept in `c.closers`.
fn escape_closer<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    nodes: &[N<'t>],
    start: usize,
    from: usize,
    m: u8,
) -> bool {
    if !escape_mark(w, c, nodes, start, from, m) {
        return false;
    }
    if let Some(&(n, at)) = c.escapes.last() {
        c.closers.insert((id(n), at));
    }
    true
}

fn closed_early<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    es: &[N<'t>],
    i: usize,
    vi: usize,
    owner: N<'t>,
) {
    let m = closing_mark(ty(owner));
    if matches!(ty(owner), "emphasis" | "strong") && has_other_form(w, c, owner) {
        change_form(c, owner);
    } else if !m.is_some_and(|m| escape_closer(w, c, es, i, vi, m)) {
        // The wrapper has already read back. If a prior child changed and a
        // later construct is missing, name that construct rather than the
        // wrapper that still surrounds it (C2-W-124).
        let lost_child = es
            .get(i)
            .copied()
            .filter(|n| ty(*n) != "text" && !c.losses.is_empty());
        c.losses.push((lost_child.unwrap_or(owner), None));
    }
}

fn is_brace_line(node: Option<V<'_>>) -> bool {
    node.is_some_and(|n| ty(n) == "text" && text(n).starts_with('{') && text(n).ends_with('}'))
}

fn opener_line(es: &[N<'_>], from: usize) -> usize {
    let mut at = from;
    while at + 2 < es.len()
        && is_brace_line(es.get(at).copied())
        && is_break(es.get(at + 1).copied())
    {
        at += 2;
    }
    at
}

fn ended_early<'t>(w: &Writer<'t>, c: &mut Check<'t>, es: &[N<'t>], i: usize, owner: N<'t>) {
    let line = if is_break(es.get(i).copied()) {
        es.get(i + 1).copied()
    } else {
        None
    };
    if let Some(l) = line.filter(|l| ty(l) == "text" && ends_line_with_cr(text(l), 0)) {
        c.losses.push((l, Some("value".into())));
        return;
    }
    let prev = i.checked_sub(1).and_then(|p| es.get(p)).copied();
    if i > 0 && !is_break(es.get(i).copied()) && escape_end(w, c, last_text(prev), false) {
        return;
    }
    if i > 0 && is_break(prev) {
        escape_line_start(w, c, es, opener_line(es, i), owner);
    } else if closing_mark(ty(owner)).is_some() {
        closed_early(w, c, es, i, 0, owner);
    } else if is_break(es.get(i).copied()) && i + 1 < es.len() {
        escape_line_start(w, c, es, opener_line(es, i + 1), owner);
    } else {
        c.losses.push((es.get(i).copied().unwrap_or(owner), None));
    }
}

fn match_characters<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    a: V<'_>,
    at: &mut Cursor,
) -> bool {
    let value = text(e).as_bytes();
    let other = text(a).as_bytes();
    let length = value
        .len()
        .saturating_sub(at.vi)
        .min(other.len().saturating_sub(at.aj));
    for k in 0..length {
        if value.get(at.vi + k) == other.get(at.aj + k) {
            continue;
        }
        if !escape_at(w, c, e, at.vi + k) {
            c.losses.push((e, Some("value".into())));
        }
        return false;
    }
    at.vi += length;
    at.aj += length;
    if at.aj == other.len() {
        at.j += 1;
        at.aj = 0;
    }
    true
}

fn starts_with_closer(node: V<'_>, next: Option<V<'_>>) -> bool {
    closing_mark(ty(node)).is_some_and(|m| {
        next.is_some_and(|n| ty(n) == "text" && text(n).as_bytes().first() == Some(&m))
    })
}

fn parse_ends_in_text<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    es: &[N<'t>],
    at: &Cursor,
    owner: N<'t>,
) -> bool {
    let cur = es.get(at.i).copied();
    if let Some(t) = cur.filter(|t| ty(t) == "text" && ends_line_with_cr(text(t), at.vi)) {
        c.losses.push((t, Some("value".into())));
        return false;
    }
    let prev = at.i.checked_sub(1).and_then(|p| es.get(p)).copied();
    if at.vi == 0 {
        if let (Some(p), Some(t)) = (prev, cur) {
            if starts_with_closer(p, Some(t)) && repairs_inside(c, p) {
                // The node read on into this text; its own repair ends it.
                return false;
            }
            if starts_with_closer(p, Some(t)) && escape_at(w, c, t, 0) {
                return false;
            }
        }
    }
    if at.vi == 0 && at.i > 0 && is_break(prev) {
        escape_line_start(w, c, es, opener_line(es, at.i), owner);
    } else if closing_mark(ty(owner)).is_some() {
        closed_early(w, c, es, at.i, at.vi, owner);
    } else {
        c.losses.push((owner, None));
    }
    false
}

/// Whether this round escapes a bracket of C2-W-125 in the content of `node`.
fn repairs_inside(c: &Check<'_>, node: V<'_>) -> bool {
    let texts: AddrSet<usize> = texts_below(node).into_iter().map(id).collect();
    c.escapes
        .iter()
        .any(|&(n, at)| c.closers.contains(&(id(n), at)) && texts.contains(&id(n)))
}

fn after_text_node<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    es: &[N<'t>],
    as_: Arr<'_>,
    at: &mut Cursor,
    owner: N<'t>,
) -> bool {
    at.i += 1;
    at.vi = 0;
    if at.aj == 0 || es.get(at.i).is_some_and(|n| ty(n) == "text") {
        return true;
    }
    if es.get(at.i).is_none() {
        if !escape_in_content(w, c, owner) && !escape_end(w, c, last_text(Some(owner)), true) {
            c.losses.push((owner, None));
        }
    } else if at.j + 1 >= as_.len()
        && matches!(ty(owner), "emphasis" | "strong")
        && !joins_next(es, at.i)
        && !es.get(at.i).is_some_and(|&n| reads_as_text(w, c, n))
    {
        closed_early(w, c, es, at.i, 0, owner);
    } else {
        return respell(w, c, es, at.i) && step_over(w, c, es, as_, at);
    }
    false
}

/// A note or extension that a lone bracket of its content keeps from forming reads as text.
fn reads_as_text(w: &Writer<'_>, c: &Check<'_>, node: V<'_>) -> bool {
    matches!(ty(node), "span" | "inline_footnote" | "inline_extension")
        && has_lone_bracket(w, c, node)
}

fn match_text<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    es: &[N<'t>],
    as_: Arr<'_>,
    at: &mut Cursor,
    owner: N<'t>,
) -> bool {
    let Some(a) = as_.get(at.j) else {
        return parse_ends_in_text(w, c, es, at, owner);
    };
    if ty(a) != "text" {
        return skip_construct(w, c, e, es, a, at);
    }
    // A prior lost node does not license a later field to disappear silently.
    if !c.losses.is_empty() {
        for name in ["lang", "dir", "metadata", "attrs", "data"] {
            if is_compared(c, e, result_fields(e), name)
                && e.get(name).is_some()
                && a.get(name).is_none()
            {
                c.losses.push((e, Some(String::new())));
                break;
            }
        }
    }
    if !match_characters(w, c, e, a, at) {
        return false;
    }
    at.vi < text(e).len() || after_text_node(w, c, es, as_, at, owner)
}

fn step_over<'t>(
    w: &mut Writer<'t>,
    c: &Check<'t>,
    es: &[N<'t>],
    as_: Arr<'_>,
    at: &mut Cursor,
) -> bool {
    let Some(&e) = es.get(at.i) else { return false };
    let spelled = write_inlines(w, &[e], c.flags);
    let Some(a) = as_.get(at.j).filter(|a| ty(a) == "text") else {
        return false;
    };
    if spelled.contains('\\')
        || !text(a)
            .get(at.aj..)
            .is_some_and(|r| r.starts_with(spelled.as_str()))
    {
        return false;
    }
    at.aj += spelled.len();
    if at.aj == text(a).len() {
        at.j += 1;
        at.aj = 0;
    }
    at.i += 1;
    true
}

fn comparison_behind_quote(
    prior: Option<V<'_>>,
    before: Option<V<'_>>,
    t: V<'_>,
    at: usize,
    parsed: V<'_>,
) -> bool {
    let neighbor = prior
        .filter(|p| ty(p) == "text")
        .and_then(|p| text(p).as_bytes().last().copied());
    let b = text(t).as_bytes();
    before.is_some_and(|x| {
        ty(x) == "smart_punctuation"
            && matches!(
                s(x, "kind"),
                Some("right_single_quote" | "right_double_quote")
            )
    }) && matches!(neighbor, Some(b'/' | b'-'))
        && ty(parsed) == "smart_punctuation"
        && matches!(
            s(parsed, "kind"),
            Some("not_equal" | "less_than_or_equal" | "greater_than_or_equal")
        )
        && b.get(at).is_some_and(|x| b"!<>".contains(x))
        && b.get(at + 1) == Some(&b'=')
}

fn skip_construct<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    es: &[N<'t>],
    a: V<'_>,
    at: &mut Cursor,
) -> bool {
    let prior = at.i.checked_sub(2).and_then(|p| es.get(p)).copied();
    let before = at.i.checked_sub(1).and_then(|p| es.get(p)).copied();
    if comparison_behind_quote(prior, before, e, at.vi, a) {
        escape_at(w, c, e, at.vi + 1);
    } else {
        escape_opener(w, c, e, es, at.i, at.vi, a);
    }
    let Some((start, end)) = span(a) else {
        return false;
    };
    let Some(stop) = spelled_end(w, e, at.vi, end - start, c.flags) else {
        return false;
    };
    at.vi = stop;
    at.j += 1;
    if at.vi == text(e).len() {
        at.i += 1;
        at.vi = 0;
    }
    true
}

fn reads_on(node: V<'_>, next: V<'_>) -> bool {
    if has(node, "attrs") {
        return false;
    }
    matches!(ty(node), "mention" | "tag")
        || (SPAN_TYPES.contains(&ty(node)) && text(next).starts_with('`'))
}

fn move_sigil_escape<'t>(w: &Writer<'t>, c: &mut Check<'t>, next: N<'t>) -> bool {
    let b = text(next).as_bytes();
    if b.first() != Some(&b':') || !is_escaped(w, c, next, 0) {
        return false;
    }
    let mut bracket = 1;
    while b.get(bracket).is_some_and(|&x| is_extension_name_char(x)) {
        bracket += 1;
    }
    if bracket == 1 || b.get(bracket) != Some(&b'[') || is_escaped(w, c, next, bracket) {
        return false;
    }
    c.retracted.push((next, 0));
    mark(c, next, 0, false);
    escape_at(w, c, next, bracket)
}

fn escape_comparison<'t>(w: &Writer<'t>, c: &mut Check<'t>, next: N<'t>) -> bool {
    let b = text(next).as_bytes();
    b.len() >= 2
        && b.first().is_some_and(|x| b"!<>".contains(x))
        && b.get(1) == Some(&b'=')
        && escape_at(w, c, next, 1)
}

fn reads_neighbor<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    es: &[N<'t>],
    i: usize,
    a: V<'_>,
) -> bool {
    let next = text_at(es, i + 1);
    if let Some(n) = next {
        let differ = match e.get("attrs") {
            Some(v) => !same_value(v, a.get("attrs")),
            None => a.get("attrs").is_some(),
        };
        if differ && text(n).starts_with('{') {
            return escape_at(w, c, n, 0);
        }
        if reads_on(e, n) && differing_field(w, c, e, a).is_some() {
            return escape_at(w, c, n, 0);
        }
    }
    if ty(e) != "smart_punctuation"
        || ty(a) != "smart_punctuation"
        || (s(e, "kind") == s(a, "kind") && s(e, "source") == s(a, "source"))
    {
        return false;
    }
    if let Some(n) = next {
        if move_sigil_escape(w, c, n) || escape_comparison(w, c, n) || escape_at(w, c, n, 0) {
            return true;
        }
    }
    let before = i.checked_sub(1).and_then(|p| text_at(es, p));
    before.is_some_and(|b| escape_at(w, c, b, text(b).len().saturating_sub(1)))
}

fn is_lost_code(w: &Writer<'_>, node: Option<V<'_>>) -> bool {
    node.is_some_and(|n| ty(n) == "code" && w.is_lost(n))
}

fn match_node<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    es: &[N<'t>],
    as_: Arr<'_>,
    at: &mut Cursor,
    owner: N<'t>,
) -> bool {
    let Some(a) = as_.get(at.j) else {
        ended_early(w, c, es, at.i, owner);
        return false;
    };
    if ty(a) != ty(e) || at.aj > 0 {
        return respell(w, c, es, at.i) && step_over(w, c, es, as_, at);
    }
    if is_lost_code(w, es.get(at.i + 1).copied()) {
        at.i += 1;
        while is_lost_code(w, es.get(at.i).copied()) {
            at.i += 1;
        }
        at.j += 1;
        return true;
    }
    if reads_neighbor(w, c, e, es, at.i, a) {
        return false;
    }
    repair_comment_closer(w, c, es, at.i, e, a);
    compare_inline(w, c, e, a);
    at.i += 1;
    at.j += 1;
    true
}

fn brackets_before_comment(value: &str) -> Vec<usize> {
    let mut open = Vec::new();
    for (i, &b) in value.as_bytes().iter().enumerate() {
        if b == b'[' {
            open.push(i);
        } else if b == b']' {
            open.pop();
        }
    }
    open
}

fn repair_comment_closer<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    es: &[N<'t>],
    at: usize,
    e: V<'_>,
    a: V<'_>,
) {
    if ty(e) != "comment" || ty(a) != "comment" || s(e, "form") != Some("line") {
        return;
    }
    let before = at.checked_sub(1).and_then(|p| es.get(p)).copied();
    if !text(e).contains(']') && text(a).contains(']') {
        if !repair_bracket_before_comment(w, c, before) {
            repair_brace_before_comment(w, c, es, at);
        }
        return;
    }
    if !text(e).contains(']') || text(a).contains(']') {
        return;
    }
    let Some(b) = before.filter(|b| ty(b) == "text") else {
        return;
    };
    for bracket in brackets_before_comment(text(b)) {
        if escape_at(w, c, b, bracket) {
            return;
        }
    }
}

fn repair_bracket_before_comment<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    before: Option<N<'t>>,
) -> bool {
    let Some(b) = before.filter(|b| ty(b) == "text") else {
        return false;
    };
    let v = text(b);
    if !v.contains("[{*{*") {
        return false;
    }
    if v.rfind('[')
        .and_then(|p| v.get(p..))
        .is_some_and(|r| r.contains("{*"))
    {
        return false;
    }
    v.ends_with([']', '^']) && escape_at(w, c, b, v.len() - 1)
}

fn repair_brace_before_comment<'t>(w: &Writer<'t>, c: &mut Check<'t>, es: &[N<'t>], at: usize) {
    for index in (0..at).rev() {
        let Some(&n) = es.get(index) else { continue };
        if is_break(Some(n)) {
            return;
        }
        if ty(n) != "text" {
            continue;
        }
        let b = text(n).as_bytes();
        for brace in (0..b.len().saturating_sub(1)).rev() {
            if b.get(brace) == Some(&b'{')
                && b.get(brace + 1).is_some_and(|x| b"*/-=".contains(x))
                && escape_at(w, c, n, brace)
            {
                return;
            }
        }
    }
}

fn set_anchor(es: &[N<'_>], as_: Arr<'_>, at: &mut Cursor) {
    if at.vi != 0 || at.aj != 0 {
        return;
    }
    if is_break(es.get(at.i).copied()) {
        at.anchor = None;
    } else if let Some((start, _)) = as_.get(at.j).and_then(span) {
        at.anchor = Some(at.i);
        at.anchor_byte = start;
    }
}

fn resync<'t>(
    w: &mut Writer<'t>,
    c: &Check<'t>,
    es: &[N<'t>],
    as_: Arr<'_>,
    at: &mut Cursor,
) -> bool {
    let (Some(anchor), Some((_, failed_end))) = (at.anchor, as_.get(at.j).and_then(span)) else {
        return false;
    };
    let mut offset = at.anchor_byte;
    let mut j = at.j;
    let mut k = anchor;
    while let Some(&e) = es.get(k) {
        if is_break(Some(e)) {
            break;
        }
        while as_
            .get(j)
            .is_some_and(|a| span(a).is_none_or(|p| p.0 < offset))
        {
            j += 1;
        }
        if k > at.i
            && offset >= failed_end
            && as_.get(j).and_then(span).map(|p| p.0) == Some(offset)
        {
            at.i = k;
            at.j = j;
            at.vi = 0;
            at.aj = 0;
            return true;
        }
        // The node alone, not as a list of one (C2-W-088, C2-W-090).
        offset += super::inline::write_inline(w, e, c.flags).len();
        k += 1;
    }
    false
}

/// A paragraph can drop leading indentation of one supplied text while its
/// surviving suffix joins the next node. Resume behind the surviving suffix.
fn step_over_lost_text(es: &[N<'_>], as_: Arr<'_>, at: &mut Cursor) -> bool {
    let (Some(&expected), Some(actual)) = (es.get(at.i), as_.get(at.j)) else {
        return false;
    };
    if ty(expected) != "text" || ty(actual) != "text" || at.vi != 0 {
        return false;
    }
    let value = text(expected);
    let kept = value.trim_start_matches([' ', '\t']);
    if kept.len() == value.len()
        || !text(actual)
            .get(at.aj..)
            .is_some_and(|rest| rest.starts_with(kept))
    {
        return false;
    }
    at.i += 1;
    at.aj += kept.len();
    if at.aj == text(actual).len() {
        at.j += 1;
        at.aj = 0;
    }
    true
}

/// A failed comparison on one line cannot hide constructs on a later line of
/// the same paragraph. Resume only at breaks whose source lines still align.
fn resync_next_line<'t>(c: &mut Check<'t>, es: &[N<'t>], as_: Arr<'_>, at: &mut Cursor) -> bool {
    let mut i = at.i + 1;
    while i < es.len() && !is_break(es.get(i).copied()) {
        i += 1;
    }
    let mut j = at.j;
    // Text values may contain line endings that the reader turns into breaks.
    // Those breaks precede the expected sibling break used as the anchor.
    let embedded_breaks: usize = es
        .get(at.i..i.min(es.len()))
        .unwrap_or_default()
        .iter()
        .filter(|node| ty(**node) == "text")
        .map(|node| text(*node).bytes().filter(|byte| *byte == b'\n').count())
        .sum();
    for skipped in 0..=embedded_breaks {
        while j < as_.len() && !is_break(as_.get(j)) {
            j += 1;
        }
        if j == as_.len() {
            return false;
        }
        if skipped < embedded_breaks {
            j += 1;
        }
    }
    let (Some(&expected), Some(actual)) = (es.get(i), as_.get(j)) else {
        return false;
    };
    if ty(expected) != ty(actual)
        || matches!(
            (
                super::node::pos_field(expected, "startLine"),
                super::node::pos_field(actual, "startLine")
            ),
            (Some(left), Some(right)) if left != right
        )
    {
        return false;
    }
    if c.changes() == 0 {
        let mut actual_at = at.j;
        for &node in es.get(at.i..i).unwrap_or_default() {
            if let Some(found) = (actual_at..j).find(|&candidate| {
                as_.get(candidate)
                    .is_some_and(|a| super::writer_equal(node, a))
            }) {
                actual_at = found + 1;
            } else {
                c.losses.push((node, None));
            }
        }
    }
    // The aligned break can still have fields that the source lost.
    at.i = i;
    at.j = j;
    at.vi = 0;
    at.aj = 0;
    true
}

fn compare_inlines<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    es: &[N<'t>],
    as_: Arr<'_>,
    owner: N<'t>,
) {
    let outer = c.owner.replace(owner);
    let mut at = Cursor::default();
    let mut done = false;
    while at.i < es.len() {
        set_anchor(es, as_, &mut at);
        let Some(&e) = es.get(at.i) else { break };
        let more = if ty(e) == "text" {
            match_text(w, c, e, es, as_, &mut at, owner)
        } else {
            match_node(w, c, e, es, as_, &mut at, owner)
        };
        if !more
            && !resync(w, c, es, as_, &mut at)
            && !step_over_lost_text(es, as_, &mut at)
            && !resync_next_line(c, es, as_, &mut at)
        {
            done = true;
            break;
        }
    }
    if !done && at.j < as_.len() {
        c.losses.push((owner, None));
    }
    c.owner = outer;
}

fn compare_raw<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, e: N<'t>, a: V<'_>) {
    let mut probe = Check {
        flags: c.flags,
        ..Check::default()
    };
    compare_fields(w, &mut probe, e, a);
    if has(e, "children") {
        compare_inlines(w, &mut probe, &refs(children(e)), children(a), e);
    }
    let differs = probe.changes() + probe.losses.len() > 0;
    if differs && c.changes() == 0 {
        change_form(c, e);
    } else if differs {
        c.misaligned = true;
    }
}

fn compare_fields<'t>(w: &Writer<'t>, c: &mut Check<'t>, e: N<'t>, a: V<'_>) {
    if let Some(field) = differing_field(w, c, e, a) {
        c.losses.push((e, Some(field)));
    }
}

fn compare_inline<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, e: N<'t>, a: V<'_>) {
    if super::extension::source_core_equivalent(e, Some(a)) {
        return;
    }
    if has(e, "key") && has_other_form(w, c, e) {
        compare_raw(w, c, e, a);
        return;
    }
    compare_fields(w, c, e, a);
    if !has(e, "children") {
        return;
    }
    if matches!(ty(e), "span" | "inline_footnote" | "inline_extension")
        && !super::node::trees_equal(e, a)
    {
        let lone = lone_brackets(w, c, e);
        if !lone.is_empty() {
            for (n, at, nested) in lone {
                c.closers.insert((id(n), at));
                // A `]` inside a mark ended the note inside that mark: a closer, it gives no kind (C2-W-088).
                if nested && text(n).as_bytes().get(at) == Some(&b']') {
                    c.trials.push((n, at));
                }
            }
            return;
        }
    }
    let outer = c.flags;
    c.flags |= match ty(e) {
        "emphasis" => IN_EMPHASIS,
        "strong" => IN_STRONG,
        "link" => IN_BRACKET,
        _ => 0,
    };
    compare_inlines(w, c, &refs(children(e)), children(a), e);
    c.flags = outer;
}

fn block_mismatch<'t>(w: &Writer<'t>, c: &mut Check<'t>, e: N<'t>) {
    c.misaligned = true;
    if ty(e) == "paragraph" && !children(e).is_empty() {
        escape_line_start(w, c, &refs(children(e)), 0, e);
    } else {
        c.losses.push((e, None));
    }
}

fn missing_block<'t>(w: &Writer<'t>, c: &mut Check<'t>, e: N<'t>) {
    let first = if ty(e) == "paragraph" {
        children(e).first()
    } else {
        None
    };
    if let Some(f) = first.filter(|f| ty(f) == "text" && ends_line_with_cr(text(f), 0)) {
        c.losses.push((f, Some("value".into())));
        return;
    }
    if ty(e) == "paragraph" && !children(e).is_empty() {
        escape_line_start(w, c, &refs(children(e)), 0, e);
    } else if has_other_form(w, c, e) {
        change_form(c, e);
    } else {
        c.losses.push((e, None));
    }
}

fn has_span_marks(table: V<'_>) -> bool {
    children(table).iter().any(|g| {
        ty(g) != "comment"
            && children(g)
                .iter()
                .any(|r| ty(r) == "table_row" && children(r).iter().any(|cell| has(cell, "span")))
    })
}

fn compare_slots<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, e: N<'t>, a: V<'_>) {
    match (title(e), title(a)) {
        (Some(t), parsed) => compare_inlines(w, c, &refs(t), parsed.unwrap_or_else(Arr::empty), e),
        (None, Some(_)) => c.losses.push((e, Some("title".into()))),
        (None, None) => {}
    }
    match (caption(e), caption(a)) {
        (Some(x), Some(y)) => {
            compare_block(w, c, x, y);
        }
        (Some(x), None) => c.losses.push((x, None)),
        (None, Some(_)) => c.losses.push((e, Some("caption".into()))),
        (None, None) => {}
    }
}

fn compare_block<'t>(w: &mut Writer<'t>, c: &mut Check<'t>, e: N<'t>, a: V<'_>) -> bool {
    if w.is_lost(e) || super::extension::source_core_equivalent(e, Some(a)) {
        return true;
    }
    if ty(e) != ty(a) {
        block_mismatch(w, c, e);
        return false;
    }
    let marks = c.span_marks;
    let flags = c.flags;
    if ty(e) == "table" {
        c.span_marks = has_span_marks(e) || has_span_marks(a);
    }
    if ty(e) == "table_cell" {
        c.flags |= IN_CELL;
    }
    compare_fields(w, c, e, a);
    if is_inline_parent(e) {
        compare_inlines(w, c, &refs(children(e)), children(a), e);
    } else if ty(e) != "include" {
        compare_blocks(w, c, children(e), children(a), e);
    }
    compare_slots(w, c, e, a);
    c.span_marks = marks;
    c.flags = flags;
    true
}

fn read_starts(
    node: V<'_>,
    offset: i64,
    paragraphs: &mut AddrSet<usize>,
    lines: &mut AddrSet<i64>,
    first_div_line: &mut Option<i64>,
) {
    for block in children(node) {
        if let (Some(line), Some(col)) = (
            super::node::pos_field(block, "startLine"),
            super::node::pos_field(block, "startColumn"),
        ) {
            lines.insert(line - offset);
            if ty(block) == "div" {
                *first_div_line =
                    Some(first_div_line.map_or(line - offset, |first| first.min(line - offset)));
            }
            if ty(block) == "paragraph" {
                if let (Ok(l), Ok(cc)) = (usize::try_from(line - offset), usize::try_from(col - 1))
                {
                    paragraphs.insert(l * PLACE_COLUMNS + cc);
                }
            }
        }
        if !is_inline_parent(block) {
            read_starts(block, offset, paragraphs, lines, first_div_line);
        }
        if let Some(cap) = caption(block) {
            read_starts(cap, offset, paragraphs, lines, first_div_line);
        }
    }
}

/// The text of the run of text nodes starting at `nodes[from]`: one source line may be split over several.
fn run_text<'a>(nodes: &[V<'a>], from: usize) -> std::borrow::Cow<'a, str> {
    let Some(first) = text_at(nodes, from) else {
        return "".into();
    };
    if text_at(nodes, from + 1).is_none() {
        return text(first).into();
    }
    let mut k = from;
    let mut joined = String::new();
    while let Some(n) = text_at(nodes, k) {
        joined.push_str(text(n));
        k += 1;
    }
    joined.into()
}

/// Escapes the block opener of the text run at `nodes[from]`, best offset first (C2-W-090, C2-W-092).
fn escape_run_opener<'t>(w: &Writer<'t>, c: &mut Check<'t>, nodes: &[N<'t>], from: usize) -> bool {
    opener_offsets(&run_text(nodes, from))
        .into_iter()
        .any(|at| run_offset(nodes, from, at).is_some_and(|(n, k)| escape_at(w, c, n, k)))
}

fn has_opener_shape(nodes: &[V<'_>], from: usize) -> bool {
    let Some(n) = nodes.get(from) else {
        return false;
    };
    if ty(n) != "text" {
        return true;
    }
    let v = run_text(nodes, from);
    let v = v.as_ref();
    match v.as_bytes().first() {
        None => false,
        Some(b) => !b.is_ascii_alphanumeric() || opener_offsets(v).len() > 1,
    }
}

/// Whether an escape was already asked in the text run at `nodes[from]`.
fn is_pending(c: &Check<'_>, nodes: &[V<'_>], from: usize) -> bool {
    let mut k = from;
    while let Some(n) = text_at(nodes, k) {
        if c.marks.contains_key(&id(n)) {
            return true;
        }
        k += 1;
    }
    false
}

fn visit_lines<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    nodes: &[N<'t>],
    line: &mut i64,
    lines: &AddrSet<i64>,
    first_div_line: Option<i64>,
    owner: N<'t>,
) {
    for (at, &node) in nodes.iter().enumerate() {
        if has(node, "children") {
            visit_lines(
                w,
                c,
                &refs(children(node)),
                line,
                lines,
                first_div_line,
                owner,
            );
        }
        if !is_break(Some(node)) {
            continue;
        }
        *line += 1;
        let later_div_fence = first_div_line.is_some_and(|first| *line >= first)
            && nodes
                .get(at + 1)
                .is_some_and(|n| ty(*n) == "text" && text(*n).starts_with(":::"));
        if (lines.contains(line) || later_div_fence)
            && has_opener_shape(nodes, at + 1)
            && !is_pending(c, nodes, at + 1)
        {
            escape_line_start(w, c, nodes, at + 1, owner);
        }
    }
}

fn escape_opener_lines<'t>(
    w: &Writer<'t>,
    c: &mut Check<'t>,
    e: N<'t>,
    place: usize,
    paragraphs: &AddrSet<usize>,
    lines: &AddrSet<i64>,
    first_div_line: Option<i64>,
) {
    let top = refs(children(e));
    if !paragraphs.contains(&place) && has_opener_shape(&top, 0) && !is_pending(c, &top, 0) {
        escape_line_start(w, c, &top, 0, e);
    }
    let mut line = i64::try_from(place / PLACE_COLUMNS).unwrap_or(0);
    visit_lines(w, c, &top, &mut line, lines, first_div_line, e);
}

/// After a block read back as another block, finds the other lines of the group that opened a block (C2-W-092).
pub fn escape_openers<'t>(w: &Writer<'t>, c: &mut Check<'t>, parsed: V<'_>, offset: i64) {
    if !c.misaligned {
        return;
    }
    let mut paragraphs = AddrSet::default();
    let mut lines = AddrSet::default();
    let mut first_div_line = None;
    read_starts(
        parsed,
        offset,
        &mut paragraphs,
        &mut lines,
        &mut first_div_line,
    );
    for &(node, place) in &w.starts.clone() {
        if !has(node, "attrs") && !has(node, "lang") {
            escape_opener_lines(w, c, node, place, &paragraphs, &lines, first_div_line);
        }
    }
}

/// Compares the blocks `es` of `owner`, as they were written, with their parse `as_`.
pub fn compare_blocks<'t>(
    w: &mut Writer<'t>,
    c: &mut Check<'t>,
    es: Arr<'t>,
    as_: Arr<'_>,
    owner: N<'t>,
) {
    let before = c.changes();
    for (e, a) in es.iter().zip(as_) {
        if !compare_block(w, c, e, a) {
            return;
        }
    }
    if es.len() == as_.len() || c.changes() > before || es.iter().any(|e| w.is_lost(e)) {
        return;
    }
    c.misaligned = true;
    let missing = es.get(as_.len());
    if let (true, Some(m)) = (ty(owner) == "line_block", missing) {
        escape_line_start(w, c, &refs(children(m)), 0, m);
    } else if let (true, Some(m)) = (es.len() > as_.len(), missing) {
        missing_block(w, c, m);
    } else {
        c.losses.push((owner, None));
    }
}

#[cfg(test)]
mod resync_break_test {
    use super::*;

    #[test]
    fn aligned_break_fields_are_checked_after_an_earlier_loss() {
        let expected = [
            serde_json::json!({"type":"text","value":"before"}),
            serde_json::json!({"type":"soft_break","dir":"rtl"}),
            serde_json::json!({"type":"text","value":"after"}),
        ];
        let actual = [
            serde_json::json!({"type":"text","value":"different"}),
            serde_json::json!({"type":"soft_break","pos":{"startLine":1,"endLine":2,"startColumn":7,"endColumn":1,"startOffset":6,"endOffset":7}}),
            serde_json::json!({"type":"text","value":"after"}),
        ];
        let owner = serde_json::json!({"type":"paragraph"});
        let nodes: Vec<_> = expected.iter().map(V::Json).collect();
        let pool = typed_arena::Arena::new();
        let mut writer = Writer::new(&pool, false);
        let mut check = Check::default();
        compare_inlines(
            &mut writer,
            &mut check,
            &nodes,
            Arr::Json(&actual),
            V::Json(&owner),
        );
        assert!(check.losses.iter().any(|(node, field)| {
            id(*node) == id(V::Json(&expected[1])) && field.as_deref() == Some("dir")
        }));
    }
}
