//! carve#5136: a node joined across a `%%` line S1 erased omits `pos`
//! (CARVE-P12-022); a container the line only trails ends at its last placed
//! child before it (CARVE-P12-014). Only nodes outside the comment's ancestry
//! are touched, so it runs over the comment's siblings, never its owners.

use super::*;
use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};
use std::collections::BTreeSet;

pub(in crate::parse) fn separate_erased_lines(
    children: &mut Vec<BlockNode>,
    notes: &mut BTreeMap<String, Vec<BlockNode>>,
    erased: &BTreeSet<usize>,
    source: &str,
    line_starts: &CodepointLineStarts,
) {
    let mut starts = Collect {
        erased,
        starts: BTreeSet::new(),
    };
    starts.blocks(children);
    for blocks in notes.values_mut() {
        starts.blocks(blocks);
    }
    if starts.starts.is_empty() {
        return;
    }
    let chars: Vec<char> = source.chars().collect();
    let mut lines = Vec::new();
    while let Some(start) = line_starts.get(lines.len()) {
        lines.push(*start);
    }
    let ctx = Ctx {
        chars: &chars,
        lines,
        bom: line_starts.has_leading_bom(),
        comments: &starts.starts,
    };
    // A note is written at the root, so a root comment inside its span is not its
    // own. The definition's span stays: the writer places the note by it.
    let root: BTreeSet<usize> = line_comments(children, &starts.starts)
        .into_iter()
        .collect();
    if !root.is_empty() {
        let mut fix = Fix {
            ctx: &ctx,
            comments: root,
        };
        for blocks in notes.values_mut() {
            fix.blocks(blocks);
        }
    }
    let mut walk = Walk { ctx: &ctx };
    walk.blocks(children);
    for blocks in notes.values_mut() {
        walk.blocks(blocks);
    }
}

/// A line comment standing on a line S1 erased; the parser also leaves
/// placeholder comments that are not authored lines.
fn line_comment(block: &BlockNode, erased: &BTreeSet<usize>) -> Option<usize> {
    match block {
        BlockNode::Comment(c) if !c.block && !c.delimited => c
            .pos
            .as_ref()
            .filter(|p| erased.contains(&p.start_line))
            .map(|p| p.start_offset),
        _ => None,
    }
}

fn line_comments(blocks: &[BlockNode], starts: &BTreeSet<usize>) -> Vec<usize> {
    blocks
        .iter()
        .filter_map(|block| match block {
            BlockNode::Comment(c) if !c.block && !c.delimited => c
                .pos
                .as_ref()
                .map(|p| p.start_offset)
                .filter(|start| starts.contains(start)),
            _ => None,
        })
        .collect()
}

struct Collect<'a> {
    erased: &'a BTreeSet<usize>,
    starts: BTreeSet<usize>,
}

impl SubtreeVisitor for Collect<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks {
            if let Some(start) = line_comment(block, self.erased) {
                self.starts.insert(start);
            }
            visit_block_children(block, self);
        }
    }
}

struct Ctx<'a> {
    chars: &'a [char],
    lines: Vec<usize>,
    bom: bool,
    comments: &'a BTreeSet<usize>,
}

impl Ctx<'_> {
    fn line_of(&self, offset: usize) -> usize {
        self.lines
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
    }

    /// The line's codepoints without its terminator.
    fn line_span(&self, line: usize) -> (usize, usize) {
        let start = self.lines.get(line).copied().unwrap_or(self.chars.len());
        let mut end = self
            .lines
            .get(line + 1)
            .copied()
            .unwrap_or(self.chars.len());
        while end > start && matches!(self.chars[end - 1], '\n' | '\r') {
            end -= 1;
        }
        (start, end)
    }

    /// Blank, quote prefixes only, or an erased comment line.
    fn holds_nothing(&self, line: usize) -> bool {
        let (mut at, end) = self.line_span(line);
        while at < end && matches!(self.chars[at], ' ' | '\t' | '>') {
            at += 1;
        }
        at == end || self.comments.contains(&at)
    }

    fn column(&self, line: usize, offset: usize) -> usize {
        let start = self.lines.get(line).copied().unwrap_or(0);
        offset - start + 1 + usize::from(line == 0 && self.bom)
    }
}

/// Every block list, with its erased comments applied to the sibling spans they sit in.
struct Walk<'a> {
    ctx: &'a Ctx<'a>,
}

impl SubtreeVisitor for Walk<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        // Inner lists first, while the spans they test against are still the parsed ones.
        for block in blocks.iter_mut() {
            visit_block_children(block, self);
        }
        let comments: BTreeSet<usize> = line_comments(blocks, self.ctx.comments)
            .into_iter()
            .collect();
        if !comments.is_empty() {
            for block in blocks.iter_mut() {
                if matches!(block, BlockNode::Comment(_)) {
                    continue;
                }
                // Siblings are disjoint, so only a comment inside this span can reach its subtree.
                let Some(pos) = crate::ast_json::block_pos(block) else {
                    continue;
                };
                if pos.start_offset >= pos.end_offset {
                    continue;
                }
                let inside: BTreeSet<usize> = comments
                    .range(pos.start_offset + 1..pos.end_offset)
                    .copied()
                    .collect();
                if !inside.is_empty() {
                    let mut fix = Fix {
                        ctx: self.ctx,
                        comments: inside,
                    };
                    visit_block_children(block, &mut fix);
                }
            }
        }
    }
}

struct Fix<'a> {
    ctx: &'a Ctx<'a>,
    comments: BTreeSet<usize>,
}

impl Fix<'_> {
    /// The first erased comment strictly inside the span.
    fn crossed(&self, pos: &Pos) -> Option<usize> {
        (pos.start_offset < pos.end_offset)
            .then(|| {
                self.comments
                    .range(pos.start_offset + 1..pos.end_offset)
                    .next()
                    .copied()
            })
            .flatten()
    }

    /// Nothing but blank and erased lines after the comment, up to the span's end.
    fn only_trails(&self, pos: &Pos, comment: usize) -> bool {
        let first = self.ctx.line_of(comment) + 1;
        let last = self.ctx.line_of(pos.end_offset.saturating_sub(1));
        (first..=last).all(|line| self.ctx.holds_nothing(line))
    }

    /// End after the last line before the comment that holds content.
    fn end_before(&self, pos: &mut Pos, comment: usize) {
        let first = self.ctx.line_of(pos.start_offset);
        let mut line = self.ctx.line_of(comment);
        while line > first {
            line -= 1;
            if self.ctx.holds_nothing(line) {
                continue;
            }
            let (start, mut end) = self.ctx.line_span(line);
            while end > start && matches!(self.ctx.chars[end - 1], ' ' | '\t') {
                end -= 1;
            }
            pos.end_line = line + 1;
            pos.end_column = self.ctx.column(line, end);
            pos.end_offset = end;
            return;
        }
    }
}

impl SubtreeVisitor for Fix<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks {
            visit_block_children(block, self);
        }
    }

    fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
        for inline in inlines {
            match inline {
                InlineNode::SoftBreak(b) | InlineNode::HardBreak(b) => {
                    if b.pos
                        .as_ref()
                        .is_some_and(|pos| self.crossed(pos).is_some())
                    {
                        b.pos = None;
                    }
                }
                _ => visit_inline_children(inline, self),
            }
        }
    }

    fn position_slot(&mut self, slot: &mut Option<Pos>) {
        let Some((pos, comment)) = slot
            .as_mut()
            .and_then(|pos| self.crossed(pos).map(|comment| (pos, comment)))
        else {
            return;
        };
        if self.only_trails(pos, comment) {
            self.end_before(pos, comment);
        } else {
            *slot = None;
        }
    }
}
