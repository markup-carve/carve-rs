//! C2-W-151: spaces and tabs that would end a source line in text are insignificant. The writer
//! writes a copy of a supplied tree without them, before a soft break and at the end of the content
//! of a line holder, and without the text nodes that hold nothing else; a loss path of the copy is
//! mapped back to the supplied tree.

use super::facts::Entry;
use super::node::slot_paths;
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
/// `delimited` says whether the table at a pointer is written in a delimited form, where a cell
/// keeps them as data (C2-W-066) while its caption is still trimmed.
pub fn trim(doc: &Value, delimited: &dyn Fn(&Value, &str) -> bool) -> Option<Trimmed> {
    if !has_spaced_text(doc) || !has_trim_target(doc) {
        return None;
    }
    let mut copy = doc.clone();
    let mut removed: HashMap<String, Vec<usize>> = HashMap::new();
    let mut run = Run {
        delimited,
        removed: &mut removed,
    };
    trim_node(&mut copy, "", &mut run);
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

/// Whether a text node below `value` ends in a space or tab, which every trim target does.
fn has_spaced_text(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(has_spaced_text),
        Value::Object(map) => {
            (ty(value) == "text"
                && map
                    .get("value")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.ends_with([' ', '\t'])))
                || map.values().any(has_spaced_text)
        }
        _ => false,
    }
}

/// Check the same line positions that `trim_list` can change before copying a tree.
fn has_trim_target(node: &Value) -> bool {
    let kind = ty(node);
    let holder = LINE_HOLDERS.contains(&kind);
    slot_paths(kind).iter().any(|path| {
        let children = matches!(path.as_slice(), [only] if only == "children");
        has_trim_target_path(node, path, holder && children)
    })
}

fn has_trim_target_path(value: &Value, parts: &[String], line_end: bool) -> bool {
    let Some((first, rest)) = parts.split_first() else {
        if let Some(items) = value.as_array() {
            return items.iter().enumerate().any(|(i, item)| {
                line_end_text(items, i, line_end).is_some()
                    || (ty(item) != "text" && has_trim_target(item))
            });
        }
        return value.get("type").is_some() && ty(value) != "text" && has_trim_target(value);
    };
    if first == "*" {
        return match value {
            Value::Array(items) => items
                .iter()
                .any(|item| has_trim_target_path(item, rest, line_end)),
            Value::Object(map) => map
                .values()
                .any(|item| has_trim_target_path(item, rest, line_end)),
            _ => false,
        };
    }
    value
        .get(first)
        .is_some_and(|inner| has_trim_target_path(inner, rest, line_end))
}

fn ty(v: &Value) -> &str {
    crate::view::json_field(v, "type")
        .and_then(Value::as_str)
        .unwrap_or("")
}

struct Run<'a> {
    delimited: &'a dyn Fn(&Value, &str) -> bool,
    removed: &'a mut HashMap<String, Vec<usize>>,
}

fn trim_node(node: &mut Value, at: &str, run: &mut Run<'_>) {
    let kind = ty(node).to_string();
    let holder = LINE_HOLDERS.contains(&kind.as_str());
    for path in slot_paths(&kind) {
        let children = matches!(path.as_slice(), [only] if only == "children");
        if kind == "table" && children {
            trim_cells(node, path, at, run);
        } else {
            trim_path(node, path, at, holder && children, run);
        }
    }
}

/// A table that trimming would turn delimited keeps its cells as they are, as one that is
/// delimited already.
fn trim_cells(table: &mut Value, path: &[String], at: &str, run: &mut Run<'_>) {
    if (run.delimited)(table, at) {
        return;
    }
    let saved = table.get("children").cloned();
    let mut removed = HashMap::new();
    let mut cells = Run {
        delimited: run.delimited,
        removed: &mut removed,
    };
    trim_path(table, path, at, false, &mut cells);
    if (run.delimited)(table, at) {
        if let (Some(saved), Some(map)) = (saved, table.as_object_mut()) {
            map.insert("children".into(), saved);
        }
        return;
    }
    run.removed.extend(removed);
}

fn escaped(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn trim_path(v: &mut Value, parts: &[String], at: &str, line_end: bool, run: &mut Run<'_>) {
    let Some((first, rest)) = parts.split_first() else {
        if v.is_array() {
            trim_list(v, at, line_end, run);
        } else if v.get("type").is_some() && ty(v) != "text" {
            trim_node(v, at, run);
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
                run,
            );
        }
        return;
    }
    if let Some(items) = v.as_array_mut() {
        for (k, inner) in items.iter_mut().enumerate() {
            trim_path(inner, rest, &format!("{at}/{k}"), line_end, run);
        }
    } else if let Some(map) = v.as_object_mut() {
        for (k, inner) in map.iter_mut() {
            trim_path(inner, rest, &format!("{at}/{}", escaped(k)), line_end, run);
        }
    }
}

/// The text `items[i]` with its line-end spaces and tabs, or `None` where it ends no line or has none.
fn line_end_text(items: &[Value], i: usize, line_end: bool) -> Option<usize> {
    let item = items.get(i)?;
    // The attribute block of a text follows it, so its spaces end no line; a field dropped with the node is a loss.
    if ty(item) != "text"
        || ["attrs", "data", "lang", "dir", "metadata"]
            .iter()
            .any(|f| item.get(f).is_some())
    {
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
fn trim_list(list: &mut Value, at: &str, line_end: bool, run: &mut Run<'_>) {
    let Some(items) = list.as_array_mut() else {
        return;
    };
    for i in (0..items.len()).rev() {
        let kept = line_end_text(items, i, line_end);
        let Some(item) = items.get_mut(i) else {
            continue;
        };
        if ty(item) != "text" {
            trim_node(item, &format!("{at}/{i}"), run);
            continue;
        }
        match kept {
            None => {}
            Some(0) => {
                items.remove(i);
                run.removed.entry(at.to_string()).or_default().push(i);
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
            let orig = index.and_then(|k| self.kept.get(&out).and_then(|keep| keep.get(k)));
            out.push('/');
            if let Some(orig) = orig {
                out.push_str(&orig.to_string());
            } else {
                out.push_str(step);
            }
        }
        out
    }

    /// The path in the copy of a path of the supplied tree, or `None` inside a removed node.
    pub fn forward(&self, path: &str) -> Option<String> {
        if self.kept.is_empty() {
            return Some(path.to_string());
        }
        let mut supplied = String::with_capacity(path.len());
        let mut out = String::with_capacity(path.len());
        for step in path.split('/').skip(1) {
            let mapped = match (self.kept.get(&supplied), step.parse::<usize>()) {
                (Some(keep), Ok(k)) => Some(keep.binary_search(&k).ok()?),
                _ => None,
            };
            supplied.push('/');
            supplied.push_str(step);
            out.push('/');
            if let Some(at) = mapped {
                out.push_str(&at.to_string());
            } else {
                out.push_str(step);
            }
        }
        Some(out)
    }

    /// The sidecar entries with their paths in the copy; an entry or wrap of a removed node is gone.
    pub fn forward_layout(&self, layout: &[Entry]) -> Vec<Entry> {
        layout
            .iter()
            .filter_map(|entry| {
                let mut entry = Entry {
                    path: self.forward(&entry.path)?,
                    ..entry.clone()
                };
                if let Some(wraps) = &mut entry.facts.wraps {
                    wraps.retain_mut(|wrap| {
                        self.forward(&wrap.path)
                            .map(|path| wrap.path = path)
                            .is_some()
                    });
                }
                Some(entry)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::trim;
    use serde_json::json;

    #[test]
    fn spaces_before_inline_children_do_not_copy_the_tree() {
        let doc = json!({ "type": "document", "children": [{ "type": "paragraph", "children": [
            { "type": "text", "value": "a " },
            { "type": "emphasis", "children": [{ "type": "text", "value": "b" }] }
        ] }] });
        assert!(trim(&doc, &|_, _| false).is_none());
    }

    #[test]
    fn paths_map_both_ways_around_a_removed_text() {
        let doc = json!({ "type": "document", "children": [{ "type": "paragraph", "children": [
            { "type": "text", "value": "a" }, { "type": "text", "value": " " },
            { "type": "soft_break" }, { "type": "text", "value": "b" }] }] });
        let trimmed = trim(&doc, &|_, _| false);
        assert!(trimmed.is_some(), "a trim");
        let Some(trimmed) = trimmed else { return };
        assert_eq!(
            trimmed.remap("/children/0/children/2"),
            "/children/0/children/3"
        );
        assert_eq!(
            trimmed.forward("/children/0/children/3").as_deref(),
            Some("/children/0/children/2")
        );
        assert_eq!(trimmed.forward("/children/0/children/1"), None);
        assert_eq!(
            trimmed.forward("/children/0").as_deref(),
            Some("/children/0")
        );
    }
}
