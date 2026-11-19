//! `codeCallouts` (carve-js#5068): a document transform (C2-X-053).

use super::{Bundle, classes, declaration, key_values, payload_text, with_attrs};
use crate::api::{Channel, DocumentTransform, Error, Extension, RenderContext, Renderer, Tree};
use crate::ast::owned::{self, Node};
use serde_json::{Value, json};
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

fn is_list(node: &Node) -> bool {
    let Node::Paragraph(paragraph) = node else {
        return false;
    };
    if paragraph.role.is_some() || paragraph.caption.is_some() {
        return false;
    }
    let mut first = true;
    let mut count = 0;
    for child in &paragraph.children {
        match child {
            Node::SoftBreak(_) => first = true,
            Node::Text(text) if first && item(&text.value).is_some() => {
                first = false;
                count += 1;
            }
            _ if first => return false,
            _ => {}
        }
    }
    count > 0
}

fn code_node(block: &owned::CodeBlock) -> Node {
    let mut fallback = block.clone();
    fallback.attrs = None;
    Node::BlockExtension(Box::new(owned::BlockExtension {
        name: CODE.into(),
        version: "1".into(),
        attrs: block.attrs.clone(),
        pos: block.pos,
        payload: Some(json!([{"format": "text/plain", "value": block.value}])),
        props: block
            .syntax
            .as_ref()
            .map(|language| json!({"language": language})),
        fallback: vec![Node::CodeBlock(Box::new(fallback))],
        ..Default::default()
    }))
}

fn list_node(paragraph: &owned::Paragraph) -> Node {
    let mut fallback = paragraph.clone();
    fallback.attrs = None;
    Node::BlockExtension(Box::new(owned::BlockExtension {
        name: LIST.into(),
        version: "1".into(),
        attrs: paragraph.attrs.clone(),
        pos: paragraph.pos,
        slots: Some(json!({"lines": paragraph.children})),
        fallback: vec![Node::Paragraph(Box::new(fallback))],
        ..Default::default()
    }))
}

fn has_marker(node: &Node) -> bool {
    matches!(node, Node::CodeBlock(code) if code.value.split('\n').any(|line| marker(line).is_some()))
}

fn bind(nodes: &mut [Node]) {
    let mut at = 0;
    while let Some(node) = nodes.get_mut(at) {
        if has_marker(node) {
            let Node::CodeBlock(code) = node else {
                unreachable!()
            };
            *node = code_node(code);
            if let Some(next) = nodes.get_mut(at + 1).filter(|n| is_list(n)) {
                let Node::Paragraph(paragraph) = next else {
                    unreachable!()
                };
                *next = list_node(paragraph);
                at += 1;
            }
        }
        at += 1;
    }
}

struct Transform;

fn any_marker(nodes: &[Node], depth: usize) -> bool {
    if depth > crate::resolve::WALK_CAP {
        return false;
    }
    nodes.iter().any(|node| {
        has_marker(node)
            || node
                .children()
                .is_some_and(|children| any_marker(children, depth + 1))
            || match node {
                Node::BlockExtension(ext) => ext
                    .slots
                    .as_ref()
                    .is_some_and(|slots| slot_has_marker(slots)),
                Node::InlineExtension(ext) => ext
                    .slots
                    .as_ref()
                    .is_some_and(|slots| slot_has_marker(slots)),
                _ => false,
            }
    })
}

fn slot_has_marker(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            (map.get("type").and_then(Value::as_str) == Some("code_block")
                && map
                    .get("value")
                    .and_then(Value::as_str)
                    .is_some_and(|text| text.split('\n').any(|line| marker(line).is_some())))
                || map.values().any(slot_has_marker)
        }
        Value::Array(items) => items.iter().any(slot_has_marker),
        _ => false,
    }
}

fn bind_slot(value: &mut Value, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    match value {
        Value::Array(items) if items.iter().any(|item| item.get("type").is_some()) => {
            if let Ok(mut nodes) = serde_json::from_value::<Vec<Node>>(Value::Array(items.clone()))
            {
                bind_tree(&mut nodes, depth + 1);
                if let Ok(updated) = serde_json::to_value(nodes) {
                    *value = updated;
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                bind_slot(item, depth + 1);
            }
        }
        Value::Object(map) => {
            for child in map.values_mut() {
                bind_slot(child, depth + 1);
            }
        }
        _ => {}
    }
}

fn bind_tree(nodes: &mut Vec<Node>, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    bind(nodes);
    for node in nodes {
        if let Some(children) = node.children_mut() {
            bind_tree(children, depth + 1);
        }
        match node {
            Node::BlockExtension(ext) => {
                if let Some(slots) = &mut ext.slots {
                    bind_slot(slots, depth + 1);
                }
            }
            Node::InlineExtension(ext) => {
                if let Some(slots) = &mut ext.slots {
                    bind_slot(slots, depth + 1);
                }
            }
            _ => {}
        }
    }
}

impl DocumentTransform for Transform {
    fn transform(&self, view: Tree) -> Result<Tree, Error> {
        if !any_marker(&view.document().children, 0) {
            return Ok(view);
        }
        let top = view.top.clone();
        let mut doc = view.into_document();
        self.transform_owned(&mut doc);
        Tree::from_owned(doc, top)
    }

    fn supports_owned(&self) -> bool {
        true
    }

    fn transform_owned(&self, doc: &mut owned::Document) {
        if !any_marker(&doc.children, 0) {
            return;
        }
        bind_tree(&mut doc.children, 0);
        let mut declarations = json!({"extensions": doc.extensions});
        crate::api::standard::declare_names(
            &mut declarations,
            &[(CODE, json!([])), (LIST, json!(["lines"]))],
        );
        doc.extensions = declarations.get_mut("extensions").cloned();
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
        host: super::HostFields::default(),
        matchers: Vec::new(),
    }
}
