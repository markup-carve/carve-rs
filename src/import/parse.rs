//! HTML parsing with source positions (C2-M-002, C2-M-081). html5ever locates
//! no node, so the tokenizer is fed one chunk per `<` and each token's end is
//! read from the input it left unconsumed.

use super::dom::{Dom, Id, Kind, NodeData};
use crate::hash::AddrMap;
use html5ever::tendril::StrTendril;
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use html5ever::tree_builder::{
    ElementFlags, NodeOrText, QuirksMode, TreeBuilder, TreeBuilderOpts, TreeSink,
};
use html5ever::{Attribute, ExpandedName, LocalName, QualName, local_name, ns};
use markup5ever_rcdom::{Handle, NodeData as Rc5, RcDom};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
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
    spans: RefCell<AddrMap<usize, (usize, usize)>>,
    keep: RefCell<Vec<Handle>>,
    /// Located elements no end tag has closed yet, in creation order.
    open: RefCell<Vec<(usize, LocalName)>>,
    input: Rc<str>,
}

fn key(h: &Handle) -> usize {
    Rc::as_ptr(h) as usize
}

impl Tracker {
    fn consumed(&self) -> usize {
        let (_, end, _) = self.chunk.get();
        let left = self.queue.peek_front_chunk_mut().map_or(0, |c| c.len());
        end.saturating_sub(left)
    }

    fn set_span(&self, h: &Handle, span: (usize, usize)) {
        self.spans.borrow_mut().insert(key(h), span);
        self.keep.borrow_mut().push(h.clone());
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

    fn text(&self, h: &Handle) {
        let Some((start, end)) = self.current_span() else {
            return;
        };
        let mut spans = self.spans.borrow_mut();
        if let Some(span) = spans.get_mut(&key(h)) {
            span.1 = span.1.max(end);
        } else {
            spans.insert(key(h), (start, end));
            self.keep.borrow_mut().push(h.clone());
        }
    }
}

struct Sink {
    dom: RcDom,
    t: Rc<Tracker>,
}

impl TreeSink for Sink {
    type Handle = Handle;
    type Output = Self;
    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> Self {
        self
    }
    fn parse_error(&self, _msg: Cow<'static, str>) {}
    fn get_document(&self) -> Handle {
        self.dom.get_document()
    }
    fn elem_name<'a>(&'a self, target: &'a Handle) -> ExpandedName<'a> {
        self.dom.elem_name(target)
    }
    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> Handle {
        let local = name.local.clone();
        let h = self.dom.create_element(name, attrs, flags);
        if let Current::Start(s, e, false) = self.t.current.get()
            && self.t.current_name.borrow().as_ref() == Some(&local)
        {
            self.t.current.set(Current::Start(s, e, true));
            self.t.set_span(&h, (s, e));
            self.t.open.borrow_mut().push((key(&h), local));
        }
        h
    }
    fn create_comment(&self, text: StrTendril) -> Handle {
        let h = self.dom.create_comment(text);
        if let Current::Other(s, e) = self.t.current.get() {
            self.t.set_span(&h, (s, e));
        }
        h
    }
    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Handle {
        self.dom.create_pi(target, data)
    }
    fn append(&self, parent: &Handle, child: NodeOrText<Handle>) {
        let text = matches!(child, NodeOrText::AppendText(_));
        self.dom.append(parent, child);
        if text && let Some(last) = parent.children.borrow().last() {
            self.t.text(last);
        }
    }
    fn append_based_on_parent_node(
        &self,
        element: &Handle,
        prev_element: &Handle,
        child: NodeOrText<Handle>,
    ) {
        let parent = element.parent.take();
        let has_parent = parent.is_some();
        element.parent.set(parent);
        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }
    fn append_doctype_to_document(&self, name: StrTendril, public: StrTendril, system: StrTendril) {
        self.dom.append_doctype_to_document(name, public, system);
        let doc = self.dom.get_document();
        if let (Some(last), Some(span)) = (doc.children.borrow().last(), self.t.current_span()) {
            self.t.set_span(last, span);
        }
    }
    fn mark_script_already_started(&self, node: &Handle) {
        self.dom.mark_script_already_started(node);
    }
    fn get_template_contents(&self, target: &Handle) -> Handle {
        self.dom.get_template_contents(target)
    }
    fn same_node(&self, x: &Handle, y: &Handle) -> bool {
        self.dom.same_node(x, y)
    }
    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.dom.set_quirks_mode(mode);
    }
    fn append_before_sibling(&self, sibling: &Handle, new_node: NodeOrText<Handle>) {
        let text = matches!(new_node, NodeOrText::AppendText(_));
        self.dom.append_before_sibling(sibling, new_node);
        if !text {
            return;
        }
        let parent = sibling.parent.take();
        sibling.parent.set(parent.clone());
        let Some(parent) = parent.and_then(|p| p.upgrade()) else {
            return;
        };
        let children = parent.children.borrow();
        if let Some(i) = children.iter().position(|c| Rc::ptr_eq(c, sibling))
            && let Some(before) = i.checked_sub(1).and_then(|i| children.get(i))
        {
            self.t.text(before);
        }
    }
    fn add_attrs_if_missing(&self, target: &Handle, attrs: Vec<Attribute>) {
        self.dom.add_attrs_if_missing(target, attrs);
    }
    fn remove_from_parent(&self, target: &Handle) {
        self.dom.remove_from_parent(target);
    }
    fn reparent_children(&self, node: &Handle, new_parent: &Handle) {
        self.dom.reparent_children(node, new_parent);
    }
    fn is_mathml_annotation_xml_integration_point(&self, handle: &Handle) -> bool {
        self.dom.is_mathml_annotation_xml_integration_point(handle)
    }
    fn allow_declarative_shadow_roots(&self, _intended_parent: &Handle) -> bool {
        false
    }
}

struct Located {
    tb: TreeBuilder<Handle, Sink>,
    t: Rc<Tracker>,
}

impl TokenSink for Located {
    type Handle = Handle;

    fn process_token(&self, token: Token, line: u64) -> TokenSinkResult<Handle> {
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
        if let Some(span) = t.spans.borrow_mut().get_mut(&k) {
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
        spans: RefCell::new(AddrMap::default()),
        keep: RefCell::new(Vec::new()),
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
    let sink = Sink {
        dom: RcDom::default(),
        t: t.clone(),
    };
    let tb = TreeBuilder::new(sink, opts());
    let tok = Tokenizer::new(Located { tb, t: t.clone() }, TokenizerOpts::default());
    run(input, &tok, &t);
    let doc = tok.sink.tb.sink.dom.get_document();
    let spans = t.spans.borrow();
    convert(&doc, &doc.children.borrow(), &spans)
}

/// A fragment (C2-M-002) with a `body` element as the context.
pub(crate) fn fragment(input: &str) -> Dom {
    let t = tracker(input);
    let sink = Sink {
        dom: RcDom::default(),
        t: t.clone(),
    };
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
    let doc = tok.sink.tb.sink.dom.get_document();
    let root = doc.children.borrow().first().cloned();
    let spans = t.spans.borrow();
    match root {
        Some(root) => convert(&doc, &root.children.borrow(), &spans),
        None => convert(&doc, &[], &spans),
    }
}

fn kind_of(h: &Handle) -> Kind {
    match &h.data {
        Rc5::Document => Kind::Fragment,
        Rc5::Doctype { .. } => Kind::Doctype,
        Rc5::Text { contents } => Kind::Text(contents.borrow().to_string()),
        Rc5::Comment { contents } | Rc5::ProcessingInstruction { contents, .. } => {
            Kind::Comment(contents.to_string())
        }
        Rc5::Element { name, attrs, .. } => Kind::Element {
            name: name.local.clone(),
            html: name.ns == ns!(html),
            attrs: attrs
                .borrow()
                .iter()
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

/// Copies the parsed nodes into the arena, without recursion.
fn convert(root: &Handle, children: &[Handle], spans: &AddrMap<usize, (usize, usize)>) -> Dom {
    let mut dom = Dom::default();
    let top = dom.push(NodeData {
        kind: kind_of(root),
        children: Vec::new(),
        parent: None,
        content: None,
        span: None,
    });
    let mut pending: Vec<(Handle, Id, bool)> = children
        .iter()
        .rev()
        .map(|c| (c.clone(), top, false))
        .collect();
    while let Some((h, parent, is_content)) = pending.pop() {
        let id = dom.push(NodeData {
            kind: kind_of(&h),
            children: Vec::with_capacity(h.children.borrow().len()),
            parent: Some(parent),
            content: None,
            span: spans.get(&key(&h)).copied(),
        });
        if let Some(p) = dom.nodes.get_mut(parent) {
            if is_content {
                p.content = Some(id);
            } else {
                p.children.push(id);
            }
        }
        if let Rc5::Element {
            template_contents, ..
        } = &h.data
            && let Some(c) = template_contents.borrow().as_ref()
        {
            pending.push((c.clone(), id, true));
        }
        for c in h.children.borrow().iter().rev() {
            pending.push((c.clone(), id, false));
        }
    }
    dom.root = top;
    dom
}
