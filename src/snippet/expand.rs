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
    pub budget: usize,
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
            "attrs" => {
                let mut a = value.as_object()?.clone();
                a.remove("id");
                if a.is_empty() {
                    return None;
                }
                self.bytes += a.get("classes").map_or(0, |c| opaque_bytes(c, false));
                for pair in a.get("keyValues").and_then(Value::as_array).into_iter().flatten() {
                    self.bytes += opaque_bytes(pair, false);
                }
                Some(Value::Object(a))
            }
            "data" => {
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
            _ => match value {
                Value::Array(a) if a.iter().all(is_node) && !a.is_empty() => {
                    Some(Value::Array(a.iter().map(|x| self.node(x)).collect()))
                }
                v if is_node(v) => Some(self.node(v)),
                v => {
                    self.bytes += opaque_bytes(v, false);
                    Some(v.clone())
                }
            },
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
    fn refuse(&mut self, name: &str, block: bool, pos: Option<Pos>, code: &'static str, limit: Option<usize>) -> Node<'a> {
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

    /// Expands one use and returns the nodes that stand in its place, their uses expanded too.
    fn expand_use(&mut self, u: &mut ast::SnippetUse<'a>) -> Vec<Node<'a>> {
        let pos = u.pos.take();
        let block = u.mode == ast::SnippetUseMode::Block;
        let Some(&index) = self.names.get(u.name) else {
            return vec![self.refuse_at(u, pos, "snippet-unresolved", None)];
        };
        if self.stack.contains(&index) {
            return vec![self.refuse_at(u, pos, "snippet-cycle", None)];
        }
        if self.stack.len() + 1 > self.depth {
            return vec![self.refuse_at(u, pos, "snippet-depth", Some(self.depth))];
        }
        let entry = &self.entries[index];
        let mut copy = Copy::default();
        let values: Vec<Value> = entry.children.iter().map(|v| copy.node(v)).collect();
        let attrs = entry.attrs.clone();
        let include = entry.include;
        let file = entry.file.clone();
        let nodes: Vec<owned::Node> = values
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        let mut nodes: Vec<Node<'a>> = nodes.iter().map(|n| n.in_arena(self.arena)).collect();
        let single = !block
            && attrs.is_none()
            && self.settings.roundtrip
            && nodes.len() == 1
            && renders_as_one(&nodes[0]);
        let wraps = block || attrs.is_some() || (self.settings.roundtrip && !single);
        let mut count = copy.count;
        let mut bytes = copy.bytes;
        if wraps {
            count += 1;
            bytes += if block { "paragraph".len() } else { "span".len() };
            if let Some(a) = &attrs {
                bytes += a.classes.iter().map(String::len).sum::<usize>();
                bytes += a.key_values.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>();
            }
        }
        if self.total + count > self.limit {
            return vec![self.refuse_at(u, pos, "snippet-node-budget", Some(self.limit))];
        }
        let cost = NODE_COST * count + bytes;
        if cost > self.budget.left {
            if self.budget.reported {
                let own = if self.stack.is_empty() { pos } else { None };
                return vec![spelled(self.arena, u.name, block, own)];
            }
            self.budget.reported = true;
            let budget = self.budget.budget;
            return vec![self.refuse_at(u, pos, "derived-text-budget", Some(budget))];
        }
        self.total += count;
        self.budget.left -= cost;
        self.admitted += 1;
        let n = self.admitted;
        let carrier = Carrier {
            n,
            block,
            name: u.name.to_string(),
            include: include != 0,
        };
        let outer_file = std::mem::replace(&mut self.file, file);
        self.stack.push(index);
        let outer = std::mem::replace(&mut self.current, n);
        for node in &nodes {
            self.mark_notes(node);
        }
        let mut slice = self.arena.fill(nodes.drain(..));
        self.walk_list(&mut slice);
        self.current = outer;
        self.stack.pop();
        self.file = outer_file;
        let placed: Vec<Node<'a>> = if wraps {
            let children = slice;
            let at = attrs.as_ref().map(|a| &*self.arena.alloc(a.in_arena(self.arena)));
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
            vec![wrapper]
        } else {
            let out: Vec<Node<'a>> = slice.iter_mut().map(|c| take(self.arena, c)).collect();
            if single {
                if let Some(first) = out.first() {
                    self.side.carriers.insert(first.payload_addr(), carrier);
                }
            }
            out
        };
        for node in &placed {
            self.side.scope.insert(node.payload_addr(), include);
        }
        placed
    }

    fn refuse_at(&mut self, u: &ast::SnippetUse<'a>, pos: Option<Pos>, code: &'static str, limit: Option<usize>) -> Node<'a> {
        self.refuse(u.name, u.mode == ast::SnippetUseMode::Block, pos, code, limit)
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
        return None;
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
            budget,
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
    let before = SIDE.with(|s| s.borrow_mut().replace(side));
    Some(Guard(Some(before)))
}

/// The positions of the definitions a document transform receives, which tell
/// a definition it kept from one it added (C2-N-095).
pub(crate) fn definition_marks(nodes: &[Node<'_>]) -> std::collections::HashSet<(u32, u32, u32)> {
    let mut out = std::collections::HashSet::new();
    registry_walk(nodes, 0, None, &mut |d, _, _| {
        if let Some(p) = d.pos {
            out.insert((p.start_line, p.start_offset, p.end_offset));
        }
    });
    out
}

/// C2-N-095: in a view a document transform returned, a snippet use is
/// written as its source text and a definition the transform added enters no
/// registry; each reports `snippet-unexpanded-transform`.
pub(crate) fn refuse_transformed<'a>(
    doc: &mut ast::Document<'a>,
    arena: &'a Arena,
    authored: &std::collections::HashSet<(u32, u32, u32)>,
) -> Vec<Finding> {
    let mut found = Vec::new();
    refuse_list(doc.children, arena, authored, true, &mut found);
    found
}

fn refuse_list<'a>(
    list: &mut [Node<'a>],
    arena: &'a Arena,
    authored: &std::collections::HashSet<(u32, u32, u32)>,
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
    authored: &std::collections::HashSet<(u32, u32, u32)>,
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
            let kept = d
                .pos
                .is_some_and(|p| authored.contains(&(p.start_line, p.start_offset, p.end_offset)));
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
