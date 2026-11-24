//! `heading_numbers` and `toc_placement` on the owned render view (carve-rs#4700).
//! The walks visit what the JSON `visit` and `rewrite` of the parent module
//! visit; extension slots stay JSON there and take the JSON forms.

use super::{
    CrossrefStyle, HeadingNumbersOptions, count_heading, freeze_heading_link, heading_link_targets,
    heading_title, nest, number_xrefs_json, place_toc, rewrite, title_xref_key, visit, visit_read,
};
use crate::ast::owned::{self, Attrs, Node};
use crate::ast::{DirectiveKind, XrefDisplay, XrefNamespace};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

enum At<'x> {
    Node(&'x mut Node),
    Slots(&'x mut Value),
}

/// Each node before what it holds, then its extension slots. `f` returns
/// false to leave what a node holds unvisited; `fallback` enters fallbacks.
fn walk(nodes: &mut [Node], fallback: bool, f: &mut dyn FnMut(At<'_>) -> bool) {
    for node in nodes {
        walk_node(node, fallback, f);
    }
}

fn walk_node(node: &mut Node, fallback: bool, f: &mut dyn FnMut(At<'_>) -> bool) {
    if !f(At::Node(node)) {
        return;
    }
    match node {
        Node::BlockExtension(e) if !fallback => walk(&mut e.children, fallback, f),
        Node::InlineExtension(e) if !fallback => walk(&mut e.children, fallback, f),
        _ => node.for_each_child_mut(&mut |child| walk_node(child, fallback, f)),
    }
    let slots = match node {
        Node::BlockExtension(e) => e.slots.as_mut(),
        Node::InlineExtension(e) => e.slots.as_mut(),
        _ => None,
    };
    if let Some(slots) = slots {
        f(At::Slots(slots));
    }
}

fn heading_id(h: &owned::Heading) -> Option<String> {
    h.attrs
        .as_ref()
        .and_then(|a| a.id.clone())
        .or_else(|| h.generated_id.as_deref().map(str::to_string))
}

/// The title nodes a copy of a heading keeps: no chrome, no footnotes.
fn derived(nodes: &[Node]) -> Vec<Node> {
    nodes
        .iter()
        .filter(|node| {
            !node
                .data()
                .and_then(Value::as_object)
                .is_some_and(|data| data.values().any(|v| v.as_str() == Some("chrome")))
                && !matches!(node, Node::FootnoteRef(_) | Node::InlineFootnote(_))
        })
        .cloned()
        .collect()
}

fn text(value: String, data: Option<Value>) -> Node {
    Node::Text(Box::new(owned::Text {
        value: value.into(),
        data: data.map(Box::new),
        ..Default::default()
    }))
}

fn to_json(nodes: &[Node]) -> Vec<Value> {
    nodes
        .iter()
        .filter_map(|n| serde_json::to_value(n).ok())
        .collect()
}

pub(super) fn freeze_heading_links(doc: &mut owned::Document) {
    let mut ids = HashSet::new();
    let mut definitions = HashSet::new();
    walk(&mut doc.children, false, &mut |at| {
        match at {
            At::Node(Node::Heading(h)) => {
                if let Some(id) = heading_id(h) {
                    ids.insert(id);
                }
            }
            At::Node(Node::LinkReferenceDefinition(d)) => {
                definitions.insert(d.key.trim().to_lowercase());
            }
            At::Node(_) => {}
            At::Slots(slots) => visit_read(slots, &mut |node| {
                heading_link_targets(node, &mut ids, &mut definitions);
            }),
        }
        true
    });
    if ids.is_empty() {
        return;
    }
    walk(&mut doc.children, false, &mut |at| {
        match at {
            At::Node(Node::Link(link)) => {
                if link.namespace.as_deref() != Some("link") {
                    return true;
                }
                let Some(key) = link.key.as_deref() else {
                    return true;
                };
                if definitions.contains(&key.trim().to_lowercase())
                    || !link
                        .href
                        .as_deref()
                        .and_then(|h| h.strip_prefix('#'))
                        .is_some_and(|id| ids.contains(id))
                {
                    return true;
                }
                link.key = None;
                link.namespace = None;
                link.ref_form = None;
                link.data = Some(Box::new(
                    json!({"carve.headingNumbers":"heading-reference"}),
                ));
            }
            At::Node(_) => {}
            At::Slots(slots) => visit(slots, &mut |node| {
                freeze_heading_link(node, &ids, &definitions);
            }),
        }
        true
    });
}

pub(super) fn number_headings(
    doc: &mut owned::Document,
    min_level: u8,
) -> BTreeMap<String, (String, Vec<Node>)> {
    let mut levels: Vec<usize> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    let mut labels = BTreeMap::new();
    // carve#6531: only top-level headings, the ones a section wraps (C2-H-026), and a number counts depth, not level.
    for node in &mut doc.children {
        let Node::Heading(h) = node else {
            continue;
        };
        let level = usize::try_from(h.level).unwrap_or(0);
        if level < usize::from(min_level.max(1)) || level > 6 {
            continue;
        }
        if h.attrs
            .as_ref()
            .is_some_and(|a| a.classes.iter().any(|c| c == "unnumbered"))
        {
            continue;
        }
        let number = count_heading(&mut levels, &mut counts, level);
        if let Some(id) = heading_id(h) {
            labels.insert(id.clone(), (number.clone(), derived(&h.children)));
            if h.attrs.as_ref().and_then(|a| a.id.as_ref()).is_none() {
                h.attrs.get_or_insert_with(Box::default).id = Some(id);
            }
        }
        let chrome = || Some(json!({"carve.headingNumbers":"chrome"}));
        h.children.insert(0, text(" ".into(), chrome()));
        h.children.insert(
            0,
            Node::Span(Box::new(owned::Span {
                data: chrome().map(Box::new),
                attrs: Some(Box::new(Attrs {
                    classes: vec!["section-number".into()],
                    ..Attrs::default()
                })),
                children: vec![text(number, None)],
                ..Default::default()
            })),
        );
    }
    labels
}

fn is_title_xref(x: &owned::Xref) -> bool {
    x.namespace == XrefNamespace::Heading && x.display.is_none_or(|d| d == XrefDisplay::Title)
}

pub(super) fn number_xrefs(
    doc: &mut owned::Document,
    labels: &BTreeMap<String, (String, Vec<Node>)>,
    titles: &mut Titles,
    options: &HeadingNumbersOptions,
) {
    if matches!(options.crossref, CrossrefStyle::Title) || labels.is_empty() {
        return;
    }
    let mut cx = Numbering {
        labels,
        copies: HashMap::new(),
        json_labels: None,
        titles,
        options,
    };
    for node in &mut doc.children {
        cx.node(node);
    }
}

/// The numbering walk: [`walk`] without fallbacks. It does not enter the
/// link a reference becomes, and the title it copies holds no reference
/// (carve#6540), so the output stays linear in the input.
struct Numbering<'x> {
    labels: &'x BTreeMap<String, (String, Vec<Node>)>,
    copies: HashMap<String, Vec<Node>>,
    json_labels: Option<BTreeMap<String, (String, Vec<Value>)>>,
    titles: &'x mut Titles,
    options: &'x HeadingNumbersOptions,
}

impl Numbering<'_> {
    fn node(&mut self, node: &mut Node) {
        if let Node::Xref(x) = &mut *node
            && is_title_xref(x)
            && let Some((number, title)) = self.labels.get(x.key.as_str())
        {
            let title = match self.options.crossref {
                CrossrefStyle::NumberTitle => {
                    let titles = &mut *self.titles;
                    self.copies
                        .entry(x.key.to_string())
                        .or_insert_with(|| titles.copy(title))
                        .as_slice()
                }
                _ => &[],
            };
            if let Some(link) = numbered(x, number, title, self.options) {
                *node = link;
            }
            return;
        }
        match node {
            Node::BlockExtension(e) => e.children.iter_mut().for_each(|c| self.node(c)),
            Node::InlineExtension(e) => e.children.iter_mut().for_each(|c| self.node(c)),
            _ => node.for_each_child_mut(&mut |c| self.node(c)),
        }
        let slots = match node {
            Node::BlockExtension(e) => e.slots.as_mut(),
            Node::InlineExtension(e) => e.slots.as_mut(),
            _ => None,
        };
        if let Some(slots) = slots {
            let (labels, titles) = (self.labels, &mut *self.titles);
            let labels = self.json_labels.get_or_insert_with(|| {
                labels
                    .iter()
                    .map(|(id, (n, title))| (id.clone(), (n.clone(), to_json(&titles.copy(title)))))
                    .collect()
            });
            number_xrefs_json(slots, labels, self.options);
        }
    }
}

/// The link a numbered cross-reference becomes; it keeps the xref's other fields.
fn numbered(
    x: &mut owned::Xref,
    number: &str,
    title: &[Node],
    options: &HeadingNumbersOptions,
) -> Option<Node> {
    let children = match options.crossref {
        CrossrefStyle::Number => vec![text(format!("{} {number}", options.label), None)],
        CrossrefStyle::NumberTitle => {
            std::iter::once(text(format!("{} {number} - ", options.label), None))
                .chain(title.iter().cloned())
                .collect()
        }
        CrossrefStyle::Title => return None,
    };
    Some(Node::Link(Box::new(owned::Link {
        attrs: x.attrs.take(),
        data: x.data.take(),
        dir: x.dir.take(),
        href: x.href.take(),
        lang: x.lang.take(),
        metadata: x.metadata.take(),
        pos: x.pos.take(),
        segments: x.segments.take(),
        children,
        ..Default::default()
    })))
}

/// The derived title of the first heading with each id, read before any
/// transform edits a title (C2-I-096), slots included.
pub(super) fn heading_texts(doc: &mut owned::Document) -> Titles {
    let mut by_id: BTreeMap<String, Vec<Node>> = BTreeMap::new();
    walk(&mut doc.children, false, &mut |at| {
        match at {
            At::Node(Node::Heading(h)) => {
                if let Some(id) = heading_id(h) {
                    by_id.entry(id).or_insert_with(|| derived(&h.children));
                }
            }
            At::Node(_) => {}
            At::Slots(slots) => {
                let mut found = BTreeMap::new();
                visit_read(slots, &mut |node| heading_title(node, &mut found));
                for (id, title) in found {
                    by_id.entry(id).or_insert_with(|| {
                        serde_json::from_value(Value::Array(title)).unwrap_or_default()
                    });
                }
            }
        }
        true
    });
    Titles::new(by_id)
}

/// Heading titles by id and the copies a cross-reference writes of them.
pub(super) struct Titles {
    texts: BTreeMap<String, Vec<Node>>,
    plain: HashMap<String, String>,
    frozen: HashMap<String, Vec<Node>>,
    frozen_json: HashMap<String, Vec<Value>>,
}

impl Titles {
    pub(super) fn new(texts: BTreeMap<String, Vec<Node>>) -> Self {
        Self {
            texts,
            plain: HashMap::new(),
            frozen: HashMap::new(),
            frozen_json: HashMap::new(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.texts.is_empty()
    }

    fn plain(&mut self, key: &str) -> Option<String> {
        if let Some(text) = self.plain.get(key) {
            return Some(text.clone());
        }
        let mut text = String::new();
        plain_text(self.texts.get(key)?, &mut text, 0);
        self.plain.insert(key.to_string(), text.clone());
        Some(text)
    }

    /// carve#6540: a copy of `title` in which a cross-reference to a heading
    /// is a link holding that heading's plain title, so nothing is copied again.
    pub(super) fn copy(&mut self, title: &[Node]) -> Vec<Node> {
        let mut nodes = title.to_vec();
        walk(&mut nodes, true, &mut |at| match at {
            At::Node(node) => {
                let Node::Xref(x) = &*node else {
                    return true;
                };
                if !is_title_xref(x) {
                    return true;
                }
                let Some(plain) = self.plain(x.key.as_str()) else {
                    return true;
                };
                *node = Node::Link(Box::new(owned::Link {
                    href: Some(format!("#{}", x.key.as_str()).into()),
                    attrs: x.attrs.clone(),
                    children: vec![text(plain, None)],
                    ..Default::default()
                }));
                false
            }
            At::Slots(slots) => {
                rewrite(slots, &mut Vec::new(), &mut |node, _| {
                    let key = title_xref_key(node)?;
                    let plain = self.plain(key)?;
                    Some(vec![json!({
                        "type": "link",
                        "href": format!("#{key}"),
                        "children": [{"type": "text", "value": plain}],
                    })])
                });
                true
            }
        });
        nodes
    }

    /// The children of the link a title cross-reference to `key` becomes.
    pub(super) fn frozen(&mut self, key: &str) -> Option<&[Node]> {
        if !self.frozen.contains_key(key) {
            let title = self.texts.get(key)?.clone();
            let copy = self.copy(&title);
            self.frozen.insert(key.to_string(), copy);
        }
        self.frozen.get(key).map(Vec::as_slice)
    }

    pub(super) fn frozen_json(&mut self, key: &str) -> Option<&[Value]> {
        if !self.frozen_json.contains_key(key) {
            let json = to_json(self.frozen(key)?);
            self.frozen_json.insert(key.to_string(), json);
        }
        self.frozen_json.get(key).map(Vec::as_slice)
    }
}

/// The plain text of owned inline nodes (C2-H-143), as [`crate::resolve::plain`]
/// reads borrowed ones.
fn plain_text(nodes: &[Node], out: &mut String, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    for node in nodes {
        if leaf_text(node, out) {
            continue;
        }
        match node {
            Node::Ruby(r) => {
                for pair in &r.pairs {
                    if let Node::RubyPair(p) = pair {
                        plain_text(&p.base, out, depth + 1);
                        out.push('(');
                        plain_text(&p.annotation, out, depth + 1);
                        out.push(')');
                    }
                }
            }
            Node::InlineExtension(e) if e.name.as_str() == "index" => {}
            Node::FootnoteRef(_)
            | Node::InlineFootnote(_)
            | Node::Xref(_)
            | Node::RawInline(_)
            | Node::CriticComment(_)
            | Node::Comment(_)
            | Node::CitationGroup(_)
            | Node::Symbol(_) => {}
            other => {
                if let Some(children) = other.children() {
                    plain_text(children, out, depth + 1);
                }
            }
        }
    }
}

/// The nodes that give a fixed text; true when `node` was one of them.
fn leaf_text(node: &Node, out: &mut String) -> bool {
    match node {
        Node::Text(t) => out.push_str(&t.value),
        Node::Code(t) => out.push_str(&t.value),
        Node::LiteralInline(t) => out.push_str(&t.value),
        Node::Math(t) => out.push_str(&t.value),
        Node::NonBreakingSpace(_) => out.push('\u{a0}'),
        Node::SoftBreak(_) | Node::HardBreak(_) => out.push(' '),
        Node::SmartPunctuation(s) => out.push_str(&s.glyph),
        Node::Image(i) => out.push_str(i.alt.as_deref().unwrap_or("")),
        Node::Autolink(a) => out.push_str(a.text.as_deref().unwrap_or(&a.href)),
        Node::Abbreviation(a) => out.push_str(&a.key),
        Node::Mention(m) => {
            out.push('@');
            out.push_str(&m.target);
        }
        Node::Tag(t) => {
            out.push('#');
            out.push_str(&t.name);
        }
        Node::CaptionNumber(c) => {
            if let Some(n) = c.n {
                out.push_str(&n.to_string());
            }
        }
        _ => return false,
    }
    true
}

pub(super) fn freeze_xrefs(doc: &mut owned::Document, titles: &mut Titles) {
    if titles.is_empty() {
        return;
    }
    walk(&mut doc.children, true, &mut |at| match at {
        At::Node(node) => {
            let Node::Xref(x) = &*node else {
                return true;
            };
            if !is_title_xref(x) {
                return true;
            }
            let key = x.key.to_string();
            let Some(children) = titles.frozen(&key) else {
                return true;
            };
            *node = Node::Link(Box::new(owned::Link {
                href: Some(format!("#{key}").into()),
                children: children.to_vec(),
                ..Default::default()
            }));
            false
        }
        At::Slots(slots) => {
            rewrite(slots, &mut Vec::new(), &mut |node, _| {
                let key = title_xref_key(node)?;
                let children = titles.frozen_json(key)?.to_vec();
                Some(vec![
                    json!({"type":"link","href":format!("#{key}"),"children":children}),
                ])
            });
            true
        }
    });
}

pub(super) fn place_tocs(doc: &mut owned::Document) {
    let entries: Vec<(u8, String, Vec<Node>)> = doc
        .children
        .iter()
        .filter_map(|node| {
            let Node::Heading(h) = node else {
                return None;
            };
            let level = u8::try_from(h.level).unwrap_or(0);
            Some((level, heading_id(h)?, derived(&h.children)))
        })
        .collect();
    let mut json_entries = None;
    let mut entries_json = || {
        json_entries
            .get_or_insert_with(|| {
                entries
                    .iter()
                    .map(|(level, id, title)| (*level, id.clone(), to_json(title)))
                    .collect::<Vec<_>>()
            })
            .clone()
    };
    walk(&mut doc.children, false, &mut |at| match at {
        At::Node(node) => {
            if !matches!(&*node, Node::Directive(d) if d.kind == DirectiveKind::Toc) {
                return true;
            }
            if let Node::Directive(d) = &mut *node
                && !needs_json(&d.children)
                && !d.title.as_deref().is_some_and(needs_json)
            {
                *node = placed(d, &entries);
                return false;
            }
            let entries = entries_json();
            if let Ok(mut value) = serde_json::to_value(&*node) {
                // The JSON walk also places a toc directive inside the new slots.
                visit(&mut value, &mut |n| place_toc(n, &entries));
                if let Ok(next) = serde_json::from_value(value) {
                    *node = next;
                }
            }
            false
        }
        At::Slots(slots) => {
            let entries = entries_json();
            visit(slots, &mut |n| place_toc(n, &entries));
            true
        }
    });
}

/// Whether a toc directive body needs the JSON walk: it holds another toc
/// directive or extension slots that the walk would enter.
fn needs_json(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| {
        matches!(node, Node::Directive(d) if d.kind == DirectiveKind::Toc)
            || matches!(node, Node::BlockExtension(e) if e.slots.is_some())
            || matches!(node, Node::InlineExtension(e) if e.slots.is_some())
            || {
                let mut found = false;
                node.for_each_child(&mut |c| found = found || needs_json(std::slice::from_ref(c)));
                found
            }
    })
}

/// [`super::list_node`] of owned entries, unordered.
fn list(entries: &[&(u8, String, Vec<Node>)]) -> Option<Node> {
    nest(
        entries.iter().map(|(level, id, title)| {
            let link = Node::Link(Box::new(owned::Link {
                href: Some(format!("#{id}").into()),
                children: title.clone(),
                ..Default::default()
            }));
            let paragraph = Node::Paragraph(Box::new(owned::Paragraph {
                children: vec![link],
                ..Default::default()
            }));
            let item = Node::ListItem(Box::new(owned::ListItem {
                children: vec![paragraph],
                ..Default::default()
            }));
            (*level, item)
        }),
        &|items| {
            Node::List(Box::new(owned::List {
                marker: owned::Marker {
                    kind: "bullet".into(),
                    bullet: Some("-".into()),
                    ..Default::default()
                },
                spacing: crate::ast::ListSpacing::Tight,
                children: items,
                ..Default::default()
            }))
        },
        &|item, list| {
            if let Some(children) = item.children_mut() {
                children.push(list);
            }
        },
    )
}

/// [`place_toc`] on an owned directive whose body needs no JSON walk.
fn placed(d: &mut owned::Directive, entries: &[(u8, String, Vec<Node>)]) -> Node {
    let mut attrs = d.attrs.take().map(|a| *a).unwrap_or_default();
    let value = |key: &str| {
        attrs
            .key_values
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.parse::<u8>().ok())
            .map(|n| n.clamp(1, 6))
    };
    let (low, high) = if value("from").is_some() || value("to").is_some() {
        let a = value("from").unwrap_or(1);
        let b = value("to").unwrap_or(6);
        (a.min(b), a.max(b))
    } else {
        (1, value("depth").unwrap_or(6))
    };
    let selected: Vec<_> = entries
        .iter()
        .filter(|(level, _, _)| *level >= low && *level <= high)
        .collect();
    let list = list(&selected);
    attrs.classes.retain(|c| c != "toc");
    attrs.classes.insert(0, "toc".into());
    attrs
        .key_values
        .retain(|(k, _)| !matches!(k.as_str(), "from" | "to" | "depth"));
    let title = d.title.take().unwrap_or_default();
    let body = std::mem::take(&mut d.children);
    let mut slots = Map::new();
    slots.insert("title".into(), Value::Array(to_json(&title)));
    slots.insert("body".into(), Value::Array(to_json(&body)));
    slots.insert(
        "list".into(),
        Value::Array(
            list.iter()
                .filter_map(|l| serde_json::to_value(l).ok())
                .collect(),
        ),
    );
    let mut fallback = Vec::new();
    if !title.is_empty() {
        fallback.push(Node::Paragraph(Box::new(owned::Paragraph {
            children: vec![Node::Strong(Box::new(owned::Strong {
                children: title,
                ..Default::default()
            }))],
            ..Default::default()
        })));
    }
    fallback.extend(body);
    fallback.extend(list);
    Node::BlockExtension(Box::new(owned::BlockExtension {
        name: "carve.tocPlacement".into(),
        version: "1".into(),
        attrs: Some(Box::new(attrs)),
        slots: Some(Value::Object(slots)),
        fallback,
        props: d
            .group_label
            .take()
            .map(|label| json!({"label": label.as_str()})),
        ..Default::default()
    }))
}
