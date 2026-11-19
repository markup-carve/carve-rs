//! The resolve stage: references, footnote numbers, heading ids, caption and
//! equation numbers, cross-references and table alignment, written as the
//! result fields of `ast-results.json` beside the authored fields (C2-A-106).
//!
//! Every result is computed on the immutable tree, keyed by node address, and
//! then written in one mutable pass.

pub mod files;
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
use unicode_normalization::UnicodeNormalization as _;

/// C2-B-190, C2-I-118: the separate caps can occur on one path, and node
/// slots can add levels that neither cap counts.
pub const WALK_CAP: usize = 2 * (crate::tables::BLOCK_DEPTH + crate::tables::INLINE_DEPTH);

/// An authored id that stood earlier in the tree, and where it stands again.
/// The last part is the address of its `pos`, for the file it stands in.
pub(crate) type Duplicate = (String, Option<crate::ast::Pos>, usize);

/// The names of the stages this resolver completes (C2-A-135).
pub const STAGES: [&str; 2] = ["referenceResolution", "tableAlignment"];

/// The stages of a tree in which expansion ran (C2-N-006).
pub const STAGES_EXPANDED: [&str; 3] =
    ["includeExpansion", "referenceResolution", "tableAlignment"];

/// One result field.
enum Res<'a> {
    Number(i64),
    GeneratedId(&'a str),
    Href(&'a str),
    Src(&'a str),
    Expansion(&'a str),
    Spans(Option<i64>, Option<i64>),
    Aligns(
        Option<TableColumnResolvedAlign>,
        Option<crate::ast::TableColumnResolvedValign>,
    ),
}

/// Fills every result field of the tree and the root `stages` (C2-A-135).
pub fn resolve<'a>(doc: &mut Document<'a>, opts: &Options, arena: &'a Arena) {
    resolve_ordered(doc, opts, arena);
}

/// C2-A-135: a supplied tree's resolution results are never render inputs.
pub fn clear_results(doc: &mut Document<'_>) {
    doc.stages = None;
    for node in doc.children.iter_mut() {
        clear_node(node, 1);
    }
}

fn clear_node(node: &mut Node<'_>, depth: usize) {
    if depth > WALK_CAP {
        return;
    }
    clear_inline_result(node);
    clear_block_result(node);
    node.for_each_child_mut(&mut |child| clear_node(child, depth + 1));
}

fn clear_inline_result(node: &mut Node<'_>) {
    match node {
        Node::Abbreviation(x) => x.expansion = None,
        Node::CaptionNumber(x) => x.n = None,
        Node::Citation(x) => {
            x.number = None;
            x.use_index = None;
        }
        Node::FootnoteRef(x) => x.number = None,
        Node::Heading(x) => x.generated_id = None,
        Node::Image(x) if x.ref_form.is_some() => x.src = None,
        Node::InlineFootnote(x) => x.number = None,
        Node::Link(x) if x.ref_form.is_some() => x.href = None,
        Node::Math(x) => x.number = None,
        _ => {}
    }
}

fn clear_block_result(node: &mut Node<'_>) {
    match node {
        Node::TableCell(x) if x.span.is_some() => {
            x.colspan = None;
            x.rowspan = None;
        }
        Node::TableColumn(x) => {
            x.resolved_align = None;
            x.resolved_valign = None;
        }
        Node::Xref(x) => x.href = None,
        _ => {}
    }
}

/// `resolve`; true when the order holds a note, a cross-reference or a
/// titled block, the parts of the order the HTML renderer reads.
pub(crate) fn resolve_ordered<'a>(
    doc: &mut Document<'a>,
    opts: &Options,
    arena: &'a Arena,
) -> bool {
    resolve_files(doc, opts, arena, None, &mut Vec::new())
}

/// `resolve_ordered` over a tree whose includes were expanded (C2-N-053).
pub(crate) fn resolve_files<'a>(
    doc: &mut Document<'a>,
    opts: &Options,
    arena: &'a Arena,
    files: Option<&files::Files<'a>>,
    dups: &mut Vec<Duplicate>,
) -> bool {
    let (results, ordered) = compute(doc, opts, arena, files, dups);
    if !results.is_empty() {
        for c in doc.children.iter_mut() {
            apply(c, &results, 1);
        }
    }
    doc.stages = Some(&STAGES);
    ordered
}

fn compute<'a>(
    doc: &Document<'a>,
    opts: &Options,
    arena: &'a Arena,
    files: Option<&files::Files<'a>>,
    dups: &mut Vec<Duplicate>,
) -> (AddrMap<usize, Res<'a>>, bool) {
    let order = Order::build_with(doc, files);
    let ordered = !order.notes.is_empty()
        || order.visits.iter().any(|v| match v.node {
            Node::Xref(_) => true,
            Node::Admonition(a) => a.title.is_some(),
            Node::Directive(d) => d.title.is_some(),
            _ => false,
        });
    let mut res = AddrMap::default();
    for (a, n) in &order.number {
        res.insert(*a, Res::Number(to_i64(*n)));
    }
    let mut all = Vec::new();
    every_node(&*doc.children, &mut all, 1);
    let ids = heading_ids(&all, &order, opts.heading_ids, &mut res, arena, dups);
    let captions = caption_numbers(&order, opts, &mut res);
    let mut index = Index::new(doc, &order, &ids, &captions);
    index.files = files;
    for n in all {
        index.reference(n, &mut res, arena);
        table::resolve_table(n, &mut res);
    }
    (res, ordered)
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
    dups: &mut Vec<Duplicate>,
) -> AddrMap<usize, &'a str> {
    let mut taken = slug::Taken::default();
    // C2-N-072: an include's own id is no anchor and is not reserved.
    for n in all.iter().filter(|n| !matches!(n, Node::Include(_))) {
        if let Some(id) = n.attrs().and_then(|a| a.id) {
            if !taken.reserve(id) {
                // C2-A-039, C2-N-058: the later occurrence is the duplicate.
                let at = n.pos().map_or(0, |p| std::ptr::from_ref(p).addr());
                dups.push((id.to_string(), n.pos().copied(), at));
            }
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

/// C2-N-027: the heading ids of one file's own parse, before it meets any other file.
pub(crate) fn heading_ids_alone<'a>(
    doc: &Document<'a>,
    mode: HeadingIds,
    arena: &'a Arena,
) -> AddrMap<usize, &'a str> {
    let order = Order::build(doc);
    let mut all = Vec::new();
    every_node(&*doc.children, &mut all, 1);
    heading_ids(
        &all,
        &order,
        mode,
        &mut AddrMap::default(),
        arena,
        &mut Vec::new(),
    )
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
struct Index<'a, 'f> {
    /// C2-N-054, C2-N-055: the definitions by file, for an expanded tree.
    files: Option<&'f files::Files<'a>>,
    links: HashMap<&'a str, &'a str>,
    abbrs: HashMap<&'a str, &'a str>,
    /// The id of each heading with one, and the first heading for each folded plain text.
    heading_ids: Vec<&'a str>,
    heading_by_text: HashMap<String, usize>,
    targets: HashMap<String, &'a str>,
}

impl<'a> Index<'a, '_> {
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
        let mut targets = HashMap::new();
        for v in &order.visits {
            let a = addr(v.node);
            if let Node::FigureGroup(group) = v.node {
                if owners.contains(&a) {
                    for panel in group.children.iter().filter(|node| {
                        matches!(
                            node,
                            Node::Paragraph(_)
                                | Node::CodeBlock(_)
                                | Node::BlockQuote(_)
                                | Node::Table(_)
                        )
                    }) {
                        if let Some(id) = panel.attrs().and_then(|attrs| attrs.id) {
                            targets.entry(id.nfc().collect()).or_insert(id);
                        }
                    }
                }
            }
            if let (Node::Heading(h), Some(id)) = (v.node, ids.get(&a)) {
                let i = heading_ids.len();
                heading_by_text
                    .entry(fold_label(&plain::text_of(h.children)).nfc().collect())
                    .or_insert(i);
                heading_ids.push(*id);
                targets.entry(id.nfc().collect()).or_insert(*id);
            } else if owners.contains(&a) && !matches!(v.node, Node::Include(_)) {
                if let Some(id) = v.node.attrs().and_then(|x| x.id) {
                    targets.entry(id.nfc().collect()).or_insert(id);
                }
            }
        }
        Index {
            files: None,
            links,
            abbrs,
            heading_ids,
            heading_by_text,
            targets,
        }
    }

    /// C2-I-081 to C2-I-084, C2-A-109, C2-H-095, C2-I-098.
    fn reference(&self, n: &Node<'a>, res: &mut AddrMap<usize, Res<'a>>, arena: &'a Arena) {
        let a = addr(n);
        match n {
            Node::Link(l) if l.namespace == Some("link") => {
                let Some(key) = l.key else { return };
                if let Some(href) = self.link(n, key) {
                    res.insert(a, Res::Href(href));
                } else if !matches!(l.ref_form, Some(crate::ast::LinkRefForm::Shortcut)) {
                    if let Some(id) = self.heading_target(key, arena) {
                        res.insert(a, Res::Href(arena.str(&format!("#{id}"))));
                    }
                }
            }
            Node::Image(i) if i.namespace == Some("link") => {
                if let Some(src) = i.key.and_then(|k| self.link(n, k)) {
                    res.insert(a, Res::Src(src));
                }
            }
            Node::Xref(x) => {
                if let Some(id) = self.targets.get(&x.key.nfc().collect::<String>()) {
                    res.insert(a, Res::Href(arena.str(&format!("#{id}"))));
                }
            }
            Node::Abbreviation(x) => {
                if let Some(e) = self.abbr(n, x.key) {
                    res.insert(a, Res::Expansion(e));
                }
            }
            _ => {}
        }
    }

    fn link(&self, n: &Node<'_>, key: &str) -> Option<&'a str> {
        match self.files {
            Some(f) => f.link(n, key),
            None => self.links.get(key).copied(),
        }
    }

    fn abbr(&self, n: &Node<'_>, key: &str) -> Option<&'a str> {
        match self.files {
            Some(f) => f.abbr(n, key),
            None => self.abbrs.get(key).copied(),
        }
    }

    fn heading_target(&self, key: &str, arena: &Arena) -> Option<&'a str> {
        let plain_key = if key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b' ') {
            std::borrow::Cow::Borrowed(key)
        } else {
            std::borrow::Cow::Owned(heading_key_plain(key, arena))
        };
        if plain_key.is_empty() {
            return None;
        }
        self.heading_by_text
            .get(plain_key.as_ref())
            .and_then(|i| self.heading_ids.get(*i))
            .copied()
    }
}

/// C2-I-084: read a reference key as inline content without resolving references.
fn heading_key_plain(key: &str, arena: &Arena) -> String {
    let src = crate::source::Source::new(key);
    let links = HashSet::new();
    let env = crate::inline::InlineEnv {
        src: &src,
        arena,
        link_keys: &links,
        abbreviations: &[],
        kind: crate::inline::ContentKind::Paragraph,
        hard_breaks: false,
    };
    let pieces = [crate::inline::Piece {
        start: 0,
        end: key.len(),
        join: crate::inline::Join::Line,
    }];
    let nodes = crate::inline::parse(&env, &pieces, &mut crate::inline::Scratch::default());
    fold_label(&plain::text_of(nodes)).nfc().collect()
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
        (Node::TableColumn(x), Res::Aligns(align, valign)) => {
            x.resolved_align = *align;
            x.resolved_valign = *valign;
        }
        _ => {}
    }
}
