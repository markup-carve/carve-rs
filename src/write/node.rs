//! The tree as the writer reads it: JSON values of the owned 2.0 tree, with
//! node identity by address, the slots of `data/node-slots.json` and the tree
//! equality of C2-A-161 (positions and the results of `data/ast-results.json`
//! are not compared).

use crate::hash::AddrHasher;
use crate::tree::view::{AsView, Field, NodeList, View};
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

fn visit_value<'t, V: View<'t>>(
    v: Field<'t, V>,
    step: &str,
    f: &mut dyn FnMut(V::List, &str, bool),
) {
    match v {
        Field::Nodes(items) => f(items, step, false),
        Field::Node(n) => f(n.one(), step, true),
        _ => {}
    }
}

fn visit_path<'t, V: View<'t>>(
    v: Field<'t, V>,
    parts: &[String],
    step: &str,
    f: &mut dyn FnMut(V::List, &str, bool),
) {
    let Some((first, rest)) = parts.split_first() else {
        visit_value(v, step, f);
        return;
    };
    let object = match v {
        Field::Node(n) => Some(n),
        Field::Json(j) if j.is_object() => Some(V::json(j)),
        _ => None,
    };
    if first == "*" {
        if let Some(o) = object {
            for (k, inner) in o.fields() {
                visit_path(inner, rest, &join_step(step, k), f);
            }
        } else if let Field::Nodes(items) = v {
            for k in 0..items.len() {
                if let Some(inner) = items.get(k) {
                    visit_path(inner.as_field(), rest, &join_step(step, &k.to_string()), f);
                }
            }
        }
        return;
    }
    if let Some(o) = object {
        let inner = o.field(first);
        if !matches!(inner, Field::Absent) {
            visit_path(inner, rest, &join_step(step, first), f);
        }
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
#[allow(clippy::type_complexity)]
pub fn for_each_slot<'t, A: AsView<'t>>(
    n: A,
    source_order: bool,
    f: &mut dyn FnMut(<A::V as View<'t>>::List, &str, bool),
) {
    let n = n.view();
    let Some(paths) = slots_table().get(n.ty()) else {
        return;
    };
    if source_order
        && paths
            .iter()
            .any(|p| p.len() == 1 && p.first().is_some_and(|x| x == "title"))
    {
        visit_value(n.field("title"), "title", f);
    }
    for p in paths {
        if source_order && p.len() == 1 && p.first().is_some_and(|x| x == "title") {
            continue;
        }
        visit_path(Field::Node(n), p, "", f);
    }
}

/// Every node below `n` in its slots, depth first.
pub fn descendants<'t, V: View<'t>>(n: V, out: &mut Vec<V>) {
    for_each_slot(n, false, &mut |nodes, _, _| {
        for i in 0..nodes.len() {
            if let Some(c) = nodes.get(i) {
                out.push(c);
                descendants(c, out);
            }
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

/// C2-A-162 for a node of either side: whether field `name` of `node` is
/// not compared. `fields` is the result table of the node's type.
fn skipped<'t, V: View<'t>>(
    node: V,
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
        Some("reference use") => !matches!(node.field("key"), Field::Absent),
        Some("derived from authored span mark") => span_marks,
        _ => true,
    }
}

/// Whether two neighbors of `items` are text nodes, which C2-A-161 compares joined.
fn has_adjacent_text<'t, V: View<'t>>(items: V::List) -> bool {
    (1..items.len()).any(|i| {
        matches!((items.get(i - 1), items.get(i)), (Some(a), Some(b)) if a.ty() == "text" && b.ty() == "text")
    })
}

/// An item of a list as C2-A-161 compares it: a node, or a run of text
/// nodes as its first node with the joined value.
struct Item<'t, V> {
    node: V,
    joined: Option<String>,
    _t: std::marker::PhantomData<&'t ()>,
}

fn coalesced<'t, V: View<'t>>(items: V::List) -> Vec<Item<'t, V>> {
    let mut out: Vec<Item<'t, V>> = Vec::with_capacity(items.len());
    for i in 0..items.len() {
        let Some(item) = items.get(i) else { continue };
        if item.ty() == "text" {
            if let Some(last) = out.last_mut().filter(|l| l.node.ty() == "text") {
                let joined = last
                    .joined
                    .get_or_insert_with(|| text(last.node).to_string());
                joined.push_str(text(item));
                continue;
            }
        }
        out.push(Item {
            node: item,
            joined: None,
            _t: std::marker::PhantomData,
        });
    }
    out
}

fn list_has_span_marks<'t, V: View<'t>>(table: V) -> bool {
    let groups = table.children();
    (0..groups.len()).filter_map(|i| groups.get(i)).any(|g| {
        let rows = g.children();
        (0..rows.len()).filter_map(|i| rows.get(i)).any(|r| {
            let cells = r.children();
            (0..cells.len())
                .filter_map(|i| cells.get(i))
                .any(|c| c.ty() == "table_cell" && c.has("span"))
        })
    })
}

fn equal_lists<'t, V: View<'t>>(x: V::List, y: V::List, span_marks: bool) -> bool {
    if !has_adjacent_text::<V>(x) && !has_adjacent_text::<V>(y) {
        return x.len() == y.len()
            && (0..x.len()).all(|i| match (x.get(i), y.get(i)) {
                (Some(l), Some(r)) => equal_fields(l.as_field(), r.as_field(), span_marks),
                _ => false,
            });
    }
    let (x, y) = (coalesced::<V>(x), coalesced::<V>(y));
    x.len() == y.len()
        && x.iter().zip(&y).all(|(l, r)| {
            if l.joined.is_none() && r.joined.is_none() {
                equal_fields(l.node.as_field(), r.node.as_field(), span_marks)
            } else {
                equal_nodes(
                    l.node,
                    l.joined.as_deref(),
                    r.node,
                    r.joined.as_deref(),
                    span_marks,
                )
            }
        })
}

fn equal_nodes<'t, V: View<'t>>(
    a: V,
    ja: Option<&str>,
    b: V,
    jb: Option<&str>,
    span_marks: bool,
) -> bool {
    let inner = if a.ty() == "table" {
        list_has_span_marks(a) || list_has_span_marks(b)
    } else {
        span_marks
    };
    let (fa, fb) = (results().get(a.ty()), results().get(b.ty()));
    let mut l = a.fields().filter(|(k, _)| !skipped(a, fa, k, inner));
    let mut r = b.fields().filter(|(k, _)| !skipped(b, fb, k, inner));
    loop {
        match (l.next(), r.next()) {
            (None, None) => return true,
            (Some((kl, p)), Some((kr, q))) if kl == kr => {
                let same = if kl == "value" && (ja.is_some() || jb.is_some()) {
                    let left = ja.or_else(|| text_of(p));
                    left.is_some() && left == jb.or_else(|| text_of(q))
                } else {
                    equal_fields(p, q, inner)
                };
                if !same {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

fn text_of<'t, V: View<'t>>(f: Field<'t, V>) -> Option<&'t str> {
    match f {
        Field::Str(s) => Some(s),
        _ => None,
    }
}

fn equal_fields<'t, V: View<'t>>(a: Field<'t, V>, b: Field<'t, V>, span_marks: bool) -> bool {
    match (a, b) {
        (Field::Nodes(x), Field::Nodes(y)) => equal_lists::<V>(x, y, span_marks),
        (Field::Node(x), Field::Node(y)) => equal_nodes(x, None, y, None, span_marks),
        (Field::Json(x), Field::Json(y)) => {
            equal_nodes(V::json(x), None, V::json(y), None, span_marks)
        }
        (Field::Str(x), Field::Str(y)) => x == y,
        (Field::Int(_) | Field::Num(_), Field::Int(_) | Field::Num(_)) => number(a) == number(b),
        (Field::Bool(x), Field::Bool(y)) => x == y,
        (Field::Null, Field::Null) => true,
        (Field::Strings(x), Field::Strings(y)) => x == y,
        (Field::Pos(x), Field::Pos(y)) => x == y,
        (Field::Segments(x), Field::Segments(y)) => x == y,
        (Field::Attrs(x), Field::Attrs(y)) => x == y,
        (Field::Marker(x), Field::Marker(y)) => x == y,
        (Field::Task(x), Field::Task(y)) => x == y,
        _ => false,
    }
}

/// C2-A-161 compares numbers by value, as JSON reads them.
#[allow(clippy::cast_precision_loss)]
fn number<'t, V: View<'t>>(f: Field<'t, V>) -> Option<f64> {
    match f {
        Field::Int(n) => Some(n as f64),
        Field::Num(n) => Some(n),
        _ => None,
    }
}

/// C2-A-161: whether two trees are the same document.
pub fn trees_equal<'t, A: AsView<'t>, B: AsView<'t, V = A::V>>(a: A, b: B) -> bool {
    equal_fields(a.view().as_field(), b.view().as_field(), false)
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
pub fn pointers<'t, V: View<'t>>(
    doc: V,
    wanted: &std::collections::HashSet<usize>,
) -> HashMap<usize, String> {
    let mut found = HashMap::new();
    if wanted.is_empty() {
        return found;
    }
    let mut stack: Vec<(V, String)> = vec![(doc, String::new())];
    while let Some((n, path)) = stack.pop() {
        if wanted.contains(&n.id()) {
            found.insert(n.id(), path.clone());
            if found.len() == wanted.len() {
                break;
            }
        }
        for_each_slot(n, false, &mut |nodes, step, single| {
            if single {
                if let Some(c) = nodes.get(0) {
                    stack.push((c, format!("{path}/{step}")));
                }
            } else {
                for i in 0..nodes.len() {
                    if let Some(c) = nodes.get(i) {
                        stack.push((c, format!("{path}/{step}/{i}")));
                    }
                }
            }
        });
    }
    found
}

#[cfg(test)]
mod tests {
    use super::{descendants, id, pointers, trees_equal};
    use crate::tree::view::Typed;

    fn parse(input: &str) -> Option<crate::api::Tree> {
        crate::api::parse(input, &crate::api::Options::default()).ok()
    }

    #[test]
    fn trees_compare_alike_on_both_sides() {
        let inputs = crate::tree::view::tests::inputs();
        let host = crate::api::Carve::new();
        let opts = crate::api::WriteOptions::default();
        let trees: Vec<_> = inputs.iter().map(|i| parse(i)).collect();
        let (mut equal, mut differ) = (0, 0);
        for (i, a) in trees.iter().enumerate() {
            let Some(a) = a else { continue };
            let va = a.value().unwrap_or_default();
            let formatted = host
                .format(inputs.get(i).map_or("", String::as_str), &opts)
                .ok()
                .and_then(|f| parse(&f));
            for b in [
                Some(a),
                trees.get(i + 1).and_then(Option::as_ref),
                formatted.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                let vb = b.value().unwrap_or_default();
                let by_value = trees_equal(&va, &vb);
                let by_type = trees_equal(Typed::Doc(a.document()), Typed::Doc(b.document()));
                assert_eq!(by_value, by_type, "input {i}");
                if by_value {
                    equal += 1;
                } else {
                    differ += 1;
                }
            }
        }
        assert!(
            equal > 1000 && differ > 1000,
            "{equal} equal, {differ} differ"
        );
    }

    #[test]
    fn slots_and_pointers_read_alike_on_both_sides() {
        for (i, input) in crate::tree::view::tests::inputs().iter().enumerate() {
            let Some(tree) = parse(input) else { continue };
            let value = tree.value().unwrap_or_default();
            let mut by_value = vec![&value];
            descendants(&value, &mut by_value);
            let mut by_type = vec![Typed::Doc(tree.document())];
            descendants(Typed::Doc(tree.document()), &mut by_type);
            assert_eq!(by_value.len(), by_type.len(), "input {i}");
            let ids = |all: &[usize]| all.iter().copied().collect();
            let pv = pointers(
                &value,
                &ids(&by_value.iter().map(|n| id(*n)).collect::<Vec<_>>()),
            );
            let pt = pointers(
                Typed::Doc(tree.document()),
                &ids(&by_type.iter().map(|n| id(*n)).collect::<Vec<_>>()),
            );
            let order = |all: &[usize], p: &std::collections::HashMap<usize, String>| {
                all.iter().map(|n| p.get(n).cloned()).collect::<Vec<_>>()
            };
            assert_eq!(
                order(&by_value.iter().map(|n| id(*n)).collect::<Vec<_>>(), &pv),
                order(&by_type.iter().map(|n| id(*n)).collect::<Vec<_>>(), &pt),
                "input {i}"
            );
        }
    }
}
