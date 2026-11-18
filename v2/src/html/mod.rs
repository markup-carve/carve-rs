//! The HTML renderer (C2-H): a resolved tree to an HTML fragment.

mod abbreviation;
pub mod attrs;
mod block;
pub mod esc;
mod inline;
mod notes;
mod profile;
mod table;
pub mod url;

use crate::ast::{Document, LinkReferenceDefinition, Node};
use crate::hash::{AddrMap, AddrSet};
use crate::options::Options;
use crate::resolve::order::{Order, addr, caption_of, mark_of};
use crate::resolve::{label_key, plain, slug};
use crate::source::SourceView;
use attrs::{El, Filter};
use std::collections::{HashMap, HashSet};

/// Renders a resolved tree (C2-H-001).
#[must_use]
pub fn render(doc: &Document<'_>, opts: &Options) -> String {
    let mut r = Renderer::new(doc, opts);
    r.document();
    r.out
}

/// Renders and returns the render losses beside the HTML (C2-H-132).
#[must_use]
pub fn render_with_losses(doc: &Document<'_>, opts: &Options) -> (String, Vec<&'static str>) {
    let (html, losses, _) = render_with_details(doc, opts);
    (html, losses)
}

pub(crate) fn render_with_details(
    doc: &Document<'_>,
    opts: &Options,
) -> (String, Vec<&'static str>, Option<usize>) {
    let mut r = Renderer::new(doc, opts);
    r.document();
    (r.out, r.losses, r.derived_failure)
}

/// Renders with losses while retaining the matching UTF-8 source text for
/// degraded inline nodes. For a tree parsed from bytes, use
/// [`render_with_losses_source_bytes`] (C2-B-218).
#[must_use]
pub fn render_with_losses_source(
    doc: &Document<'_>,
    opts: &Options,
    src: &str,
) -> (String, Vec<&'static str>) {
    if opts.profile != crate::options::Profile::Full && src.contains('\0') {
        let normalized = crate::source::Source::new(src);
        render_mapped_source(doc, opts, normalized.render_view(), true, None, None, None)
    } else {
        render_source(doc, opts, src, true, None, None, None)
    }
}

/// C2-B-218: renders a tree parsed from the same byte source, retaining its
/// original byte positions when a profile degrades inline nodes.
#[must_use]
pub fn render_with_losses_source_bytes(
    doc: &Document<'_>,
    opts: &Options,
    src: &[u8],
) -> (String, Vec<&'static str>) {
    let normalized = crate::source::Source::from_bytes(src);
    render_mapped_source(doc, opts, normalized.render_view(), true, None, None, None)
}

/// `render_with_losses_source`; `ordered` false when resolve found no note,
/// cross-reference or titled block, so the renderer needs no order of its own.
pub(crate) fn render_source(
    doc: &Document<'_>,
    opts: &Options,
    src: &str,
    ordered: bool,
    destinations: Option<&mut Vec<DestinationLoss>>,
    raws: Option<&mut Vec<RawLoss>>,
    carriers: Option<&mut Vec<CarrierLoss>>,
) -> (String, Vec<&'static str>) {
    let mut r = Renderer::with_order(doc, opts, ordered);
    r.src = Some(SourceView::direct(src));
    r.document();
    if let Some(destinations) = destinations {
        destinations.extend(r.destination_losses);
    }
    if let Some(raws) = raws {
        raws.extend(r.raw_losses);
    }
    if let Some(carriers) = carriers {
        carriers.extend(r.carrier_losses);
    }
    (r.out, r.losses)
}

/// `render_source` of a tree whose includes were expanded and flattened.
pub(crate) fn render_files<'a>(
    doc: &Document<'a>,
    opts: &Options,
    src: &str,
    files: &crate::resolve::files::Files<'a>,
    destinations: Option<&mut Vec<DestinationLoss>>,
    raws: Option<&mut Vec<RawLoss>>,
    carriers: Option<&mut Vec<CarrierLoss>>,
) -> (String, Vec<&'static str>) {
    let mut r = Renderer::build(doc, opts, true, Some(files));
    r.src = Some(SourceView::direct(src));
    r.document();
    if let Some(destinations) = destinations {
        destinations.extend(r.destination_losses);
    }
    if let Some(raws) = raws {
        raws.extend(r.raw_losses);
    }
    if let Some(carriers) = carriers {
        carriers.extend(r.carrier_losses);
    }
    (r.out, r.losses)
}

pub(crate) fn render_mapped_source(
    doc: &Document<'_>,
    opts: &Options,
    src: SourceView<'_>,
    ordered: bool,
    destinations: Option<&mut Vec<DestinationLoss>>,
    raws: Option<&mut Vec<RawLoss>>,
    carriers: Option<&mut Vec<CarrierLoss>>,
) -> (String, Vec<&'static str>) {
    let mut r = Renderer::with_order(doc, opts, ordered);
    r.src = Some(src);
    r.document();
    if let Some(destinations) = destinations {
        destinations.extend(r.destination_losses);
    }
    if let Some(raws) = raws {
        raws.extend(r.raw_losses);
    }
    if let Some(carriers) = carriers {
        carriers.extend(r.carrier_losses);
    }
    (r.out, r.losses)
}

pub(crate) struct DestinationLoss {
    pub image: bool,
    pub attribute: bool,
    pub pos: Option<crate::ast::Pos>,
    pub value: String,
    pub file: Option<String>,
}

pub(crate) struct RawLoss {
    pub kind: &'static str,
    pub format: String,
    pub pos: Option<crate::ast::Pos>,
    pub file: Option<String>,
}

pub(crate) struct CarrierLoss {
    pub key: String,
    pub pos: Option<crate::ast::Pos>,
    pub regions: Vec<(u32, u32)>,
    pub file: Option<String>,
}

/// About the output length: twice the source length the tree covers.
fn out_estimate(doc: &Document<'_>) -> usize {
    let end = doc
        .children
        .last()
        .and_then(Node::pos)
        .map_or(0, crate::ast::Pos::end);
    2 * end
}

/// The state of one render.
pub(crate) struct Renderer<'r, 'a> {
    opts: &'r Options,
    doc: &'r Document<'a>,
    src: Option<SourceView<'r>>,
    /// The files of an expanded tree: an included node reads no top-level source.
    files: Option<&'r crate::resolve::files::Files<'a>>,
    out: String,
    order: Order<'r, 'a>,
    /// The anchor id of each note mark (C2-H-088).
    mark_ids: AddrMap<usize, String>,
    /// The anchor id of each note, in number order (C2-H-202).
    note_ids: Vec<String>,
    links: HashMap<&'a str, &'r LinkReferenceDefinition<'a>>,
    hidden_heading_ids: HashSet<&'a str>,
    xref_text: HashMap<String, String>,
    derived_remaining: usize,
    derived_exhausted: bool,
    derived_failure: Option<usize>,
    adm_ids: AddrMap<usize, String>,
    /// The top-level `footnotes` directive that places the section (C2-H-093).
    placing: Option<usize>,
    /// Note marks met inside link text, written after the outermost link.
    pending: String,
    in_link: usize,
    in_abbr: usize,
    level: usize,
    caption_inner: Option<usize>,
    /// The `longDescription` blocks met in the block being written (C2-H-080).
    long_desc: Vec<&'r [Node<'a>]>,
    /// The notes the section writes.
    shown: Vec<bool>,
    /// A line was written (an empty first line still counts, C2-H-134).
    started: bool,
    /// Inside a node that stays while its children are degraded.
    degrading: usize,
    losses: Vec<&'static str>,
    destination_losses: Vec<DestinationLoss>,
    raw_losses: Vec<RawLoss>,
    carrier_losses: Vec<CarrierLoss>,
    /// Comments of comment-only lines and the break each takes (C2-H-069).
    hidden: AddrSet<usize>,
    /// The row of `opts.profile` in the profile table.
    profile: Option<&'static crate::tables::ProfileRow>,
}

impl<'r, 'a> Renderer<'r, 'a> {
    fn new(doc: &'r Document<'a>, opts: &'r Options) -> Self {
        Self::with_order(doc, opts, true)
    }

    fn with_order(doc: &'r Document<'a>, opts: &'r Options, ordered: bool) -> Self {
        Self::build(doc, opts, ordered, None)
    }

    fn build(
        doc: &'r Document<'a>,
        opts: &'r Options,
        ordered: bool,
        files: Option<&'r crate::resolve::files::Files<'a>>,
    ) -> Self {
        let profile = crate::tables::PROFILES
            .iter()
            .find(|p| p.name == opts.profile.name());
        let hides_headings = profile.is_some_and(|row| {
            row.denied_block.contains(&"heading")
                || row
                    .allowed_block
                    .is_some_and(|allow| !allow.contains(&"heading"))
        });
        let order = if ordered || hides_headings {
            Order::build_with(doc, files)
        } else {
            Order::default()
        };
        let hidden_heading_ids = if hides_headings {
            order
                .visits
                .iter()
                .filter_map(|v| heading_id(v.node))
                .collect()
        } else {
            HashSet::new()
        };
        let mut r = Renderer {
            opts,
            doc,
            src: None,
            files,
            out: String::with_capacity(out_estimate(doc)),
            order,
            mark_ids: AddrMap::default(),
            note_ids: Vec::new(),
            links: HashMap::new(),
            hidden_heading_ids,
            xref_text: HashMap::new(),
            derived_remaining: crate::tables::DERIVED_TEXT_BYTES.max(
                usize::try_from(doc.src_byte_length.unwrap_or(0))
                    .unwrap_or(0)
                    .saturating_mul(crate::tables::DERIVED_TEXT_FACTOR),
            ),
            derived_exhausted: false,
            derived_failure: None,
            adm_ids: AddrMap::default(),
            placing: None,
            pending: String::new(),
            in_link: 0,
            in_abbr: 0,
            level: 0,
            caption_inner: None,
            long_desc: Vec::new(),
            shown: Vec::new(),
            started: false,
            degrading: 0,
            losses: Vec::new(),
            destination_losses: Vec::new(),
            raw_losses: Vec::new(),
            carrier_losses: Vec::new(),
            hidden: AddrSet::default(),
            profile,
        };
        r.index();
        r
    }

    /// The source of the file `n` stands in, when the renderer holds it.
    fn src_of(&self, n: &Node<'_>) -> Option<SourceView<'r>> {
        self.src
            .filter(|_| !self.files.is_some_and(|f| f.included(n)))
    }

    fn file_of(&self, pos: Option<&crate::ast::Pos>) -> Option<String> {
        self.files
            .and_then(|files| pos.and_then(|pos| files.file_of_pos(pos)))
            .map(str::to_string)
    }

    fn hidden_heading(&self, href: &str) -> bool {
        href.strip_prefix('#')
            .is_some_and(|id| self.hidden_heading_ids.contains(id))
    }

    fn charge_derived(&mut self, bytes: usize, node: &Node<'_>) -> bool {
        if bytes <= self.derived_remaining {
            self.derived_remaining -= bytes;
            return true;
        }
        if !self.derived_exhausted {
            self.losses.push("derived-text-budget");
            self.derived_exhausted = true;
            self.derived_failure = Some(addr(node));
        }
        false
    }

    fn filter(&self) -> Filter<'r> {
        Filter {
            roundtrip: self.opts.roundtrip,
            policy: Some(&self.opts.url_policy),
        }
    }

    fn index(&mut self) {
        let doc = self.doc;
        for c in doc.children.iter() {
            if let Node::LinkReferenceDefinition(d) = c {
                self.links.entry(d.key).or_insert(d);
            }
        }
        // Gap HTML-015: a note whose marks the profile degrades is not written.
        self.shown = self
            .order
            .notes
            .iter()
            .map(|n| {
                self.profile_allows_type(
                    if matches!(n.node, Node::Footnote(_)) {
                        "footnote_ref"
                    } else {
                        "inline_footnote"
                    },
                    false,
                )
            })
            .collect();
        self.placing = if self.shown.contains(&true) {
            doc.children.iter().find(|c| matches!(c, Node::Directive(d) if d.kind == crate::ast::DirectiveKind::Footnotes)).map(addr)
        } else {
            None
        };
        self.targets();
        self.generated_ids();
    }

    /// The text of each cross-reference target (C2-H-096).
    fn targets(&mut self) {
        if !self
            .order
            .visits
            .iter()
            .any(|v| matches!(v.node, Node::Xref(_)))
        {
            return;
        }
        for v in &self.order.visits {
            if let Node::FigureGroup(group) = v.node {
                for (id, text) in panel_targets(group, self.opts) {
                    self.xref_text.entry(id).or_insert(text);
                }
            }
            let (id, text) = match v.node {
                Node::Heading(h) => match heading_id(v.node) {
                    Some(id) => (id.to_string(), plain::text_of(h.children)),
                    None => continue,
                },
                n => {
                    let Some((Node::CaptionNumber(c), before)) = caption_of(n).and_then(mark_of)
                    else {
                        continue;
                    };
                    let (Some(num), Some(id)) = (c.n, n.attrs().and_then(|a| a.id)) else {
                        continue;
                    };
                    let label = label_key(before, Some(&self.opts.symbols));
                    let text = if label.is_empty() {
                        num.to_string()
                    } else {
                        format!("{label} {num}")
                    };
                    (id.to_string(), text)
                }
            };
            self.xref_text.entry(id).or_insert_with(|| {
                let mut escaped = String::new();
                esc::text(&text, &mut escaped);
                escaped
            });
        }
    }

    /// C2-H-156: `adm-N` for titled admonitions and the titled placing
    /// directive, after every authored id and generated heading id.
    fn generated_ids(&mut self) {
        if self.order.notes.is_empty()
            && !self.order.visits.iter().any(|visit| {
                matches!(visit.node, Node::Admonition(a) if a.title.is_some())
                    || matches!(visit.node, Node::Directive(d) if d.title.is_some())
            })
        {
            return;
        }
        let mut taken = slug::Taken::default();
        reserve_ids(self.doc.children, &mut taken, 0);
        let collisions = !taken.is_empty();
        self.admonition_ids(&mut taken, collisions);
        self.note_ids(&mut taken, collisions);
    }

    fn admonition_ids(&mut self, taken: &mut slug::Taken, collisions: bool) {
        let placing = self.placing;
        let titled = |v: &crate::resolve::order::Visit<'_, '_>| match v.node {
            Node::Admonition(a) => a.title.is_some() && !authored_name(a.attrs),
            Node::Directive(d) => d.title.is_some() && placing == Some(addr(v.node)),
            _ => false,
        };
        if !self.order.visits.iter().any(titled) {
            return;
        }
        let mut n = 0usize;
        for v in &self.order.visits {
            if titled(v) {
                n += 1;
                let id = allocated_id(taken, format!("adm-{n}"), collisions);
                self.adm_ids.insert(addr(v.node), id);
            }
        }
    }

    fn note_ids(&mut self, taken: &mut slug::Taken, collisions: bool) {
        self.note_ids = vec![String::new(); self.order.notes.len()];
        let mut counts = vec![0usize; self.order.notes.len()];
        for visit in &self.order.visits {
            let Some(num) = self.order.number.get(&addr(visit.node)).copied() else {
                continue;
            };
            let Some(count) = counts.get_mut(num.saturating_sub(1)) else {
                continue;
            };
            if *count == 0 {
                if let Some(slot) = self.note_ids.get_mut(num - 1) {
                    *slot = allocated_id(taken, format!("fn{num}"), collisions);
                }
            }
            *count += 1;
            let base = if *count == 1 {
                format!("fnref{num}")
            } else {
                format!("fnref{num}-{count}")
            };
            let id = visit
                .node
                .attrs()
                .and_then(|a| a.id)
                .map_or_else(|| allocated_id(taken, base, collisions), str::to_string);
            self.mark_ids.insert(addr(visit.node), id);
        }
    }

    /// Appends one line at `depth` (C2-H-005).
    fn line(&mut self, depth: usize, s: &str) {
        if self.started {
            self.out.push('\n');
        }
        self.started = true;
        for _ in 0..depth {
            self.out.push_str("  ");
        }
        self.out.push_str(s);
    }

    /// One line at `depth` that `f` writes into the output; left out when
    /// `f` returns false.
    fn line_with(&mut self, depth: usize, f: impl FnOnce(&mut Self, &mut String) -> bool) {
        let mut out = std::mem::take(&mut self.out);
        let (mark, was) = (out.len(), self.started);
        if self.started {
            out.push('\n');
        }
        self.started = true;
        for _ in 0..depth {
            out.push_str("  ");
        }
        if !f(self, &mut out) {
            out.truncate(mark);
            self.started = was;
        }
        self.out = out;
    }

    /// The empty line of an empty container body (C2-H-008).
    fn blank(&mut self) {
        self.out.push('\n');
    }

    fn document(&mut self) {
        let doc = self.doc;
        for node in doc.children.iter() {
            reserved_losses(
                node,
                &mut self.losses,
                &mut self.destination_losses,
                &mut self.carrier_losses,
                &self.opts.url_policy,
                self.files,
                1,
            );
        }
        if self.opts.sections {
            self.sectioned(doc.children);
        } else {
            let mut i = 0;
            while let Some(c) = doc.children.get(i) {
                if self.opts.roundtrip && matches!(c, Node::AbbreviationDef(_)) {
                    i += self.abbreviation_run(doc.children.get(i..).unwrap_or(&[]), 0);
                } else {
                    self.block(c, 0);
                    i += 1;
                }
            }
        }
        if self.placing.is_none() && self.shown.contains(&true) {
            self.footnote_section(0, None);
        }
    }

    /// C2-H-026 to C2-H-028: each top-level heading opens a section.
    fn sectioned(&mut self, children: &'r [Node<'a>]) {
        let mut stack: Vec<i64> = Vec::new();
        let mut i = 0;
        while let Some(c) = children.get(i) {
            if self.opts.roundtrip && matches!(c, Node::AbbreviationDef(_)) {
                i += self.abbreviation_run(children.get(i..).unwrap_or(&[]), stack.len());
                continue;
            }
            if let Node::Heading(h) = c {
                if self.profile_allows(c, true) {
                    while stack.last().is_some_and(|l| *l >= h.level) {
                        stack.pop();
                        self.line(stack.len(), "</section>");
                    }
                    let mut el = El {
                        id: heading_id(c).map(std::borrow::Cow::Borrowed),
                        ..El::default()
                    };
                    self.id_carriers(c, &mut el);
                    let f = self.filter();
                    self.line_with(stack.len(), |_, out| {
                        out.push_str("<section");
                        el.write_into(f, out);
                        out.push('>');
                        true
                    });
                    stack.push(h.level);
                    self.level += 1;
                    self.heading(c, stack.len(), false);
                    self.level -= 1;
                    i += 1;
                    continue;
                }
            }
            self.block(c, stack.len());
            i += 1;
        }
        while stack.pop().is_some() {
            self.line(stack.len(), "</section>");
        }
    }

    /// `data-carve-explicit-id` or `data-carve-generated-id` (C2-H-114).
    fn id_carriers(&self, heading: &Node, el: &mut El<'_>) {
        let Node::Heading(h) = heading else { return };
        if !self.opts.roundtrip {
            return;
        }
        if h.attrs.is_some_and(|a| a.id.is_some()) {
            el.carriers.push(("data-carve-explicit-id", None));
        } else if h.generated_id.is_some() {
            el.carriers.push(("data-carve-generated-id", None));
        }
    }
}

fn panel_targets(group: &crate::ast::FigureGroup<'_>, opts: &Options) -> Vec<(String, String)> {
    let Some((Node::CaptionNumber(number), before)) = group.caption.as_deref().and_then(mark_of)
    else {
        return Vec::new();
    };
    let Some(number) = number.n else {
        return Vec::new();
    };
    let label = label_key(before, Some(&opts.symbols));
    group
        .children
        .iter()
        .filter(|node| {
            matches!(
                node,
                Node::Paragraph(_) | Node::CodeBlock(_) | Node::BlockQuote(_) | Node::Table(_)
            )
        })
        .enumerate()
        .filter_map(|(index, panel)| {
            let id = panel.attrs().and_then(|attrs| attrs.id)?;
            let letter = panel_letter(index);
            let text = if label.is_empty() {
                format!("{number}{letter}")
            } else {
                format!("{label} {number}{letter}")
            };
            Some((id.to_string(), esc::text_str(&text)))
        })
        .collect()
}

fn panel_letter(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop {
        let letter = u8::try_from(index % 26).unwrap_or(0);
        letters.push(char::from(b'a' + letter));
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters.into_iter().rev().collect()
}

/// C2-H-116: each authored reserved carrier is a render loss.
fn reserved_losses(
    node: &Node<'_>,
    losses: &mut Vec<&'static str>,
    destinations: &mut Vec<DestinationLoss>,
    carriers: &mut Vec<CarrierLoss>,
    policy: &crate::options::UrlPolicy,
    files: Option<&crate::resolve::files::Files<'_>>,
    depth: usize,
) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    if let Some(attrs) = node
        .attrs()
        .filter(|_| !matches!(node, Node::AbbreviationDef(_)))
    {
        for (key, value) in attrs.key_values {
            if attrs::RESERVED
                .iter()
                .any(|name| name.eq_ignore_ascii_case(key))
            {
                losses.push("carrier-attribute-dropped");
                carriers.push(CarrierLoss {
                    key: key.to_string(),
                    pos: node.pos().copied(),
                    regions: carrier_attribute_regions(node),
                    file: files
                        .and_then(|files| node.pos().and_then(|pos| files.file_of_pos(pos)))
                        .map(str::to_string),
                });
            }
            if !url::filter_value_in(key, value, None).is_empty()
                && url::filter_value_in(key, value, Some(policy)).is_empty()
            {
                losses.push("destination-denied");
                destinations.push(DestinationLoss {
                    image: false,
                    attribute: true,
                    pos: node.pos().copied(),
                    value: value.to_string(),
                    file: files
                        .and_then(|files| node.pos().and_then(|pos| files.file_of_pos(pos)))
                        .map(str::to_string),
                });
            }
        }
    }
    node.for_each_child(&mut |child| {
        reserved_losses(
            child,
            losses,
            destinations,
            carriers,
            policy,
            files,
            depth + 1,
        );
    });
}

fn carrier_attribute_regions(node: &Node<'_>) -> Vec<(u32, u32)> {
    let Some(span) = node.pos() else {
        return Vec::new();
    };
    let mut children = Vec::new();
    node.for_each_child(&mut |child| {
        if let Some(pos) = child.pos() {
            children.push(*pos);
        }
    });
    children.sort_by_key(|pos| pos.start_offset);
    let mut regions = Vec::new();
    let mut cursor = span.start_offset;
    for child in children {
        if cursor < child.start_offset {
            regions.push((cursor, child.start_offset));
        }
        cursor = cursor.max(child.end_offset);
    }
    if cursor < span.end_offset {
        regions.push((cursor, span.end_offset));
    }
    regions
}

/// The id a heading carries: authored, else generated.
fn heading_id<'a>(n: &Node<'a>) -> Option<&'a str> {
    let Node::Heading(h) = n else { return None };
    h.attrs.as_ref().and_then(|a| a.id).or(h.generated_id)
}

/// An authored `aria-label` or `aria-labelledby` (C2-H-155).
fn authored_name(a: Option<&crate::ast::Attrs>) -> bool {
    a.is_some_and(|a| {
        a.key_values.iter().any(|(k, _)| {
            k.eq_ignore_ascii_case("aria-label") || k.eq_ignore_ascii_case("aria-labelledby")
        })
    })
}

fn reserve_ids(nodes: &[Node], taken: &mut slug::Taken, depth: usize) {
    if depth > crate::resolve::WALK_CAP {
        return;
    }
    for n in nodes {
        if let Some(id) = n.attrs().and_then(|a| a.id) {
            if id.starts_with("fn") || id.starts_with("adm-") {
                taken.reserve(id);
            }
        }
        if let Node::Heading(h) = n {
            if let Some(id) = &h.generated_id {
                if id.starts_with("fn") || id.starts_with("adm-") {
                    taken.reserve(id);
                }
            }
        }
        for c in crate::resolve::order::slots(n) {
            reserve_ids(std::slice::from_ref(c), taken, depth + 1);
        }
    }
}

fn allocated_id(taken: &mut slug::Taken, base: String, collisions: bool) -> String {
    if collisions {
        taken.unique(&base)
    } else {
        base
    }
}
