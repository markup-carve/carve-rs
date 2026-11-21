//! `codeCallouts` (carve-js#5068): a document transform (C2-X-053).

use super::{
    Bundle, Parts, classes, declaration, key_values, payload_text, typed, typed_payload, with_attrs,
};
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
            if let Node::CodeBlock(code) = node {
                *node = code_node(code);
            }
            if let Some(next) = nodes.get_mut(at + 1).filter(|n| is_list(n)) {
                if let Node::Paragraph(paragraph) = next {
                    *next = list_node(paragraph);
                }
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
                Node::BlockExtension(ext) => ext.slots.as_ref().is_some_and(slot_has_marker),
                Node::InlineExtension(ext) => ext.slots.as_ref().is_some_and(slot_has_marker),
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
                || map
                    .iter()
                    .any(|(key, child)| descends_slot(key) && slot_has_marker(child))
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
            for (key, child) in map.iter_mut() {
                if descends_slot(key) {
                    bind_slot(child, depth + 1);
                }
            }
        }
        _ => {}
    }
}

fn descends_slot(key: &str) -> bool {
    !matches!(
        key,
        "fallback" | "source" | "payload" | "props" | "pos" | "attrs" | "data"
    )
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
        doc.extensions = declared(doc.extensions.as_ref());
    }

    // The binding reads code text and paragraph shapes, no resolution result.
    fn supports_parsed_arena(&self) -> bool {
        true
    }

    fn transform_parsed_arena<'a>(
        &self,
        doc: &mut crate::ast::Document<'a>,
        arena: &'a crate::Arena,
    ) {
        if !arena::any_marker(doc.children, 0) {
            return;
        }
        arena::bind_tree(doc.children, arena, 0);
        doc.extensions = declared(doc.extensions).map(|v| arena.json(v));
    }
}

/// The `extensions` of a document with the two callout extensions declared.
fn declared(extensions: Option<&Value>) -> Option<Value> {
    let mut declarations = json!({"extensions": extensions});
    crate::api::standard::declare_names(
        &mut declarations,
        &[(CODE, json!([])), (LIST, json!(["lines"]))],
    );
    declarations.get_mut("extensions").cloned()
}

/// The binding of the transform on a tree in an arena: a node it rewrites
/// is built as the owned binding builds it and copied into the arena.
mod arena {
    use super::{item, marker, slot_has_marker};
    use crate::ast::Node;
    use serde_json::json;

    fn has_marker(node: &Node<'_>) -> bool {
        matches!(node, Node::CodeBlock(code) if code.value.split('\n').any(|line| marker(line).is_some()))
    }

    fn is_list(node: &Node<'_>) -> bool {
        let Node::Paragraph(paragraph) = node else {
            return false;
        };
        if paragraph.role.is_some() || paragraph.caption.is_some() {
            return false;
        }
        let mut first = true;
        let mut count = 0;
        for child in &*paragraph.children {
            match child {
                Node::SoftBreak(_) => first = true,
                Node::Text(text) if first && item(text.value).is_some() => {
                    first = false;
                    count += 1;
                }
                _ if first => return false,
                _ => {}
            }
        }
        count > 0
    }

    pub(super) fn any_marker(nodes: &[Node<'_>], depth: usize) -> bool {
        if depth > crate::resolve::WALK_CAP {
            return false;
        }
        nodes.iter().any(|node| {
            has_marker(node)
                || node
                    .children()
                    .is_some_and(|children| any_marker(children, depth + 1))
                || match node {
                    Node::BlockExtension(ext) => ext.slots.is_some_and(slot_has_marker),
                    Node::InlineExtension(ext) => ext.slots.is_some_and(slot_has_marker),
                    _ => false,
                }
        })
    }

    /// [`super::code_node`] in the arena; the code block moves into the fallback.
    fn code_node<'a>(node: &mut Node<'a>, arena: &'a crate::Arena) {
        let Node::CodeBlock(code) = node else {
            return;
        };
        let ext = crate::ast::BlockExtension {
            name: super::CODE,
            version: "1",
            attrs: code.attrs,
            pos: code.pos,
            payload: Some(arena.json(json!([{"format": "text/plain", "value": code.value}]))),
            props: code
                .syntax
                .map(|language| arena.json(json!({"language": language}))),
            ..Default::default()
        };
        let ext = arena.alloc(ext);
        ext.fallback = crate::api::extensions::moved_fallback(node, arena);
        *node = Node::BlockExtension(ext);
    }

    /// [`super::list_node`] in the arena; the paragraph moves into the fallback.
    fn list_node<'a>(node: &mut Node<'a>, arena: &'a crate::Arena) {
        let Node::Paragraph(paragraph) = node else {
            return;
        };
        let Ok(lines) = serde_json::to_value(&*paragraph.children) else {
            return;
        };
        let ext = crate::ast::BlockExtension {
            name: super::LIST,
            version: "1",
            attrs: paragraph.attrs,
            pos: paragraph.pos,
            slots: Some(arena.json(json!({"lines": lines}))),
            ..Default::default()
        };
        let ext = arena.alloc(ext);
        ext.fallback = crate::api::extensions::moved_fallback(node, arena);
        *node = Node::BlockExtension(ext);
    }

    fn bind<'a>(nodes: &mut [Node<'a>], arena: &'a crate::Arena) {
        let mut at = 0;
        while let Some(node) = nodes.get_mut(at) {
            if has_marker(node) {
                code_node(node, arena);
                if let Some(next) = nodes.get_mut(at + 1).filter(|n| is_list(n)) {
                    list_node(next, arena);
                    at += 1;
                }
            }
            at += 1;
        }
    }

    pub(super) fn bind_tree<'a>(nodes: &mut [Node<'a>], arena: &'a crate::Arena, depth: usize) {
        if depth > crate::resolve::WALK_CAP {
            return;
        }
        bind(nodes, arena);
        for node in nodes {
            let slots = match node {
                Node::BlockExtension(ext) => ext.slots.is_some(),
                Node::InlineExtension(ext) => ext.slots.is_some(),
                _ => false,
            };
            if slots {
                // Nothing to bind in it: the owned binding would leave it as it is.
                if !any_marker(std::slice::from_ref(node), depth) {
                    continue;
                }
                // A node with slots binds as the owned tree does; its own
                // level is bound above, so `bind` finds nothing on it again.
                let mut one = vec![node.owned()];
                super::bind_tree(&mut one, depth);
                if let Some(bound) = one.first() {
                    *node = bound.in_arena(arena);
                }
            } else if let Some(children) = node.children_mut() {
                bind_tree(children, arena, depth + 1);
            }
        }
    }
}

struct Code;

impl Code {
    fn html(
        payload: &str,
        language: Option<&str>,
        attributes: &str,
        cx: &RenderContext<'_>,
    ) -> String {
        let body: Vec<String> = payload
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
        let code = language
            .map(|l| format!(r#" class="language-{}""#, cx.escape_attribute(l)))
            .unwrap_or_default();
        format!(
            "<pre{attributes}><code{code}>{}</code></pre>",
            body.join("\n")
        )
    }
}

impl Renderer for Code {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        Ok(Self::html(
            payload_text(node),
            node.pointer("/props/language").and_then(Value::as_str),
            &cx.attributes(node),
            cx,
        ))
    }

    fn render_typed(
        &self,
        node: &crate::ast::Node<'_>,
        cx: &RenderContext<'_>,
    ) -> Option<Result<String, Error>> {
        let node = typed(node)?;
        Some(Ok(Self::html(
            typed_payload(node),
            node.props
                .and_then(|p| p.get("language"))
                .and_then(Value::as_str),
            &cx.write_parts(&Parts::of(node.attrs), node.pos.as_ref()),
            cx,
        )))
    }
}

struct List;

impl List {
    fn items(slots: Option<&Value>, cx: &RenderContext<'_>) -> Result<String, Error> {
        let slot = slots
            .and_then(|s| s.get("lines"))
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
        Ok(items)
    }
}

impl Renderer for List {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        let items = Self::items(node.get("slots"), cx)?;
        let attrs = node.get("attrs");
        let mut cls = vec![Value::from("callouts")];
        cls.extend(classes(attrs));
        Ok(format!(
            "<ol{}>{items}</ol>",
            cx.attributes(&with_attrs(node, cls, key_values(attrs)))
        ))
    }

    fn render_typed(
        &self,
        node: &crate::ast::Node<'_>,
        cx: &RenderContext<'_>,
    ) -> Option<Result<String, Error>> {
        let node = typed(node)?;
        Some(Self::items(node.slots, cx).map(|items| {
            let parts = Parts::of(node.attrs).with_classes(&["callouts"]);
            format!(
                "<ol{}>{items}</ol>",
                cx.write_parts(&parts, node.pos.as_ref())
            )
        }))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_transform_skips_fallback_and_metadata() {
        let code = json!({"type": "code_block", "value": "x <1>"});
        let mut slots = json!({"body": [{
            "type": "block_extension", "name": "sample.outer", "version": "1",
            "children": [], "fallback": [code.clone()],
            "props": {"shadow": code.clone()},
            "slots": {"nested": [code]}
        }]});
        bind_slot(&mut slots, 0);
        assert_eq!(
            slots.pointer("/body/0/fallback/0/type"),
            Some(&json!("code_block"))
        );
        assert_eq!(
            slots.pointer("/body/0/props/shadow/type"),
            Some(&json!("code_block"))
        );
        assert_eq!(
            slots.pointer("/body/0/slots/nested/0/type"),
            Some(&json!("block_extension"))
        );
    }
}
