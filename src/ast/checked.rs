use super::*;

/// Maximum nesting budget accepted by the checked recursive AST operations.
///
/// Block and inline nesting share one budget. Intermediate recursive storage,
/// such as list items, table rows/cells and figure targets, also consumes it.
/// This conservative ceiling is separate from the renderer's depth limit.
pub const MAX_CHECKED_AST_DEPTH: usize = 16;

/// An AST exceeds the nesting budget of a checked clone or comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AstDepthError {
    limit: usize,
}

impl AstDepthError {
    /// The nesting budget that the operation refused to exceed.
    pub fn limit(&self) -> usize {
        self.limit
    }
}

impl std::fmt::Display for AstDepthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AST exceeds the checked operation depth limit of {}",
            self.limit
        )
    }
}
impl std::error::Error for AstDepthError {}

impl Document {
    /// Clone after an iterative depth check, preserving all document metadata.
    ///
    /// Trees exceeding [`MAX_CHECKED_AST_DEPTH`] return an error before the
    /// recursive `Clone` implementation runs. The ceiling is conservative and
    /// is tested on a 256 KiB thread stack. As with
    /// other Rust operations, callers must provide adequate stack space.
    pub fn try_clone(&self) -> Result<Self, AstDepthError> {
        check(self)?;
        Ok(self.clone())
    }

    /// Compare every field after checking both documents' nesting budgets.
    ///
    /// Both trees are checked even when root metadata already differs. A deep
    /// operand therefore returns an error regardless of comparison order.
    pub fn try_eq(&self, other: &Self) -> Result<bool, AstDepthError> {
        check(self)?;
        check(other)?;
        Ok(self == other)
    }
}

enum Work<'a> {
    Block(&'a BlockNode, usize),
    Inline(&'a InlineNode, usize),
}

fn blocks<'a>(pending: &mut Vec<Work<'a>>, nodes: &'a [BlockNode], depth: usize) {
    pending.extend(nodes.iter().map(|node| Work::Block(node, depth)));
}
fn inlines<'a>(pending: &mut Vec<Work<'a>>, nodes: &'a [InlineNode], depth: usize) {
    pending.extend(nodes.iter().map(|node| Work::Inline(node, depth)));
}
fn optional_inlines<'a>(
    pending: &mut Vec<Work<'a>>,
    nodes: &'a Option<Vec<InlineNode>>,
    depth: usize,
) {
    if let Some(nodes) = nodes {
        inlines(pending, nodes, depth);
    }
}

fn check(doc: &Document) -> Result<(), AstDepthError> {
    let mut pending = Vec::new();
    blocks(&mut pending, &doc.children, 1);
    for body in doc.footnote_defs.values() {
        blocks(&mut pending, body, 1);
    }
    while let Some(work) = pending.pop() {
        let depth = match &work {
            Work::Block(_, depth) | Work::Inline(_, depth) => *depth,
        };
        if depth > MAX_CHECKED_AST_DEPTH {
            return Err(AstDepthError {
                limit: MAX_CHECKED_AST_DEPTH,
            });
        }
        match work {
            Work::Block(node, _) => block_children(&mut pending, node, depth + 1),
            Work::Inline(node, _) => inline_children(&mut pending, node, depth + 1),
        }
    }
    Ok(())
}

fn table_children<'a>(pending: &mut Vec<Work<'a>>, table: &'a Table, depth: usize) {
    optional_inlines(pending, &table.caption, depth);
    optional_inlines(pending, &table.short_caption, depth);
    for row in &table.rows {
        for cell in &row.cells {
            inlines(pending, &cell.children, depth + 2);
            if let Some(children) = &cell.blocks {
                blocks(pending, children, depth + 2);
            }
        }
    }
}

fn block_children<'a>(pending: &mut Vec<Work<'a>>, node: &'a BlockNode, depth: usize) {
    match node {
        BlockNode::Heading(n) => inlines(pending, &n.children, depth),
        BlockNode::Paragraph(n) => inlines(pending, &n.children, depth),
        BlockNode::List(n) => {
            for item in &n.items {
                blocks(pending, &item.children, depth + 1);
            }
        }
        BlockNode::BlockQuote(n) => blocks(pending, &n.children, depth),
        BlockNode::Table(n) => table_children(pending, n, depth),
        BlockNode::Admonition(n) => {
            optional_inlines(pending, &n.title, depth);
            blocks(pending, &n.children, depth);
        }
        BlockNode::Directive(n) => {
            optional_inlines(pending, &n.title, depth);
            blocks(pending, &n.children, depth);
        }
        BlockNode::Div(n) => blocks(pending, &n.children, depth),
        BlockNode::Section(n) => blocks(pending, &n.children, depth),
        BlockNode::LineBlock(n) => blocks(pending, &n.children, depth),
        BlockNode::DefinitionList(n) => {
            for item in &n.items {
                for term in &item.terms {
                    inlines(pending, &term.children, depth + 2);
                }
                for definition in &item.definitions {
                    blocks(pending, &definition.children, depth + 2);
                }
            }
        }
        BlockNode::Figure(n) => {
            inlines(pending, &n.caption, depth);
            optional_inlines(pending, &n.short_caption, depth);
            match n.target.as_ref() {
                FigureTarget::BlockQuote(target) => blocks(pending, &target.children, depth + 1),
                FigureTarget::Table(target) => table_children(pending, target, depth + 1),
                FigureTarget::Paragraph(target) => inlines(pending, &target.children, depth + 1),
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        BlockNode::FigureGroup(n) => {
            blocks(pending, &n.children, depth);
            optional_inlines(pending, &n.caption, depth);
        }
        BlockNode::CitationDefinition(n) => inlines(pending, &n.children, depth),
        BlockNode::BlockExtension(n) => pending.push(Work::Block(&n.fallback, depth)),
        BlockNode::ExtensionCarrier(n) => {
            blocks(pending, &n.children, depth);
            optional_inlines(pending, &n.summary, depth);
        }
        BlockNode::CodeBlock(_)
        | BlockNode::AbbreviationDef(_)
        | BlockNode::LinkReferenceDefinition(_)
        | BlockNode::RawBlock(_)
        | BlockNode::Comment(_)
        | BlockNode::BlockImage(_)
        | BlockNode::ThematicBreak(_) => {}
    }
}

fn inline_children<'a>(pending: &mut Vec<Work<'a>>, node: &'a InlineNode, depth: usize) {
    match node {
        InlineNode::Emphasis(n) => inlines(pending, &n.children, depth),
        InlineNode::Link(n) => inlines(pending, &n.children, depth),
        InlineNode::Span(n) => inlines(pending, &n.children, depth),
        InlineNode::Extension(n) => inlines(pending, &n.children, depth),
        InlineNode::Ruby(n) => {
            for pair in &n.pairs {
                inlines(pending, &pair.base, depth + 1);
                inlines(pending, &pair.annotation, depth + 1);
            }
        }
        InlineNode::CitationGroup(n) => {
            for item in &n.items {
                optional_inlines(pending, &item.prefix, depth + 1);
                optional_inlines(pending, &item.locator, depth + 1);
                optional_inlines(pending, &item.suffix, depth + 1);
            }
        }
        InlineNode::Footnote(n) => optional_inlines(pending, &n.inline, depth),
        InlineNode::CriticInsert(n) => inlines(pending, &n.children, depth),
        InlineNode::CriticDelete(n) => inlines(pending, &n.children, depth),
        InlineNode::CriticSubstitute(n) => {
            inlines(pending, &n.old, depth);
            inlines(pending, &n.new, depth);
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
        | InlineNode::NonBreakingSpace(_)
        | InlineNode::SoftBreak(_)
        | InlineNode::HardBreak(_)
        | InlineNode::CriticComment(_)
        | InlineNode::Comment(_) => {}
    }
}
