//! Extension nodes in the writer: the source form of a claimed node
//! (C2-X-023, C2-X-026) and the claims the readback parse makes for the
//! standard extensions a write enables (C2-X-005).

use super::LinkKeys as Keys;
use super::blocks::is_claimable_name;
use super::node::{children, classes, has, s, trees_equal, ty};
use crate::view::{Arr, V};
use serde_json::Value;
use std::sync::Arc;

/// The children of a parsed JSON node.
fn json_kids(v: &Value) -> &[Value] {
    v.get("children")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Whether the body of the source form of `node` is canonical Carve.
pub fn canonical_body(node: V<'_>) -> bool {
    let Some(body) = node.get("source").and_then(|src| s(src, "body")) else {
        return false;
    };
    super::format_plain(body).is_some_and(|f| f == body)
}

/// C2-X-023: a core readback of the source form is allowed only for its canonical content.
pub fn source_core_equivalent(expected: V<'_>, actual: Option<V<'_>>) -> bool {
    let Some(actual) = actual else { return false };
    match ty(expected) {
        "inline_extension" => inline_source_equivalent(expected, actual),
        "block_extension" => block_source_equivalent(expected, actual),
        _ => false,
    }
}

fn block_source_equivalent(expected: V<'_>, actual: V<'_>) -> bool {
    let Some(source) = expected.get("source") else {
        return false;
    };
    let form = s(source, "form").unwrap_or("");
    let name = s(source, "name").unwrap_or("");
    if form == "inline"
        || !is_claimable_name(
            form,
            name,
            has(source, "title") || has(source, "groupLabel"),
        )
    {
        return false;
    }
    let body = s(source, "body").unwrap_or("");
    if super::format_plain(body).is_none_or(|f| f != body) {
        return false;
    }
    if form != "div" {
        return ty(actual) == "code_block"
            && s(actual, "syntax") == Some(name)
            && s(actual, "value") == Some(body);
    }
    if ty(actual) != "div" || !classes(actual).contains(&name) {
        return false;
    }
    let Some(parsed) = super::parse_plain(body) else {
        return false;
    };
    let mine = children(&parsed);
    let theirs = children(actual);
    mine.len() == theirs.len() && mine.iter().zip(theirs).all(|(x, y)| trees_equal(x, y))
}

/// C2-X-023: the inline source form reads back as the core inline extension
/// of that name, attributes and canonical content `body`.
fn inline_source_equivalent(expected: V<'_>, actual: V<'_>) -> bool {
    core_inline(expected).is_some_and(|core| trees_equal(&core, actual))
}

/// The core node the inline source form of `node` reads back as.
fn core_inline(node: V<'_>) -> Option<Value> {
    let source = node
        .get("source")
        .filter(|x| s(x, "form") == Some("inline"))?;
    let content = body_inlines(s(source, "body").unwrap_or(""))?;
    let mut core = serde_json::Map::new();
    core.insert("type".into(), "inline_extension".into());
    core.insert("name".into(), s(source, "name")?.into());
    if let Some(attrs) = node.get("attrs") {
        core.insert("attrs".into(), attrs.to_value());
    }
    core.insert("children".into(), Value::Array(content));
    Some(Value::Object(core))
}

/// The inline content `body` parses to; the readback asks for one body many
/// times, so the last few are kept.
fn body_inlines(body: &str) -> Option<Vec<Value>> {
    thread_local! {
        static SEEN: std::cell::RefCell<Vec<(String, Option<Vec<Value>>)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }
    if let Some(hit) = SEEN.with_borrow(|seen| {
        seen.iter()
            .find(|(b, _)| b == body)
            .map(|(_, content)| content.clone())
    }) {
        return hit;
    }
    let content = super::parse_plain(body).and_then(|parsed| match json_kids(&parsed) {
        [p] if ty(p) == "paragraph" => Some(json_kids(p).to_vec()),
        [] => Some(Vec::new()),
        _ => None,
    });
    SEEN.with_borrow_mut(|seen| {
        if seen.len() == 64 {
            seen.remove(0);
        }
        seen.push((body.to_string(), content.clone()));
    });
    content
}

/// `tree` with each inline extension that carries an inline source form
/// replaced by the core node it reads back as, or `None` when it has none.
pub fn with_core_inlines(tree: V<'_>) -> Option<Value> {
    fn walk(v: &mut Value, changed: &mut bool) {
        if ty(&*v) == "inline_extension"
            && let Some(core) = core_inline(V::Json(v))
        {
            *v = core;
            *changed = true;
            return;
        }
        match v {
            Value::Object(map) => map.values_mut().for_each(|x| walk(x, changed)),
            Value::Array(items) => items.iter_mut().for_each(|x| walk(x, changed)),
            _ => {}
        }
    }
    let mut out = tree.to_value();
    let mut changed = false;
    walk(&mut out, &mut changed);
    changed.then_some(out)
}

/// C2-X-005: the claims of the enabled standard extensions on a parsed chunk.
pub fn apply_claims(doc: &mut Value, extensions: &[String], source: &str) {
    if !extensions.iter().any(|e| e == "carve.tabs") {
        return;
    }
    let keys = super::link_keys(json_kids(doc));
    let mut used = false;
    if let Some(kids) = doc.get_mut("children").and_then(Value::as_array_mut) {
        for child in kids.iter_mut() {
            used |= claim_tree(child, source, &keys);
        }
    }
    if used {
        if let Some(m) = doc.as_object_mut() {
            m.insert(
                "extensions".into(),
                serde_json::json!([
                    {"name": "carve.tab", "version": "1", "required": false, "slots": ["title", "body"]},
                    {"name": "carve.tabs", "version": "1", "required": false, "slots": ["tabs"]},
                ]),
            );
        }
    }
}

/// The name on the opener of the div `node`: the word behind its colon run.
fn opener_name<'s>(node: &Value, source: &'s str) -> Option<&'s str> {
    let (start, end) = super::node::span(node)?;
    let text = source.get(start..end)?;
    for line in text.split('\n') {
        let t = line.trim_start_matches([' ', '\t', '>']);
        let t = t.trim_start_matches([' ', '\t']);
        if !t.starts_with(":::") {
            continue;
        }
        let rest = t.trim_start_matches(':').trim_start_matches([' ', '\t']);
        let end = rest
            .bytes()
            .position(|b| !(b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
            .unwrap_or(rest.len());
        return rest.get(..end).filter(|n| !n.is_empty());
    }
    None
}

fn claim_tree(node: &mut Value, source: &str, keys: &Keys) -> bool {
    if ty(&*node) == "div" && opener_name(node, source) == Some("tabs") {
        if let Some(claimed) = tabs(node, source, keys) {
            *node = claimed;
            return true;
        }
    }
    let mut used = false;
    if let Some(kids) = node.get_mut("children").and_then(Value::as_array_mut) {
        for child in kids.iter_mut() {
            used |= claim_tree(child, source, keys);
        }
    }
    used
}

fn plain(nodes: Arr<'_>) -> String {
    super::node::plain_text(nodes)
}

fn remaining_attrs(node: &Value) -> Option<Value> {
    let attrs = node.get("attrs")?.as_object()?;
    let rest: Vec<Value> = attrs
        .get("classes")
        .and_then(Value::as_array)
        .map(|c| c.iter().skip(1).cloned().collect())
        .unwrap_or_default();
    if !attrs.contains_key("id") && !attrs.contains_key("keyValues") && rest.is_empty() {
        return None;
    }
    let mut out = serde_json::Map::new();
    if let Some(id) = attrs.get("id") {
        out.insert("id".into(), id.clone());
    }
    if let Some(kv) = attrs.get("keyValues") {
        out.insert("keyValues".into(), kv.clone());
    }
    if !rest.is_empty() {
        out.insert("classes".into(), Value::Array(rest));
    }
    Some(Value::Object(out))
}

/// C2-X-026: the body of a div source form, written in its context: the link
/// keys of the document stand in for its definitions, which a copy per body
/// would make quadratic.
fn source_body(node: &Value, keys: &Keys) -> String {
    let doc = serde_json::json!({"type": "document", "astVersion": "2.0", "children": [node]});
    let settings = super::Settings {
        link_keys: Some(Arc::clone(keys)),
        ..super::Settings::default()
    };
    let text = super::write_value(&doc, &settings).0;
    let lines: Vec<&str> = text.split('\n').collect();
    let Some(opener) = lines.iter().position(|l| l.starts_with(":::")) else {
        return String::new();
    };
    let run: String = lines.get(opener).map_or(":::".into(), |l| {
        l.chars().take_while(|&c| c == ':').collect()
    });
    let close = lines.iter().rposition(|l| *l == run).unwrap_or(opener);
    let body = lines
        .get(opener + 1..close)
        .map_or(String::new(), |b| b.join("\n"));
    if body.is_empty() { body } else { body + "\n" }
}

fn div_source(node: &Value, name: &str, keys: &Keys) -> Value {
    let mut src = serde_json::json!({"form": "div", "name": name, "body": source_body(node, keys)});
    if let (Some(t), Some(m)) = (super::node::title(node), src.as_object_mut()) {
        m.insert("title".into(), Value::String(plain(t)));
    }
    if let (Some(l), Some(m)) = (s(node, "groupLabel"), src.as_object_mut()) {
        m.insert("groupLabel".into(), Value::String(l.into()));
    }
    src
}

fn tab_node(node: &Value, keys: &Keys) -> Value {
    let title = node
        .get("title")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut fallback = Vec::new();
    if !title.is_empty() {
        fallback.push(serde_json::json!({"type": "paragraph", "children": [{"type": "strong", "children": title}]}));
    }
    fallback.extend(json_kids(node).iter().cloned());
    let mut out = serde_json::json!({
        "type": "block_extension", "name": "carve.tab", "version": "1", "children": [],
        "slots": {"title": title, "body": json_kids(node)}, "fallback": fallback,
        "source": div_source(node, "tab", keys),
    });
    if let Some(m) = out.as_object_mut() {
        if let Some(pos) = node.get("pos") {
            m.insert("pos".into(), pos.clone());
        }
        if let Some(a) = remaining_attrs(node) {
            m.insert("attrs".into(), a);
        }
    }
    out
}

fn tabs(node: &Value, source: &str, keys: &Keys) -> Option<Value> {
    let body = json_kids(node);
    let is_tab = |n: &Value| {
        ty(n) == "div"
            && opener_name(n, source) == Some("tab")
            && super::node::title(n).is_some_and(|t| !plain(t).trim().is_empty())
    };
    if body.is_empty() || !body.iter().all(is_tab) {
        return None;
    }
    let tabs: Vec<Value> = body.iter().map(|t| tab_node(t, keys)).collect();
    let fallback: Vec<Value> = tabs
        .iter()
        .flat_map(|t| {
            t.get("fallback")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        })
        .collect();
    let mut out = serde_json::json!({
        "type": "block_extension", "name": "carve.tabs", "version": "1", "children": [],
        "slots": {"tabs": tabs}, "fallback": fallback,
        "source": div_source(node, "tabs", keys),
    });
    if let Some(m) = out.as_object_mut() {
        if let Some(pos) = node.get("pos") {
            m.insert("pos".into(), pos.clone());
        }
        if let Some(a) = remaining_attrs(node) {
            m.insert("attrs".into(), a);
        }
    }
    Some(out)
}
