//! Mutable walking of authored AST children.
//!
//! The pass has to do three things to an arbitrary subtree: expand block
//! sequences, expand inline sequences, and stamp a file identity onto every
//! position. Each needs the same shape knowledge - which container holds a
//! block sequence, which holds inlines, which holds both - so it is written
//! once here as a visitor rather than three times at each call site.
//!
//! EXHAUSTIVE on `BlockNode` and `InlineNode` on purpose. A `_ => {}` arm would
//! silently skip a container added later, and a directive inside it would stop
//! expanding with nothing to say why.

use crate::ast::{BlockNode, Div, FigureTarget, Image, InlineNode, Pos};

/// What to do with each sequence the walk reaches.
pub(crate) trait SubtreeVisitor {
    fn blocks(&mut self, _blocks: &mut Vec<BlockNode>) {}
    fn inlines(&mut self, _inlines: &mut Vec<InlineNode>) {}
    fn image(&mut self, _image: &mut Image) {}
    /// Called for every node's position, block and inline alike.
    fn position(&mut self, _pos: &mut Pos) {}
    /// The position SLOT, for a pass that REMOVES a position rather than
    /// rewriting one. The default hands a present position to `position`, so a
    /// visitor that only rewrites coordinates is unaffected.
    fn position_slot(&mut self, pos: &mut Option<Pos>) {
        if let Some(pos) = pos.as_mut() {
            self.position(pos);
        }
    }
}

/// A position on a part that is not a node of its own: a list item, a table row
/// or cell, a definition term or body, a citation item. It reaches the wire all
/// the same.
fn visit_pos<V: SubtreeVisitor>(pos: &mut Option<Pos>, v: &mut V) {
    v.position_slot(pos);
}

/// Visit the sequences held directly by one block, without revisiting the block
/// itself. The caller decides whether to recurse further.
pub(crate) fn visit_block_children<V: SubtreeVisitor>(block: &mut BlockNode, v: &mut V) {
    v.position_slot(block_pos_slot(block));
    match block {
        BlockNode::Heading(h) => v.inlines(&mut h.children),
        BlockNode::Paragraph(p) => v.inlines(&mut p.children),
        BlockNode::LineBlock(b) => v.blocks(&mut b.children),
        BlockNode::BlockQuote(b) => v.blocks(&mut b.children),
        BlockNode::Admonition(a) => {
            if let Some(title) = &mut a.title {
                v.inlines(title);
            }
            v.blocks(&mut a.children);
        }
        BlockNode::Directive(d) => {
            if let Some(title) = &mut d.title {
                v.inlines(title);
            }
            v.blocks(&mut d.children);
        }
        BlockNode::Div(d) => v.blocks(&mut d.children),
        BlockNode::Section(d) => v.blocks(&mut d.children),
        BlockNode::List(l) => {
            for item in &mut l.items {
                visit_pos(&mut item.pos, v);
                v.blocks(&mut item.children);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &mut d.items {
                for term in &mut item.terms {
                    visit_pos(&mut term.pos, v);
                    v.inlines(&mut term.children);
                }
                for def in &mut item.definitions {
                    visit_pos(&mut def.pos, v);
                    v.blocks(&mut def.children);
                }
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &mut t.short_caption {
                v.inlines(caption);
            }
            if let Some(caption) = &mut t.caption {
                v.inlines(caption);
            }
            for row in &mut t.rows {
                visit_pos(&mut row.pos, v);
                for cell in &mut row.cells {
                    visit_pos(&mut cell.pos, v);
                    v.inlines(&mut cell.children);
                    if let Some(blocks) = &mut cell.blocks {
                        v.blocks(blocks);
                    }
                }
            }
        }
        BlockNode::FigureGroup(g) => {
            v.blocks(&mut g.children);
            if let Some(caption) = &mut g.caption {
                v.inlines(caption);
            }
        }
        BlockNode::Figure(f) => {
            if let Some(caption) = &mut f.short_caption {
                v.inlines(caption);
            }
            v.inlines(&mut f.caption);
            match &mut *f.target {
                FigureTarget::BlockQuote(b) => {
                    visit_pos(&mut b.pos, v);
                    v.blocks(&mut b.children);
                }
                FigureTarget::Paragraph(p) => {
                    visit_pos(&mut p.pos, v);
                    v.inlines(&mut p.children);
                }
                FigureTarget::Table(t) => {
                    if let Some(caption) = &mut t.short_caption {
                        v.inlines(caption);
                    }
                    if let Some(caption) = &mut t.caption {
                        v.inlines(caption);
                    }
                    for row in &mut t.rows {
                        visit_pos(&mut row.pos, v);
                        for cell in &mut row.cells {
                            visit_pos(&mut cell.pos, v);
                            v.inlines(&mut cell.children);
                            if let Some(blocks) = &mut cell.blocks {
                                v.blocks(blocks);
                            }
                        }
                    }
                }
                FigureTarget::Image(i) => {
                    v.image(i);
                    visit_pos(&mut i.pos, v);
                }
                FigureTarget::CodeBlock(c) => visit_pos(&mut c.pos, v),
            }
        }
        BlockNode::BlockImage(image) => v.image(image),
        // The fallback is a single-node field, and the visitor takes a list it
        // may grow (an include expands one node into several). Wrap it, walk it,
        // and fold what comes back into the one node the schema requires.
        BlockNode::BlockExtension(e) => {
            let mut wrapper = vec![(*e.fallback).clone()];
            v.blocks(&mut wrapper);
            *e.fallback = match wrapper.len() {
                1 => wrapper.pop().expect("length checked"),
                _ => BlockNode::Div(Div {
                    attrs: None,
                    label: None,
                    children: wrapper,
                    pos: None,
                }),
            };
        }
        BlockNode::ExtensionCarrier(e) => v.blocks(&mut e.children),
        // A CODE BLOCK AND A RAW BLOCK HOLD NO NODES, which is also why a
        // directive inside one stays literal (I9): verbatim content is not
        // parsed, so there is nothing here to expand.
        BlockNode::CodeBlock(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::LinkReferenceDefinition(_)
        | BlockNode::CitationDefinition(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

/// Visit the sequences held by one inline node.
pub(crate) fn visit_inline_children<V: SubtreeVisitor>(node: &mut InlineNode, v: &mut V) {
    v.position_slot(inline_pos_slot(node));
    match node {
        InlineNode::Image(image) => v.image(image),
        InlineNode::Emphasis(e) => v.inlines(&mut e.children),
        InlineNode::Link(l) => v.inlines(&mut l.children),
        InlineNode::Span(s) => v.inlines(&mut s.children),
        InlineNode::Ruby(r) => {
            for pair in &mut r.pairs {
                v.inlines(&mut pair.base);
                v.inlines(&mut pair.annotation);
            }
        }
        InlineNode::CriticInsert(c) => v.inlines(&mut c.children),
        InlineNode::CriticDelete(c) => v.inlines(&mut c.children),
        InlineNode::CriticSubstitute(c) => {
            v.inlines(&mut c.old);
            v.inlines(&mut c.new);
        }
        InlineNode::Extension(e) => v.inlines(&mut e.children),
        InlineNode::Footnote(f) => {
            if let Some(inline) = &mut f.inline {
                v.inlines(inline);
            }
        }
        InlineNode::CitationGroup(g) => {
            for item in &mut g.items {
                visit_pos(&mut item.pos, v);
                if let Some(prefix) = &mut item.prefix {
                    v.inlines(prefix);
                }
                if let Some(locator) = &mut item.locator {
                    v.inlines(locator);
                }
                if let Some(suffix) = &mut item.suffix {
                    v.inlines(suffix);
                }
            }
        }
        InlineNode::Text(_)
        | InlineNode::EscapedText(_)
        | InlineNode::SmartPunctuation(_)
        | InlineNode::Code(_)
        | InlineNode::Math(_)
        | InlineNode::RawInline(_)
        | InlineNode::LiteralInline(_)
        | InlineNode::Symbol(_)
        | InlineNode::AutoLink(_)
        | InlineNode::CrossRef(_)
        | InlineNode::CaptionNumber(_)
        | InlineNode::Mention(_)
        | InlineNode::Tag(_)
        | InlineNode::Abbreviation(_)
        | InlineNode::NonBreakingSpace(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticComment(_)
        | InlineNode::Comment(_) => {}
    }
}

// Keep position matches exhaustive so new variants cannot silently lose spans.
pub(crate) fn block_pos_slot(block: &mut BlockNode) -> &mut Option<Pos> {
    match block {
        BlockNode::Heading(n) => &mut n.pos,
        BlockNode::Paragraph(n) => &mut n.pos,
        BlockNode::CodeBlock(n) => &mut n.pos,
        BlockNode::List(n) => &mut n.pos,
        BlockNode::BlockQuote(n) => &mut n.pos,
        BlockNode::Table(n) => &mut n.pos,
        BlockNode::Admonition(n) => &mut n.pos,
        BlockNode::Directive(n) => &mut n.pos,
        BlockNode::Div(n) => &mut n.pos,
        BlockNode::Section(n) => &mut n.pos,
        BlockNode::LineBlock(n) => &mut n.pos,
        BlockNode::DefinitionList(n) => &mut n.pos,
        BlockNode::Figure(n) => &mut n.pos,
        BlockNode::FigureGroup(n) => &mut n.pos,
        BlockNode::AbbreviationDef(n) => &mut n.pos,
        BlockNode::LinkReferenceDefinition(n) => &mut n.pos,
        BlockNode::CitationDefinition(n) => &mut n.pos,
        BlockNode::RawBlock(n) => &mut n.pos,
        BlockNode::Comment(n) => &mut n.pos,
        BlockNode::BlockExtension(n) => &mut n.pos,
        BlockNode::ExtensionCarrier(n) => &mut n.pos,
        BlockNode::BlockImage(n) => &mut n.pos,
        BlockNode::ThematicBreak(n) => &mut n.pos,
    }
}

pub(crate) fn block_pos_mut(block: &mut BlockNode) -> Option<&mut Pos> {
    block_pos_slot(block).as_mut()
}

pub(crate) fn inline_pos_slot(node: &mut InlineNode) -> &mut Option<Pos> {
    match node {
        InlineNode::Text(n) => &mut n.pos,
        InlineNode::EscapedText(n) => &mut n.pos,
        InlineNode::SmartPunctuation(n) => &mut n.pos,
        InlineNode::Emphasis(n) => &mut n.pos,
        InlineNode::Code(n) => &mut n.pos,
        InlineNode::Link(n) => &mut n.pos,
        InlineNode::Image(n) => &mut n.pos,
        InlineNode::Span(n) => &mut n.pos,
        InlineNode::Ruby(n) => &mut n.pos,
        InlineNode::Math(n) => &mut n.pos,
        InlineNode::RawInline(n) => &mut n.pos,
        InlineNode::LiteralInline(n) => &mut n.pos,
        InlineNode::Symbol(n) => &mut n.pos,
        InlineNode::AutoLink(n) => &mut n.pos,
        InlineNode::CrossRef(n) => &mut n.pos,
        InlineNode::CaptionNumber(n) => &mut n.pos,
        InlineNode::Mention(n) => &mut n.pos,
        InlineNode::Tag(n) => &mut n.pos,
        InlineNode::CitationGroup(n) => &mut n.pos,
        InlineNode::Extension(n) => &mut n.pos,
        InlineNode::Abbreviation(n) => &mut n.pos,
        InlineNode::Footnote(n) => &mut n.pos,
        InlineNode::NonBreakingSpace(n) => &mut n.pos,
        InlineNode::SoftBreak(n) => &mut n.pos,
        InlineNode::HardBreak(n) => &mut n.pos,
        InlineNode::CriticInsert(n) => &mut n.pos,
        InlineNode::CriticDelete(n) => &mut n.pos,
        InlineNode::CriticSubstitute(n) => &mut n.pos,
        InlineNode::CriticComment(n) => &mut n.pos,
        InlineNode::Comment(n) => &mut n.pos,
    }
}

pub(crate) fn inline_pos_mut(node: &mut InlineNode) -> Option<&mut Pos> {
    inline_pos_slot(node).as_mut()
}
