//! The tables and helpers both directions share: a port of carve-grammars
//! `tiptap/tree.js`, through the carve-php bridge.
//!
//! Each ProseMirror node and mark keeps the tree fields the editor does not
//! own in its `carve` attribute. Keys of `carve` that start with `$` are the
//! kit's own bookkeeping; no tree field starts with `$`.

use serde_json::{Map, Value};

/// Inline containers that become marks: tree type, mark name, owned fields.
pub(super) const MARKS: &[(&str, &str, &[&str])] = &[
    ("strong", "bold", &[]),
    ("emphasis", "italic", &[]),
    ("underline", "underline", &[]),
    ("strike", "strike", &[]),
    ("highlight", "highlight", &[]),
    ("subscript", "subscript", &[]),
    ("superscript", "superscript", &[]),
    ("insert", "carveInsert", &[]),
    ("delete", "carveDelete", &[]),
    ("small_caps", "carveSmallCaps", &[]),
    ("span", "carveSpan", &[]),
    ("link", "link", &["href"]),
    ("inline_footnote", "carveInlineFootnote", &[]),
    ("inline_extension", "carveInlineExtension", &[]),
];

/// Block containers whose children are blocks.
pub(super) const CONTAINERS: &[(&str, &str, &[&str])] = &[
    ("block_quote", "blockquote", &[]),
    ("div", "carveDiv", &[]),
    ("admonition", "carveAdmonition", &["kind"]),
    ("directive", "carveDirective", &[]),
    ("section", "carveSection", &[]),
    ("figure_group", "carveFigureGroup", &[]),
    ("block_extension", "carveBlockExtension", &[]),
    ("footnote", "carveFootnote", &[]),
    ("definition_description", "carveDefinitionDescription", &[]),
    ("caption", "carveCaption", &[]),
];

/// Textblocks whose children are inlines.
pub(super) const TEXTBLOCKS: &[(&str, &str, &[&str])] = &[
    ("paragraph", "paragraph", &[]),
    ("heading", "heading", &["level"]),
    ("definition_term", "carveDefinitionTerm", &[]),
    ("line", "carveLine", &[]),
    ("citation_definition", "carveCitationDefinition", &[]),
];

/// Blocks whose `value` is their editable text.
pub(super) const CODE_BLOCKS: &[(&str, &str)] = &[
    ("comment", "carveComment"),
    ("raw_block", "carveRawBlock"),
    ("frontmatter", "carveFrontmatter"),
];

/// Nodes whose title is edited as a leading `carveTitle` child.
pub(super) const TITLED: &[&str] = &["div", "admonition", "directive"];

const DROPPED: &[&str] = &["type", "pos", "segments", "srcByteLength"];
const POSITIONS: &[&str] = &["pos", "segments", "srcByteLength"];

pub(super) type Spec = (&'static str, &'static [&'static str]);

/// The entry of `table` for a tree type: (ProseMirror name, owned fields).
pub(super) fn by_tree(
    table: &[(&'static str, &'static str, &'static [&'static str])],
    tree: &str,
) -> Option<Spec> {
    table
        .iter()
        .find(|(t, _, _)| *t == tree)
        .map(|(_, n, o)| (*n, *o))
}

/// The entry of `table` for a ProseMirror name: (tree type, owned fields).
pub(super) fn by_name(
    table: &[(&'static str, &'static str, &'static [&'static str])],
    name: &str,
) -> Option<Spec> {
    table
        .iter()
        .find(|(_, n, _)| *n == name)
        .map(|(t, _, o)| (*t, *o))
}

/// A span count of an editor attribute: an integer above 1, or 1.
pub(super) fn span_count(value: Option<&Value>) -> i64 {
    value
        .and_then(Value::as_i64)
        .filter(|n| *n > 1)
        .unwrap_or(1)
}

/// The width of each row (`(colspan, rowspan)` per cell), row spans from
/// above included.
pub(super) fn grid_widths(rows: &[Vec<(i64, i64)>]) -> Vec<i64> {
    let mut below: std::collections::BTreeMap<i64, i64> = std::collections::BTreeMap::new();
    let mut out = Vec::with_capacity(rows.len());
    for cells in rows {
        let mut taken: std::collections::BTreeMap<i64, bool> =
            below.iter().map(|(k, v)| (*k, *v > 0)).collect();
        let mut column = 0;
        for &(colspan, rowspan) in cells {
            while taken.get(&column).copied().unwrap_or(false) {
                column += 1;
            }
            for offset in 0..colspan {
                if rowspan > 1 {
                    let at = below.entry(column + offset).or_insert(0);
                    *at = (*at).max(rowspan);
                }
                taken.insert(column + offset, true);
            }
            column += colspan;
        }
        let last = taken.keys().next_back().map_or(0, |k| k + 1);
        out.push(column.max(last));
        for v in below.values_mut() {
            *v = (*v - 1).max(0);
        }
    }
    out
}

/// A copy of `value` without positions, at any depth.
pub(super) fn clean(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(clean).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !POSITIONS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), clean(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The fields of `node` other than its type, positions and `omit`.
pub(super) fn fields(node: &Map<String, Value>, omit: &[&str]) -> Map<String, Value> {
    node.iter()
        .filter(|(k, _)| !DROPPED.contains(&k.as_str()) && !omit.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), clean(v)))
        .collect()
}

/// `carve` as an attribute value: null when it holds nothing.
pub(super) fn carve_attr(carve: Map<String, Value>) -> Value {
    if carve.is_empty() {
        Value::Null
    } else {
        Value::Object(carve)
    }
}

/// The tree fields of a `carve` attribute, without the kit's own keys.
pub(super) fn tree_fields(carve: &Map<String, Value>) -> Map<String, Value> {
    carve
        .iter()
        .filter(|(k, _)| !k.starts_with('$'))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// An object member, or an empty map.
pub(super) fn map(value: Option<&Value>) -> Map<String, Value> {
    match value {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    }
}

/// The `carve` attribute of a ProseMirror node or mark.
pub(super) fn carve_of(pm: &Map<String, Value>) -> Map<String, Value> {
    match pm.get("attrs").and_then(|a| a.get("carve")) {
        Some(Value::Object(m)) => m.clone(),
        _ => Map::new(),
    }
}

/// The attrs object of a ProseMirror node or mark.
pub(super) fn attrs_of(pm: &Map<String, Value>) -> Map<String, Value> {
    map(pm.get("attrs"))
}

/// The objects of an array; an empty object is skipped, as the PHP bridge
/// decodes it as no node at all.
pub(super) fn nodes(value: Option<&Value>) -> Vec<Map<String, Value>> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Object(m) if !m.is_empty() => Some(m.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// A string, or a number as text, or "".
pub(super) fn str_of(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// The `type` of a node.
pub(super) fn type_of(node: &Map<String, Value>) -> String {
    str_of(node.get("type"))
}

/// True when the key is set and not null, as PHP `isset`.
pub(super) fn isset(node: &Map<String, Value>, key: &str) -> bool {
    node.get(key).is_some_and(|v| !v.is_null())
}

/// `node` with `fields` set over it.
pub(super) fn with(mut node: Map<String, Value>, fields: Map<String, Value>) -> Map<String, Value> {
    for (k, v) in fields {
        node.insert(k, v);
    }
    node
}

/// A map of one `type`.
pub(super) fn typed(ty: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("type".into(), Value::String(ty.into()));
    m
}

/// Sets `key` only when it is not there yet, as PHP's array union.
pub(super) fn or_insert(node: &mut Map<String, Value>, key: &str, value: Value) {
    if !node.contains_key(key) {
        node.insert(key.into(), value);
    }
}

/// The JSON array of maps.
pub(super) fn array(items: Vec<Map<String, Value>>) -> Value {
    Value::Array(items.into_iter().map(Value::Object).collect())
}

/// The value as a JSON text, a string as it is.
pub(super) fn attribute_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
