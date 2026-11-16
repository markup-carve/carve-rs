//! The HTML renderer (C2-H): a resolved tree to an HTML fragment.

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
use attrs::{El, Filter};
use std::collections::HashMap;

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
    let mut r = Renderer::new(doc, opts);
    r.document();
    (r.out, r.losses)
}

/// Renders with losses while retaining source text for degraded inline nodes.
#[must_use]
pub fn render_with_losses_source(
    doc: &Document<'_>,
    opts: &Options,
    src: &str,
) -> (String, Vec<&'static str>) {
    render_source(doc, opts, src, true)
}

/// `render_with_losses_source`; `ordered` false when resolve found no note,
/// cross-reference or titled block, so the renderer needs no order of its own.
pub(crate) fn render_source(
    doc: &Document<'_>,
    opts: &Options,
    src: &str,
    ordered: bool,
) -> (String, Vec<&'static str>) {
    let mut r = Renderer::with_order(doc, opts, ordered);
    r.src = Some(src);
    r.document();
    (r.out, r.losses)
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
    src: Option<&'r str>,
    out: String,
    order: Order<'r, 'a>,
    /// The anchor id of each note mark (C2-H-088).
    mark_ids: AddrMap<usize, String>,
    links: HashMap<&'a str, &'r LinkReferenceDefinition<'a>>,
    xref_text: HashMap<String, String>,
    adm_ids: AddrMap<usize, String>,
    /// The top-level `footnotes` directive that places the section (C2-H-093).
    placing: Option<usize>,
    /// Note marks met inside link text, written after the outermost link.
    pending: String,
    in_link: usize,
    in_abbr: usize,
    level: usize,
    /// The `longDescription` blocks met in the block being written (C2-H-080).
    long_desc: Vec<&'r [Node<'a>]>,
    /// The notes the section writes.
    shown: Vec<bool>,
    /// A line was written (an empty first line still counts, C2-H-134).
    started: bool,
    /// Inside a node that stays while its children are degraded.
    degrading: usize,
    losses: Vec<&'static str>,
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
        let order = if ordered {
            Order::build(doc)
        } else {
            Order::default()
        };
        let mut r = Renderer {
            opts,
            doc,
            src: None,
            out: String::with_capacity(out_estimate(doc)),
            order,
            mark_ids: AddrMap::default(),
            links: HashMap::new(),
            xref_text: HashMap::new(),
            adm_ids: AddrMap::default(),
            placing: None,
            pending: String::new(),
            in_link: 0,
            in_abbr: 0,
            level: 0,
            long_desc: Vec::new(),
            shown: Vec::new(),
            started: false,
            degrading: 0,
            losses: Vec::new(),
            hidden: AddrSet::default(),
            profile: crate::tables::PROFILES
                .iter()
                .find(|p| p.name == opts.profile.name()),
        };
        r.index();
        r
    }

    fn filter(&self) -> Filter {
        Filter {
            roundtrip: self.opts.roundtrip,
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
        for (i, note) in self.order.notes.iter().enumerate() {
            for (k, r) in note.refs.iter().enumerate() {
                let authored = r.attrs().and_then(|a| a.id).map(str::to_string);
                let id = authored.unwrap_or_else(|| {
                    if k == 0 {
                        format!("fnref{}", i + 1)
                    } else {
                        format!("fnref{}-{}", i + 1, k + 1)
                    }
                });
                self.mark_ids.insert(addr(r), id);
            }
        }
        self.targets();
        self.admonition_ids();
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
            self.xref_text.entry(id).or_insert(text);
        }
    }

    /// C2-H-156: `adm-N` for titled admonitions and the titled placing
    /// directive, after every authored id and generated heading id.
    fn admonition_ids(&mut self) {
        let placing = self.placing;
        let titled = |v: &crate::resolve::order::Visit<'_, '_>| match v.node {
            Node::Admonition(a) => a.title.is_some() && !authored_name(a.attrs),
            Node::Directive(d) => d.title.is_some() && placing == Some(addr(v.node)),
            _ => false,
        };
        if !self.order.visits.iter().any(titled) {
            return;
        }
        let mut taken = slug::Taken::default();
        reserve_ids(self.doc.children, &mut taken, 0);
        let mut n = 0usize;
        for v in &self.order.visits {
            if titled(v) {
                n += 1;
                let id = taken.unique(&format!("adm-{n}"));
                self.adm_ids.insert(addr(v.node), id);
            }
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
        if self.opts.sections {
            self.sectioned(doc.children);
        } else {
            for c in doc.children.iter() {
                self.block(c, 0);
            }
        }
        if self.placing.is_none() && self.shown.contains(&true) {
            self.footnote_section(0, None);
        }
        // Gap HTML-008: C2-H-114 keeps `data-carve-abbreviations` but places no
        // template, and the oracle writes none.
    }

    /// C2-H-026 to C2-H-028: each top-level heading opens a section.
    fn sectioned(&mut self, children: &'r [Node<'a>]) {
        let mut stack: Vec<i64> = Vec::new();
        for c in children {
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
                    continue;
                }
            }
            self.block(c, stack.len());
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
            taken.reserve(id);
        }
        if let Node::Heading(h) = n {
            if let Some(id) = &h.generated_id {
                taken.reserve(id);
            }
        }
        for c in crate::resolve::order::slots(n) {
            reserve_ids(std::slice::from_ref(c), taken, depth + 1);
        }
    }
}
