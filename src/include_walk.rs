//! Mutable tree walking for the include pass.
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

use crate::ast::{BlockNode, Div, FigureTarget, InlineNode, Pos};

/// What to do with each sequence the walk reaches.
pub(crate) trait SubtreeVisitor {
    fn blocks(&mut self, _blocks: &mut Vec<BlockNode>) {}
    fn inlines(&mut self, _inlines: &mut Vec<InlineNode>) {}
    /// Called for every node's position, block and inline alike.
    fn position(&mut self, _pos: &mut Pos) {}
}

/// A position on a part that is not a node of its own: a list item, a table row
/// or cell, a definition term or body, a citation item. It reaches the wire all
/// the same.
fn visit_pos<V: SubtreeVisitor>(pos: &mut Option<Pos>, v: &mut V) {
    if let Some(pos) = pos.as_mut() {
        v.position(pos);
    }
}

/// Visit the sequences held directly by one block, without revisiting the block
/// itself. The caller decides whether to recurse further.
pub(crate) fn visit_block_children<V: SubtreeVisitor>(block: &mut BlockNode, v: &mut V) {
    if let Some(pos) = block_pos_mut(block) {
        v.position(pos);
    }
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
            if let Some(caption) = &mut t.caption {
                v.inlines(caption);
            }
            for row in &mut t.rows {
                visit_pos(&mut row.pos, v);
                for cell in &mut row.cells {
                    visit_pos(&mut cell.pos, v);
                    v.inlines(&mut cell.children);
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
            v.inlines(&mut f.caption);
            match &mut *f.target {
                FigureTarget::BlockQuote(b) => v.blocks(&mut b.children),
                FigureTarget::Paragraph(p) => v.inlines(&mut p.children),
                FigureTarget::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            v.inlines(&mut cell.children);
                        }
                    }
                }
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
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
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

/// Visit the sequences held by one inline node.
pub(crate) fn visit_inline_children<V: SubtreeVisitor>(node: &mut InlineNode, v: &mut V) {
    if let Some(pos) = inline_pos_mut(node) {
        v.position(pos);
    }
    match node {
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
        | InlineNode::Image(_)
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
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticComment(_)
        | InlineNode::Comment(_) => {}
    }
}

pub(crate) fn block_pos_mut(block: &mut BlockNode) -> Option<&mut Pos> {
    match block {
        BlockNode::Heading(n) => n.pos.as_mut(),
        BlockNode::Paragraph(n) => n.pos.as_mut(),
        BlockNode::CodeBlock(n) => n.pos.as_mut(),
        BlockNode::List(n) => n.pos.as_mut(),
        BlockNode::BlockQuote(n) => n.pos.as_mut(),
        BlockNode::Table(n) => n.pos.as_mut(),
        BlockNode::Admonition(n) => n.pos.as_mut(),
        BlockNode::Directive(n) => n.pos.as_mut(),
        BlockNode::Div(n) => n.pos.as_mut(),
        BlockNode::LineBlock(n) => n.pos.as_mut(),
        BlockNode::DefinitionList(n) => n.pos.as_mut(),
        BlockNode::Figure(n) => n.pos.as_mut(),
        BlockNode::FigureGroup(n) => n.pos.as_mut(),
        BlockNode::AbbreviationDef(n) => n.pos.as_mut(),
        BlockNode::LinkReferenceDefinition(n) => n.pos.as_mut(),
        BlockNode::CitationDefinition(n) => n.pos.as_mut(),
        BlockNode::RawBlock(n) => n.pos.as_mut(),
        BlockNode::Comment(n) => n.pos.as_mut(),
        BlockNode::BlockExtension(n) => n.pos.as_mut(),
        BlockNode::ExtensionCarrier(n) => n.pos.as_mut(),
        BlockNode::BlockImage(n) => n.pos.as_mut(),
        BlockNode::ThematicBreak(n) => n.pos.as_mut(),
    }
}

pub(crate) fn inline_pos_mut(node: &mut InlineNode) -> Option<&mut Pos> {
    match node {
        InlineNode::Text(n) => n.pos.as_mut(),
        InlineNode::EscapedText(n) => n.pos.as_mut(),
        InlineNode::SmartPunctuation(n) => n.pos.as_mut(),
        InlineNode::Emphasis(n) => n.pos.as_mut(),
        InlineNode::Code(n) => n.pos.as_mut(),
        InlineNode::Link(n) => n.pos.as_mut(),
        InlineNode::Image(n) => n.pos.as_mut(),
        InlineNode::Span(n) => n.pos.as_mut(),
        InlineNode::Ruby(n) => n.pos.as_mut(),
        InlineNode::Math(n) => n.pos.as_mut(),
        InlineNode::RawInline(n) => n.pos.as_mut(),
        InlineNode::LiteralInline(n) => n.pos.as_mut(),
        InlineNode::Symbol(n) => n.pos.as_mut(),
        InlineNode::AutoLink(n) => n.pos.as_mut(),
        InlineNode::CrossRef(n) => n.pos.as_mut(),
        InlineNode::CaptionNumber(n) => n.pos.as_mut(),
        InlineNode::Mention(n) => n.pos.as_mut(),
        InlineNode::Tag(n) => n.pos.as_mut(),
        InlineNode::CitationGroup(n) => n.pos.as_mut(),
        InlineNode::Extension(n) => n.pos.as_mut(),
        InlineNode::Abbreviation(n) => n.pos.as_mut(),
        InlineNode::Footnote(n) => n.pos.as_mut(),
        InlineNode::SoftBreak(n) => n.pos.as_mut(),
        InlineNode::HardBreak(n) => n.pos.as_mut(),
        InlineNode::CriticInsert(n) => n.pos.as_mut(),
        InlineNode::CriticDelete(n) => n.pos.as_mut(),
        InlineNode::CriticSubstitute(n) => n.pos.as_mut(),
        InlineNode::CriticComment(n) => n.pos.as_mut(),
        InlineNode::Comment(n) => n.pos.as_mut(),
    }
}
