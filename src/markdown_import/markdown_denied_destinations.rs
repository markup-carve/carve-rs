//! Markdown import does not write a destination the PART 9 §25 sink blanks
//! (markup-carve/carve#2254, markup-carve/carve#6010).

use std::collections::BTreeSet;

use crate::ast::*;
use crate::escape::has_denied_url_scheme;
use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};

pub(crate) const DENIED_LINK_DESTINATION: &str =
    "Dropped a link destination with a denied URL scheme";
pub(crate) const DENIED_IMAGE_SOURCE: &str = "Dropped an image source with a denied URL scheme";
pub(crate) const DENIED_REFERENCE_DESTINATION: &str =
    "Dropped a reference definition with a denied URL scheme";

pub(crate) fn is_denied(url: &str) -> bool {
    !url.is_empty() && has_denied_url_scheme(url)
}

pub(crate) fn reference_key(label: &str) -> String {
    crate::parse::label_key(label).to_uppercase()
}

/// Replace every link, image and autolink whose destination the renderer's
/// denylist blanks with its content, and remove every reference definition with
/// such a destination. Returns one message per dropped inline destination; a
/// use of a reference in `denied_references` is covered by the row its
/// definition owes, which the caller reports.
pub(crate) fn drop_denied_destinations(
    document: &mut Document,
    denied_references: &BTreeSet<String>,
) -> Vec<String> {
    let mut pass = Pass {
        denied_references,
        rows: Vec::new(),
        depth: 0,
    };
    pass.blocks(&mut document.children);
    for blocks in document.footnote_defs.values_mut() {
        pass.blocks(blocks);
    }
    pass.rows
}

struct Pass<'a> {
    denied_references: &'a BTreeSet<String>,
    rows: Vec<String>,
    /// Zero for a block's own inline run, which a reader trims at its edges.
    depth: usize,
}

impl Pass<'_> {
    fn covered(&self, label: Option<&String>) -> bool {
        label.is_some_and(|label| self.denied_references.contains(&reference_key(label)))
    }

    fn report(&mut self, covered: bool, message: &str) {
        if !covered {
            self.rows.push(message.to_owned());
        }
    }

    /// The alt text an image leaves, or `None` when the image was not denied.
    fn denied_image(&mut self, image: &Image) -> Option<Vec<InlineNode>> {
        if !is_denied(&image.src) {
            return None;
        }
        let covered = self.covered(image.ref_label.as_ref());
        self.report(covered, DENIED_IMAGE_SOURCE);
        Some(stand_in(alt_text(image), image.attrs.clone()))
    }
}

/// Content only, in a span where attributes survive: the HTML importer's shape.
fn stand_in(children: Vec<InlineNode>, attrs: Option<Attrs>) -> Vec<InlineNode> {
    match attrs {
        Some(attrs) if attrs != Attrs::default() => vec![InlineNode::Span(Span {
            attrs: Some(attrs),
            children,
            injected: false,
            pos: None,
        })],
        _ => children,
    }
}

fn alt_text(image: &Image) -> Vec<InlineNode> {
    if image.alt.is_empty() {
        Vec::new()
    } else {
        vec![InlineNode::Text(Text {
            value: image.alt.clone(),
            pos: None,
        })]
    }
}

fn paragraph(children: Vec<InlineNode>, attrs: Option<Attrs>) -> Paragraph {
    Paragraph {
        attrs,
        children,
        at_content_column: true,
        block_image: false,
        pos: None,
    }
}

/// The whitespace a dropped edge construct leaves at a block's start or end.
fn trim_edges(nodes: &mut Vec<InlineNode>) {
    if let Some(InlineNode::Text(first)) = nodes.first_mut() {
        first.value = first.value.trim_start_matches([' ', '\t']).to_owned();
        if first.value.is_empty() {
            nodes.remove(0);
        }
    }
    if let Some(InlineNode::Text(last)) = nodes.last_mut() {
        last.value = last.value.trim_end_matches([' ', '\t']).to_owned();
        if last.value.is_empty() {
            nodes.pop();
        }
    }
}

/// Inline formatting with no spelling once it is empty.
fn wrapper_children(node: &InlineNode) -> Option<(&Vec<InlineNode>, &Option<Attrs>)> {
    match node {
        InlineNode::Emphasis(n) => Some((&n.children, &n.attrs)),
        InlineNode::Span(n) => Some((&n.children, &n.attrs)),
        InlineNode::CriticInsert(n) => Some((&n.children, &n.attrs)),
        InlineNode::CriticDelete(n) => Some((&n.children, &n.attrs)),
        _ => None,
    }
}

impl SubtreeVisitor for Pass<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        let depth = std::mem::replace(&mut self.depth, 0);
        let mut out = Vec::with_capacity(blocks.len());
        for mut block in std::mem::take(blocks) {
            match &mut block {
                BlockNode::LinkReferenceDefinition(definition) if is_denied(&definition.href) => {
                    continue;
                }
                BlockNode::BlockImage(image) => {
                    if let Some(children) = self.denied_image(image) {
                        if !children.is_empty() || image.attrs.is_some() {
                            out.push(BlockNode::Paragraph(paragraph(
                                alt_text(image),
                                image.attrs.clone(),
                            )));
                        }
                        continue;
                    }
                }
                BlockNode::Figure(figure) => {
                    if let FigureTarget::Image(image) = figure.target.as_ref() {
                        if let Some(children) = self.denied_image(image) {
                            *figure.target = FigureTarget::Paragraph(paragraph(children, None));
                        }
                    }
                }
                _ => {}
            }
            let emptied = matches!(&block, BlockNode::Paragraph(p) if !p.children.is_empty());
            let rows = self.rows.len();
            visit_block_children(&mut block, self);
            if let BlockNode::Paragraph(p) = &mut block {
                if p.children.is_empty() && p.attrs.is_none() && emptied {
                    continue;
                }
                if self.rows.len() != rows {
                    p.block_image = false;
                }
            }
            out.push(block);
        }
        *blocks = out;
        self.depth = depth;
    }

    fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
        let mut out = Vec::with_capacity(inlines.len());
        let mut dropped = false;
        for mut node in std::mem::take(inlines) {
            match &mut node {
                InlineNode::Link(link) if is_denied(&link.href) => {
                    dropped = true;
                    let covered = self.covered(link.ref_label.as_ref());
                    self.report(covered, DENIED_LINK_DESTINATION);
                    let mut children = std::mem::take(&mut link.children);
                    self.depth += 1;
                    self.inlines(&mut children);
                    self.depth -= 1;
                    out.extend(stand_in(children, link.attrs.take()));
                }
                InlineNode::Image(image) if is_denied(&image.src) => {
                    dropped = true;
                    let children = self.denied_image(image).unwrap_or_default();
                    out.extend(children);
                }
                InlineNode::AutoLink(autolink) if is_denied(&autolink.href) => {
                    dropped = true;
                    self.report(false, DENIED_LINK_DESTINATION);
                    let text = if autolink.text.is_empty() {
                        autolink.href.clone()
                    } else {
                        autolink.text.clone()
                    };
                    out.extend(stand_in(
                        vec![InlineNode::Text(Text {
                            value: text,
                            pos: None,
                        })],
                        autolink.attrs.take(),
                    ));
                }
                _ => {
                    let held = wrapper_children(&node).is_some_and(|(c, _)| !c.is_empty());
                    self.depth += 1;
                    visit_inline_children(&mut node, self);
                    self.depth -= 1;
                    // A wrapper whose only content was a dropped empty link goes with it.
                    if held
                        && wrapper_children(&node)
                            .is_some_and(|(c, attrs)| c.is_empty() && attrs.is_none())
                    {
                        dropped = true;
                    } else {
                        out.push(node);
                    }
                }
            }
        }
        if dropped && self.depth == 0 {
            trim_edges(&mut out);
        }
        *inlines = out;
    }
}
