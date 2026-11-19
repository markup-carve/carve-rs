//! `codeCallouts` (carve-js#5068): a document transform (C2-X-053).

use super::{Bundle, bare_fallback, classes, declaration, key_values, payload_text, with_attrs};
use crate::api::{Channel, DocumentTransform, Error, Extension, RenderContext, Renderer, Tree};
use serde_json::{Map, Value, json};
use std::fmt::Write as _;

const CODE: &str = "carve.codeCallouts.code";
const LIST: &str = "carve.codeCallouts.list";

/// A `<n>` that ends its line: the byte offset of `<` and the number.
fn marker(line: &str) -> Option<(usize, &str)> {
    let body = line.trim_end_matches([' ', '\t']).strip_suffix('>')?;
    let digits = body.len() - body.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let start = body.len() - digits;
    let open = start.checked_sub(1)?;
    let n = body.get(start..)?;
    (digits > 0 && body.get(open..start) == Some("<")).then_some((open, n))
}

/// A list line that starts with `<n> `: the number and the rest.
fn item(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix('<')?;
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let after = rest.get(digits..)?.strip_prefix("> ")?;
    (digits > 0).then_some((rest.get(..digits)?, after))
}

fn ty(v: &Value) -> &str {
    v.get("type").and_then(Value::as_str).unwrap_or("")
}

fn lines(children: &[Value]) -> Vec<Vec<Value>> {
    let mut out: Vec<Vec<Value>> = vec![Vec::new()];
    for child in children {
        if ty(child) == "soft_break" {
            out.push(Vec::new());
        } else if let Some(last) = out.last_mut() {
            last.push(child.clone());
        }
    }
    out.retain(|l| !l.is_empty());
    out
}

fn item_text(v: &Value) -> Option<&str> {
    (ty(v) == "text").then(|| v.get("value").and_then(Value::as_str))?
}

fn is_list(node: &Value) -> bool {
    if ty(node) != "paragraph" || node.get("role").is_some() || node.get("caption").is_some() {
        return false;
    }
    let children = node
        .get("children")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let split = lines(children);
    !split.is_empty()
        && split.iter().all(|l| {
            l.first()
                .and_then(item_text)
                .is_some_and(|t| item(t).is_some())
        })
}

fn copy_into(out: &mut Map<String, Value>, from: &Value, keys: &[&str]) {
    for key in keys {
        if let Some(v) = from.get(*key) {
            out.insert((*key).into(), v.clone());
        }
    }
}

fn code_node(block: &Value) -> Value {
    let mut out = Map::new();
    out.insert("type".into(), "block_extension".into());
    out.insert("name".into(), CODE.into());
    out.insert("version".into(), "1".into());
    out.insert("children".into(), json!([]));
    let value = block.get("value").cloned().unwrap_or_else(|| "".into());
    out.insert(
        "payload".into(),
        json!([{"format": "text/plain", "value": value}]),
    );
    out.insert("fallback".into(), json!([bare_fallback(block)]));
    if let Some(syntax) = block.get("syntax") {
        out.insert("props".into(), json!({ "language": syntax }));
    }
    copy_into(&mut out, block, &["attrs", "pos"]);
    Value::Object(out)
}

fn list_node(paragraph: &Value) -> Value {
    let mut out = Map::new();
    out.insert("type".into(), "block_extension".into());
    out.insert("name".into(), LIST.into());
    out.insert("version".into(), "1".into());
    out.insert("children".into(), json!([]));
    let children = paragraph.get("children").cloned().unwrap_or_default();
    out.insert("slots".into(), json!({ "lines": children }));
    out.insert("fallback".into(), json!([bare_fallback(paragraph)]));
    copy_into(&mut out, paragraph, &["attrs", "pos"]);
    Value::Object(out)
}

fn has_marker(node: &Value) -> bool {
    ty(node) == "code_block"
        && node
            .get("value")
            .and_then(Value::as_str)
            .is_some_and(|v| v.split('\n').any(|l| marker(l).is_some()))
}

fn bind(nodes: &mut [Value]) {
    let mut at = 0;
    while let Some(node) = nodes.get_mut(at) {
        if has_marker(node) {
            *node = code_node(node);
            if let Some(next) = nodes.get_mut(at + 1).filter(|n| is_list(n)) {
                *next = list_node(next);
                at += 1;
            }
        }
        at += 1;
    }
}

struct Transform;

impl DocumentTransform for Transform {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        let mut doc: Value = serde_json::from_str(&crate::api::encode_tree(&view))
            .map_err(|e| Error::option("documentTransforms", e.to_string()))?;
        let mut pending: Vec<&mut Value> = doc.get_mut("children").into_iter().collect();
        while let Some(v) = pending.pop() {
            match v {
                Value::Array(items) => {
                    if items.iter().any(|i| i.get("type").is_some()) {
                        bind(items);
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
        crate::api::standard::declare_names(
            &mut doc,
            &[(CODE, json!([])), (LIST, json!(["lines"]))],
        );
        let mut next = Tree::from_value(&doc)?;
        next.set_file(view.top.clone());
        Ok(next)
    }
}

struct Code;

impl Renderer for Code {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let body: Vec<String> = payload_text(node)
            .split('\n')
            .map(|line| {
                let Some((at, n)) = marker(line) else {
                    return cx.escape(line);
                };
                let before = line.get(..at).unwrap_or("");
                let text = before.trim_end();
                format!(
                    r#"{}{}<b class="callout" data-callout="{n}">{n}</b>"#,
                    cx.escape(text),
                    before.get(text.len()..).unwrap_or("")
                )
            })
            .collect();
        let code = node
            .pointer("/props/language")
            .and_then(Value::as_str)
            .map(|l| format!(r#" class="language-{}""#, cx.escape_attribute(l)))
            .unwrap_or_default();
        Ok(format!(
            "<pre{}><code{code}>{}</code></pre>",
            cx.attributes(node),
            body.join("\n")
        ))
    }
}

struct List;

impl Renderer for List {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let slot = node
            .pointer("/slots/lines")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let mut items = String::new();
        for line in lines(slot) {
            let Some((n, text)) = line.first().and_then(item_text).and_then(item) else {
                continue;
            };
            let mut rest = vec![json!({"type": "text", "value": text})];
            rest.extend(line.iter().skip(1).cloned());
            let _ = write!(
                items,
                r#"<li value="{n}">{}</li>"#,
                cx.render_inlines(&Value::Array(rest))?
            );
        }
        let attrs = node.get("attrs");
        let mut cls = vec![Value::from("callouts")];
        cls.extend(classes(attrs));
        Ok(format!(
            "<ol{}>{items}</ol>",
            cx.attributes(&with_attrs(node, cls, key_values(attrs)))
        ))
    }
}

/// `<n>` markers that end a code line become callout bubbles, and a
/// following `<n> text` paragraph their list. Both channels write the same
/// self-contained HTML (C2-X-052).
#[must_use]
pub fn code_callouts() -> Bundle {
    Bundle {
        extensions: vec![
            Extension {
                declaration: declaration(CODE, &[]),
                renderers: vec![
                    (Channel::Interactive, Box::new(Code)),
                    (Channel::BuildTime, Box::new(Code)),
                ],
                transform: Some(Box::new(Transform)),
            },
            Extension {
                declaration: declaration(LIST, &["lines"]),
                renderers: vec![
                    (Channel::Interactive, Box::new(List)),
                    (Channel::BuildTime, Box::new(List)),
                ],
                transform: None,
            },
        ],
        matchers: Vec::new(),
    }
}
