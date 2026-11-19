//! The parsed HTML as an arena of nodes, with the source span of each node the
//! parser located.

use std::fmt::Write as _;

pub(crate) type Id = usize;

#[derive(Debug, Clone)]
pub(crate) enum Kind {
    Fragment,
    Doctype,
    Text(String),
    Comment(String),
    Element {
        name: String,
        html: bool,
        attrs: Vec<(String, String)>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct NodeData {
    pub kind: Kind,
    pub children: Vec<Id>,
    pub parent: Option<Id>,
    /// The content fragment of a `<template>`.
    pub content: Option<Id>,
    pub span: Option<(usize, usize)>,
}

static EMPTY: NodeData = NodeData {
    kind: Kind::Fragment,
    children: Vec::new(),
    parent: None,
    content: None,
    span: None,
};

#[derive(Debug, Default)]
pub(crate) struct Dom {
    pub nodes: Vec<NodeData>,
    pub root: Id,
}

const VOID: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];

const RAW_TEXT: &[&str] = &[
    "style",
    "script",
    "xmp",
    "iframe",
    "noembed",
    "noframes",
    "plaintext",
    "noscript",
];

impl Dom {
    pub fn push(&mut self, node: NodeData) -> Id {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    pub fn node(&self, id: Id) -> &NodeData {
        self.nodes.get(id).unwrap_or(&EMPTY)
    }

    pub fn children(&self, id: Id) -> &[Id] {
        &self.node(id).children
    }

    pub fn parent(&self, id: Id) -> Option<Id> {
        self.node(id).parent
    }

    pub fn tag(&self, id: Id) -> Option<&str> {
        match &self.node(id).kind {
            Kind::Element { name, .. } => Some(name),
            _ => None,
        }
    }

    pub fn is(&self, id: Id, name: &str) -> bool {
        self.tag(id) == Some(name)
    }

    pub fn is_element(&self, id: Id) -> bool {
        matches!(self.node(id).kind, Kind::Element { .. })
    }

    pub fn is_text(&self, id: Id) -> bool {
        matches!(self.node(id).kind, Kind::Text(_))
    }

    pub fn is_comment(&self, id: Id) -> bool {
        matches!(self.node(id).kind, Kind::Comment(_))
    }

    pub fn text(&self, id: Id) -> &str {
        match &self.node(id).kind {
            Kind::Text(t) => t,
            _ => "",
        }
    }

    pub fn set_text(&mut self, id: Id, value: String) {
        if let Some(NodeData {
            kind: Kind::Text(t),
            ..
        }) = self.nodes.get_mut(id)
        {
            *t = value;
        }
    }

    pub fn set_children(&mut self, id: Id, children: Vec<Id>) {
        if let Some(n) = self.nodes.get_mut(id) {
            n.children = children;
        }
    }

    pub fn attrs(&self, id: Id) -> &[(String, String)] {
        match &self.node(id).kind {
            Kind::Element { attrs, .. } => attrs,
            _ => &[],
        }
    }

    pub fn attr(&self, id: Id, name: &str) -> Option<&str> {
        self.attrs(id)
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn has(&self, id: Id, name: &str) -> bool {
        self.attrs(id).iter().any(|(n, _)| n == name)
    }

    pub fn elements(&self, id: Id) -> Vec<Id> {
        self.children(id)
            .iter()
            .copied()
            .filter(|&c| self.is_element(c))
            .collect()
    }

    /// Whitespace text, as JavaScript's `trim` sees it.
    pub fn is_blank(&self, id: Id) -> bool {
        self.is_text(id) && js_trim(self.text(id)).is_empty()
    }

    /// The text of a node and everything below it.
    pub fn plain(&self, id: Id) -> String {
        let mut out = String::new();
        let mut pending = vec![id];
        while let Some(n) = pending.pop() {
            match &self.node(n).kind {
                Kind::Text(t) => out.push_str(t),
                Kind::Element { .. } => pending.extend(self.children(n).iter().rev()),
                _ => {}
            }
        }
        out
    }

    /// The deepest nesting below `id`.
    pub fn depth(&self, id: Id) -> usize {
        let mut deepest = 0;
        let mut pending = vec![(id, 0usize)];
        while let Some((n, d)) = pending.pop() {
            deepest = deepest.max(d);
            let node = self.node(n);
            pending.extend(node.children.iter().map(|&c| (c, d + 1)));
            if let Some(c) = node.content {
                pending.push((c, d + 1));
            }
        }
        deepest
    }

    /// C2-M-081: the span of `id`, or of the nearest element above it the
    /// parser located.
    pub fn located(&self, id: Id) -> Option<(usize, usize)> {
        if let Some(span) = self.node(id).span {
            return Some(span);
        }
        let mut at = self.parent(id);
        while let Some(p) = at {
            if !self.is_element(p) {
                return None;
            }
            if let Some(span) = self.node(p).span {
                return Some(span);
            }
            at = self.parent(p);
        }
        None
    }

    /// The outer HTML of a node by the HTML fragment serialization algorithm,
    /// without the attributes `skip` names on the node itself.
    pub fn outer_html(&self, id: Id, skip: &dyn Fn(&str) -> bool) -> String {
        let mut out = String::new();
        self.serialize(id, Some(skip), &mut out);
        out
    }

    fn in_raw_text(&self, n: Id) -> bool {
        self.parent(n)
            .and_then(|p| match &self.node(p).kind {
                Kind::Element { name, html, .. } => {
                    Some(*html && RAW_TEXT.contains(&name.as_str()))
                }
                _ => None,
            })
            .unwrap_or(false)
    }

    fn serialize(&self, id: Id, skip: Option<&dyn Fn(&str) -> bool>, out: &mut String) {
        enum Step {
            Open(Id, bool),
            Close(String),
        }
        let mut pending = vec![Step::Open(id, true)];
        while let Some(step) = pending.pop() {
            let n = match step {
                Step::Close(name) => {
                    let _ = write!(out, "</{name}>");
                    continue;
                }
                Step::Open(n, top) => (n, top),
            };
            let (n, top) = n;
            match &self.node(n).kind {
                Kind::Text(t) => {
                    if self.in_raw_text(n) {
                        out.push_str(t);
                    } else {
                        escape(t, false, out);
                    }
                }
                Kind::Comment(c) => {
                    let _ = write!(out, "<!--{c}-->");
                }
                Kind::Doctype => out.push_str("<!DOCTYPE html>"),
                Kind::Fragment => {
                    pending.extend(self.children(n).iter().rev().map(|&c| Step::Open(c, false)));
                }
                Kind::Element { name, html, attrs } => {
                    start_tag(name, attrs, skip.filter(|_| top), out);
                    if *html && VOID.contains(&name.as_str()) {
                        continue;
                    }
                    pending.push(Step::Close(name.clone()));
                    let node = self.node(n);
                    let inner = if *html && name == "template" {
                        node.content.map_or(&[][..], |c| self.children(c))
                    } else {
                        &node.children
                    };
                    pending.extend(inner.iter().rev().map(|&c| Step::Open(c, false)));
                }
            }
        }
    }
}

fn start_tag(
    name: &str,
    attrs: &[(String, String)],
    skip: Option<&dyn Fn(&str) -> bool>,
    out: &mut String,
) {
    out.push('<');
    out.push_str(name);
    for (k, v) in attrs {
        if skip.is_some_and(|f| f(k)) {
            continue;
        }
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape(v, true, out);
        out.push('"');
    }
    out.push('>');
}

fn escape(s: &str, attribute: bool, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '\u{a0}' => out.push_str("&nbsp;"),
            '"' if attribute => out.push_str("&quot;"),
            '<' if !attribute => out.push_str("&lt;"),
            '>' if !attribute => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
}

/// JavaScript's `String.prototype.trim`.
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}
