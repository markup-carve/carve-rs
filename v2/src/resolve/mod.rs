//! The resolve stage: references, footnote numbers, heading ids, caption and
//! equation numbers, cross-references and table alignment, written as the
//! result fields of `ast-results.json` beside the authored fields (C2-A-106).
//!
//! Every result is computed on the immutable tree, keyed by node address, and
//! then written in one mutable pass.

pub mod order;
pub mod plain;
pub mod slug;
pub mod table;

use crate::Arena;
use crate::ast::{Document, Node, TableColumnResolvedAlign};
use crate::chars::fold_label;
use crate::hash::{AddrMap, AddrSet};
use crate::options::{HeadingIds, Options};

use order::{Order, Visit, addr, caption_of, mark_of};
use plain::Symbols;
use std::collections::{HashMap, HashSet};

/// The bound of every recursive walk over a tree: the nesting cap counts
/// blocks and inlines together (C2-B-190), and a node's slots can add a level
/// the cap does not count, so the walk allows twice the cap.
pub const WALK_CAP: usize = 2 * crate::tables::DEPTH_CAP;

/// The names of the stages this resolver completes (C2-A-135).
pub const STAGES: [&str; 2] = ["referenceResolution", "tableAlignment"];

/// One result field.
enum Res<'a> {
    Number(i64),
    GeneratedId(&'a str),
    Href(&'a str),
    Src(&'a str),
    Expansion(&'a str),
    Spans(Option<i64>, Option<i64>),
    Align(TableColumnResolvedAlign),
}

/// Fills every result field of the tree and the root `stages` (C2-A-135).
pub fn resolve<'a>(doc: &mut Document<'a>, opts: &Options, arena: &'a Arena) {
    let results = compute(doc, opts, arena);
    if !results.is_empty() {
        for c in doc.children.iter_mut() {
            apply(c, &results, 1);
        }
    }
    doc.stages = Some(&STAGES);
}

fn compute<'a>(doc: &Document<'a>, opts: &Options, arena: &'a Arena) -> AddrMap<usize, Res<'a>> {
    let order = Order::build(doc);
    let mut res = AddrMap::default();
    for (a, n) in &order.number {
        res.insert(*a, Res::Number(to_i64(*n)));
    }
    let mut all = Vec::new();
    every_node(&*doc.children, &mut all, 1);
    let ids = heading_ids(&all, &order, opts.heading_ids, &mut res, arena);
    let captions = caption_numbers(&order, opts, &mut res);
    let index = Index::new(doc, &order, &ids, &captions);
    for n in all {
        index.reference(n, &mut res, arena);
        table::resolve_table(n, &mut res);
    }
    res
}

fn to_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// The nodes of the tree the order-free results read (an authored id,
/// references, tables), in document order.
fn every_node<'r, 'a>(nodes: &'r [Node<'a>], out: &mut Vec<&'r Node<'a>>, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    for n in nodes {
        if n.attrs().is_some()
            || matches!(
                n,
                Node::Link(_)
                    | Node::Image(_)
                    | Node::Xref(_)
                    | Node::Abbreviation(_)
                    | Node::Table(_)
            )
        {
            out.push(n);
        }
        let columns: &[Node<'a>] = match n {
            Node::Table(t) => &*t.columns,
            _ => &[],
        };
        for c in order::slots(n).chain(columns) {
            every_node(std::slice::from_ref(c), out, depth + 1);
        }
    }
}

/// C2-H-022 to C2-H-025: authored ids are reserved first, in document order;
/// generated ids follow in resolution order. Returns the heading id of each
/// heading address that has one.
fn heading_ids<'a>(
    all: &[&Node<'a>],
    order: &Order<'_, 'a>,
    mode: HeadingIds,
    res: &mut AddrMap<usize, Res<'a>>,
    arena: &'a Arena,
) -> AddrMap<usize, &'a str> {
    let mut taken = slug::Taken::default();
    for n in all {
        if let Some(id) = n.attrs().and_then(|a| a.id) {
            taken.reserve(id);
        }
    }
    let mut ids = AddrMap::default();
    for v in &order.visits {
        let Node::Heading(h) = v.node else { continue };
        if let Some(id) = h.attrs.and_then(|a| a.id) {
            ids.insert(addr(v.node), id);
        } else if mode != HeadingIds::Off {
            let id = arena.str(&taken.unique(&slug::slug(&plain::text_of(h.children), mode)));
            ids.insert(addr(v.node), id);
            res.insert(addr(v.node), Res::GeneratedId(id));
        }
    }
    ids
}

/// C2-H-149 to C2-H-151 and C2-H-170: one count per sequence key, in
/// resolution order, a node before the nodes inside it.
fn caption_numbers(
    order: &Order<'_, '_>,
    opts: &Options,
    res: &mut AddrMap<usize, Res<'_>>,
) -> AddrSet<usize> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    let mut out = AddrSet::default();
    for v in &order.visits {
        if let Some((mark, label)) = caption_mark(v, opts) {
            let n = next(&mut counts, &label);
            res.insert(addr(mark), Res::Number(n));
            out.insert(addr(v.node));
        } else if let Node::Math(m) = v.node {
            if let (true, Some(prefix), false) = (m.display, &m.number_prefix, v.in_marked_figure) {
                res.insert(addr(v.node), Res::Number(next(&mut counts, prefix)));
            }
        }
    }
    out
}

fn next(counts: &mut HashMap<String, i64>, key: &str) -> i64 {
    let c = counts.entry(key.to_string()).or_insert(0);
    *c += 1;
    *c
}

/// The mark of a numbered caption and its sequence key (C2-B-208).
fn caption_mark<'r, 'a>(v: &Visit<'r, 'a>, opts: &Options) -> Option<(&'r Node<'a>, String)> {
    if v.panel {
        return None;
    }
    let (mark, before) = mark_of(caption_of(v.node)?)?;
    Some((mark, label_key(before, Some(&opts.symbols))))
}

/// The plain text of a caption label without trailing whitespace; symbols give
/// their written text (C2-B-208).
#[must_use]
pub fn label_key(
    before: &[Node<'_>],
    symbols: Option<&std::collections::BTreeMap<String, String>>,
) -> String {
    let mut s = String::new();
    let sym = symbols.map_or(Symbols::Nothing, Symbols::Written);
    plain::inline_text(before, sym, &mut s);
    s.trim_end().to_string()
}

/// The lookup tables of reference resolution.
struct Index<'a> {
    links: HashMap<&'a str, &'a str>,
    abbrs: HashMap<&'a str, &'a str>,
    /// The id of each heading with one, and for each folded plain text and
    /// folded source the first heading that has it.
    heading_ids: Vec<&'a str>,
    heading_by_text: HashMap<String, usize>,
    heading_by_src: HashMap<String, usize>,
    targets: HashSet<&'a str>,
}

impl<'a> Index<'a> {
    fn new(
        doc: &Document<'a>,
        order: &Order<'_, 'a>,
        ids: &AddrMap<usize, &'a str>,
        owners: &AddrSet<usize>,
    ) -> Self {
        let mut links = HashMap::new();
        let mut abbrs = HashMap::new();
        for c in doc.children.iter() {
            match c {
                Node::LinkReferenceDefinition(d) => {
                    links.entry(d.key).or_insert(d.href);
                }
                Node::AbbreviationDef(d) => {
                    abbrs.entry(d.key).or_insert(d.expansion);
                }
                _ => {}
            }
        }
        let mut heading_ids = Vec::new();
        let mut heading_by_text = HashMap::new();
        let mut heading_by_src = HashMap::new();
        let mut targets = HashSet::new();
        for v in &order.visits {
            let a = addr(v.node);
            if let (Node::Heading(h), Some(id)) = (v.node, ids.get(&a)) {
                let mut src = String::new();
                plain::source_text(h.children, &mut src, 0);
                let i = heading_ids.len();
                heading_by_text
                    .entry(fold_label(&plain::text_of(h.children)))
                    .or_insert(i);
                heading_by_src.entry(fold_label(&src)).or_insert(i);
                heading_ids.push(*id);
                targets.insert(*id);
            } else if owners.contains(&a) {
                if let Some(id) = v.node.attrs().and_then(|x| x.id) {
                    targets.insert(id);
                }
            }
        }
        Index {
            links,
            abbrs,
            heading_ids,
            heading_by_text,
            heading_by_src,
            targets,
        }
    }

    /// C2-I-081 to C2-I-084, C2-A-109, C2-H-095, C2-I-098.
    fn reference(&self, n: &Node<'a>, res: &mut AddrMap<usize, Res<'a>>, arena: &'a Arena) {
        let a = addr(n);
        match n {
            Node::Link(l) if l.namespace == Some("link") => {
                let Some(key) = l.key else { return };
                if let Some(href) = self.links.get(key) {
                    res.insert(a, Res::Href(href));
                } else if !matches!(l.ref_form, Some(crate::ast::LinkRefForm::Shortcut)) {
                    // Gap HTML-002: the heading is matched on its plain text only; the tree has no source.
                    let first = [&self.heading_by_text, &self.heading_by_src]
                        .iter()
                        .filter_map(|m| m.get(key).copied())
                        .min();
                    if let Some(id) = first.and_then(|i| self.heading_ids.get(i)) {
                        res.insert(a, Res::Href(arena.str(&format!("#{id}"))));
                    }
                }
            }
            Node::Image(i) if i.namespace == Some("link") => {
                if let Some(src) = i.key.and_then(|k| self.links.get(k)) {
                    res.insert(a, Res::Src(src));
                }
            }
            Node::Xref(x) => {
                if self.targets.contains(x.key) {
                    res.insert(a, Res::Href(arena.str(&format!("#{}", x.key))));
                }
            }
            Node::Abbreviation(x) => {
                if let Some(e) = self.abbrs.get(x.key) {
                    res.insert(a, Res::Expansion(e));
                }
            }
            _ => {}
        }
    }
}

fn apply<'a>(n: &mut Node<'a>, res: &AddrMap<usize, Res<'a>>, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    if takes_result(n)
        && let Some(r) = res.get(&std::ptr::from_ref::<Node>(n).addr())
    {
        set(n, r);
    }
    n.for_each_child_mut(&mut |c| apply(c, res, depth + 1));
}

/// The node types `set` writes.
fn takes_result(n: &Node<'_>) -> bool {
    matches!(
        n,
        Node::FootnoteRef(_)
            | Node::InlineFootnote(_)
            | Node::CaptionNumber(_)
            | Node::Math(_)
            | Node::Heading(_)
            | Node::Link(_)
            | Node::Xref(_)
            | Node::Image(_)
            | Node::Abbreviation(_)
            | Node::TableCell(_)
            | Node::TableColumn(_)
    )
}

fn set<'a>(n: &mut Node<'a>, r: &Res<'a>) {
    match (n, r) {
        (Node::FootnoteRef(x), Res::Number(v)) => x.number = Some(*v),
        (Node::InlineFootnote(x), Res::Number(v)) => x.number = Some(*v),
        (Node::CaptionNumber(x), Res::Number(v)) => x.n = Some(*v),
        (Node::Math(x), Res::Number(v)) => x.number = Some(*v),
        (Node::Heading(x), Res::GeneratedId(v)) => x.generated_id = Some(*v),
        (Node::Link(x), Res::Href(v)) => x.href = Some(*v),
        (Node::Xref(x), Res::Href(v)) => x.href = Some(*v),
        (Node::Image(x), Res::Src(v)) => x.src = Some(*v),
        (Node::Abbreviation(x), Res::Expansion(v)) => x.expansion = Some(*v),
        (Node::TableCell(x), Res::Spans(c, r)) => {
            x.colspan = *c;
            x.rowspan = *r;
        }
        (Node::TableColumn(x), Res::Align(v)) => x.resolved_align = Some(*v),
        _ => {}
    }
}
