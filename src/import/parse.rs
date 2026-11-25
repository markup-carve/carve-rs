//! HTML parsing with source positions (C2-M-002, C2-M-081). html5ever locates
//! no node, so the tokenizer is fed one chunk per `<` and each token's end is
//! read from the input it left unconsumed.

use super::dom::{Dom, Id, Kind, NodeData};
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::tree_builder::{
    ElemName, ElementFlags, NodeOrText, QuirksMode, TreeBuilder, TreeBuilderOpts, TreeSink,
};
use html5ever::{Attribute, LocalName, Namespace, QualName, local_name, ns};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Current {
    None,
    Start(usize, usize, bool),
    End(usize, usize),
    Other(usize, usize),
    Chars(usize, usize),
}

struct Tracker {
    queue: BufferQueue,
    chunk: Cell<(usize, usize, bool)>,
    chunk_spoiled: Cell<bool>,
    tag_start: Cell<Option<usize>>,
    last_end: Cell<usize>,
    current: Cell<Current>,
    current_name: RefCell<Option<LocalName>>,
    /// Located elements no end tag has closed yet, in creation order.
    open: RefCell<Vec<(Id, LocalName)>>,
    input: Rc<str>,
}

impl Tracker {
    fn consumed(&self) -> usize {
        let (_, end, _) = self.chunk.get();
        let left = self.queue.peek_front_chunk_mut().map_or(0, |c| c.len());
        end.saturating_sub(left)
    }

    fn current_span(&self) -> Option<(usize, usize)> {
        match self.current.get() {
            Current::None => None,
            Current::Start(s, e, _)
            | Current::End(s, e)
            | Current::Other(s, e)
            | Current::Chars(s, e) => Some((s, e)),
        }
    }

    fn tag_start(&self, end: usize) -> usize {
        self.tag_start.take().unwrap_or_else(|| {
            self.input
                .get(..end)
                .and_then(|s| s.rfind('<'))
                .unwrap_or(0)
        })
    }
}

/// A node as the tree builder leaves it, before [`arena`] puts the reachable ones in order.
enum Built {
    Document,
    Doctype,
    Text(String),
    Comment(String),
    Element {
        name: QualName,
        attrs: Vec<Attribute>,
        template: Option<Id>,
        mathml: bool,
    },
}

struct Node {
    data: Built,
    children: Vec<Id>,
    parent: Option<Id>,
    span: Option<(usize, usize)>,
}

/// The tree builder's sink: an arena whose handles are indices, with the tree operations of
/// `markup5ever_rcdom`, so no reference-counted tree is built and copied.
struct Sink {
    nodes: RefCell<Vec<Node>>,
    t: Rc<Tracker>,
}

#[derive(Debug)]
struct Name {
    ns: Namespace,
    local: LocalName,
}

impl ElemName for Name {
    fn ns(&self) -> &Namespace {
        &self.ns
    }
    fn local_name(&self) -> &LocalName {
        &self.local
    }
}

const DOCUMENT: Id = 0;

impl Sink {
    fn new(t: Rc<Tracker>) -> Self {
        let sink = Self {
            nodes: RefCell::new(Vec::new()),
            t,
        };
        sink.push(Built::Document);
        sink
    }

    fn push(&self, data: Built) -> Id {
        let mut nodes = self.nodes.borrow_mut();
        nodes.push(Node {
            data,
            children: Vec::new(),
            parent: None,
            span: None,
        });
        nodes.len() - 1
    }

    fn set_span(&self, id: Id, span: (usize, usize)) {
        if let Some(n) = self.nodes.borrow_mut().get_mut(id) {
            n.span = Some(span);
        }
    }

    /// The located token reaches the text node `id`: its span starts at the first and ends at the last.
    fn text(&self, id: Id) {
        let Some((start, end)) = self.t.current_span() else {
            return;
        };
        if let Some(n) = self.nodes.borrow_mut().get_mut(id) {
            n.span = Some(n.span.map_or((start, end), |(s, e)| (s, e.max(end))));
        }
    }

    fn detach(nodes: &mut [Node], child: Id) {
        let Some(parent) = nodes.get_mut(child).and_then(|n| n.parent.take()) else {
            return;
        };
        if let Some(p) = nodes.get_mut(parent)
            && let Some(i) = p.children.iter().position(|&c| c == child)
        {
            p.children.remove(i);
        }
    }

    fn attach(&self, parent: Id, child: Id) {
        let mut nodes = self.nodes.borrow_mut();
        Self::detach(&mut nodes, child);
        if let Some(c) = nodes.get_mut(child) {
            c.parent = Some(parent);
        }
        if let Some(p) = nodes.get_mut(parent) {
            p.children.push(child);
        }
    }

    /// Appends `text` to `id` when that is a text node.
    fn merge_text(&self, id: Option<Id>, text: &str) -> bool {
        let mut nodes = self.nodes.borrow_mut();
        match id.and_then(|id| nodes.get_mut(id)) {
            Some(Node {
                data: Built::Text(t),
                ..
            }) => {
                t.push_str(text);
                true
            }
            _ => false,
        }
    }

    fn last_child(&self, parent: Id) -> Option<Id> {
        self.nodes
            .borrow()
            .get(parent)
            .and_then(|p| p.children.last().copied())
    }
}

impl TreeSink for Sink {
    type Handle = Id;
    type Output = Self;
    type ElemName<'a> = Name;

    fn finish(self) -> Self {
        self
    }
    fn parse_error(&self, _msg: Cow<'static, str>) {}
    fn get_document(&self) -> Id {
        DOCUMENT
    }
    fn elem_name<'a>(&'a self, target: &'a Id) -> Name {
        match self.nodes.borrow().get(*target).map(|n| &n.data) {
            Some(Built::Element { name, .. }) => Name {
                ns: name.ns.clone(),
                local: name.local.clone(),
            },
            _ => Name {
                ns: ns!(),
                local: local_name!(""),
            },
        }
    }
    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> Id {
        let local = name.local.clone();
        let template = flags.template.then(|| self.push(Built::Document));
        let h = self.push(Built::Element {
            name,
            attrs,
            template,
            mathml: flags.mathml_annotation_xml_integration_point,
        });
        if let Current::Start(s, e, false) = self.t.current.get()
            && self.t.current_name.borrow().as_ref() == Some(&local)
        {
            self.t.current.set(Current::Start(s, e, true));
            self.set_span(h, (s, e));
            self.t.open.borrow_mut().push((h, local));
        }
        h
    }
    fn create_comment(&self, text: StrTendril) -> Id {
        let h = self.push(Built::Comment(text.to_string()));
        if let Current::Other(s, e) = self.t.current.get() {
            self.set_span(h, (s, e));
        }
        h
    }
    fn create_pi(&self, _target: StrTendril, data: StrTendril) -> Id {
        self.push(Built::Comment(data.to_string()))
    }
    fn append(&self, parent: &Id, child: NodeOrText<Id>) {
        match child {
            NodeOrText::AppendText(text) => {
                let last = self.last_child(*parent);
                if !self.merge_text(last, &text) {
                    let node = self.push(Built::Text(text.to_string()));
                    self.attach(*parent, node);
                }
                if let Some(last) = self.last_child(*parent) {
                    self.text(last);
                }
            }
            NodeOrText::AppendNode(node) => self.attach(*parent, node),
        }
    }
    fn append_based_on_parent_node(&self, element: &Id, prev_element: &Id, child: NodeOrText<Id>) {
        let has_parent = self
            .nodes
            .borrow()
            .get(*element)
            .is_some_and(|n| n.parent.is_some());
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }
    fn append_doctype_to_document(
        &self,
        _name: StrTendril,
        _public: StrTendril,
        _system: StrTendril,
    ) {
        let h = self.push(Built::Doctype);
        self.attach(DOCUMENT, h);
        if let Some(span) = self.t.current_span() {
            self.set_span(h, span);
        }
    }
    fn get_template_contents(&self, target: &Id) -> Id {
        match self.nodes.borrow().get(*target).map(|n| &n.data) {
            Some(Built::Element {
                template: Some(c), ..
            }) => *c,
            _ => *target,
        }
    }
    fn same_node(&self, x: &Id, y: &Id) -> bool {
        x == y
    }
    fn set_quirks_mode(&self, _mode: QuirksMode) {}
    fn append_before_sibling(&self, sibling: &Id, new_node: NodeOrText<Id>) {
        let place = {
            let nodes = self.nodes.borrow();
            nodes.get(*sibling).and_then(|n| n.parent).and_then(|p| {
                let i = nodes.get(p)?.children.iter().position(|c| c == sibling)?;
                Some((p, i))
            })
        };
        let Some((parent, i)) = place else {
            return;
        };
        let child = match new_node {
            NodeOrText::AppendText(text) => {
                let prev = i
                    .checked_sub(1)
                    .and_then(|j| self.nodes.borrow().get(parent)?.children.get(j).copied());
                if self.merge_text(prev, &text) {
                    if let Some(prev) = prev {
                        self.text(prev);
                    }
                    return;
                }
                let node = self.push(Built::Text(text.to_string()));
                // The tree builder promises no text node after the insertion point.
                let mut nodes = self.nodes.borrow_mut();
                Self::detach(&mut nodes, node);
                drop(nodes);
                self.insert(parent, i, node);
                self.text(node);
                return;
            }
            NodeOrText::AppendNode(node) => node,
        };
        let mut nodes = self.nodes.borrow_mut();
        Self::detach(&mut nodes, child);
        drop(nodes);
        self.insert(parent, i, child);
    }
    fn add_attrs_if_missing(&self, target: &Id, attrs: Vec<Attribute>) {
        if let Some(Node {
            data: Built::Element {
                attrs: existing, ..
            },
            ..
        }) = self.nodes.borrow_mut().get_mut(*target)
        {
            let names: HashSet<QualName> = existing.iter().map(|e| e.name.clone()).collect();
            existing.extend(attrs.into_iter().filter(|a| !names.contains(&a.name)));
        }
    }
    fn remove_from_parent(&self, target: &Id) {
        Self::detach(&mut self.nodes.borrow_mut(), *target);
    }
    fn reparent_children(&self, node: &Id, new_parent: &Id) {
        let mut nodes = self.nodes.borrow_mut();
        let moved = nodes
            .get_mut(*node)
            .map(|n| std::mem::take(&mut n.children))
            .unwrap_or_default();
        for &c in &moved {
            if let Some(c) = nodes.get_mut(c) {
                c.parent = Some(*new_parent);
            }
        }
        if let Some(p) = nodes.get_mut(*new_parent) {
            p.children.extend(moved);
        }
    }
    fn is_mathml_annotation_xml_integration_point(&self, handle: &Id) -> bool {
        matches!(
            self.nodes.borrow().get(*handle).map(|n| &n.data),
            Some(Built::Element { mathml: true, .. })
        )
    }
    fn allow_declarative_shadow_roots(&self, _intended_parent: &Id) -> bool {
        false
    }
}

impl Sink {
    fn insert(&self, parent: Id, at: usize, child: Id) {
        let mut nodes = self.nodes.borrow_mut();
        if let Some(c) = nodes.get_mut(child) {
            c.parent = Some(parent);
        }
        if let Some(p) = nodes.get_mut(parent) {
            let at = at.min(p.children.len());
            p.children.insert(at, child);
        }
    }
}

struct Located {
    tb: TreeBuilder<Id, Sink>,
    t: Rc<Tracker>,
}

impl TokenSink for Located {
    type Handle = Id;

    fn process_token(&self, token: Token, line: u64) -> TokenSinkResult<Id> {
        let t = &self.t;
        let end = t.consumed();
        match &token {
            Token::TagToken(tag) => {
                let start = t.tag_start(end);
                *t.current_name.borrow_mut() = Some(tag.name.clone());
                t.current.set(match tag.kind {
                    TagKind::StartTag => Current::Start(start, end, false),
                    TagKind::EndTag => Current::End(start, end),
                });
                t.last_end.set(end);
                t.chunk_spoiled.set(true);
            }
            Token::CommentToken(_) | Token::DoctypeToken(_) => {
                let start = t.tag_start(end);
                t.current.set(Current::Other(start, end));
                t.last_end.set(end);
                t.chunk_spoiled.set(true);
            }
            Token::CharacterTokens(text) => {
                if text.contains('<') {
                    t.chunk_spoiled.set(true);
                }
                self.chars(end);
            }
            Token::NullCharacterToken => self.chars(end),
            Token::EOFToken | Token::ParseError(_) => {}
        }
        let result = self.tb.process_token(token, line);
        if let Current::End(_, e) = t.current.get() {
            self.close(e);
        }
        t.current.set(Current::None);
        result
    }

    fn end(&self) {
        self.tb.end();
    }

    fn adjusted_current_node_present_but_not_in_html_namespace(&self) -> bool {
        self.tb
            .adjusted_current_node_present_but_not_in_html_namespace()
    }
}

impl Located {
    /// The end of an element is that of the end tag that closes it.
    fn close(&self, end: usize) {
        let t = &self.t;
        let name = t.current_name.borrow().clone();
        let mut open = t.open.borrow_mut();
        let Some(i) = open.iter().rposition(|(_, n)| Some(n) == name.as_ref()) else {
            return;
        };
        let (k, _) = open.remove(i);
        if let Some(n) = self.tb.sink.nodes.borrow_mut().get_mut(k)
            && let Some(span) = n.span.as_mut()
        {
            span.1 = end;
        }
    }

    fn chars(&self, end: usize) {
        let t = &self.t;
        let start = t.last_end.get().min(end);
        t.current.set(Current::Chars(start, end));
        t.last_end.set(end);
        let (chunk_start, _, lt) = t.chunk.get();
        t.tag_start
            .set((lt && !t.chunk_spoiled.get()).then_some(chunk_start));
    }
}

fn tracker(input: &str) -> Rc<Tracker> {
    Rc::new(Tracker {
        queue: BufferQueue::default(),
        chunk: Cell::new((0, 0, false)),
        chunk_spoiled: Cell::new(false),
        tag_start: Cell::new(None),
        last_end: Cell::new(0),
        current: Cell::new(Current::None),
        current_name: RefCell::new(None),
        open: RefCell::new(Vec::new()),
        input: Rc::from(input),
    })
}

fn run(input: &str, tok: &Tokenizer<Located>, t: &Tracker) {
    // One shared buffer: each chunk is a view of it, not a copy.
    let whole = StrTendril::from_slice(input);
    let mut at = 0;
    while at < input.len() {
        let next = input
            .get(at + 1..)
            .and_then(|rest| rest.find('<'))
            .map_or(input.len(), |i| at + 1 + i);
        let lt = input.as_bytes().get(at) == Some(&b'<');
        t.chunk.set((at, next, lt));
        t.chunk_spoiled.set(false);
        if lt && t.tag_start.get().is_none() {
            t.tag_start.set(Some(at));
        }
        let chunk = match (u32::try_from(at), u32::try_from(next - at)) {
            (Ok(from), Ok(len)) => whole.subtendril(from, len),
            _ => StrTendril::from_slice(input.get(at..next).unwrap_or("")),
        };
        t.queue.push_back(chunk);
        while let html5ever::TokenizerResult::Script(_) = tok.feed(&t.queue) {}
        at = next;
    }
    tok.end();
}

fn opts() -> TreeBuilderOpts {
    TreeBuilderOpts {
        scripting_enabled: true,
        ..TreeBuilderOpts::default()
    }
}

/// A document (C2-M-002): the children of the document node.
pub(crate) fn document(input: &str) -> Dom {
    let t = tracker(input);
    let tb = TreeBuilder::new(Sink::new(t.clone()), opts());
    let tok = Tokenizer::new(Located { tb, t: t.clone() }, TokenizerOpts::default());
    run(input, &tok, &t);
    let nodes = tok.sink.tb.sink.nodes.take();
    arena(nodes, None)
}

/// A fragment (C2-M-002) with a `body` element as the context.
pub(crate) fn fragment(input: &str) -> Dom {
    let t = tracker(input);
    let sink = Sink::new(t.clone());
    let context = html5ever::tree_builder::create_element(
        &sink,
        QualName::new(None, ns!(html), local_name!("body")),
        Vec::new(),
    );
    let tb = TreeBuilder::new_for_fragment(sink, context, None, opts());
    let tokenizer = TokenizerOpts {
        initial_state: Some(tb.tokenizer_state_for_context_elem(true)),
        ..TokenizerOpts::default()
    };
    let tok = Tokenizer::new(Located { tb, t: t.clone() }, tokenizer);
    run(input, &tok, &t);
    let nodes = tok.sink.tb.sink.nodes.take();
    let root = nodes
        .get(DOCUMENT)
        .and_then(|d| d.children.first().copied());
    arena(nodes, Some(root))
}

fn kind_of(data: Built) -> Kind {
    match data {
        Built::Document => Kind::Fragment,
        Built::Doctype => Kind::Doctype,
        Built::Text(text) => Kind::Text(text),
        Built::Comment(text) => Kind::Comment(text),
        Built::Element { name, attrs, .. } => Kind::Element {
            html: name.ns == ns!(html),
            name: name.local,
            attrs: attrs
                .into_iter()
                .map(|a| {
                    let local = a.name.local.to_string();
                    let name = match &a.name.prefix {
                        Some(p) => format!("{p}:{local}"),
                        None => local,
                    };
                    (name, a.value.to_string())
                })
                .collect(),
        },
    }
}

/// The nodes reachable from the document in preorder, a template's content after its children, without
/// recursion. `root` names the node whose children are the top: the document's own children when `None`,
/// none when `Some(None)`.
fn arena(mut built: Vec<Node>, root: Option<Option<Id>>) -> Dom {
    let mut dom = Dom::default();
    dom.nodes.reserve(built.len());
    let mut take = |id: Id| {
        built.get_mut(id).map(|n| {
            (
                std::mem::replace(&mut n.data, Built::Document),
                std::mem::take(&mut n.children),
                n.span,
            )
        })
    };
    let (_, document_children, _) = take(DOCUMENT).unwrap_or((Built::Document, Vec::new(), None));
    let children = match root {
        None => document_children,
        Some(Some(r)) => take(r).map(|(_, c, _)| c).unwrap_or_default(),
        Some(None) => Vec::new(),
    };
    let top = dom.push(NodeData {
        kind: Kind::Fragment,
        children: Vec::new(),
        parent: None,
        content: None,
        span: None,
    });
    let mut pending: Vec<(Id, Id, bool)> =
        children.iter().rev().map(|&c| (c, top, false)).collect();
    while let Some((h, parent, is_content)) = pending.pop() {
        let Some((data, children, span)) = take(h) else {
            continue;
        };
        let template = match &data {
            Built::Element { template, .. } => *template,
            _ => None,
        };
        let id = dom.push(NodeData {
            kind: kind_of(data),
            children: Vec::with_capacity(children.len()),
            parent: Some(parent),
            content: None,
            span,
        });
        if let Some(p) = dom.nodes.get_mut(parent) {
            if is_content {
                p.content = Some(id);
            } else {
                p.children.push(id);
            }
        }
        if let Some(c) = template {
            pending.push((c, id, true));
        }
        pending.extend(children.iter().rev().map(|&c| (c, id, false)));
    }
    dom.root = top;
    dom
}
