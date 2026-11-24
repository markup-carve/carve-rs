//! Shared document-view operations for glossary, index and citations.

use super::{classes, key_values, with_attrs};
use crate::api::{Error, RenderContext, Tree};
use serde_json::{Value, json};
use std::collections::HashSet;

pub(super) fn ty(node: &Value) -> &str {
    node.get("type").and_then(Value::as_str).unwrap_or("")
}

pub(super) fn children(node: &Value) -> &[Value] {
    node.get("children")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

pub(super) fn plain(nodes: &[Value]) -> String {
    super::super::standard::plain_text(nodes)
}

/// A mutable document-order visit. A false result leaves the visited subtree
/// alone, including a newly replaced node. Fallback and metadata are inert.
pub(super) fn walk(view: &mut Value, visit: &mut impl FnMut(&mut Value) -> bool) {
    fn descend(node: &mut Value, visit: &mut impl FnMut(&mut Value) -> bool, depth: usize) {
        if depth > crate::resolve::WALK_CAP {
            return;
        }
        match node {
            Value::Array(nodes) => {
                for child in nodes {
                    descend(child, visit, depth + 1);
                }
            }
            Value::Object(map) => {
                if map.contains_key("type") {
                    let mut whole = Value::Object(std::mem::take(map));
                    let go = visit(&mut whole);
                    let Value::Object(next) = whole else {
                        return;
                    };
                    *map = next;
                    if !go {
                        return;
                    }
                }
                for (key, child) in map.iter_mut() {
                    if !matches!(
                        key.as_str(),
                        "fallback" | "source" | "payload" | "props" | "pos" | "attrs" | "data"
                    ) {
                        descend(child, visit, depth + 1);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(body) = view.get_mut("children") {
        descend(body, visit, 0);
    }
}

pub(super) fn view_ids(view: &Value) -> HashSet<String> {
    let mut ids = HashSet::new();
    let mut copy = view.clone();
    walk(&mut copy, &mut |node| {
        if let Some(id) = node.pointer("/attrs/id").and_then(Value::as_str) {
            ids.insert(id.to_string());
        }
        if ty(node) == "heading" {
            if let Some(id) = node.get("generatedId").and_then(Value::as_str) {
                ids.insert(id.to_string());
            }
        }
        true
    });
    ids
}

pub(super) fn fresh_id(base: &str, taken: &mut HashSet<String>) -> String {
    let mut id = base.to_string();
    let mut n = 2;
    while taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    taken.insert(id.clone());
    id
}

pub(super) fn attr<'a>(node: &'a Value, key: &str) -> Option<&'a str> {
    node.pointer("/attrs/keyValues")
        .and_then(Value::as_array)?
        .iter()
        .rev()
        .find_map(|pair| {
            (pair.get(0).and_then(Value::as_str) == Some(key))
                .then(|| pair.get(1).and_then(Value::as_str))
                .flatten()
        })
}

pub(super) fn core_nodes(nodes: &[Value]) -> Vec<Value> {
    fn strip(node: &Value) -> Vec<Value> {
        if ty(node) == "block_extension" {
            return core_nodes(
                node.get("fallback")
                    .and_then(Value::as_array)
                    .map_or(&[], Vec::as_slice),
            );
        }
        if ty(node) == "inline_extension" {
            if node
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.contains('.'))
            {
                return core_nodes(
                    node.get("fallback")
                        .and_then(Value::as_array)
                        .map_or(&[], Vec::as_slice),
                );
            }
            let name = node.get("name").and_then(Value::as_str).unwrap_or("");
            return vec![
                json!({"type": "span", "attrs": {"classes": [format!("ext-{name}")]},
                "children": core_nodes(children(node))}),
            ];
        }
        let mut copy = node.clone();
        if let Some(body) = copy.get_mut("children").and_then(Value::as_array_mut) {
            *body = core_nodes(body);
        }
        if let Some(slots) = copy.get_mut("slots") {
            if let Some(map) = slots.as_object_mut() {
                for part in map.values_mut() {
                    if let Some(list) = part.as_array_mut() {
                        *list = core_nodes(list);
                    }
                }
            }
        }
        vec![copy]
    }
    nodes.iter().flat_map(strip).collect()
}

pub(super) fn claim_directive(name: &str, authored: &Value, body: &[Value]) -> Value {
    let mut node = json!({"type": "block_extension", "name": name, "version": "1",
        "children": [], "slots": {"body": body}, "fallback": core_nodes(std::slice::from_ref(authored))});
    if let Some(pos) = authored.get("pos") {
        if let Some(map) = node.as_object_mut() {
            map.insert("pos".into(), pos.clone());
        }
    }
    node
}

pub(super) fn inline_attrs(
    node: &Value,
    cx: &RenderContext<'_>,
    base: &str,
    drop: &[&str],
) -> String {
    let attrs = node.get("attrs");
    let mut names = vec![Value::from(base)];
    names.extend(classes(attrs));
    let pairs = key_values(attrs)
        .into_iter()
        .filter(|pair| {
            !pair
                .get(0)
                .and_then(Value::as_str)
                .is_some_and(|name| drop.contains(&name))
        })
        .collect();
    cx.attributes(&with_attrs(node, names, pairs))
}

pub(super) fn view(view: &Tree) -> Result<Value, Error> {
    view.value()
}

pub(super) fn finish(view: &Tree, doc: Value) -> Result<Tree, Error> {
    Tree::trusted_view(doc, view.top.clone())
}

pub(super) struct Body;
impl crate::api::Renderer for Body {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        cx.render_blocks(node.pointer("/slots/body").unwrap_or(&Value::Null))
    }
}
