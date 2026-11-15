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
enum Res {
    Number(i64),
    GeneratedId(String),
    Href(String),
    Src(String),
    Expansion(String),
    Spans(Option<i64>, Option<i64>),
    Align(TableColumnResolvedAlign),
}

/// Fills every result field of the tree and the root `stages` (C2-A-135).
pub fn resolve(doc: &mut Document, opts: &Options) {
    let results = compute(doc, opts);
    if !results.is_empty() {
        for c in &mut doc.children {
            apply(c, &results, 1);
        }
    }
    doc.stages = Some(STAGES.iter().map(|s| (*s).to_string()).collect());
}

fn compute(doc: &Document, opts: &Options) -> AddrMap<usize, Res> {
    let order = Order::build(doc);
    let mut res = AddrMap::default();
    for (a, n) in &order.number {
        res.insert(*a, Res::Number(to_i64(*n)));
    }
    let mut all = Vec::new();
    every_node(&doc.children, &mut all, 1);
    let ids = heading_ids(&all, &order, opts.heading_ids, &mut res);
    let captions = caption_numbers(&order, opts, &mut res);
    let index = Index::new(doc, &order, &ids, &captions);
    for n in all {
        index.reference(n, &mut res);
        table::resolve_table(n, &mut res);
    }
    res
}

fn to_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Every node of the tree in document order, for the order-free results.
fn every_node<'a>(nodes: &'a [Node], out: &mut Vec<&'a Node>, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    for n in nodes {
        out.push(n);
        let columns: &[Node] = match n {
            Node::Table(t) => &t.columns,
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
fn heading_ids(
    all: &[&Node],
    order: &Order<'_>,
    mode: HeadingIds,
    res: &mut AddrMap<usize, Res>,
) -> AddrMap<usize, String> {
    let mut taken = slug::Taken::default();
    for n in all {
        if let Some(id) = n.attrs().and_then(|a| a.id.as_deref()) {
            taken.reserve(id);
        }
    }
    let mut ids = AddrMap::default();
    for v in &order.visits {
        let Node::Heading(h) = v.node else { continue };
        if let Some(id) = h.attrs.as_ref().and_then(|a| a.id.clone()) {
            ids.insert(addr(v.node), id);
        } else if mode != HeadingIds::Off {
            let id = taken.unique(&slug::slug(&plain::text_of(&h.children), mode));
            ids.insert(addr(v.node), id.clone());
            res.insert(addr(v.node), Res::GeneratedId(id));
        }
    }
    ids
}

/// C2-H-149 to C2-H-151 and C2-H-170: one count per sequence key, in
/// resolution order, a node before the nodes inside it.
fn caption_numbers(
    order: &Order<'_>,
    opts: &Options,
    res: &mut AddrMap<usize, Res>,
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
fn caption_mark<'a>(v: &Visit<'a>, opts: &Options) -> Option<(&'a Node, String)> {
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
    before: &[Node],
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
    heading_ids: Vec<String>,
    heading_by_text: HashMap<String, usize>,
    heading_by_src: HashMap<String, usize>,
    targets: HashSet<String>,
}

impl<'a> Index<'a> {
    fn new(
        doc: &'a Document,
        order: &Order<'a>,
        ids: &AddrMap<usize, String>,
        owners: &AddrSet<usize>,
    ) -> Self {
        let mut links = HashMap::new();
        let mut abbrs = HashMap::new();
        for c in &doc.children {
            match c {
                Node::LinkReferenceDefinition(d) => {
                    links.entry(d.key.as_str()).or_insert(d.href.as_str());
                }
                Node::AbbreviationDef(d) => {
                    abbrs.entry(d.key.as_str()).or_insert(d.expansion.as_str());
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
                plain::source_text(&h.children, &mut src, 0);
                let i = heading_ids.len();
                heading_by_text
                    .entry(fold_label(&plain::text_of(&h.children)))
                    .or_insert(i);
                heading_by_src.entry(fold_label(&src)).or_insert(i);
                heading_ids.push(id.clone());
                targets.insert(id.clone());
            } else if owners.contains(&a) {
                if let Some(id) = v.node.attrs().and_then(|x| x.id.clone()) {
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
    fn reference(&self, n: &Node, res: &mut AddrMap<usize, Res>) {
        let a = addr(n);
        match n {
            Node::Link(l) if l.namespace.as_deref() == Some("link") => {
                let Some(key) = l.key.as_deref() else { return };
                if let Some(href) = self.links.get(key) {
                    res.insert(a, Res::Href((*href).to_string()));
                } else if !matches!(l.ref_form, Some(crate::ast::LinkRefForm::Shortcut)) {
                    // Gap HTML-002: the heading is matched on its plain text only; the tree has no source.
                    let first = [&self.heading_by_text, &self.heading_by_src]
                        .iter()
                        .filter_map(|m| m.get(key).copied())
                        .min();
                    if let Some(id) = first.and_then(|i| self.heading_ids.get(i)) {
                        res.insert(a, Res::Href(format!("#{id}")));
                    }
                }
            }
            Node::Image(i) if i.namespace.as_deref() == Some("link") => {
                if let Some(src) = i.key.as_deref().and_then(|k| self.links.get(k)) {
                    res.insert(a, Res::Src((*src).to_string()));
                }
            }
            Node::Xref(x) => {
                if self.targets.contains(&x.key) {
                    res.insert(a, Res::Href(format!("#{}", x.key)));
                }
            }
            Node::Abbreviation(x) => {
                if let Some(e) = self.abbrs.get(x.key.as_str()) {
                    res.insert(a, Res::Expansion((*e).to_string()));
                }
            }
            _ => {}
        }
    }
}

fn apply(n: &mut Node, res: &AddrMap<usize, Res>, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    if let Some(r) = res.get(&std::ptr::from_ref::<Node>(n).addr()) {
        set(n, r);
    }
    n.for_each_child_mut(&mut |c| apply(c, res, depth + 1));
}

fn set(n: &mut Node, r: &Res) {
    match (n, r) {
        (Node::FootnoteRef(x), Res::Number(v)) => x.number = Some(*v),
        (Node::InlineFootnote(x), Res::Number(v)) => x.number = Some(*v),
        (Node::CaptionNumber(x), Res::Number(v)) => x.n = Some(*v),
        (Node::Math(x), Res::Number(v)) => x.number = Some(*v),
        (Node::Heading(x), Res::GeneratedId(v)) => x.generated_id = Some(v.clone()),
        (Node::Link(x), Res::Href(v)) => x.href = Some(v.clone()),
        (Node::Xref(x), Res::Href(v)) => x.href = Some(v.clone()),
        (Node::Image(x), Res::Src(v)) => x.src = Some(v.clone()),
        (Node::Abbreviation(x), Res::Expansion(v)) => x.expansion = Some(v.clone()),
        (Node::TableCell(x), Res::Spans(c, r)) => {
            x.colspan = *c;
            x.rowspan = *r;
        }
        (Node::TableColumn(x), Res::Align(v)) => x.resolved_align = Some(*v),
        _ => {}
    }
}
