//! C2-W-151: spaces and tabs that would end a source line in text are insignificant. The writer
//! writes a copy of a supplied tree without them, before a soft break and at the end of the content
//! of a line holder, and without the text nodes that hold nothing else; a loss path of the copy is
//! mapped back to the supplied tree.

use super::node::slot_paths;
use super::table::writes_delimited;
use crate::view::V;
use serde_json::Value;
use std::collections::HashMap;

/// The block nodes whose inline content ends at a line end.
const LINE_HOLDERS: &[&str] = &[
    "paragraph",
    "heading",
    "definition_term",
    "cell_paragraph",
    "line",
    "citation_definition",
];

/// The copy of the tree, and for each list it removed nodes from, by its pointer in the supplied
/// tree, the supplied index of each node it keeps.
pub struct Trimmed {
    pub doc: Value,
    kept: HashMap<String, Vec<usize>>,
}

/// The copy without line-end spaces and tabs of text, or `None` where the tree has none.
pub fn trim(doc: &Value) -> Option<Trimmed> {
    if !has_any(doc) {
        return None;
    }
    let mut copy = doc.clone();
    let mut removed: HashMap<String, Vec<usize>> = HashMap::new();
    trim_node(&mut copy, "", &mut removed);
    let kept = removed
        .into_iter()
        .map(|(list, gone)| {
            let keep = (0..len_at(doc, &list))
                .filter(|k| !gone.contains(k))
                .collect();
            (list, keep)
        })
        .collect();
    Some(Trimmed { doc: copy, kept })
}

fn len_at(doc: &Value, pointer: &str) -> usize {
    doc.pointer(pointer)
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

/// Whether a text value ends with a space or a tab: the cheap test before any copy.
fn has_any(v: &Value) -> bool {
    match v {
        Value::Array(items) => items.iter().any(has_any),
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("text") {
                return map
                    .get("value")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.ends_with([' ', '\t']));
            }
            map.iter().any(|(k, inner)| k != "data" && has_any(inner))
        }
        _ => false,
    }
}

fn ty(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

fn trim_node(node: &mut Value, at: &str, removed: &mut HashMap<String, Vec<usize>>) {
    let kind = ty(node).to_string();
    if kind == "table" && writes_delimited(V::Json(node)) {
        return;
    }
    let holder = LINE_HOLDERS.contains(&kind.as_str());
    for path in slot_paths(&kind) {
        let line_end = holder && matches!(path.as_slice(), [only] if only == "children");
        trim_path(node, path, at, line_end, removed);
    }
}

fn escaped(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn trim_path(
    v: &mut Value,
    parts: &[String],
    at: &str,
    line_end: bool,
    removed: &mut HashMap<String, Vec<usize>>,
) {
    let Some((first, rest)) = parts.split_first() else {
        if v.is_array() {
            trim_list(v, at, line_end, removed);
        } else if v.get("type").is_some() && ty(v) != "text" {
            trim_node(v, at, removed);
        }
        return;
    };
    if first != "*" {
        if let Some(inner) = v.get_mut(first.as_str()) {
            trim_path(
                inner,
                rest,
                &format!("{at}/{}", escaped(first)),
                line_end,
                removed,
            );
        }
        return;
    }
    if let Some(items) = v.as_array_mut() {
        for (k, inner) in items.iter_mut().enumerate() {
            trim_path(inner, rest, &format!("{at}/{k}"), line_end, removed);
        }
    } else if let Some(map) = v.as_object_mut() {
        for (k, inner) in map.iter_mut() {
            trim_path(
                inner,
                rest,
                &format!("{at}/{}", escaped(k)),
                line_end,
                removed,
            );
        }
    }
}

/// The text `items[i]` with its line-end spaces and tabs, or `None` where it ends no line or has none.
fn line_end_text(items: &[Value], i: usize, line_end: bool) -> Option<usize> {
    let item = items.get(i)?;
    if ty(item) != "text" {
        return None;
    }
    let ends = items
        .get(i + 1)
        .map_or(line_end, |next| ty(next) == "soft_break");
    let value = item.get("value").and_then(Value::as_str)?;
    let kept = value.trim_end_matches([' ', '\t']).len();
    (ends && kept < value.len()).then_some(kept)
}

/// Right to left, so a text before a dropped text sees what follows it then.
fn trim_list(
    list: &mut Value,
    at: &str,
    line_end: bool,
    removed: &mut HashMap<String, Vec<usize>>,
) {
    let Some(items) = list.as_array_mut() else {
        return;
    };
    for i in (0..items.len()).rev() {
        let kept = line_end_text(items, i, line_end);
        let Some(item) = items.get_mut(i) else {
            continue;
        };
        if ty(item) != "text" {
            trim_node(item, &format!("{at}/{i}"), removed);
            continue;
        }
        match kept {
            None => {}
            Some(0) => {
                items.remove(i);
                removed.entry(at.to_string()).or_default().push(i);
            }
            Some(kept) => {
                if let Some(Value::String(value)) = item.get_mut("value") {
                    value.truncate(kept);
                }
            }
        }
    }
}

impl Trimmed {
    /// The path in the supplied tree of a path of the copy.
    pub fn remap(&self, path: &str) -> String {
        if self.kept.is_empty() {
            return path.to_string();
        }
        let mut out = String::with_capacity(path.len());
        for step in path.split('/').skip(1) {
            let index = step.parse::<usize>().ok();
            match index.and_then(|k| self.kept.get(&out).and_then(|keep| keep.get(k))) {
                Some(orig) => {
                    out.push('/');
                    out.push_str(&orig.to_string());
                }
                None => {
                    out.push('/');
                    out.push_str(step);
                }
            }
        }
        out
    }
}
