//! The tree as the writer reads it: JSON values of the owned 2.0 tree, with
//! node identity by address, the slots of `data/node-slots.json` and the tree
//! equality of C2-A-161 (positions and the results of `data/ast-results.json`
//! are not compared).

use crate::hash::{AddrMap, AddrSet};
use crate::view::{Arr, V};
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::OnceLock;

/// The tables keyed by node type: fixed spec data, so the default hasher's
/// protection buys nothing.
type TypeMap<V> = HashMap<String, V, BuildHasherDefault<TypeHasher>>;

/// One multiply per eight bytes of a type name, as [`crate::hash::AddrHasher`] takes one per address.
#[derive(Default, Clone, Copy)]
struct TypeHasher(u64);

impl TypeHasher {
    const K: u64 = 0x9E37_79B9_7F4A_7C15;

    fn mix(&mut self, word: u64) {
        self.0 = (self.0 ^ word).wrapping_mul(Self::K);
    }
}

impl Hasher for TypeHasher {
    fn finish(&self) -> u64 {
        self.0.rotate_left(26)
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            self.mix(u64::from_le_bytes(word));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word.get_mut(..rest.len())
                .unwrap_or(&mut [])
                .copy_from_slice(rest);
            self.mix(u64::from_le_bytes(word) ^ (u64::try_from(rest.len()).unwrap_or(0) << 59));
        }
    }
}

pub type N<'t> = V<'t>;

/// The identity of a node: its address, stable while the tree is borrowed.
pub fn id<'t>(n: impl Into<V<'t>>) -> usize {
    let n: V<'t> = n.into();
    n.addr()
}

pub fn ty<'t>(n: impl Into<V<'t>>) -> &'t str {
    let n: V<'t> = n.into();
    match n {
        V::Node(node) => node.type_name(),
        _ => n.get("type").and_then(V::as_str).unwrap_or(""),
    }
}

pub fn is(n: Option<V<'_>>, t: &str) -> bool {
    n.is_some_and(|n| ty(n) == t)
}

pub fn children<'t>(n: impl Into<V<'t>>) -> Arr<'t> {
    let n: V<'t> = n.into();
    n.get("children")
        .and_then(V::as_array)
        .unwrap_or_else(Arr::empty)
}

pub fn list<'t>(n: impl Into<V<'t>>, field: &str) -> Option<Arr<'t>> {
    let n: V<'t> = n.into();
    n.get(field).and_then(V::as_array)
}

pub fn s<'t>(n: impl Into<V<'t>>, field: &str) -> Option<&'t str> {
    let n: V<'t> = n.into();
    n.get(field).and_then(V::as_str)
}

pub fn text<'t>(n: impl Into<V<'t>>) -> &'t str {
    let n: V<'t> = n.into();
    if let V::Node(crate::ast::Node::Text(t)) = n {
        return t.value;
    }
    s(n, "value").unwrap_or("")
}

pub fn flag<'t>(n: impl Into<V<'t>>, field: &str) -> bool {
    let n: V<'t> = n.into();
    n.get(field).and_then(V::as_bool).unwrap_or(false)
}

pub fn int<'t>(n: impl Into<V<'t>>, field: &str) -> Option<i64> {
    let n: V<'t> = n.into();
    n.get(field).and_then(V::as_i64)
}

pub fn has<'t>(n: impl Into<V<'t>>, field: &str) -> bool {
    let n: V<'t> = n.into();
    n.get(field).is_some_and(|v| !v.is_null())
}

pub fn caption<'t>(n: impl Into<V<'t>>) -> Option<V<'t>> {
    let n: V<'t> = n.into();
    n.get("caption").filter(|c| c.is_object())
}

pub fn title<'t>(n: impl Into<V<'t>>) -> Option<Arr<'t>> {
    let n: V<'t> = n.into();
    list(n, "title")
}

pub fn is_break(n: Option<V<'_>>) -> bool {
    n.is_some_and(|n| matches!(ty(n), "soft_break" | "hard_break"))
}

pub fn attrs<'t>(n: impl Into<V<'t>>) -> Option<V<'t>> {
    let n: V<'t> = n.into();
    n.get("attrs").filter(|a| a.is_object())
}

pub fn classes<'t>(n: impl Into<V<'t>>) -> Vec<&'t str> {
    let n: V<'t> = n.into();
    attrs(n)
        .and_then(|a| a.get("classes"))
        .and_then(V::as_array)
        .map(|c| c.iter().filter_map(V::as_str).collect())
        .unwrap_or_default()
}

pub fn key_values<'t>(n: impl Into<V<'t>>) -> Vec<(&'t str, &'t str)> {
    let n: V<'t> = n.into();
    attrs(n)
        .and_then(|a| a.get("keyValues"))
        .and_then(V::as_array)
        .map(|kv| {
            kv.iter()
                .filter_map(|p| {
                    let p = p.as_array()?;
                    Some((p.first()?.as_str()?, p.get(1)?.as_str()?))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A start and an end byte of a node, from `pos`.
pub fn span<'t>(n: impl Into<V<'t>>) -> Option<(usize, usize)> {
    let n: V<'t> = n.into();
    n.offsets()
}

pub fn pos_field<'t>(n: impl Into<V<'t>>, field: &str) -> Option<i64> {
    let n: V<'t> = n.into();
    n.pos()?.get(field)?.as_i64()
}

/// `serde_json::Value::pointer` on the view: RFC 6901, `~1` and `~0` unescaped, no leading zero in an index.
pub fn pointer<'t>(n: V<'t>, path: &str) -> Option<V<'t>> {
    if path.is_empty() {
        return Some(n);
    }
    let mut at = n;
    for part in path.strip_prefix('/')?.split('/') {
        let part = part.replace("~1", "/").replace("~0", "~");
        at = if let Some(items) = at.as_array() {
            if part.starts_with('+') || (part.starts_with('0') && part.len() != 1) {
                return None;
            }
            items.get(part.parse::<usize>().ok()?)?
        } else {
            at.get(&part)?
        };
    }
    Some(at)
}

pub fn is_inline_parent<'t>(n: impl Into<V<'t>>) -> bool {
    let n: V<'t> = n.into();
    matches!(
        ty(n),
        "paragraph"
            | "heading"
            | "definition_term"
            | "cell_paragraph"
            | "line"
            | "citation_definition"
            | "emphasis"
            | "strong"
            | "underline"
            | "strike"
            | "highlight"
            | "superscript"
            | "subscript"
            | "insert"
            | "delete"
            | "span"
            | "link"
            | "inline_footnote"
            | "inline_extension"
            | "small_caps"
    )
}

/// The slot paths of a node type, and whether one of them is the `title` alone.
struct TypeSlots {
    paths: Vec<Vec<String>>,
    title: bool,
}

fn slots_table() -> &'static TypeMap<TypeSlots> {
    static SLOTS: OnceLock<TypeMap<TypeSlots>> = OnceLock::new();
    SLOTS.get_or_init(|| {
        let mut out = TypeMap::default();
        for (name, slots) in crate::tables::NODE_SLOTS {
            let paths: Vec<Vec<String>> = slots
                .iter()
                .filter(|p| !p.starts_with("metadata."))
                .map(|p| p.split('.').map(str::to_string).collect())
                .collect();
            let title = paths.iter().any(|p| is_step(p, "title"));
            out.insert((*name).to_string(), TypeSlots { paths, title });
        }
        out
    })
}

fn visit_value<'t>(v: V<'t>, step: &str, f: &mut dyn FnMut(Arr<'t>, &str, bool)) {
    if let Some(items) = v.as_array() {
        f(items, step, false);
    } else if v.is_object() && v.get("type").is_some() {
        if let Some(one) = Arr::one(v) {
            f(one, step, true);
        }
    }
}

fn visit_path<'t>(v: V<'t>, parts: &[String], step: &str, f: &mut dyn FnMut(Arr<'t>, &str, bool)) {
    let Some((first, rest)) = parts.split_first() else {
        visit_value(v, step, f);
        return;
    };
    if first == "*" {
        if let Some(items) = v.as_array() {
            for (k, inner) in items.iter().enumerate() {
                visit_path(inner, rest, &join_step(step, &k.to_string()), f);
            }
        } else if v.is_object() {
            v.each(&mut |k, inner| visit_path(inner, rest, &join_step(step, k), f));
        }
        return;
    }
    if let Some(inner) = v.get(first.as_str()) {
        visit_path(inner, rest, &join_step(step, first), f);
    }
}

fn join_step<'a>(step: &str, name: &'a str) -> Cow<'a, str> {
    let key = escaped_key(name);
    if step.is_empty() {
        key
    } else {
        Cow::Owned(format!("{step}/{key}"))
    }
}

fn escaped_key(name: &str) -> Cow<'_, str> {
    if name.contains(['~', '/']) {
        Cow::Owned(name.replace('~', "~0").replace('/', "~1"))
    } else {
        Cow::Borrowed(name)
    }
}

/// C2-A-024: the slots of a node in their table order; in source order a title
/// precedes the other slots. `f` takes the nodes, the pointer step and whether
/// the slot holds one node.
fn is_step(path: &[String], name: &str) -> bool {
    matches!(path, [only] if only == name)
}

/// The slot paths of a node type, split at their dots, without the metadata slots.
pub fn slot_paths(ty: &str) -> &'static [Vec<String>] {
    slots_table()
        .get(ty)
        .map_or(&[], |slots| slots.paths.as_slice())
}

pub fn for_each_slot<'t>(n: V<'t>, source_order: bool, f: &mut dyn FnMut(Arr<'t>, &str, bool)) {
    let Some(slots) = slots_table().get(ty(n)) else {
        return;
    };
    if source_order && slots.title {
        if let Some(t) = n.get("title") {
            visit_value(t, "title", f);
        }
    }
    for p in &slots.paths {
        if source_order && slots.title && is_step(p, "title") {
            continue;
        }
        if let V::Node(node) = n
            && is_step(p, "children")
        {
            if let Some(kids) = node.children() {
                visit_value(V::Nodes(kids), "children", f);
            }
            continue;
        }
        visit_path(n, p, "", f);
    }
}

/// Every node below `n` in its slots, depth first.
pub fn descendants<'t>(n: V<'t>, out: &mut Vec<V<'t>>) {
    for_each_slot(n, false, &mut |nodes, _, _| {
        for c in nodes {
            out.push(c);
            descendants(c, out);
        }
    });
}

pub type Fields = Vec<(String, Option<String>)>;
type Results = TypeMap<Fields>;

fn results() -> &'static Results {
    static RESULTS: OnceLock<Results> = OnceLock::new();
    RESULTS.get_or_init(|| {
        let mut out = TypeMap::default();
        for (name, fields) in crate::tables::NODE_RESULTS {
            let fields = fields
                .iter()
                .map(|(field, when)| ((*field).to_string(), when.map(str::to_string)))
                .collect();
            out.insert((*name).to_string(), fields);
        }
        out
    })
}

/// The result fields of the node's type, `None` for a node without a known type.
pub fn result_fields(node: V<'_>) -> Option<&'static Fields> {
    let name = match node {
        V::Node(n) => n.type_name(),
        _ => node.get("type").and_then(V::as_str)?,
    };
    results().get(name)
}

/// C2-A-162: positions and resolution results are not compared. `fields` is
/// [`result_fields`] of `node`.
pub fn skipped_in(node: V<'_>, fields: Option<&Fields>, name: &str, span_marks: bool) -> bool {
    let Some(fields) = fields else {
        return false;
    };
    if matches!(name, "pos" | "segments" | "srcByteLength") {
        return true;
    }
    let Some((_, when)) = fields.iter().find(|(f, _)| f == name) else {
        return false;
    };
    match when.as_deref() {
        Some("reference use") => node.get("key").is_some(),
        Some("derived from authored span mark") => span_marks,
        _ => true,
    }
}

/// The compared fields of a node held on the stack; a node with more spills to a vector.
const FIELDS: usize = 12;

/// The compared fields of one node, in key order.
struct Compared<'t> {
    inline: [(&'t str, V<'t>); FIELDS],
    len: usize,
    spill: Vec<(&'t str, V<'t>)>,
}

impl<'t> Compared<'t> {
    fn of(node: V<'t>, span_marks: bool) -> Self {
        let mut out = Self {
            inline: [("", V::Null); FIELDS],
            len: 0,
            spill: Vec::new(),
        };
        let fields = result_fields(node);
        node.each(&mut |k, v| {
            if skipped_in(node, fields, k, span_marks) {
                return;
            }
            if let Some(slot) = out.inline.get_mut(out.len) {
                *slot = (k, v);
            } else {
                out.spill.push((k, v));
            }
            out.len += 1;
        });
        out
    }

    fn get(&self, at: usize) -> Option<&(&'t str, V<'t>)> {
        self.inline
            .get(..self.len.min(FIELDS))
            .and_then(|i| i.get(at))
            .or_else(|| self.spill.get(at.checked_sub(FIELDS)?))
    }
}

/// Whether two neighbors of `items` are text nodes, which C2-A-161 compares joined.
fn has_adjacent_text(items: Arr<'_>) -> bool {
    if let Arr::Json(values) = items {
        let is_text =
            |v: &Value| crate::view::json_field(v, "type").and_then(Value::as_str) == Some("text");
        return values
            .windows(2)
            .any(|w| matches!(w, [x, y] if is_text(x) && is_text(y)));
    }
    let mut last_text = false;
    for item in items {
        let is_text = ty(item) == "text";
        if is_text && last_text {
            return true;
        }
        last_text = is_text;
    }
    false
}

/// A node of a compared array: a text node can stand for neighbors joined, with their joined value.
#[derive(Clone)]
struct Item<'t> {
    node: V<'t>,
    joined: Option<String>,
}

/// Two text nodes whose other fields agree; only such runs join, so a field that differs is compared.
fn joinable_texts(last: V<'_>, next: V<'_>) -> bool {
    if ty(last) != "text" || ty(next) != "text" {
        return false;
    }
    let run = |k: &str| matches!(k, "type" | "value" | "pos" | "segments");
    let (mut own, mut other, mut same) = (0, 0, true);
    last.each(&mut |k, p| {
        if !run(k) {
            own += 1;
            same = same && next.get(k).is_some_and(|q| equal_values(p, q, false));
        }
    });
    next.each(&mut |k, _| other += usize::from(!run(k)));
    same && own == other
}

fn coalesced(items: Arr<'_>) -> Vec<Item<'_>> {
    let mut out: Vec<Item<'_>> = Vec::with_capacity(items.len());
    for item in items {
        if let Some(last) = out.last_mut()
            && joinable_texts(last.node, item)
        {
            let mut joined = last
                .joined
                .take()
                .unwrap_or_else(|| text(last.node).to_string());
            joined.push_str(text(item));
            last.joined = Some(joined);
            continue;
        }
        out.push(Item {
            node: item,
            joined: None,
        });
    }
    out
}

fn list_has_span_marks(table: V<'_>) -> bool {
    children(table).iter().any(|g| {
        children(g).iter().any(|r| {
            children(r)
                .iter()
                .any(|c| ty(c) == "table_cell" && has(c, "span"))
        })
    })
}

pub fn equal_scalars(a: V<'_>, b: V<'_>) -> bool {
    if let (Some(x), Some(y)) = (a.as_str(), b.as_str()) {
        return x == y;
    }
    if a.is_number() && b.is_number() {
        return a.as_f64() == b.as_f64();
    }
    if let (Some(x), Some(y)) = (a.as_bool(), b.as_bool()) {
        return x == y;
    }
    a.is_null() && b.is_null()
}

fn equal_arrays(x: Arr<'_>, y: Arr<'_>, span_marks: bool) -> bool {
    if !has_adjacent_text(x) && !has_adjacent_text(y) {
        return x.len() == y.len()
            && x.iter()
                .zip(y.iter())
                .all(|(l, r)| equal_values(l, r, span_marks));
    }
    let (x, y) = (coalesced(x), coalesced(y));
    x.len() == y.len()
        && x.iter().zip(&y).all(|(l, r)| {
            equal_objects(
                l.node,
                l.joined.as_deref(),
                r.node,
                r.joined.as_deref(),
                span_marks,
            )
        })
}

/// Two objects, where `lv` and `rv` replace the `value` field of a joined text node.
/// The `value` of a joined text node stands in for its own (C2-A-161).
fn equal_field(
    k: &str,
    p: V<'_>,
    q: V<'_>,
    joined: (Option<&str>, Option<&str>),
    span_marks: bool,
) -> bool {
    let (lv, rv) = joined;
    if k == "value" && (lv.is_some() || rv.is_some()) {
        let x = lv.or_else(|| p.as_str());
        return x.is_some() && x == rv.or_else(|| q.as_str());
    }
    equal_values(p, q, span_marks)
}

/// [`equal_objects`] of two typed nodes, their fields zipped; `None` for two node types.
fn equal_nodes(
    a: V<'_>,
    b: V<'_>,
    joined: (Option<&str>, Option<&str>),
    inner: bool,
) -> Option<bool> {
    let (V::Node(x), V::Node(y)) = (a, b) else {
        return None;
    };
    let fields = result_fields(a);
    let mut same = true;
    let zipped = x.view_zip(y, &mut |k, p, q| {
        if !same {
            return;
        }
        let p = p.filter(|_| !skipped_in(a, fields, k, inner));
        let q = q.filter(|_| !skipped_in(b, fields, k, inner));
        same = match (p, q) {
            (None, None) => true,
            (Some(p), Some(q)) => equal_field(k, p, q, joined, inner),
            _ => false,
        };
    });
    zipped.then_some(same)
}

/// [`equal_objects`] of a JSON node and a typed node of the same type, the two
/// sorted field lists walked side by side; `None` for any other pair.
fn equal_json_node(
    a: V<'_>,
    b: V<'_>,
    joined: (Option<&str>, Option<&str>),
    span_marks: bool,
) -> Option<bool> {
    let (V::Json(value @ Value::Object(map)), V::Node(node)) = (a, b) else {
        return None;
    };
    let name = node.type_name();
    if crate::view::json_field(value, "type").and_then(Value::as_str) != Some(name) {
        return None;
    }
    let inner = if name == "table" {
        list_has_span_marks(a) || list_has_span_marks(b)
    } else {
        span_marks
    };
    let fields = results().get(name);
    let mut left = map.iter().filter(|(k, _)| !skipped_in(a, fields, k, inner));
    let mut same = true;
    node.view_each(&mut |kr, q| {
        if !same || skipped_in(b, fields, kr, inner) {
            return;
        }
        same = match left.next() {
            Some((kl, p)) => kl == kr && equal_field(kl, V::Json(p), q, joined, inner),
            None => false,
        };
    });
    Some(same && left.next().is_none())
}

fn equal_objects(a: V<'_>, lv: Option<&str>, b: V<'_>, rv: Option<&str>, span_marks: bool) -> bool {
    if !(a.is_object() && b.is_object()) {
        return lv.is_none() && rv.is_none() && equal_values(a, b, span_marks);
    }
    if let (V::Json(_), V::Node(_)) = (a, b)
        && let Some(same) = equal_json_node(a, b, (lv, rv), span_marks)
    {
        return same;
    }
    let inner = if ty(a) == "table" {
        list_has_span_marks(a) || list_has_span_marks(b)
    } else {
        span_marks
    };
    if let Some(same) = equal_nodes(a, b, (lv, rv), inner) {
        return same;
    }
    let l = Compared::of(a, inner);
    let fields = result_fields(b);
    let mut at = 0;
    let mut same = true;
    b.each(&mut |kr, q| {
        if !same || skipped_in(b, fields, kr, inner) {
            return;
        }
        let Some(&(kl, p)) = l.get(at) else {
            same = false;
            return;
        };
        at += 1;
        same = kl == kr && equal_field(kl, p, q, (lv, rv), inner);
    });
    same && at == l.len
}

fn equal_values(a: V<'_>, b: V<'_>, span_marks: bool) -> bool {
    // The JSON side of an imported tree, read without the view's dispatch.
    if let V::Json(p) = a {
        match p {
            Value::String(x) => return b.as_str() == Some(x.as_str()),
            Value::Bool(x) => return b.as_bool() == Some(*x),
            Value::Array(x) => {
                return b
                    .as_array()
                    .is_some_and(|y| equal_arrays(Arr::Json(x), y, span_marks));
            }
            Value::Object(_) => {
                if let V::Node(_) = b
                    && let Some(same) = equal_json_node(a, b, (None, None), span_marks)
                {
                    return same;
                }
            }
            _ => {}
        }
    }
    match (a.as_array(), b.as_array()) {
        (Some(x), Some(y)) => return equal_arrays(x, y, span_marks),
        (Some(_), None) | (None, Some(_)) => return false,
        (None, None) => {}
    }
    if a.is_object() || b.is_object() {
        return a.is_object() && b.is_object() && equal_objects(a, None, b, None, span_marks);
    }
    equal_scalars(a, b)
}

/// C2-A-161 on two node lists, as the children of two paragraphs compare.
pub fn lists_equal(a: Arr<'_>, b: Arr<'_>) -> bool {
    equal_arrays(a, b, false)
}

/// C2-A-161: whether two trees are the same document.
pub fn trees_equal<'a, 'b>(a: impl Into<V<'a>>, b: impl Into<V<'b>>) -> bool {
    equal_values(a.into(), b.into(), false)
}

/// `n` without the standard data namespaces (C2-A-193).
pub fn without_standard_data(n: V<'_>) -> Value {
    let mut copy = n.to_value();
    strip_data(&mut copy);
    copy
}

fn strip_data(v: &mut Value) {
    match v {
        Value::Object(m) => {
            if let Some(Value::Object(d)) = m.get_mut("data") {
                d.retain(|k, _| {
                    !matches!(
                        k.as_str(),
                        "carve.source" | "carve.style" | "carve.annotation"
                    )
                });
                if d.is_empty() {
                    m.remove("data");
                }
            }
            m.values_mut().for_each(strip_data);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_data),
        _ => {}
    }
}

/// The line endings an inline node spells: its breaks and the newlines of the values below it, as [`plain_text`]
/// writes them.
pub fn newlines(n: V<'_>) -> usize {
    match ty(n) {
        "text" | "code" | "literal_inline" | "math" | "raw_inline" | "smart_punctuation" => {
            s(n, "value")
                .or_else(|| s(n, "text"))
                .map_or(0, |v| v.bytes().filter(|&b| b == b'\n').count())
        }
        "soft_break" | "hard_break" => 1,
        _ => children(n).iter().map(newlines).sum(),
    }
}

/// The plain text of inline nodes (C2-B-120).
pub fn plain_text(nodes: Arr<'_>) -> String {
    let mut out = String::new();
    for n in nodes {
        match ty(n) {
            "text" | "code" | "literal_inline" | "math" | "raw_inline" | "smart_punctuation" => {
                out.push_str(s(n, "value").or_else(|| s(n, "text")).unwrap_or(""));
            }
            "soft_break" | "hard_break" => out.push('\n'),
            "non_breaking_space" => out.push('\u{a0}'),
            _ => out.push_str(&plain_text(children(n))),
        }
    }
    out
}

/// The JSON pointer of each wanted node in the document, by identity.
pub fn pointers(doc: V<'_>, wanted: &AddrSet<usize>) -> AddrMap<usize, String> {
    let mut found = AddrMap::default();
    if wanted.is_empty() {
        return found;
    }
    let mut stack: Vec<(V<'_>, String)> = vec![(doc, String::new())];
    while let Some((n, path)) = stack.pop() {
        if wanted.contains(&id(n)) {
            found.insert(id(n), path.clone());
            if found.len() == wanted.len() {
                break;
            }
        }
        for_each_slot(n, false, &mut |nodes, step, single| {
            if single {
                if let Some(c) = nodes.first() {
                    stack.push((c, format!("{path}/{step}")));
                }
            } else {
                for (i, c) in nodes.iter().enumerate() {
                    stack.push((c, format!("{path}/{step}/{i}")));
                }
            }
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::trees_equal;
    use serde_json::{Value, json};

    fn paragraph(children: &[Value]) -> Value {
        json!({ "type": "paragraph", "children": children })
    }

    #[test]
    fn adjacent_texts_join_only_where_their_other_fields_agree() {
        let with = |v: &str, lang: &str| json!({ "type": "text", "value": v, "lang": lang });
        let text = |v: &str| json!({ "type": "text", "value": v });
        let one = paragraph(&[with("ab", "de")]);
        assert!(trees_equal(
            &paragraph(&[with("a", "de"), with("b", "de")]),
            &one
        ));
        assert!(!trees_equal(
            &paragraph(&[with("a", "de"), with("b", "fr")]),
            &one
        ));
        assert!(!trees_equal(
            &paragraph(&[with("a", "de"), text("b")]),
            &one
        ));
        let null = json!({ "type": "text", "value": "b", "lang": null });
        assert!(!trees_equal(
            &paragraph(&[text("a"), null]),
            &paragraph(&[text("ab")])
        ));
        assert!(trees_equal(
            &paragraph(&[text("a"), text("b")]),
            &paragraph(&[text("ab")])
        ));
    }
}
