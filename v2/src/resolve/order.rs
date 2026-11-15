//! The resolution order (C2-A-025, C2-H-025, C2-H-087, C2-H-150, C2-H-170):
//! the document body in document order, a node before the nodes inside it and
//! a title before the children, then the notes in the order of their numbers.
//! Footnote numbers are assigned while the order is built, because they decide
//! the order of the notes.

use crate::ast::{Document, Node};
use crate::tables::DEPTH_CAP;
use std::collections::HashMap;

/// The address of a node, the key of every per-node result.
#[must_use]
pub fn addr(n: &Node) -> usize {
    std::ptr::from_ref(n).addr()
}

/// One visited node and the facts of its place that numbering reads.
#[derive(Clone, Copy)]
pub struct Visit<'a> {
    pub node: &'a Node,
    /// A `figure` or `table` directly inside a `figure_group` (C2-H-066).
    pub panel: bool,
    /// Inside a `figure` or `figure_group` whose caption has a mark (C2-H-151).
    pub in_marked_figure: bool,
    /// Inside the body of note N, 0 for the document body.
    pub note: usize,
}

/// A note of the footnote sequence: a `footnote` definition or an
/// `inline_footnote`, with the references that use it in order.
pub struct Note<'a> {
    pub node: &'a Node,
    pub refs: Vec<&'a Node>,
}

/// The resolution order and the footnote sequence.
#[derive(Default)]
pub struct Order<'a> {
    pub visits: Vec<Visit<'a>>,
    pub notes: Vec<Note<'a>>,
    /// Address of a `footnote_ref` or `inline_footnote` to its number.
    pub number: HashMap<usize, usize>,
    note_of_key: HashMap<&'a str, usize>,
}

/// The first `footnote` definition of each key (C2-A-080).
#[must_use]
pub fn footnote_defs(doc: &Document) -> HashMap<&str, &Node> {
    let mut defs = HashMap::new();
    for c in &doc.children {
        if let Node::Footnote(f) = c {
            defs.entry(f.key.as_str()).or_insert(c);
        }
    }
    defs
}

impl<'a> Order<'a> {
    /// Builds the order of `doc`.
    #[must_use]
    pub fn build(doc: &'a Document) -> Self {
        let defs = footnote_defs(doc);
        let mut o = Order::default();
        let top = Ctx { panel: false, in_marked_figure: false, note: 0 };
        for c in &doc.children {
            if !matches!(c, Node::Footnote(_)) {
                o.walk(c, &defs, top, 1);
            }
        }
        let mut i = 0;
        while let Some(note) = o.notes.get(i) {
            let content: &'a [Node] = match note.node {
                Node::Footnote(f) => &f.children,
                Node::InlineFootnote(f) => &f.children,
                _ => &[],
            };
            i += 1;
            let ctx = Ctx { panel: false, in_marked_figure: false, note: i };
            for c in content {
                o.walk(c, &defs, ctx, 1);
            }
        }
        o
    }

    fn walk(&mut self, n: &'a Node, defs: &HashMap<&str, &'a Node>, ctx: Ctx, depth: usize) {
        if depth > DEPTH_CAP {
            return;
        }
        self.visits.push(Visit { node: n, panel: ctx.panel, in_marked_figure: ctx.in_marked_figure, note: ctx.note });
        match n {
            Node::FootnoteRef(r) => {
                if let Some(def) = defs.get(r.key.as_str()) {
                    self.use_note(n, def, Some(r.key.as_str()));
                }
                return;
            }
            Node::InlineFootnote(_) => {
                self.use_note(n, n, None);
                return;
            }
            _ => {}
        }
        let group = matches!(n, Node::FigureGroup(_));
        let marked = ctx.in_marked_figure || (matches!(n, Node::Figure(_) | Node::FigureGroup(_)) && caption_marked(n));
        let mut list = Vec::new();
        slots(n, &mut list);
        for c in list {
            let panel = group && matches!(c, Node::Figure(_) | Node::Table(_));
            self.walk(c, defs, Ctx { panel, in_marked_figure: marked, note: ctx.note }, depth + 1);
        }
    }

    fn use_note(&mut self, r: &'a Node, note: &'a Node, key: Option<&'a str>) {
        let existing = key.and_then(|k| self.note_of_key.get(k).copied());
        let idx = existing.unwrap_or_else(|| {
            self.notes.push(Note { node: note, refs: Vec::new() });
            let idx = self.notes.len() - 1;
            if let Some(k) = key {
                self.note_of_key.insert(k, idx);
            }
            idx
        });
        if let Some(n) = self.notes.get_mut(idx) {
            n.refs.push(r);
        }
        self.number.insert(addr(r), idx + 1);
    }
}

#[derive(Clone, Copy)]
struct Ctx {
    panel: bool,
    in_marked_figure: bool,
    note: usize,
}

/// True when the caption of a figure, figure group or table holds a mark.
#[must_use]
pub fn caption_marked(n: &Node) -> bool {
    caption_of(n).is_some_and(|c| mark_of(c).is_some())
}

/// The caption slot of a node.
#[must_use]
pub fn caption_of(n: &Node) -> Option<&Node> {
    match n {
        Node::Figure(f) => f.caption.as_deref(),
        Node::FigureGroup(f) => f.caption.as_deref(),
        Node::Table(t) => t.caption.as_deref(),
        _ => None,
    }
}

/// The caption number node of a caption and the inline nodes before it: the
/// mark stands in the top-level inline content (C2-B-207).
#[must_use]
pub fn mark_of(caption: &Node) -> Option<(&Node, &[Node])> {
    let Node::Caption(c) = caption else { return None };
    for p in &c.children {
        if let Node::Paragraph(p) = p {
            if let Some(i) = p.children.iter().position(|x| matches!(x, Node::CaptionNumber(_))) {
                return Some((p.children.get(i)?, p.children.get(..i)?));
            }
        }
    }
    None
}

/// The child slots of a node in resolution order: title, children, caption,
/// then the slots that only some types have.
pub fn slots<'a>(n: &'a Node, out: &mut Vec<&'a Node>) {
    let title = match n {
        Node::Admonition(x) => x.title.as_deref(),
        Node::Directive(x) => x.title.as_deref(),
        Node::Div(x) => x.title.as_deref(),
        Node::CodeBlock(x) => x.title.as_deref(),
        _ => None,
    };
    out.extend(title.into_iter().flatten());
    match n {
        Node::BlockExtension(x) => out.extend(&x.fallback),
        Node::InlineExtension(x) if x.name.contains('.') => out.extend(x.fallback.iter().flatten()),
        _ => out.extend(n.children().into_iter().flatten()),
    }
    if let Some(c) = caption_of(n) {
        out.push(c);
    }
    match n {
        Node::Image(x) => out.extend(x.long_description.iter().flatten()),
        Node::Ruby(x) => out.extend(&x.pairs),
        Node::RubyPair(x) => out.extend(x.base.iter().chain(&x.annotation)),
        _ => {}
    }
}
