//! The document transforms of carve-js#5070 (C2-X-053): `defaultAttributes`,
//! `tabNormalize` and `inlineFootnotesToParentheses`.

use super::{Bundle, declaration};
use crate::api::{DocumentTransform, Error, Extension, Tree};
use serde_json::{Map, Value, json};

/// Node type to default attributes: `class` adds classes, `id` sets a
/// missing id, any other key a missing pair.
pub type DefaultAttributes = Vec<(String, Vec<(String, String)>)>;

/// Runs `edit` on every node list of the view except fallbacks, each list
/// before the nodes in it, and decodes the result.
fn walk(view: &Tree, edit: &dyn Fn(&mut Vec<Value>)) -> Result<Tree, Error> {
    let mut doc: Value = crate::api::deep_json(&crate::api::encode_tree(view))
        .map_err(|e| Error::option("documentTransforms", e.to_string()))?;
    let mut pending: Vec<&mut Value> = doc.get_mut("children").into_iter().collect();
    while let Some(v) = pending.pop() {
        match v {
            Value::Array(items) => {
                if items.iter().any(|i| i.get("type").is_some()) {
                    edit(items);
                }
                pending.extend(items.iter_mut());
            }
            Value::Object(map) => {
                for (key, child) in map.iter_mut() {
                    if !matches!(
                        key.as_str(),
                        "fallback" | "source" | "payload" | "props" | "pos" | "attrs"
                    ) {
                        pending.push(child);
                    }
                }
            }
            _ => {}
        }
    }
    // Engine code: only attribute values, code text and inline lists change.
    Tree::trusted(doc, view.top.clone())
}

fn bundle(name: &str, transform: Box<dyn DocumentTransform>) -> Bundle {
    Bundle {
        extensions: vec![Extension {
            declaration: declaration(name, &[]),
            renderers: Vec::new(),
            transform: Some(transform),
        }],
        matchers: Vec::new(),
    }
}

fn apply_defaults(node: &mut Map<String, Value>, defaults: &[(String, String)]) {
    let mut attrs = node
        .get("attrs")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for (name, value) in defaults {
        match name.as_str() {
            "class" => {
                let mut list = attrs
                    .get("classes")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for c in value.split(' ').filter(|c| !c.is_empty()) {
                    if !list.iter().any(|x| x == c) {
                        list.push(c.into());
                    }
                }
                if !list.is_empty() {
                    attrs.insert("classes".into(), Value::Array(list));
                }
            }
            "id" => {
                attrs.entry("id").or_insert_with(|| value.as_str().into());
            }
            _ => {
                let mut kv = attrs
                    .get("keyValues")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !kv
                    .iter()
                    .any(|p| p.get(0).and_then(Value::as_str) == Some(name))
                {
                    kv.push(json!([name, value]));
                    attrs.insert("keyValues".into(), Value::Array(kv));
                }
            }
        }
    }
    if !attrs.is_empty() {
        node.insert("attrs".into(), Value::Object(attrs));
    }
}

struct Defaults(DefaultAttributes);

impl DocumentTransform for Defaults {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        walk(&view, &|nodes| {
            for node in nodes.iter_mut() {
                let Some(map) = node.as_object_mut() else {
                    continue;
                };
                let ty = map.get("type").and_then(Value::as_str).unwrap_or("");
                if let Some((_, defaults)) = self.0.iter().find(|(t, _)| t == ty) {
                    apply_defaults(map, defaults);
                }
            }
        })
    }
}

/// Default attributes per node type, merged under the authored ones in the
/// render view.
#[must_use]
pub fn default_attributes(defaults: DefaultAttributes) -> Bundle {
    bundle("carve.defaultAttributes", Box::new(Defaults(defaults)))
}

struct Tabs(String);

impl DocumentTransform for Tabs {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        walk(&view, &|nodes| {
            for node in nodes.iter_mut() {
                let code = matches!(
                    node.get("type").and_then(Value::as_str),
                    Some("code_block" | "code")
                );
                if let (true, Some(value)) = (code, node.get_mut("value"))
                    && let Some(text) = value.as_str()
                {
                    *value = text.replace('\t', &self.0).into();
                }
            }
        })
    }
}

/// Replaces each tab in code blocks and code spans with `width` spaces in
/// the render view.
///
/// # Errors
///
/// [`Error::Option`] for a width above 16.
pub fn tab_normalize(width: usize) -> Result<Bundle, Error> {
    if width > 16 {
        return Err(Error::option(
            "width",
            "tab width must be an integer from 0 to 16",
        ));
    }
    Ok(bundle(
        "carve.tabNormalize",
        Box::new(Tabs(" ".repeat(width))),
    ))
}

struct Parentheses;

impl DocumentTransform for Parentheses {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        walk(&view, &|nodes| {
            if !nodes
                .iter()
                .any(|n| n.get("type").and_then(Value::as_str) == Some("inline_footnote"))
            {
                return;
            }
            let mut out = Vec::with_capacity(nodes.len() + 2);
            for node in nodes.drain(..) {
                if node.get("type").and_then(Value::as_str) != Some("inline_footnote") {
                    out.push(node);
                    continue;
                }
                out.push(json!({"type": "text", "value": " ("}));
                if let Some(children) = node.get("children").and_then(Value::as_array) {
                    out.extend(children.iter().cloned());
                }
                out.push(json!({"type": "text", "value": ")"}));
            }
            *nodes = out;
        })
    }
}

/// Writes each inline footnote as its content in parentheses after a
/// space, for targets without notes.
#[must_use]
pub fn inline_footnotes_to_parentheses() -> Bundle {
    bundle("carve.inlineFootnotesToParentheses", Box::new(Parentheses))
}
