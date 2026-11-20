//! The resolution order (C2-A-025, C2-H-025, C2-H-087, C2-H-150, C2-H-170):
//! the document body in document order, a node before the nodes inside it and
//! a title before the children, then the notes in the order of their numbers.
//! Footnote numbers are assigned while the order is built, because they decide
//! the order of the notes.

use crate::ast::{Document, Node};
use crate::hash::AddrMap;

use std::collections::HashMap;

/// The address of a node, the key of every per-node result.
#[must_use]
pub fn addr(n: &Node<'_>) -> usize {
    std::ptr::from_ref(n).addr()
}

/// One visited node and the facts of its place that numbering reads.
#[derive(Clone, Copy)]
pub struct Visit<'r, 'a> {
    pub node: &'r Node<'a>,
    /// A `figure` or `table` directly inside a `figure_group` (C2-H-066).
    pub panel: bool,
    /// Inside a `figure` or `figure_group` whose caption has a mark (C2-H-151).
    pub in_marked_figure: bool,
    /// Inside the body of note N, 0 for the document body.
    pub note: usize,
}

/// A note of the footnote sequence: a `footnote` definition or an
/// `inline_footnote`, with the references that use it in order.
pub struct Note<'r, 'a> {
    pub node: &'r Node<'a>,
    pub refs: Vec<&'r Node<'a>>,
}

/// The resolution order and the footnote sequence.
#[derive(Default)]
pub struct Order<'r, 'a> {
    pub visits: Vec<Visit<'r, 'a>>,
    pub notes: Vec<Note<'r, 'a>>,
    /// Address of a `footnote_ref` or `inline_footnote` to its number.
    pub number: AddrMap<usize, usize>,
    /// The note of each definition, by the address of the definition.
    note_of_def: AddrMap<usize, usize>,
    /// With [`Order::build_collecting`]: the nodes of `every_node`, gathered
    /// on the walk, and whether the walk moved off document order.
    found: Option<Vec<&'r Node<'a>>>,
    moved: bool,
}

/// Where a footnote reference finds its definition.
enum Defs<'r, 'a, 'f> {
    /// The first definition of each key at the top level (C2-A-080).
    Top(HashMap<&'a str, &'r Node<'a>>),
    /// C2-N-055: the definitions of the reference's own file, then outward.
    Files(&'f super::files::Files<'a>, AddrMap<usize, &'r Node<'a>>),
}

impl<'r, 'a> Defs<'r, 'a, '_> {
    fn get(&self, r: &Node<'a>, key: &str) -> Option<&'r Node<'a>> {
        match self {
            Defs::Top(m) => m.get(key).copied(),
            Defs::Files(f, by) => f.note(r, key).and_then(|a| by.get(&a).copied()),
        }
    }
}

/// Every footnote definition of a tree whose includes were expanded, by its
/// [`super::files::inner`] address; they stand where the includes stood.
fn all_defs<'r, 'a>(nodes: &'r [Node<'a>], out: &mut AddrMap<usize, &'r Node<'a>>) {
    for n in nodes {
        match n {
            Node::Footnote(_) => {
                out.insert(super::files::inner(n), n);
            }
            Node::Include(i) => all_defs(i.children.as_deref().unwrap_or_default(), out),
            Node::List(l) => l
                .children
                .iter()
                .for_each(|c| all_defs(std::slice::from_ref(c), out)),
            Node::DefinitionList(d) => d
                .children
                .iter()
                .for_each(|c| all_defs(std::slice::from_ref(c), out)),
            _ if crate::include::holds_blocks(n) => all_defs(n.children().unwrap_or_default(), out),
            _ => {}
        }
    }
}

/// The first `footnote` definition of each key (C2-A-080).
#[must_use]
pub fn footnote_defs<'r, 'a>(doc: &'r Document<'a>) -> HashMap<&'a str, &'r Node<'a>> {
    let mut defs = HashMap::new();
    for c in doc.children.iter() {
        if let Node::Footnote(f) = c {
            defs.entry(f.key).or_insert(c);
        }
    }
    defs
}

impl<'r, 'a> Order<'r, 'a> {
    /// Builds the order of `doc`.
    #[must_use]
    pub fn build(doc: &'r Document<'a>) -> Self {
        Self::build_with(doc, None)
    }

    /// The order of a tree, with the files of its expanded includes.
    #[must_use]
    pub fn build_with(doc: &'r Document<'a>, files: Option<&super::files::Files<'a>>) -> Self {
        Self::build_inner(doc, files, false)
    }

    /// [`Order::build_with`], also gathering the nodes of `every_node`.
    pub(crate) fn build_collecting(
        doc: &'r Document<'a>,
        files: Option<&super::files::Files<'a>>,
    ) -> Self {
        Self::build_inner(doc, files, true)
    }

    /// The nodes `every_node` gives, where the walk met them in the same
    /// order: no note, no skipped definition and no expanded include moved it.
    pub(crate) fn take_found(&mut self) -> Option<Vec<&'r Node<'a>>> {
        let found = self.found.take();
        found.filter(|_| !self.moved)
    }

    fn build_inner(
        doc: &'r Document<'a>,
        files: Option<&super::files::Files<'a>>,
        collect: bool,
    ) -> Self {
        let defs = match files {
            None => Defs::Top(footnote_defs(doc)),
            Some(f) => {
                let mut by = AddrMap::default();
                all_defs(&*doc.children, &mut by);
                Defs::Files(f, by)
            }
        };
        let mut o = Order {
            found: collect.then(Vec::new),
            moved: files.is_some(),
            ..Order::default()
        };
        let top = Ctx {
            panel: false,
            in_marked_figure: false,
            note: 0,
        };
        for c in doc.children.iter() {
            if matches!(c, Node::Footnote(_)) {
                o.moved = true;
            } else {
                o.walk(c, &defs, top, 1);
            }
        }
        let mut i = 0;
        while let Some(note) = o.notes.get(i) {
            let content: &'r [Node<'a>] = match note.node {
                Node::Footnote(f) => &*f.children,
                Node::InlineFootnote(f) => &*f.children,
                _ => &[],
            };
            i += 1;
            let ctx = Ctx {
                panel: false,
                in_marked_figure: false,
                note: i,
            };
            for c in content {
                o.walk(c, &defs, ctx, 1);
            }
        }
        o
    }

    fn walk(&mut self, n: &'r Node<'a>, defs: &Defs<'r, 'a, '_>, ctx: Ctx, depth: usize) {
        if depth > crate::resolve::WALK_CAP {
            return;
        }
        if let Some(found) = self.found.as_mut()
            && crate::resolve::gathered(n)
        {
            found.push(n);
        }
        // No reader of the order looks at these leaves.
        if matches!(
            n,
            Node::Text(_)
                | Node::SoftBreak(_)
                | Node::HardBreak(_)
                | Node::NonBreakingSpace(_)
                | Node::SmartPunctuation(_)
        ) {
            return;
        }
        self.visits.push(Visit {
            node: n,
            panel: ctx.panel,
            in_marked_figure: ctx.in_marked_figure,
            note: ctx.note,
        });
        match n {
            Node::FootnoteRef(_) | Node::Footnote(_) | Node::InlineFootnote(_) => {
                self.moved = true;
            }
            _ => {}
        }
        match n {
            Node::FootnoteRef(r) => {
                if let Some(def) = defs.get(n, r.key) {
                    self.use_note(n, def, true);
                }
                return;
            }
            // Definitions are read as notes, in the order of their numbers.
            Node::Footnote(_) if matches!(defs, Defs::Files(..)) => {
                self.visits.pop();
                return;
            }
            Node::InlineFootnote(_) => {
                self.use_note(n, n, false);
                return;
            }
            _ => {}
        }
        let group = matches!(n, Node::FigureGroup(_));
        let marked = ctx.in_marked_figure
            || (matches!(
                n,
                Node::Paragraph(_)
                    | Node::CodeBlock(_)
                    | Node::BlockQuote(_)
                    | Node::FigureGroup(_)
            ) && caption_marked(n));
        for c in slots(n) {
            let panel = group
                && matches!(
                    c,
                    Node::Paragraph(_) | Node::CodeBlock(_) | Node::BlockQuote(_) | Node::Table(_)
                );
            self.walk(
                c,
                defs,
                Ctx {
                    panel,
                    in_marked_figure: marked,
                    note: ctx.note,
                },
                depth + 1,
            );
        }
        if let (Node::Table(t), Some(found)) = (n, self.found.as_mut()) {
            crate::resolve::every_node(t.columns, found, depth + 1);
        }
    }

    fn use_note(&mut self, r: &'r Node<'a>, note: &'r Node<'a>, shared: bool) {
        let existing = shared
            .then(|| self.note_of_def.get(&addr(note)).copied())
            .flatten();
        let idx = existing.unwrap_or_else(|| {
            self.notes.push(Note {
                node: note,
                refs: Vec::new(),
            });
            let idx = self.notes.len() - 1;
            if shared {
                self.note_of_def.insert(addr(note), idx);
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
pub fn caption_marked(n: &Node<'_>) -> bool {
    caption_of(n).is_some_and(|c| mark_of(c).is_some())
}

/// The caption slot of a node.
#[must_use]
pub fn caption_of<'r, 'a>(n: &'r Node<'a>) -> Option<&'r Node<'a>> {
    match n {
        Node::Paragraph(p) => p.caption.as_deref(),
        Node::CodeBlock(c) => c.caption.as_deref(),
        Node::BlockQuote(q) => q.caption.as_deref(),
        Node::FigureGroup(f) => f.caption.as_deref(),
        Node::Table(t) => t.caption.as_deref(),
        // C2-N-074: only an unexpanded include numbers and writes its own caption.
        Node::Include(i) if i.children.is_none() => i.caption.as_deref(),
        _ => None,
    }
}

/// The caption number node of a caption and the inline nodes before it: the
/// mark stands in the top-level inline content (C2-B-207).
#[must_use]
pub fn mark_of<'r, 'a>(caption: &'r Node<'a>) -> Option<(&'r Node<'a>, &'r [Node<'a>])> {
    let Node::Caption(c) = caption else {
        return None;
    };
    for p in c.children.iter() {
        if let Node::Paragraph(p) = p {
            if let Some(i) = p
                .children
                .iter()
                .position(|x| matches!(x, Node::CaptionNumber(_)))
            {
                return Some((p.children.get(i)?, p.children.get(..i)?));
            }
        }
    }
    None
}

/// The child slots of a node in resolution order: title, children, caption,
/// then the slots that only some types have.
pub fn slots<'r, 'a>(n: &'r Node<'a>) -> impl Iterator<Item = &'r Node<'a>> {
    let title: &[Node<'a>] = match n {
        Node::Admonition(x) => x.title.as_deref(),
        Node::Directive(x) => x.title.as_deref(),
        Node::Div(x) => x.title.as_deref(),
        Node::CodeBlock(x) => x.title.as_deref(),
        _ => None,
    }
    .unwrap_or_default();
    let main: &[Node<'a>] = match n {
        Node::BlockExtension(x) => &*x.fallback,
        Node::InlineExtension(x) if x.name.contains('.') => {
            x.fallback.as_deref().unwrap_or_default()
        }
        _ => n.children().unwrap_or_default(),
    };
    let (a, b): (&[Node<'a>], &[Node<'a>]) = match n {
        Node::Image(x) => (x.long_description.as_deref().unwrap_or_default(), &[]),
        Node::Ruby(x) => (&*x.pairs, &[]),
        Node::RubyPair(x) => (&*x.base, &*x.annotation),
        _ => (&[], &[]),
    };
    title
        .iter()
        .chain(main)
        .chain(caption_of(n))
        .chain(a)
        .chain(b)
}
