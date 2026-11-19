//! `migrateTree` (C2-A-176 to C2-A-180): stored 1.x and retired draft trees
//! upgraded to contract 2.0 outside the reader, as carve-js does. The
//! with source-layout facts kept beside the migrated tree.

mod shapes;
mod table;

use super::{Entry, Error, Tree};
use crate::reader::TreeError;
use serde_json::{Map, Value, json};

/// The options of [`migrate_tree_with`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrateOptions {
    /// The source the tree was parsed from: 0.1 offsets count code points,
    /// and with the source they are converted to UTF-8 bytes (C2-A-141).
    pub source: Option<String>,
}

/// The migrated tree, the envelope when the input was one, and the losses.
#[derive(Debug, Clone)]
pub struct Migrated {
    pub tree: Tree,
    /// The JSON text of the 2.0 envelope, for an input envelope.
    pub envelope: Option<String>,
    pub diagnostics: Vec<Entry>,
    /// The version-2 sidecar; without source, only known migrated spelling is recorded.
    pub layout: Value,
}

type Fields = Map<String, Value>;

static NULL: Value = Value::Null;

/// The member `key` of `v`, null where it has none.
fn field<'v>(v: &'v Value, key: &str) -> &'v Value {
    v.get(key).unwrap_or(&NULL)
}

/// The item `i` of `v`, null where it has none.
fn at_i(v: &Value, i: usize) -> &Value {
    v.get(i).unwrap_or(&NULL)
}

/// Sets the member `key` of the object `v`.
fn put(v: &mut Value, key: &str, value: Value) {
    if let Some(m) = v.as_object_mut() {
        m.insert(key.into(), value);
    }
}

struct Declaration {
    name: String,
    version: String,
    required: bool,
    slots: Vec<String>,
}

impl Declaration {
    fn value(&self) -> Value {
        json!({"name": self.name, "version": self.version, "required": self.required, "slots": self.slots})
    }
}

struct State<'s> {
    source: Option<&'s str>,
    draft: bool,
    diagnostics: Vec<Entry>,
    declared: Vec<Declaration>,
    used: Vec<Declaration>,
}

fn loss(state: &mut State<'_>, path: &str, message: &str) {
    state.diagnostics.push(Entry {
        code: "migration-loss".into(),
        severity: "warning".into(),
        channel: "render-loss".into(),
        message: message.into(),
        path: Some(path.into()),
        range: None,
        file: None,
        foreign_type: None,
    });
}

fn refuse(reason: impl Into<String>) -> Error {
    Error::Tree(TreeError::Reader {
        path: String::new(),
        reason: reason.into(),
    })
}

fn string(v: Option<&Value>) -> Option<&str> {
    v.and_then(Value::as_str)
}

/// A JSON number with an integral value, as `Number.isInteger` reads it.
fn integer(v: Option<&Value>) -> Option<i64> {
    let n = v?.as_f64()?;
    #[allow(clippy::cast_possible_truncation)]
    (n.fract() == 0.0 && n.abs() < 9.0e15).then_some(n as i64)
}

fn node_list(v: Option<&Value>) -> Vec<Value> {
    v.and_then(Value::as_array)
        .map(|items| items.iter().filter(|i| i.is_object()).cloned().collect())
        .unwrap_or_default()
}

fn ty(node: &Fields) -> &str {
    string(node.get("type")).unwrap_or("")
}

fn folded(v: Option<&Value>) -> Option<String> {
    string(v).map(crate::chars::fold_label)
}

/// `pos` of `start` with the end of `end`, an end field absent there absent.
fn joined_pos(start: &Value, end: &Value) -> Value {
    let mut pos = start.as_object().cloned().unwrap_or_default();
    for key in ["endLine", "endColumn", "endOffset"] {
        match end.get(key) {
            Some(v) => pos.insert(key.into(), v.clone()),
            None => pos.remove(key),
        };
    }
    Value::Object(pos)
}

fn coalesce_text(node: &mut Fields) {
    if !node.get("children").is_some_and(Value::is_array) {
        return;
    }
    let mut out: Vec<Value> = Vec::new();
    for child in node_list(node.get("children")) {
        if let Some(prev) = out.last_mut()
            && field(&child, "type") == "text"
            && field(prev, "type") == "text"
            && let (Some(a), Some(b)) = (
                field(prev, "value").as_str(),
                field(&child, "value").as_str(),
            )
        {
            let value = format!("{a}{b}");
            if field(prev, "pos").is_object() && field(&child, "pos").is_object() {
                put(
                    prev,
                    "pos",
                    joined_pos(field(prev, "pos"), field(&child, "pos")),
                );
            }
            put(prev, "value", value.into());
            continue;
        }
        out.push(child);
    }
    node.insert("children".into(), Value::Array(out));
}

const SLOTS: &[&str] = &[
    "children",
    "items",
    "rows",
    "cells",
    "old",
    "new",
    "prefix",
    "suffix",
    "inline",
    "title",
    "shortCaption",
    "base",
    "annotation",
    "pairs",
    "content",
];

/// (rule id, node types, field, renamed to)
const FIELDS: &[(&str, &[&str], &str, Option<&str>)] = &[
    ("T-004", &["code", "math", "raw_inline"], "cellWraps", None),
    ("T-005", &["link", "image"], "rawRef", None),
    ("T-012", &["definition_list"], "spacing", None),
    ("T-012", &["definition_list"], "loose", None),
    ("T-015", &["link", "image"], "form", Some("refForm")),
    ("T-016", &["xref", "heading_ref"], "form", Some("display")),
    (
        "T-017",
        &["link_reference_definition", "link", "image"],
        "title",
        Some("tooltip"),
    ),
    ("T-021", &["citation_group"], "mode", None),
    ("T-023", &["task"], "status", None),
    ("T-025", &["thematic_break"], "marker", None),
    ("T-025", &["table"], "separatorWidths", None),
    ("T-025", &["table_row"], "continuationLines", None),
    ("T-025", &["table_cell"], "sourceContent", None),
    ("T-025", &["strong"], "boldItalic", None),
    (
        "T-030",
        &["abbreviation", "abbreviation_def"],
        "abbr",
        Some("key"),
    ),
    (
        "T-030",
        &[
            "footnote",
            "footnote_ref",
            "link_reference_definition",
            "citation_definition",
        ],
        "label",
        Some("key"),
    ),
    ("T-031", &["code_block"], "content", Some("value")),
    ("T-031", &["code_block"], "lang", Some("syntax")),
    ("T-031", &["code_block"], "header", Some("title")),
    ("T-031", &["code_block"], "label", Some("groupLabel")),
    (
        "T-031",
        &["div", "directive", "admonition"],
        "label",
        Some("groupLabel"),
    ),
    (
        "T-032",
        &[
            "frontmatter",
            "raw_block",
            "raw_inline",
            "literal_inline",
            "math",
        ],
        "content",
        Some("value"),
    ),
    ("T-032", &["critic_comment"], "text", Some("value")),
    (
        "T-035",
        &["list", "definition_list", "citation_group"],
        "items",
        Some("children"),
    ),
    ("T-035", &["table_row"], "cells", Some("children")),
    ("T-038", &["figure"], "target", Some("kind")),
    ("T-039", &["mention"], "user", Some("target")),
    ("T-040", &["inline_footnote"], "inline", Some("children")),
    ("T-041", &["inline_extension"], "content", Some("children")),
    ("T-053", &["table_body"], "headRows", None),
];

fn renamed(t: &str) -> &str {
    match t {
        "rubyPair" => "ruby_pair",
        "heading_ref" => "xref",
        "escaped_text" => "text",
        other => other,
    }
}

fn migrate_list(values: Option<&Value>, path: &str, state: &mut State<'_>) -> Vec<Value> {
    let mut out = Vec::new();
    for (index, value) in values
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(old) = value.as_object() {
            out.extend(migrate_node(old, &format!("{path}/{index}"), state));
        }
    }
    out
}

fn migrate_slots(node: &mut Fields, old: &Fields, path: &str, state: &mut State<'_>) {
    for &name in SLOTS {
        let Some(Value::Array(entries)) = old.get(name) else {
            continue;
        };
        let values: Vec<Value> = if name == "pairs" {
            entries
                .iter()
                .map(|pair| match pair {
                    Value::Object(m) if !m.contains_key("type") => {
                        let mut m = m.clone();
                        m.insert("type".into(), "ruby_pair".into());
                        Value::Object(m)
                    }
                    other => other.clone(),
                })
                .collect()
        } else {
            entries.clone()
        };
        let list = migrate_list(
            Some(&Value::Array(values)),
            &format!("{path}/{name}"),
            state,
        );
        node.insert(name.into(), Value::Array(list));
    }
    if old.get("blocks").is_some_and(Value::is_array) {
        let list = migrate_list(old.get("blocks"), &format!("{path}/blocks"), state);
        node.insert("blocks".into(), Value::Array(list));
    }
    migrate_caption(node, old, path, state);
    if let (Some(Value::Object(target)), true) = (old.get("target"), ty(old) == "figure") {
        match migrate_node(target, &format!("{path}/target"), state)
            .into_iter()
            .next()
        {
            Some(t) => node.insert("target".into(), t),
            None => node.remove("target"),
        };
    }
    if let Some(Value::Object(locator)) = old.get("locator") {
        let mut locator = locator.clone();
        if locator.get("children").is_some_and(Value::is_array) {
            let at = format!("{path}/locator/children");
            let list = migrate_list(locator.get("children"), &at, state);
            locator.insert("children".into(), Value::Array(list));
        }
        node.insert("locator".into(), Value::Object(locator));
    }
}

fn migrate_caption(node: &mut Fields, old: &Fields, path: &str, state: &mut State<'_>) {
    let at = format!("{path}/caption");
    match old.get("caption") {
        Some(Value::Object(caption)) => {
            if let Some(c) = migrate_node(caption, &at, state).into_iter().next() {
                node.insert("caption".into(), c);
            }
        }
        Some(list @ Value::Array(_)) => {
            let list = migrate_list(Some(list), &at, state);
            node.insert("caption".into(), Value::Array(list));
        }
        _ => {}
    }
}

fn mechanical(node: &mut Fields, old_type: &str) {
    for &(_, types, from, to) in FIELDS {
        if !types.contains(&old_type) {
            continue;
        }
        let Some(value) = node.remove(from) else {
            continue;
        };
        if let Some(to) = to
            && !node.contains_key(to)
        {
            node.insert(to.into(), value);
        }
    }
}

fn substitution(old: &Fields, path: &str, state: &mut State<'_>) -> Vec<Value> {
    let side = |name: &str, state: &mut State<'_>| match old.get(name) {
        Some(list @ Value::Array(_)) => migrate_list(Some(list), &format!("{path}/{name}"), state),
        other => vec![json!({"type": "text", "value": string(other).unwrap_or("")})],
    };
    let previous = side("old", state);
    let next = side("new", state);
    if old
        .get("attrs")
        .and_then(Value::as_object)
        .is_some_and(|a| !a.is_empty())
    {
        loss(
            state,
            &format!("{path}/attrs"),
            "a substitution splits into delete and insert; its attributes are dropped",
        );
    }
    let mut delete = json!({"type": "delete", "children": previous});
    let mut insert = json!({"type": "insert", "children": next});
    if let Some(pos) = old.get("pos") {
        put(&mut delete, "pos", pos.clone());
        put(&mut insert, "pos", pos.clone());
    }
    vec![delete, insert]
}

fn before(node: &mut Fields) {
    match ty(node) {
        "block_quote" => {
            node.remove("fenced");
        }
        "line_block" => {
            node.remove("lines");
        }
        "document" => {
            node.insert("astVersion".into(), "2.0".into());
            if let Some(Value::Array(stages)) = node.get_mut("stages") {
                stages.retain(|s| s != "blockImagePromotion");
            }
        }
        _ => {}
    }
}

/// Spelling fields of the retired shapes are attached temporarily to the
/// corresponding migrated node. The final walk removes them before reading
/// the tree and gives them JSON Pointer paths in the new tree.
fn retired_layout(old: &Fields, node: &mut Fields) {
    let mut facts = Fields::new();
    match ty(node) {
        "block_quote" => {
            if let Some(v) = old.get("fenced") {
                facts.insert("fenced".into(), v.clone());
            }
        }
        "list" => {
            if let Some(v) = old
                .get("bareMarker")
                .or_else(|| old.get("marker").and_then(|m| m.get("bare")))
            {
                facts.insert("bare".into(), v.clone());
            }
        }
        "link" | "image" => {
            if let Some(mut raw) = old
                .get("rawRef")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                if ty(node) == "image" {
                    raw = raw.strip_prefix('!').unwrap_or(&raw).to_string();
                }
                facts.insert("rawRef".into(), raw.into());
            }
        }
        "table" => {
            facts.insert(
                "sourceFormat".into(),
                old.get("sourceFormat")
                    .cloned()
                    .unwrap_or_else(|| "pipe".into()),
            );
        }
        "table_row" => {
            if let Some(v) = old.get("continuationLines") {
                facts.insert("continuationLines".into(), v.clone());
            }
        }
        "include" => {
            let mut spelling = Fields::new();
            for item in old
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let pair = if let Some(items) = item.as_array() {
                    items
                        .first()
                        .and_then(Value::as_str)
                        .zip(items.get(1).and_then(Value::as_str))
                } else {
                    item.get("name")
                        .and_then(Value::as_str)
                        .zip(item.get("value").and_then(Value::as_str))
                };
                if let Some((key, raw)) = pair {
                    spelling.entry(key).or_insert_with(|| raw.into());
                }
            }
            if !spelling.is_empty() {
                facts.insert("optionSpelling".into(), Value::Object(spelling));
            }
        }
        _ => {}
    }
    if !facts.is_empty() {
        node.insert("__layout".into(), Value::Object(facts));
    }
}

fn strip_layout(node: &mut Value, path: &str, entries: &mut Vec<Value>) {
    let Some(map) = node.as_object_mut() else {
        return;
    };
    if let Some(Value::Object(facts)) = map.remove("__layout") {
        let mut entry = json!({"path":path,"startByte":0,"endByte":0});
        for (key, value) in facts {
            put(&mut entry, &key, value);
        }
        entries.push(entry);
    }
    for slot in [
        "title",
        "children",
        "columns",
        "pairs",
        "prefix",
        "suffix",
        "base",
        "annotation",
        "short",
        "caption",
    ] {
        if let Some(Value::Array(children)) = map.get_mut(slot) {
            for (index, child) in children.iter_mut().enumerate() {
                strip_layout(child, &format!("{path}/{slot}/{index}"), entries);
            }
        } else if let Some(child @ Value::Object(_)) = map.get_mut(slot) {
            strip_layout(child, &format!("{path}/{slot}"), entries);
        }
    }
}

fn arrange_definitions(node: &mut Fields) {
    if ty(node) != "document" {
        return;
    }
    let mut children = node_list(node.get("children"));
    if children.iter().all(|c| field(c, "pos").is_object()) {
        let key = |c: &Value| {
            field(field(c, "pos"), "startOffset")
                .as_f64()
                .unwrap_or(0.0)
        };
        children.sort_by(|a, b| {
            key(a)
                .partial_cmp(&key(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        node.insert("children".into(), Value::Array(children));
    }
}

fn promote_images(node: &mut Fields) {
    const HOSTS: &[&str] = &[
        "document",
        "block_quote",
        "div",
        "admonition",
        "directive",
        "section",
        "figure",
        "figure_group",
        "list_item",
        "definition_description",
        "caption",
    ];
    if !HOSTS.contains(&ty(node)) {
        return;
    }
    let children: Vec<Value> = node_list(node.get("children"))
        .into_iter()
        .map(|child| {
            if field(&child, "type") != "image" {
                return child;
            }
            let mut p = json!({"type": "paragraph", "role": "image", "children": [child.clone()]});
            if let Some(pos) = child.get("pos") {
                put(&mut p, "pos", pos.clone());
            }
            p
        })
        .collect();
    node.insert("children".into(), Value::Array(children));
}

fn migrate_node(old: &Fields, path: &str, state: &mut State<'_>) -> Vec<Value> {
    let Some(old_type) = string(old.get("type")) else {
        loss(state, path, "a node has no type");
        return Vec::new();
    };
    match old_type {
        "critic_substitution" | "substitution" => return substitution(old, path, state),
        "cell_wrap" => {
            let mut text = json!({"type": "text", "value": " "});
            if let Some(pos) = old.get("pos") {
                put(&mut text, "pos", pos.clone());
            }
            return vec![text];
        }
        _ => {}
    }
    let mut node = old.clone();
    node.insert("type".into(), renamed(old_type).into());
    migrate_slots(&mut node, old, path, state);
    mechanical(&mut node, old_type);
    before(&mut node);
    shapes::shape(&mut node, old, state, path);
    if state.source.is_none() {
        retired_layout(old, &mut node);
    }
    coalesce_text(&mut node);
    promote_images(&mut node);
    arrange_definitions(&mut node);
    if matches!(ty(&node), "block_extension" | "inline_extension") {
        let fallback = extension_fallback(&node, old, path, state);
        return extension(node, fallback, path, state);
    }
    if ty(&node) == "figure" {
        return unwrap_figure(node);
    }
    vec![Value::Object(node)]
}

fn extension_fallback(
    node: &Fields,
    old: &Fields,
    path: &str,
    state: &mut State<'_>,
) -> Vec<Value> {
    let at = format!("{path}/fallback");
    match old.get("fallback") {
        Some(list @ Value::Array(_)) => migrate_list(Some(list), &at, state),
        _ if ty(node) == "inline_extension" => node_list(node.get("children")),
        Some(Value::Object(f)) => migrate_node(f, &at, state),
        _ => Vec::new(),
    }
}

fn captionable(node: Option<&Value>) -> bool {
    let Some(node) = node else { return false };
    match field(node, "type").as_str() {
        Some("code_block" | "block_quote") => true,
        Some("paragraph") => field(node, "role") == "image" || field(node, "role") == "math",
        _ => false,
    }
}

/// T-051, T-052 (C2-A-082): a figure of one captionable block and comments
/// becomes that block with the caption and attributes of the figure; any
/// other figure becomes a figure group.
fn unwrap_figure(mut figure: Fields) -> Vec<Value> {
    let children = node_list(figure.get("children"));
    figure.remove("kind");
    let rest = children.get(1..).unwrap_or(&[]);
    if !captionable(children.first()) || !rest.iter().all(|c| field(c, "type") == "comment") {
        figure.insert("type".into(), "figure_group".into());
        return vec![Value::Object(figure)];
    }
    let mut block = children.first().cloned().unwrap_or(Value::Null);
    for field in ["caption", "attrs", "lang", "dir", "pos"] {
        if let Some(v) = figure.get(field) {
            put(&mut block, field, v.clone());
        }
    }
    if field(&block, "type") == "code_block" {
        let mut out = vec![block];
        out.extend(rest.iter().cloned());
        return out;
    }
    let breaks = |leaf: &mut Value| {
        let mut kids = node_list(leaf.get("children"));
        for comment in rest {
            kids.push(json!({"type": "soft_break"}));
            kids.push(comment.clone());
        }
        put(leaf, "children", Value::Array(kids));
    };
    let mut inner = node_list(block.get("children"));
    if field(&block, "type") == "paragraph" {
        breaks(&mut block);
    } else if let Some(last) = inner.last_mut().filter(|l| field(l, "type") == "paragraph") {
        breaks(last);
        put(&mut block, "children", Value::Array(inner));
    } else {
        inner.extend(rest.iter().cloned());
        put(&mut block, "children", Value::Array(inner));
    }
    vec![block]
}

fn qualified(name: &str) -> bool {
    let part = |p: &str| {
        let mut chars = p.chars();
        chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    };
    name.contains('.') && name.split('.').all(part)
}

fn payload(node: &mut Fields, path: &str, state: &mut State<'_>) {
    let Some(value) = node.get("payload") else {
        return;
    };
    let all: Vec<Value> = match value {
        Value::Array(items) => items.clone(),
        other => vec![other.clone()],
    };
    let entries: Vec<Value> = all
        .iter()
        .filter(|e| {
            e.is_object()
                && field(e, "format").as_str().is_some_and(|f| !f.is_empty())
                && !field(e, "value").is_null()
        })
        .map(|e| json!({"format": field(e, "format"), "value": field(e, "value")}))
        .collect();
    if entries.len() == all.len() && !entries.is_empty() {
        node.insert("payload".into(), Value::Array(entries));
        return;
    }
    loss(
        state,
        &format!("{path}/payload"),
        "a payload without format and value",
    );
    if entries.is_empty() {
        node.remove("payload");
    } else {
        node.insert("payload".into(), Value::Array(entries));
    }
}

/// T-041, T-050: the node itself, or its fallback with a loss when 2.0
/// cannot hold it.
fn extension(
    mut node: Fields,
    fallback: Vec<Value>,
    path: &str,
    state: &mut State<'_>,
) -> Vec<Value> {
    let name = string(node.get("name")).unwrap_or("").to_string();
    let block = ty(&node) == "block_extension";
    if !block && !name.contains('.') {
        node.remove("version");
        return vec![Value::Object(node)];
    }
    let declared = state.declared.iter().position(|d| d.name == name);
    let own = string(node.get("version")).map(str::to_string);
    let version = declared
        .and_then(|i| state.declared.get(i))
        .map(|d| d.version.clone())
        .or_else(|| own.clone());
    let Some(version) = version
        .filter(|v| !v.is_empty() && qualified(&name) && own.as_ref().is_none_or(|o| o == v))
    else {
        loss(
            state,
            path,
            "an extension node without a qualified name and one version; its fallback was kept",
        );
        return fallback;
    };
    node.insert("version".into(), version.as_str().into());
    node.insert("fallback".into(), Value::Array(fallback));
    if block && !node.get("children").is_some_and(Value::is_array) {
        node.insert("children".into(), json!([]));
    }
    payload(&mut node, path, state);
    let entry = match declared.and_then(|i| state.declared.get(i)) {
        Some(d) => Declaration {
            name: d.name.clone(),
            version: d.version.clone(),
            required: d.required,
            slots: d.slots.clone(),
        },
        None => Declaration {
            name: name.clone(),
            version,
            required: false,
            slots: Vec::new(),
        },
    };
    match state.used.iter_mut().find(|d| d.name == name) {
        Some(slot) => *slot = entry,
        None => state.used.push(entry),
    }
    vec![Value::Object(node)]
}

const CORE_VOCABULARY: &str = "https://markup-carve.org/ast/core";

fn old_declarations(envelope: Option<&Fields>, state: &mut State<'_>) {
    let Some(envelope) = envelope else { return };
    if let Some(v) = envelope.get("vocabulary")
        && !v.is_object()
        && v != CORE_VOCABULARY
    {
        loss(
            state,
            "/vocabulary",
            "0.2 has no vocabulary other than the core one in this form",
        );
    }
    let list = envelope.get("extensions").and_then(Value::as_array);
    for (index, entry) in list.into_iter().flatten().enumerate() {
        let Some(e) = entry.as_object() else { continue };
        let name = string(e.get("id")).or_else(|| string(e.get("name")));
        let version = string(e.get("version")).filter(|v| !v.is_empty());
        let (Some(name), Some(version)) = (name.filter(|n| qualified(n)), version) else {
            loss(
                state,
                &format!("/extensions/{index}"),
                "an extension declaration without a qualified name and a version",
            );
            continue;
        };
        let decl = Declaration {
            name: name.into(),
            version: version.into(),
            required: e.get("required") != Some(&Value::Bool(false)),
            slots: e
                .get("slots")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect(),
        };
        match state.declared.iter_mut().find(|d| d.name == name) {
            Some(slot) => *slot = decl,
            None => state.declared.push(decl),
        }
    }
}

fn root_declarations(state: &State<'_>) -> Option<Value> {
    if state.used.is_empty() {
        return None;
    }
    let mut used: Vec<&Declaration> = state.used.iter().collect();
    used.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
    Some(Value::Array(used.iter().map(|d| d.value()).collect()))
}

fn envelope_declarations(state: &State<'_>) -> Option<Value> {
    (!state.declared.is_empty())
        .then(|| Value::Array(state.declared.iter().map(Declaration::value).collect()))
}

fn envelope_extensions(value: Option<&Value>) -> Option<Value> {
    let list = value?.as_array()?;
    let valid = list.iter().all(|e| {
        field(e, "name").is_string()
            && field(e, "version").is_string()
            && field(e, "required").is_boolean()
            && field(e, "slots")
                .as_array()
                .is_some_and(|s| s.iter().all(Value::is_string))
    });
    valid.then(|| {
        Value::Array(
            list.iter()
                .map(|e| json!({"name": field(e, "name"), "version": field(e, "version"), "required": field(e, "required"), "slots": field(e, "slots")}))
                .collect(),
        )
    })
}

fn envelope_of(input: &Fields, document: &Value, extensions: Option<Value>) -> Value {
    let mut out = json!({"astVersion": "2.0", "document": document});
    if let Some(v) = input.get("vocabulary").filter(|v| v.is_object()) {
        put(&mut out, "vocabulary", v.clone());
    }
    if let Some(e) = extensions {
        put(&mut out, "extensions", e);
    }
    out
}

/// The tree of `document`, read with every required declaration taken as
/// known: the migration emits a tree whose extensions it cannot know.
fn read(document: &Value, envelope: Option<&Value>) -> Result<Tree, Error> {
    let mut checked = envelope.unwrap_or(document).clone();
    let mut pending = vec![&mut checked];
    while let Some(v) = pending.pop() {
        if let Some(m) = v.as_object_mut() {
            if let Some(Value::Array(decls)) = m.get_mut("extensions") {
                for d in decls.iter_mut().filter_map(Value::as_object_mut) {
                    d.insert("required".into(), false.into());
                }
            }
            if let Some(doc) = m.get_mut("document") {
                pending.push(doc);
            }
        }
    }
    let read = crate::reader::read_tree_value(checked)?;
    Ok(Tree {
        document: read.into_document(),
        json: document.to_string(),
        top: None,
    })
}

/// C2-A-141: 0.1 offsets count code points, 2.0 offsets count UTF-8 bytes.
fn byte_offset_positions(document: &mut Value, state: &mut State<'_>, base: &str) {
    let offsets: Option<Vec<i64>> = state.source.map(|s| {
        let mut out = vec![0];
        let mut bytes = 0i64;
        for c in s.chars() {
            bytes += i64::try_from(c.len_utf8()).unwrap_or(4);
            out.push(bytes);
        }
        out
    });
    if offsets.is_none() && !non_ascii(document) {
        return;
    }
    let mut first: Option<String> = None;
    walk_positions(document, base, offsets.as_deref(), &mut first);
    if let Some(path) = first {
        let message = if offsets.is_none() {
            "positions dropped: 0.1 offsets count code points and no source was given to count bytes"
        } else {
            "positions dropped: an offset lies past the end of the source"
        };
        loss(state, &path, message);
    }
}

fn walk_positions(
    value: &mut Value,
    path: &str,
    offsets: Option<&[i64]>,
    first: &mut Option<String>,
) {
    match value {
        Value::Array(items) => {
            for (i, item) in items.iter_mut().enumerate() {
                walk_positions(item, &format!("{path}/{i}"), offsets, first);
            }
        }
        Value::Object(map) => {
            if map.get("type").is_some_and(Value::is_string)
                && let Some(Value::Object(pos)) = map.get("pos")
            {
                let at = |key: &str| {
                    let i = usize::try_from(integer(pos.get(key))?).ok()?;
                    offsets?.get(i).copied()
                };
                if let (Some(start), Some(end)) = (at("startOffset"), at("endOffset")) {
                    let mut pos = pos.clone();
                    pos.insert("startOffset".into(), start.into());
                    pos.insert("endOffset".into(), end.into());
                    map.insert("pos".into(), Value::Object(pos));
                } else {
                    map.remove("pos");
                    first.get_or_insert_with(|| format!("{path}/pos"));
                }
            }
            for (key, entry) in map.iter_mut() {
                if !matches!(key.as_str(), "pos" | "data" | "payload" | "props") {
                    walk_positions(entry, &format!("{path}/{key}"), offsets, first);
                }
            }
        }
        _ => {}
    }
}

fn non_ascii(value: &Value) -> bool {
    match value {
        Value::String(s) => !s.is_ascii(),
        Value::Array(items) => items.iter().any(non_ascii),
        Value::Object(map) => map.iter().any(|(k, v)| k != "pos" && non_ascii(v)),
        _ => false,
    }
}

fn migrate_document(
    old: &Fields,
    envelope: Option<&Fields>,
    state: &mut State<'_>,
) -> Result<(Value, Option<Value>), Error> {
    old_declarations(envelope, state);
    let base = if envelope.is_some() { "/document" } else { "" };
    let mut document = migrate_node(old, base, state)
        .into_iter()
        .next()
        .ok_or_else(|| refuse("migration input is not a document"))?;
    if let Some(decls) = root_declarations(state)
        && document.get("extensions").is_none()
    {
        put(&mut document, "extensions", decls);
    }
    if !state.draft {
        byte_offset_positions(&mut document, state, base);
    }
    let wrapped = envelope.map(|input| {
        let extensions =
            envelope_declarations(state).or_else(|| document.get("extensions").cloned());
        envelope_of(input, &document, extensions)
    });
    Ok((document, wrapped))
}

/// `migrateTree` with its options.
///
/// # Errors
///
/// [`Error::Tree`] for a value that is no document, an unknown AST version,
/// or a result the reader refuses.
pub fn migrate_tree_with(json: &Value, opts: &MigrateOptions) -> Result<Migrated, Error> {
    let parsed;
    let input = match json {
        Value::String(text) => {
            parsed = serde_json::from_str::<Value>(text).map_err(|e| refuse(e.to_string()))?;
            &parsed
        }
        other => other,
    };
    let envelope = input.as_object().filter(|m| !m.contains_key("type"));
    let value = envelope.map_or(Some(input), |e| e.get("document"));
    let Some(old) = value
        .and_then(Value::as_object)
        .filter(|o| ty(o) == "document")
    else {
        return Err(refuse("migration input is not a document"));
    };
    let version = string(old.get("astVersion"))
        .or_else(|| envelope.and_then(|e| string(e.get("astVersion"))));
    if let Some(v) = version.filter(|v| !["0.2", "1.0", "1.1", "1.2", "2.0"].contains(v)) {
        return Err(refuse(format!("unsupported AST version {v}")));
    }
    let tag = string(old.get("astVersion"));
    if tag == Some("2.0")
        && let Ok(tree) = Tree::read(&Value::Object(old.clone()).to_string(), None)
    {
        let document = Value::Object(old.clone());
        let envelope = envelope.map(|input| {
            let extensions = document
                .get("extensions")
                .cloned()
                .or_else(|| envelope_extensions(input.get("extensions")));
            envelope_of(input, &document, extensions).to_string()
        });
        return Ok(Migrated {
            tree,
            envelope,
            diagnostics: Vec::new(),
            layout: migration_layout(opts.source.as_deref(), Vec::new())?,
        });
    }
    let mut state = State {
        source: opts.source.as_deref(),
        draft: matches!(tag, Some("2.0" | "0.2")),
        diagnostics: Vec::new(),
        declared: Vec::new(),
        used: Vec::new(),
    };
    let (mut document, mut wrapped) = migrate_document(old, envelope, &mut state)?;
    let mut entries = Vec::new();
    strip_layout(&mut document, "", &mut entries);
    if let Some(envelope) = wrapped.as_mut().and_then(Value::as_object_mut)
        && let Some(doc) = envelope.get_mut("document")
    {
        strip_layout(doc, "", &mut Vec::new());
    }
    let tree = read(&document, wrapped.as_ref())?;
    Ok(Migrated {
        tree,
        envelope: wrapped.map(|w| w.to_string()),
        diagnostics: state.diagnostics,
        layout: migration_layout(opts.source.as_deref(), entries)?,
    })
}

fn migration_layout(source: Option<&str>, entries: Vec<Value>) -> Result<Value, Error> {
    if let Some(source) = source {
        return super::parse_with_layout(source, &crate::Options::default())
            .map(|parsed| parsed.layout);
    }
    Ok(json!({
        "version": 2, "encoding": "utf-8", "source": "", "lineEndings": "none",
        "bom": false, "nodes": entries,
    }))
}
