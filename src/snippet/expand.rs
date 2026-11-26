//! C2-N-090 to C2-N-096: the snippet registry and the one expansion pass over
//! the render's own tree, after includes and before resolution and document
//! transforms. What the pass leaves for resolution and the renderer (the
//! carriers, the note marks, the scope of each copy and the budget left) is
//! kept by the address of each node's own struct while the render runs.

use crate::Arena;
use crate::ast::{self, Node, Pos, SlotMut, owned};
use crate::hash::AddrMap;
use serde_json::{Map, Value};
use std::cell::{Cell, RefCell};

/// C2-H-215: what the carriers of one expanded use say.
#[derive(Debug, Clone)]
pub(crate) struct Carrier {
    pub n: usize,
    pub block: bool,
    pub name: String,
    pub include: bool,
}

/// The derived-text budget after the pass, which the renderer goes on from (C2-N-094).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Budget {
    pub bytes: usize,
    pub left: usize,
    pub reported: bool,
}

/// The side tables of one expanded render.
#[derive(Debug, Default)]
pub(crate) struct Side {
    pub carriers: AddrMap<usize, Carrier>,
    /// The `N` of the innermost expanded use around each copied note reference and link.
    pub marks: AddrMap<usize, usize>,
    /// The root nodes of each copy: the include whose file holds the definition, 0 for the top.
    pub scope: AddrMap<usize, usize>,
    /// The definitions of included files, which a roundtrip template marks.
    pub included_defs: crate::hash::AddrSet<usize>,
    pub budget: Option<Budget>,
}

thread_local! {
    static SIDE: RefCell<Option<Side>> = const { RefCell::new(None) };
    static LIMITS: Cell<Limits> = const { Cell::new(Limits { depth: None, nodes: None, derived: None }) };
}

/// The limits a conformance case lowers through `host.limits`; never a Carve option (C2-H-201).
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Limits {
    pub depth: Option<usize>,
    pub nodes: Option<usize>,
    pub derived: Option<usize>,
}

/// Runs `run` with the lowered limits of one conformance case. Test-only.
#[doc(hidden)]
pub fn with_limits<T>(limits: Limits, run: impl FnOnce() -> T) -> T {
    let before = LIMITS.with(|l| l.replace(limits));
    let out = run();
    LIMITS.with(|l| l.set(before));
    out
}

/// The derived-text budget a conformance case lowered, if any.
pub(crate) fn test_derived_bytes() -> Option<usize> {
    LIMITS.with(Cell::get).derived
}

/// Reads the side tables of the running render.
pub(crate) fn with_side<R>(f: impl FnOnce(Option<&Side>) -> R) -> R {
    SIDE.with(|s| f(s.borrow().as_ref()))
}

/// The include whose file holds the definition `n` is a copy of, 0 for the top level.
pub(crate) fn copy_scope(n: &Node<'_>) -> Option<usize> {
    SIDE.with(|s| {
        s.borrow()
            .as_ref()
            .and_then(|side| side.scope.get(&n.payload_addr()).copied())
    })
}

/// Holds the side tables for one render; they go when it drops.
#[must_use]
pub(crate) struct Guard(Option<Option<Side>>);

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(before) = self.0.take() {
            SIDE.with(|s| *s.borrow_mut() = before);
        }
    }
}

/// A use the pass did not expand (C2-N-093, C2-N-094).
#[derive(Debug, Clone)]
pub(crate) struct Finding {
    pub code: &'static str,
    pub name: String,
    pub pos: Option<Pos>,
    pub file: Option<String>,
    /// The node that stands in place of the use, by [`Node::payload_addr`].
    pub addr: usize,
    pub limit: Option<usize>,
}

impl Finding {
    pub(crate) fn diagnostic(&self) -> crate::diagnostic::Diagnostic {
        let limit = self.limit.map(|l| l.to_string()).unwrap_or_default();
        let mut values = vec![("name", self.name.as_str())];
        if self.limit.is_some() {
            values.push(("limit", limit.as_str()));
        }
        crate::diagnostic::Diagnostic::new(self.code, &values, self.file.as_deref(), self.pos)
    }
}

/// One winning definition: its value as tree JSON, and the include whose file holds it (0 for the top).
struct Entry {
    children: Vec<Value>,
    attrs: Option<owned::Attrs>,
    include: usize,
    file: Option<String>,
    /// The copy, its node count and its bytes, made at the first use: every copy of a definition is alike.
    copy: Option<(Vec<owned::Node>, usize, usize)>,
}

/// C2-N-090: the definitions of the expanded tree in document order; a later
/// definition of a name is passed to `duplicate` with its file.
pub(crate) fn registry_walk<'a>(
    nodes: &[Node<'a>],
    include: usize,
    file: Option<&'a str>,
    f: &mut dyn FnMut(&ast::SnippetDefinition<'a>, usize, Option<&'a str>),
) {
    for n in nodes {
        match n {
            Node::SnippetDefinition(d) => f(d, include, file),
            Node::Include(i) => {
                if let Some(children) = i.children.as_deref() {
                    registry_walk(children, n.payload_addr(), i.file.or(file), f);
                }
            }
            _ if crate::include::holds_blocks(n) => {
                for c in crate::resolve::order::slots(n) {
                    registry_walk(std::slice::from_ref(c), include, file, f);
                }
            }
            _ => {}
        }
    }
}

/// Whether `nodes` hold a snippet use anywhere.
pub(crate) fn holds_use(nodes: &[Node<'_>]) -> bool {
    nodes.iter().any(|n| {
        if matches!(n, Node::SnippetUse(_)) {
            return true;
        }
        let mut found = false;
        n.for_each_child(&mut |c| found = found || holds_use(std::slice::from_ref(c)));
        found
    })
}

const NODE_COST: usize = 32;

fn opaque_bytes(v: &Value, keys: bool) -> usize {
    match v {
        Value::String(s) => s.len(),
        Value::Array(a) => a.iter().map(|x| opaque_bytes(x, keys)).sum(),
        Value::Object(m) => m
            .iter()
            .map(|(k, x)| if keys { k.len() } else { 0 } + opaque_bytes(x, keys))
            .sum(),
        _ => 0,
    }
}

fn is_node(v: &Value) -> bool {
    v.as_object().is_some_and(|m| m.contains_key("type"))
}

/// A copy of tree JSON with its node count and the bytes of its strings (C2-N-094).
#[derive(Default)]
struct Copy {
    count: usize,
    bytes: usize,
}

fn dropped(node: &Map<String, Value>, field: &str) -> bool {
    if field == "pos" || field == "segments" {
        return true;
    }
    let Some(name) = node.get("type").and_then(Value::as_str) else {
        return false;
    };
    let Some((_, fields)) = crate::tables::NODE_RESULTS.iter().find(|(n, _)| *n == name) else {
        return false;
    };
    let Some((_, when)) = fields.iter().find(|(f, _)| *f == field) else {
        return false;
    };
    match *when {
        Some("reference use") => node.contains_key("key"),
        Some("derived from authored span mark") => node.contains_key("span"),
        _ => true,
    }
}

impl Copy {
    /// C2-N-092: the copy of one node, without positions, authored ids,
    /// annotation ids and resolution fields. A use keeps its position, which the
    /// pass reads and drops (the finding of a refused copied use names the use the copy came from).
    fn node(&mut self, v: &Value) -> Value {
        let Some(m) = v.as_object() else {
            return v.clone();
        };
        self.count += 1;
        let keep_pos = m.get("type").and_then(Value::as_str) == Some("snippet_use");
        let mut out = Map::new();
        for (field, value) in m {
            if (field == "pos" && keep_pos) || !dropped(m, field) {
                if field == "pos" {
                    out.insert(field.clone(), value.clone());
                } else if let Some(c) = self.field(field, value) {
                    out.insert(field.clone(), c);
                }
            }
        }
        Value::Object(out)
    }

    fn field(&mut self, field: &str, value: &Value) -> Option<Value> {
        match field {
            "attrs" => self.attrs(value),
            "data" => self.data(value),
            "metadata" => Some(self.metadata(value)),
            "slots" if value.is_object() => Some(Value::Object(
                value
                    .as_object()?
                    .iter()
                    .map(|(k, v)| (k.clone(), self.list_or(v)))
                    .collect(),
            )),
            "props" | "payload" => {
                self.bytes += opaque_bytes(value, true);
                Some(value.clone())
            }
            _ => Some(self.plain(value)),
        }
    }

    /// The attributes without the authored id; their classes and pairs are counted.
    fn attrs(&mut self, value: &Value) -> Option<Value> {
        let mut a = value.as_object()?.clone();
        a.remove("id");
        if a.is_empty() {
            return None;
        }
        self.bytes += a.get("classes").map_or(0, |c| opaque_bytes(c, false));
        self.bytes += a
            .get("keyValues")
            .and_then(Value::as_array)
            .map_or(0, |pairs| {
                pairs.iter().map(|p| opaque_bytes(p, false)).sum()
            });
        Some(Value::Object(a))
    }

    /// The data without annotation ids; its keys and strings are counted.
    fn data(&mut self, value: &Value) -> Option<Value> {
        let mut d = value.as_object()?.clone();
        if let Some(Value::Object(note)) = d.get_mut("carve.annotation") {
            note.remove("ids");
            if note.is_empty() {
                d.remove("carve.annotation");
            }
        }
        if d.is_empty() {
            return None;
        }
        let d = Value::Object(d);
        self.bytes += opaque_bytes(&d, true);
        Some(d)
    }

    /// A field that is a node, a list of nodes, or a value whose strings are counted.
    fn plain(&mut self, value: &Value) -> Value {
        match value {
            Value::Array(a) if !a.is_empty() && a.iter().all(is_node) => {
                Value::Array(a.iter().map(|x| self.node(x)).collect())
            }
            v if is_node(v) => self.node(v),
            v => {
                self.bytes += opaque_bytes(v, false);
                v.clone()
            }
        }
    }

    fn list_or(&mut self, v: &Value) -> Value {
        match v {
            Value::Array(a) => Value::Array(a.iter().map(|x| self.node(x)).collect()),
            v => self.node(v),
        }
    }

    /// C2-N-094: metadata is opaque, its keys counted, except that a node it holds is a node of the copy.
    fn metadata(&mut self, v: &Value) -> Value {
        if is_node(v) {
            return self.node(v);
        }
        match v {
            Value::Array(a) => Value::Array(a.iter().map(|x| self.metadata(x)).collect()),
            Value::String(s) => {
                self.bytes += s.len();
                v.clone()
            }
            Value::Object(m) => Value::Object(
                m.iter()
                    .map(|(k, x)| {
                        self.bytes += k.len();
                        (k.clone(), self.metadata(x))
                    })
                    .collect(),
            ),
            _ => v.clone(),
        }
    }
}

/// The settings the pass reads.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Settings {
    pub roundtrip: bool,
    /// The source length the budget is drawn from (C2-N-094).
    pub source_len: usize,
}

struct Pass<'a, 'p> {
    arena: &'a Arena,
    entries: Vec<Entry>,
    names: std::collections::HashMap<String, usize>,
    stack: Vec<usize>,
    depth: usize,
    limit: usize,
    total: usize,
    budget: Budget,
    admitted: usize,
    current: usize,
    settings: Settings,
    findings: &'p mut Vec<Finding>,
    side: Side,
    file: Option<String>,
}

/// The nodes that render as exactly one element that takes the carriers.
fn renders_as_one(n: &Node<'_>) -> bool {
    match n {
        Node::Emphasis(_)
        | Node::Strong(_)
        | Node::Underline(_)
        | Node::Strike(_)
        | Node::Highlight(_)
        | Node::Superscript(_)
        | Node::Subscript(_)
        | Node::Insert(_)
        | Node::Delete(_)
        | Node::Span(_)
        | Node::Code(_) => true,
        // A use inside the link may expand to a note that moves out behind it.
        Node::Link(_) => !holds_note(n),
        _ => false,
    }
}

fn holds_note(n: &Node<'_>) -> bool {
    if matches!(
        n,
        Node::FootnoteRef(_) | Node::InlineFootnote(_) | Node::SnippetUse(_)
    ) {
        return true;
    }
    let mut found = false;
    n.for_each_child(&mut |c| found = found || holds_note(c));
    found
}

/// The bytes of the wrapper a copy gets: its type and its attributes (C2-N-094).
fn wrapper_bytes(block: bool, attrs: Option<&owned::Attrs>) -> usize {
    let ty = if block { "paragraph" } else { "span" };
    ty.len()
        + attrs.map_or(0, |a| {
            a.classes.iter().map(String::len).sum::<usize>()
                + a.key_values
                    .iter()
                    .map(|(k, v)| k.len() + v.len())
                    .sum::<usize>()
        })
}

/// C2-N-093: the use written as its source spelling.
fn spelled<'a>(arena: &'a Arena, name: &str, block: bool, pos: Option<Pos>) -> Node<'a> {
    let text = Node::Text(arena.alloc(ast::Text {
        value: arena.str(&super::spelling(name)),
        pos,
        ..Default::default()
    }));
    if !block {
        return text;
    }
    Node::Paragraph(arena.alloc(ast::Paragraph {
        children: arena.one(text),
        pos,
        ..Default::default()
    }))
}

impl<'a> Pass<'a, '_> {
    /// Writes a use as its source text and reports `code` at `pos`, the authored use.
    fn refuse(
        &mut self,
        name: &str,
        block: bool,
        pos: Option<Pos>,
        code: &'static str,
        limit: Option<usize>,
    ) -> Node<'a> {
        // A copied use has no position of its own; its finding names the use the copy came from.
        let own = if self.stack.is_empty() { pos } else { None };
        let out = spelled(self.arena, name, block, own);
        self.findings.push(Finding {
            code,
            name: name.to_string(),
            pos,
            file: self.file.clone(),
            addr: out.payload_addr(),
            limit,
        });
        out
    }

    /// C2-N-093: the code of a use that is not expanded and its limit, or the index of its definition.
    fn refusal(&self, name: &str) -> Result<usize, (&'static str, Option<usize>)> {
        let &index = self.names.get(name).ok_or(("snippet-unresolved", None))?;
        if self.stack.contains(&index) {
            return Err(("snippet-cycle", None));
        }
        if self.stack.len() + 1 > self.depth {
            return Err(("snippet-depth", Some(self.depth)));
        }
        Ok(index)
    }

    /// The copy of the definition at `index` in the arena, with its node count and bytes.
    fn copy_of(&mut self, index: usize) -> Option<(Vec<Node<'a>>, usize, usize)> {
        let entry = self.entries.get_mut(index)?;
        let (owned, count, bytes) = entry.copy.get_or_insert_with(|| {
            let mut copy = Copy::default();
            let values: Vec<Value> = entry.children.iter().map(|v| copy.node(v)).collect();
            let nodes = values
                .into_iter()
                .filter_map(|v| serde_json::from_value(v).ok())
                .collect();
            (nodes, copy.count, copy.bytes)
        });
        let nodes = owned.iter().map(|n| n.in_arena(self.arena)).collect();
        Some((nodes, *count, *bytes))
    }

    /// C2-N-094: whether the node and text budgets admit `count` nodes and `bytes`;
    /// where not, the code to report and its limit, `None` for a budget already reported.
    fn admits(&mut self, count: usize, bytes: usize) -> Result<(), Option<(&'static str, usize)>> {
        if self.total + count > self.limit {
            return Err(Some(("snippet-node-budget", self.limit)));
        }
        let cost = NODE_COST * count + bytes;
        if cost > self.budget.left {
            if self.budget.reported {
                return Err(None);
            }
            self.budget.reported = true;
            return Err(Some(("derived-text-budget", self.budget.bytes)));
        }
        self.total += count;
        self.budget.left -= cost;
        Ok(())
    }

    /// Expands one use and returns the nodes that stand in its place, their uses expanded too.
    fn expand_use(&mut self, u: &mut ast::SnippetUse<'a>) -> Vec<Node<'a>> {
        let pos = u.pos.take();
        let block = u.mode == ast::SnippetUseMode::Block;
        let index = match self.refusal(u.name) {
            Ok(index) => index,
            Err((code, limit)) => return vec![self.refuse_at(u, pos, code, limit)],
        };
        let Some((nodes, count, bytes)) = self.copy_of(index) else {
            return vec![self.refuse_at(u, pos, "snippet-unresolved", None)];
        };
        let (attrs, include) = self
            .entries
            .get(index)
            .map_or((None, 0), |e| (e.attrs.clone(), e.include));
        let (single, wraps) = self.shape(block, attrs.is_some(), &nodes);
        let (count, bytes) = if wraps {
            (count + 1, bytes + wrapper_bytes(block, attrs.as_ref()))
        } else {
            (count, bytes)
        };
        if let Err(code) = self.admits(count, bytes) {
            return vec![self.refuse_budget(u, pos, code)];
        }
        self.admitted += 1;
        let carrier = Carrier {
            n: self.admitted,
            block,
            name: u.name.to_string(),
            include: include != 0,
        };
        let slice = self.walk_copy(index, nodes);
        let placed = if wraps {
            vec![self.wrapper(slice, block, attrs.as_ref(), carrier)]
        } else {
            self.unwrapped(slice, single, carrier)
        };
        for node in &placed {
            self.side.scope.insert(node.payload_addr(), include);
        }
        placed
    }

    /// C2-H-215: whether a copy carries the carriers on its one element, and whether it gets a wrapper.
    fn shape(&self, block: bool, attrs: bool, nodes: &[Node<'_>]) -> (bool, bool) {
        let one = matches!(nodes, [n] if renders_as_one(n));
        let single = !block && !attrs && self.settings.roundtrip && one;
        (
            single,
            block || attrs || (self.settings.roundtrip && !single),
        )
    }

    /// A use the budgets do not admit, reported unless the text budget was reported already.
    fn refuse_budget(
        &mut self,
        u: &ast::SnippetUse<'a>,
        pos: Option<Pos>,
        code: Option<(&'static str, usize)>,
    ) -> Node<'a> {
        if let Some((code, limit)) = code {
            return self.refuse_at(u, pos, code, Some(limit));
        }
        let own = if self.stack.is_empty() { pos } else { None };
        spelled(
            self.arena,
            u.name,
            u.mode == ast::SnippetUseMode::Block,
            own,
        )
    }

    /// The nodes of a copy without a wrapper; one element that renders alone takes the carriers.
    fn unwrapped(
        &mut self,
        slice: &mut [Node<'a>],
        single: bool,
        carrier: Carrier,
    ) -> Vec<Node<'a>> {
        let out: Vec<Node<'a>> = slice.iter_mut().map(|c| take(self.arena, c)).collect();
        if single && let Some(first) = out.first() {
            self.side.carriers.insert(first.payload_addr(), carrier);
        }
        out
    }

    /// The copy with its own uses expanded, inside the definition's file and the cycle stack.
    fn walk_copy(&mut self, index: usize, mut nodes: Vec<Node<'a>>) -> &'a mut [Node<'a>] {
        let file = self.entries.get(index).and_then(|e| e.file.clone());
        let outer_file = std::mem::replace(&mut self.file, file);
        self.stack.push(index);
        let outer = std::mem::replace(&mut self.current, self.admitted);
        for node in &nodes {
            self.mark_notes(node);
        }
        let mut slice = self.arena.fill(nodes.drain(..));
        self.walk_list(&mut slice);
        self.current = outer;
        self.stack.pop();
        self.file = outer_file;
        slice
    }

    /// The paragraph of a block use, or the span of an inline use with attributes or carriers.
    fn wrapper(
        &mut self,
        children: &'a mut [Node<'a>],
        block: bool,
        attrs: Option<&owned::Attrs>,
        carrier: Carrier,
    ) -> Node<'a> {
        let at = attrs.map(|a| &*self.arena.alloc(a.in_arena(self.arena)));
        let wrapper = if block {
            Node::Paragraph(self.arena.alloc(ast::Paragraph {
                children,
                attrs: at,
                ..Default::default()
            }))
        } else {
            Node::Span(self.arena.alloc(ast::Span {
                children,
                attrs: at,
                ..Default::default()
            }))
        };
        if self.settings.roundtrip {
            self.side.carriers.insert(wrapper.payload_addr(), carrier);
        }
        wrapper
    }

    fn refuse_at(
        &mut self,
        u: &ast::SnippetUse<'a>,
        pos: Option<Pos>,
        code: &'static str,
        limit: Option<usize>,
    ) -> Node<'a> {
        self.refuse(
            u.name,
            u.mode == ast::SnippetUseMode::Block,
            pos,
            code,
            limit,
        )
    }

    fn mark_notes(&mut self, n: &Node<'a>) {
        if matches!(
            n,
            Node::FootnoteRef(_) | Node::InlineFootnote(_) | Node::Link(_)
        ) {
            self.side.marks.insert(n.payload_addr(), self.current);
        }
        n.for_each_child(&mut |c| self.mark_notes(c));
    }

    #[allow(clippy::mut_mut)] // a list with a use is rebuilt into a new arena slice
    fn walk_list(&mut self, list: &mut &'a mut [Node<'a>]) {
        if !list.iter().any(|n| matches!(n, Node::SnippetUse(_))) {
            for n in list.iter_mut() {
                self.walk_node(n);
            }
            return;
        }
        let mut out = Vec::with_capacity(list.len());
        for n in list.iter_mut() {
            if let Node::SnippetUse(u) = n {
                out.extend(self.expand_use(u));
            } else {
                self.walk_node(n);
                out.push(take(self.arena, n));
            }
        }
        *list = self.arena.fill(out.into_iter());
    }

    fn walk_node(&mut self, n: &mut Node<'a>) {
        match n {
            Node::SnippetDefinition(_) => {}
            Node::Include(i) => {
                let file = i.file.map(str::to_string);
                if let Some(children) = &mut i.children {
                    let outer = std::mem::replace(&mut self.file, file);
                    self.walk_list(children);
                    self.file = outer;
                }
            }
            _ => n.for_each_slot_mut(&mut |slot| match slot {
                SlotMut::List(l) => self.walk_list(l),
                SlotMut::One(c) => self.walk_node(c),
            }),
        }
    }
}

fn take<'a>(arena: &'a Arena, n: &mut Node<'a>) -> Node<'a> {
    std::mem::replace(n, Node::SoftBreak(arena.alloc(ast::SoftBreak::default())))
}

/// The derived-text budget of a render (C2-N-094).
pub(crate) fn budget_of(source_len: usize) -> usize {
    test_derived_bytes().unwrap_or_else(|| {
        crate::tables::DERIVED_TEXT_BYTES
            .max(source_len.saturating_mul(crate::tables::DERIVED_TEXT_FACTOR))
    })
}

/// C2-N-091: expands the snippet uses of `doc` in place, which must be the
/// render's own tree. The side tables stay for the render while the guard lives.
pub(crate) fn expand<'a>(
    doc: &mut ast::Document<'a>,
    arena: &'a Arena,
    settings: Settings,
    findings: &mut Vec<Finding>,
) -> Option<Guard> {
    if !holds_use(doc.children) {
        // C2-H-216: a template still marks the definitions of an included file.
        return settings
            .roundtrip
            .then(|| install(included_side(doc.children)))
            .flatten();
    }
    let mut entries = Vec::new();
    let mut names = std::collections::HashMap::new();
    let mut included_defs = crate::hash::AddrSet::default();
    registry_walk(doc.children, 0, None, &mut |d, include, file| {
        if include != 0 {
            included_defs.insert(std::ptr::from_ref(d).addr());
        }
        if names.contains_key(d.name) {
            return;
        }
        names.insert(d.name.to_string(), entries.len());
        entries.push(Entry {
            children: d
                .children
                .iter()
                .filter_map(|c| serde_json::to_value(c).ok())
                .collect(),
            attrs: d.attrs.map(ast::Attrs::owned),
            include,
            file: file.map(str::to_string),
            copy: None,
        });
    });
    let budget = budget_of(settings.source_len);
    let limits = LIMITS.with(Cell::get);
    let mut pass = Pass {
        arena,
        entries,
        names,
        stack: Vec::new(),
        depth: limits.depth.unwrap_or(crate::tables::SNIPPET_DEPTH),
        limit: limits.nodes.unwrap_or(crate::tables::SNIPPET_NODES),
        total: 0,
        budget: Budget {
            bytes: budget,
            left: budget,
            reported: false,
        },
        admitted: 0,
        current: 0,
        settings,
        findings,
        side: Side::default(),
        file: None,
    };
    pass.walk_list(&mut doc.children);
    let mut side = pass.side;
    side.budget = Some(pass.budget);
    side.included_defs = included_defs;
    install(Some(side))
}

/// The side tables of a render that expanded no use: the definitions of included files.
fn included_side(nodes: &[Node<'_>]) -> Option<Side> {
    let mut side = Side::default();
    registry_walk(nodes, 0, None, &mut |d, include, _| {
        if include != 0 {
            side.included_defs.insert(std::ptr::from_ref(d).addr());
        }
    });
    (!side.included_defs.is_empty()).then_some(side)
}

/// Makes `side` the side tables of the running render until the guard drops.
fn install(side: Option<Side>) -> Option<Guard> {
    let side = side?;
    let before = SIDE.with(|s| s.borrow_mut().replace(side));
    Some(Guard(Some(before)))
}

/// What one node carries in the side tables, kept across a document transform.
#[derive(Debug, Clone)]
struct Carried {
    carrier: Option<Carrier>,
    mark: Option<usize>,
    /// The include of the copy's definition by its place among the includes, 0 for the top.
    scope: Option<usize>,
}

/// The side tables of an expanded view as tree JSON, to find again in the view a
/// document transform returns: its nodes have new addresses there (C2-H-215, C2-N-092).
#[derive(Debug, Default)]
pub(crate) struct Kept {
    /// Each carrying node: its place, its JSON and what it carries.
    nodes: Vec<(Vec<usize>, Value, Carried)>,
    budget: Option<Budget>,
}

/// Visits every node in document order with its place: the index of each node
/// on the way down among the nodes its parent's slots hold.
fn each_node<'a>(nodes: &[Node<'a>], f: &mut dyn FnMut(&Node<'a>, &[usize])) {
    fn walk<'a>(nodes: &[Node<'a>], at: &mut Vec<usize>, f: &mut dyn FnMut(&Node<'a>, &[usize])) {
        for (i, n) in nodes.iter().enumerate() {
            at.push(i);
            f(n, at);
            if let Node::Include(inc) = n {
                walk(inc.children.as_deref().unwrap_or(&[]), at, f);
            } else {
                let kids: Vec<&Node<'a>> = crate::resolve::order::slots(n).collect();
                for (k, c) in kids.into_iter().enumerate() {
                    at.push(k);
                    f(c, at);
                    walk_below(c, at, f);
                    at.pop();
                }
            }
            at.pop();
        }
    }
    fn walk_below<'a>(n: &Node<'a>, at: &mut Vec<usize>, f: &mut dyn FnMut(&Node<'a>, &[usize])) {
        if let Node::Include(inc) = n {
            walk(inc.children.as_deref().unwrap_or(&[]), at, f);
            return;
        }
        let kids: Vec<&Node<'a>> = crate::resolve::order::slots(n).collect();
        for (k, c) in kids.into_iter().enumerate() {
            at.push(k);
            f(c, at);
            walk_below(c, at, f);
            at.pop();
        }
    }
    walk(nodes, &mut Vec::new(), f);
}

/// The side tables of the running render, keyed by the JSON of each node.
pub(crate) fn keep(nodes: &[Node<'_>]) -> Kept {
    with_side(|side| {
        let Some(side) = side else {
            return Kept::default();
        };
        let mut includes = std::collections::HashMap::new();
        each_node(nodes, &mut |n, _| {
            if matches!(n, Node::Include(_)) {
                let at = includes.len() + 1;
                includes.insert(n.payload_addr(), at);
            }
        });
        let mut kept = Kept {
            nodes: Vec::new(),
            budget: side.budget,
        };
        each_node(nodes, &mut |n, place| {
            let at = n.payload_addr();
            let carried = Carried {
                carrier: side.carriers.get(&at).cloned(),
                mark: side.marks.get(&at).copied(),
                scope: side.scope.get(&at).map(|i| {
                    if *i == 0 {
                        0
                    } else {
                        includes.get(i).copied().unwrap_or(0)
                    }
                }),
            };
            if carried.carrier.is_some() || carried.mark.is_some() || carried.scope.is_some() {
                if let Ok(v) = serde_json::to_value(n) {
                    kept.nodes.push((place.to_vec(), v, carried));
                }
            }
        });
        kept
    })
}

/// Finds the kept nodes again in `nodes`, in document order, and makes their
/// side tables those of the render until the guard drops.
pub(crate) fn restore(nodes: &[Node<'_>], kept: Kept) -> Option<Guard> {
    let mut side = included_side(nodes).unwrap_or_default();
    side.budget = kept.budget;
    let mut includes = Vec::new();
    each_node(nodes, &mut |n, _| {
        if matches!(n, Node::Include(_)) {
            includes.push(n.payload_addr());
        }
    });
    // A kept node is found at its place where a node of its type stands there (a transform
    // may change its content); else as the first equal node after the last match.
    let mut found: Vec<(usize, Vec<usize>, Value)> = Vec::new();
    each_node(nodes, &mut |n, place| {
        if let Ok(v) = serde_json::to_value(n) {
            found.push((n.payload_addr(), place.to_vec(), v));
        }
    });
    let mut from = 0;
    for (place, value, carried) in kept.nodes {
        let same_place = found
            .iter()
            .skip(from)
            .position(|(_, p, v)| *p == place && v.get("type") == value.get("type"));
        let Some(at) =
            same_place.or_else(|| found.iter().skip(from).position(|(_, _, v)| *v == value))
        else {
            continue;
        };
        let Some(&(addr, _, _)) = found.get(from + at) else {
            continue;
        };
        from += at + 1;
        if let Some(c) = carried.carrier {
            side.carriers.insert(addr, c);
        }
        if let Some(m) = carried.mark {
            side.marks.insert(addr, m);
        }
        if let Some(i) = carried.scope {
            let include = if i == 0 {
                0
            } else {
                includes.get(i - 1).copied().unwrap_or(0)
            };
            side.scope.insert(addr, include);
        }
    }
    install(Some(side))
}

/// What tells a definition a document transform kept from one it added: its
/// position, or for a definition without one its JSON (C2-N-095).
fn definition_key(d: &ast::SnippetDefinition<'_>) -> String {
    match d.pos {
        Some(p) => format!("{}:{}:{}", p.start_line, p.start_offset, p.end_offset),
        None => serde_json::to_string(d).unwrap_or_default(),
    }
}

/// The keys of the definitions a document transform receives.
pub(crate) fn definition_marks(nodes: &[Node<'_>]) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    registry_walk(nodes, 0, None, &mut |d, _, _| {
        out.insert(definition_key(d));
    });
    out
}

/// C2-N-095: in a view a document transform returned, a snippet use is
/// written as its source text and a definition the transform added enters no
/// registry; each reports `snippet-unexpanded-transform`.
pub(crate) fn refuse_transformed<'a>(
    doc: &mut ast::Document<'a>,
    arena: &'a Arena,
    authored: &std::collections::HashSet<String>,
) -> Vec<Finding> {
    let mut found = Vec::new();
    refuse_list(doc.children, arena, authored, true, &mut found);
    found
}

fn refuse_list<'a>(
    list: &mut [Node<'a>],
    arena: &'a Arena,
    authored: &std::collections::HashSet<String>,
    top: bool,
    found: &mut Vec<Finding>,
) {
    for n in list.iter_mut() {
        refuse_node(n, arena, authored, top, found);
    }
}

fn refuse_node<'a>(
    n: &mut Node<'a>,
    arena: &'a Arena,
    authored: &std::collections::HashSet<String>,
    top: bool,
    found: &mut Vec<Finding>,
) {
    let finding = |addr| Finding {
        code: "snippet-unexpanded-transform",
        name: String::new(),
        pos: None,
        file: None,
        addr,
        limit: None,
    };
    match n {
        Node::SnippetUse(u) => {
            let out = spelled(arena, u.name, u.mode == ast::SnippetUseMode::Block, u.pos);
            found.push(finding(out.payload_addr()));
            *n = out;
        }
        Node::SnippetDefinition(d) => {
            let kept = authored.contains(&definition_key(d));
            if !top || !kept {
                found.push(finding(n.payload_addr()));
            }
        }
        Node::Include(i) => {
            if let Some(children) = &mut i.children {
                refuse_list(children, arena, authored, true, found);
            }
        }
        _ => n.for_each_slot_mut(&mut |slot| match slot {
            SlotMut::List(l) => refuse_list(l, arena, authored, false, found),
            SlotMut::One(c) => refuse_node(c, arena, authored, false, found),
        }),
    }
}
