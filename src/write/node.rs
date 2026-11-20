//! The tree as the writer reads it: JSON values of the owned 2.0 tree, with
//! node identity by address, the slots of `data/node-slots.json` and the tree
//! equality of C2-A-161 (positions and the results of `data/ast-results.json`
//! are not compared).

use crate::hash::AddrHasher;
use crate::tree::view::{AsView, View};
use serde_json::{Map, Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::sync::OnceLock;

/// The tables keyed by node type: fixed spec data, so the default hasher's
/// protection buys nothing.
type TypeMap<V> = HashMap<String, V, BuildHasherDefault<AddrHasher>>;

pub type N<'t> = &'t Value;

/// The identity of a node: its address, stable while the tree is borrowed.
pub fn id<'t, A: AsView<'t>>(n: A) -> usize {
    n.view().id()
}

pub fn ty<'t, A: AsView<'t>>(n: A) -> &'t str {
    n.view().ty()
}

pub fn is<'t, A: AsView<'t>>(n: Option<A>, t: &str) -> bool {
    n.is_some_and(|n| n.view().ty() == t)
}

pub fn children<'t, A: AsView<'t>>(n: A) -> <A::V as View<'t>>::List {
    n.view().children()
}

pub fn list<'t, A: AsView<'t>>(n: A, field: &str) -> Option<<A::V as View<'t>>::List> {
    n.view().list(field)
}

pub fn s<'t, A: AsView<'t>>(n: A, field: &str) -> Option<&'t str> {
    n.view().s(field)
}

pub fn text<'t, A: AsView<'t>>(n: A) -> &'t str {
    n.view().s("value").unwrap_or("")
}

pub fn flag<'t, A: AsView<'t>>(n: A, field: &str) -> bool {
    n.view().flag(field)
}

pub fn int<'t, A: AsView<'t>>(n: A, field: &str) -> Option<i64> {
    n.view().int(field)
}

pub fn has<'t, A: AsView<'t>>(n: A, field: &str) -> bool {
    n.view().has(field)
}

pub fn caption<'t, A: AsView<'t>>(n: A) -> Option<A::V> {
    n.view().node("caption")
}

pub fn title<'t, A: AsView<'t>>(n: A) -> Option<<A::V as View<'t>>::List> {
    n.view().list("title")
}

pub fn is_break<'t, A: AsView<'t>>(n: Option<A>) -> bool {
    n.is_some_and(|n| matches!(n.view().ty(), "soft_break" | "hard_break"))
}

pub fn attrs(n: &Value) -> Option<&Map<String, Value>> {
    n.get("attrs").and_then(Value::as_object)
}

pub fn classes<'t, A: AsView<'t>>(n: A) -> Vec<&'t str> {
    n.view().classes()
}

pub fn key_values<'t, A: AsView<'t>>(n: A) -> Vec<(&'t str, &'t str)> {
    n.view().key_values()
}

/// A start and an end byte of a node, from `pos`.
pub fn span<'t, A: AsView<'t>>(n: A) -> Option<(usize, usize)> {
    n.view().span()
}

pub fn pos_field<'t, A: AsView<'t>>(n: A, field: &str) -> Option<i64> {
    n.view().pos_field(field)
}

pub const INLINE_PARENTS: &[&str] = &[
    "paragraph",
    "heading",
    "definition_term",
    "cell_paragraph",
    "line",
    "citation_definition",
    "emphasis",
    "strong",
    "underline",
    "strike",
    "highlight",
    "superscript",
    "subscript",
    "insert",
    "delete",
    "span",
    "link",
    "inline_footnote",
    "inline_extension",
    "small_caps",
];

pub fn is_inline_parent<'t, A: AsView<'t>>(n: A) -> bool {
    INLINE_PARENTS.contains(&n.view().ty())
}

fn slots_table() -> &'static TypeMap<Vec<Vec<String>>> {
    static SLOTS: OnceLock<TypeMap<Vec<Vec<String>>>> = OnceLock::new();
    SLOTS.get_or_init(|| {
        let data: Value = serde_json::from_str(include_str!("../../spec-data/node-slots.json"))
            .unwrap_or_default();
        let mut out = TypeMap::default();
        if let Some(nodes) = data.get("nodes").and_then(Value::as_object) {
            for (name, slots) in nodes {
                let paths = slots
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .filter(|p| !p.starts_with("metadata."))
                    .map(|p| p.split('.').map(str::to_string).collect())
                    .collect();
                out.insert(name.clone(), paths);
            }
        }
        out
    })
}

fn visit_value<'t>(v: &'t Value, step: &str, f: &mut dyn FnMut(&'t [Value], &str, bool)) {
    match v {
        Value::Array(items) => f(items, step, false),
        Value::Object(_) if v.get("type").is_some() => f(std::slice::from_ref(v), step, true),
        _ => {}
    }
}

fn visit_path<'t>(
    v: &'t Value,
    parts: &[String],
    step: &str,
    f: &mut dyn FnMut(&'t [Value], &str, bool),
) {
    let Some((first, rest)) = parts.split_first() else {
        visit_value(v, step, f);
        return;
    };
    if first == "*" {
        match v {
            Value::Object(m) => {
                for (k, inner) in m {
                    visit_path(inner, rest, &join_step(step, k), f);
                }
            }
            Value::Array(items) => {
                for (k, inner) in items.iter().enumerate() {
                    visit_path(inner, rest, &join_step(step, &k.to_string()), f);
                }
            }
            _ => {}
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
pub fn for_each_slot<'t>(
    n: &'t Value,
    source_order: bool,
    f: &mut dyn FnMut(&'t [Value], &str, bool),
) {
    let Some(paths) = slots_table().get(ty(n)) else {
        return;
    };
    if source_order
        && paths
            .iter()
            .any(|p| p.len() == 1 && p.first().is_some_and(|x| x == "title"))
    {
        if let Some(t) = n.get("title") {
            visit_value(t, "title", f);
        }
    }
    for p in paths {
        if source_order && p.len() == 1 && p.first().is_some_and(|x| x == "title") {
            continue;
        }
        visit_path(n, p, "", f);
    }
}

/// Every node below `n` in its slots, depth first.
pub fn descendants<'t>(n: &'t Value, out: &mut Vec<&'t Value>) {
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
        let data: Value = serde_json::from_str(include_str!("../../spec-data/ast-results.json"))
            .unwrap_or_default();
        let mut out = TypeMap::default();
        if let Some(nodes) = data.get("nodes").and_then(Value::as_object) {
            for (name, fields) in nodes {
                let fields = fields
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|f| {
                        Some((
                            f.get("field")?.as_str()?.to_string(),
                            f.get("when").and_then(Value::as_str).map(str::to_string),
                        ))
                    })
                    .collect();
                out.insert(name.clone(), fields);
            }
        }
        out
    })
}

/// The result fields of the node's type, `None` for a node without a known type.
pub fn result_fields(node: &Map<String, Value>) -> Option<&'static Fields> {
    results().get(node.get("type").and_then(Value::as_str)?)
}

/// C2-A-162: positions and resolution results are not compared. `fields` is
/// [`result_fields`] of `node`.
pub fn skipped_in(
    node: &Map<String, Value>,
    fields: Option<&Fields>,
    name: &str,
    span_marks: bool,
) -> bool {
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

/// The compared fields of `node`, in key order.
fn compared(
    node: &Map<String, Value>,
    span_marks: bool,
) -> impl Iterator<Item = (&String, &Value)> {
    let fields = result_fields(node);
    node.iter()
        .filter(move |(k, _)| !skipped_in(node, fields, k, span_marks))
}

/// Whether two neighbors of `items` are text nodes, which C2-A-161 compares joined.
fn has_adjacent_text(items: &[Value]) -> bool {
    items
        .windows(2)
        .any(|p| matches!(p, [a, b] if ty(a) == "text" && ty(b) == "text"))
}

fn coalesced(items: &[Value]) -> Vec<std::borrow::Cow<'_, Value>> {
    let mut out: Vec<std::borrow::Cow<'_, Value>> = Vec::with_capacity(items.len());
    for item in items {
        if ty(item) == "text" {
            if let Some(last) = out.last_mut() {
                if ty(&**last) == "text" {
                    let mut merged = last.clone().into_owned();
                    let joined = format!("{}{}", text(&merged), text(item));
                    if let Some(m) = merged.as_object_mut() {
                        m.insert("value".into(), Value::String(joined));
                    }
                    *last = std::borrow::Cow::Owned(merged);
                    continue;
                }
            }
        }
        out.push(std::borrow::Cow::Borrowed(item));
    }
    out
}

fn list_has_span_marks(table: &Value) -> bool {
    children(table).iter().any(|g| {
        children(g).iter().any(|r| {
            children(r)
                .iter()
                .any(|c| ty(c) == "table_cell" && has(c, "span"))
        })
    })
}

fn equal_values(a: &Value, b: &Value, span_marks: bool) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            if !has_adjacent_text(x) && !has_adjacent_text(y) {
                return x.len() == y.len()
                    && x.iter().zip(y).all(|(l, r)| equal_values(l, r, span_marks));
            }
            let (x, y) = (coalesced(x), coalesced(y));
            x.len() == y.len()
                && x.iter()
                    .zip(&y)
                    .all(|(l, r)| equal_values(l, r, span_marks))
        }
        (Value::Object(x), Value::Object(y)) => {
            let inner = if x.get("type").and_then(Value::as_str) == Some("table") {
                list_has_span_marks(a) || list_has_span_marks(b)
            } else {
                span_marks
            };
            let (mut l, mut r) = (compared(x, inner), compared(y, inner));
            loop {
                match (l.next(), r.next()) {
                    (None, None) => return true,
                    (Some((kl, p)), Some((kr, q))) if kl == kr => {
                        if !equal_values(p, q, inner) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
        }
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// C2-A-161: whether two trees are the same document.
pub fn trees_equal(a: &Value, b: &Value) -> bool {
    equal_values(a, b, false)
}

/// `n` without the standard data namespaces (C2-A-193).
pub fn without_standard_data(n: &Value) -> Value {
    let mut copy = n.clone();
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

/// The plain text of inline nodes (C2-B-120).
pub fn plain_text(nodes: &[Value]) -> String {
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
pub fn pointers(doc: &Value, wanted: &std::collections::HashSet<usize>) -> HashMap<usize, String> {
    let mut found = HashMap::new();
    if wanted.is_empty() {
        return found;
    }
    let mut stack: Vec<(&Value, String)> = vec![(doc, String::new())];
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
