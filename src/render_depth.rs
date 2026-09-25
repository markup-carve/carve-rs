//! The typed refusal a renderer owes its caller at the depth ceiling (PART 9
//! §25).
//!
//! §25 gives every renderer a bound above the parser's and says what happens AT
//! it: the render MUST fail with a typed, documented error naming the bound -
//! the same rule PART 12 §9(b) already applies to ingest, at the other end of
//! the same pipe. Returning empty output instead is the failure the clause was
//! written against: the caller gets a string that looks complete and has had its
//! body deleted (carve-rs#511 item 5). carve-js raises `RenderDepthError` and
//! carve-php `RenderDepthExceededException`; a `Result` is the same statement in
//! this language.
//!
//! It costs nothing on any path a document travels. The ceiling exceeds
//! `parse::MAX_NESTING_DEPTH` by construction, so a tree that came from the
//! parser cannot reach it - which is why the source-level `to_*` entry points
//! keep returning `String`. What is left is a tree built through the API or read
//! by `from_json`, where the caller is the one who can act on the error.
//!
//! The bound is recorded through a thread-local rather than by threading a
//! `Result` through every recursive renderer function, following
//! [`crate::abbr_budget`]: the guard is installed for one render and unwinds on
//! drop, so a nested render (a block extension rendering sub-blocks) stacks
//! correctly instead of reporting its parent's state.
//!
//! [`refuse_if_too_deep`] is why any of that is reachable. The recording guards
//! bound the renderer's OWN descent, but every entry point touches the tree
//! structurally before that descent starts, and none of those touches has a
//! ceiling to consult. See that function for the list and for the measurement
//! that replaces it.

use std::cell::Cell;
use std::fmt;

use crate::ast::{BlockNode, Document, FigureTarget, InlineNode};
use crate::render::MAX_RENDER_DEPTH;

/// A render refused because the tree is deeper than the renderer's ceiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderDepthError {
    renderer: &'static str,
    limit: usize,
}

impl RenderDepthError {
    pub(crate) fn new(renderer: &'static str, limit: usize) -> Self {
        Self { renderer, limit }
    }

    /// Which target refused: `"html"`, `"markdown"`, `"plain"`, `"ansi"` or
    /// `"carve"`.
    pub fn renderer(&self) -> &'static str {
        self.renderer
    }

    /// The bound that was reached, in AST levels.
    pub fn limit(&self) -> usize {
        self.limit
    }
}

impl fmt::Display for RenderDepthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "the {} renderer refused: the tree is deeper than its ceiling of {} levels",
            self.renderer, self.limit
        )
    }
}

impl std::error::Error for RenderDepthError {}

thread_local! {
    /// The renderer that reached the ceiling during the render currently running
    /// on this thread, if any. `None` outside a render, and while one is running
    /// until a guard site records.
    static REACHED: Cell<Option<&'static str>> = const { Cell::new(None) };
}

/// Record that this renderer reached the ceiling.
///
/// Called at each depth guard, beside the early return that keeps the recursion
/// bounded. The guard still returns - the bound is what stops the stack from
/// growing - and the top-level entry point turns the record into the error.
pub(crate) fn record(renderer: &'static str) {
    REACHED.with(|cell| {
        if cell.get().is_none() {
            cell.set(Some(renderer));
        }
    });
}

/// RAII watch installed for one render, restoring the previous value on drop so
/// nested renders stack and unwind (the reason spelled out in the module note).
pub(crate) struct RenderDepthWatch {
    previous: Option<&'static str>,
}

impl RenderDepthWatch {
    pub(crate) fn new() -> Self {
        let previous = REACHED.with(|cell| cell.replace(None));
        RenderDepthWatch { previous }
    }

    /// The render's output, or the refusal if any guard recorded one.
    pub(crate) fn into_result(self, output: String) -> Result<String, RenderDepthError> {
        match REACHED.with(|cell| cell.get()) {
            Some(renderer) => Err(RenderDepthError::new(
                renderer,
                crate::render::MAX_RENDER_DEPTH,
            )),
            None => Ok(output),
        }
    }
}

impl Drop for RenderDepthWatch {
    fn drop(&mut self) {
        REACHED.with(|cell| cell.set(self.previous));
    }
}

/// The refusal a renderer owes a tree it must not walk at all, or `Ok(())`.
///
/// Called FIRST at every entry point that accepts a tree from outside the
/// parser, before anything reads the tree's shape.
///
/// The recording guards above cannot carry this on their own, because they sit
/// inside the renderer's own recursion and several things recurse over the tree
/// before it starts: the defensive `doc.clone()` in the borrowed-AST entry
/// points, the second one in `render_carve::redundant_heading_ids`, and prepasses
/// such as `parse::crossref_index_for_document`. Derived `Clone` has no ceiling
/// to consult at all, so a tree deeper than the C stack aborted the process -
/// SIGABRT, no exit code a caller can branch on, no diagnostic, and in a server a
/// dropped request - and the typed error above was never reached
/// (carve-rs#1877).
///
/// Guarding those prepasses one at a time only moves the cliff: the markdown
/// importer aborted at 2,000 levels through the writer's clone and at 100,000 it
/// reached `collect_caption_titles` instead. Refusing here, before any of them
/// run, is what removes it.
pub(crate) fn refuse_if_too_deep(
    doc: &Document,
    renderer: &'static str,
) -> Result<(), RenderDepthError> {
    if exceeds_ceiling(doc) {
        return Err(RenderDepthError::new(renderer, MAX_RENDER_DEPTH));
    }
    Ok(())
}

/// Whether the tree nests deeper than [`MAX_RENDER_DEPTH`], measured on an
/// EXPLICIT STACK.
///
/// Iterative because the whole point is to answer before anything recursive
/// touches the tree; a recursive depth check would abort on the input it exists
/// to refuse.
///
/// Block and inline depth are counted SEPARATELY against the same bound, which
/// is exactly what the guards inside the renderers do (`block_depth` and
/// `inline_depth` are two counters there). Summing them would refuse trees that
/// render today.
///
/// A root sequence is depth 0, which is the index the renderers' own counters
/// carry - `render_blocks` tests the depth it was ENTERED at, before
/// incrementing. Seeding at 1 instead refused one tree that renders on main: 432
/// nested quotes, the deepest document the ceiling admits.
///
/// Stops at the first node past the ceiling, so a deep document costs the depth
/// rather than the tree. A document under the ceiling pays one pointer walk.
fn exceeds_ceiling(doc: &Document) -> bool {
    let mut blocks: Vec<(&BlockNode, usize)> = Vec::new();
    let mut inlines: Vec<(&InlineNode, usize)> = Vec::new();
    push_blocks(&mut blocks, &doc.children, 0);
    for body in doc.footnote_defs.values() {
        push_blocks(&mut blocks, body, 0);
    }
    while let Some((block, depth)) = blocks.pop() {
        if depth > MAX_RENDER_DEPTH {
            return true;
        }
        push_block_children(block, depth, &mut blocks, &mut inlines);
        while let Some((inline, depth)) = inlines.pop() {
            if depth > MAX_RENDER_DEPTH {
                return true;
            }
            push_inline_children(inline, depth, &mut inlines);
        }
    }
    false
}

fn push_blocks<'a>(out: &mut Vec<(&'a BlockNode, usize)>, children: &'a [BlockNode], depth: usize) {
    out.extend(children.iter().map(|child| (child, depth)));
}

fn push_inlines<'a>(
    out: &mut Vec<(&'a InlineNode, usize)>,
    children: &'a [InlineNode],
    depth: usize,
) {
    out.extend(children.iter().map(|child| (child, depth)));
}

/// The sequences one block holds directly, mirroring
/// [`crate::include_walk::visit_block_children`].
///
/// EXHAUSTIVE for the reason that one is: a `_ => {}` arm would leave a
/// container added later out of the measurement, and the abort this refusal
/// exists to prevent would come back through it with nothing to say why.
fn push_block_children<'a>(
    block: &'a BlockNode,
    depth: usize,
    blocks: &mut Vec<(&'a BlockNode, usize)>,
    inlines: &mut Vec<(&'a InlineNode, usize)>,
) {
    let deeper = depth + 1;
    match block {
        BlockNode::Heading(h) => push_inlines(inlines, &h.children, 0),
        BlockNode::Paragraph(p) => push_inlines(inlines, &p.children, 0),
        BlockNode::LineBlock(b) => push_blocks(blocks, &b.children, deeper),
        BlockNode::BlockQuote(b) => push_blocks(blocks, &b.children, deeper),
        BlockNode::Admonition(a) => {
            if let Some(title) = &a.title {
                push_inlines(inlines, title, 0);
            }
            push_blocks(blocks, &a.children, deeper);
        }
        BlockNode::Directive(d) => push_blocks(blocks, &d.children, deeper),
        BlockNode::Div(d) => push_blocks(blocks, &d.children, deeper),
        BlockNode::List(l) => {
            for item in &l.items {
                push_blocks(blocks, &item.children, deeper);
            }
        }
        BlockNode::DefinitionList(d) => {
            for item in &d.items {
                for term in &item.terms {
                    push_inlines(inlines, &term.children, 0);
                }
                for def in &item.definitions {
                    push_blocks(blocks, &def.children, deeper);
                }
            }
        }
        BlockNode::Table(t) => {
            if let Some(caption) = &t.caption {
                push_inlines(inlines, caption, 0);
            }
            for row in &t.rows {
                for cell in &row.cells {
                    push_inlines(inlines, &cell.children, 0);
                }
            }
        }
        BlockNode::FigureGroup(g) => {
            push_blocks(blocks, &g.children, deeper);
            if let Some(caption) = &g.caption {
                push_inlines(inlines, caption, 0);
            }
        }
        BlockNode::Figure(f) => {
            push_inlines(inlines, &f.caption, 0);
            match &*f.target {
                FigureTarget::BlockQuote(b) => push_blocks(blocks, &b.children, deeper),
                FigureTarget::Paragraph(p) => push_inlines(inlines, &p.children, 0),
                FigureTarget::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            push_inlines(inlines, &cell.children, 0);
                        }
                    }
                }
                FigureTarget::Image(_) | FigureTarget::CodeBlock(_) => {}
            }
        }
        // The fallback is ONE node, and it is a block: a level of its own, the
        // same as any other child.
        BlockNode::BlockExtension(e) => blocks.push((&e.fallback, deeper)),
        BlockNode::ExtensionCarrier(e) => push_blocks(blocks, &e.children, deeper),
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

/// The sequences one inline node holds, mirroring
/// [`crate::include_walk::visit_inline_children`], and exhaustive for the same
/// reason as above.
fn push_inline_children<'a>(
    node: &'a InlineNode,
    depth: usize,
    inlines: &mut Vec<(&'a InlineNode, usize)>,
) {
    let deeper = depth + 1;
    match node {
        InlineNode::Emphasis(e) => push_inlines(inlines, &e.children, deeper),
        InlineNode::Link(l) => push_inlines(inlines, &l.children, deeper),
        InlineNode::Span(s) => push_inlines(inlines, &s.children, deeper),
        InlineNode::Ruby(r) => {
            for pair in &r.pairs {
                push_inlines(inlines, &pair.base, deeper);
                push_inlines(inlines, &pair.annotation, deeper);
            }
        }
        InlineNode::CriticInsert(c) => push_inlines(inlines, &c.children, deeper),
        InlineNode::CriticDelete(c) => push_inlines(inlines, &c.children, deeper),
        InlineNode::CriticSubstitute(c) => {
            push_inlines(inlines, &c.old, deeper);
            push_inlines(inlines, &c.new, deeper);
        }
        InlineNode::Extension(e) => push_inlines(inlines, &e.children, deeper),
        InlineNode::Footnote(f) => {
            if let Some(inline) = &f.inline {
                push_inlines(inlines, inline, deeper);
            }
        }
        InlineNode::CitationGroup(g) => {
            for item in &g.items {
                if let Some(prefix) = &item.prefix {
                    push_inlines(inlines, prefix, deeper);
                }
                if let Some(locator) = &item.locator {
                    push_inlines(inlines, locator, deeper);
                }
                if let Some(suffix) = &item.suffix {
                    push_inlines(inlines, suffix, deeper);
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
