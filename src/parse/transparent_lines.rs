//! PART 0 S1 (carve#5012): a `%%` line comment and a one-line link or
//! abbreviation definition are erased before layout. The visible lines are
//! parsed as a document of their own; the erased lines come back as AST nodes
//! at their authored positions, and the definitions still register.

use super::*;
use crate::include_walk::{visit_block_children, SubtreeVisitor};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Comment,
    LinkDef,
    Abbreviation,
}

struct ErasedLine {
    index: usize,
    kind: Kind,
    owner: String,
    /// Codepoints before the comment or definition: quote prefixes and indent.
    lead: usize,
    rest: String,
    /// The authored line, quote prefixes included.
    text: String,
    width: usize,
}

pub(in crate::parse) struct Transparent {
    pub(in crate::parse) visible: String,
    /// Visible 0-based line to authored 0-based line.
    lines: Vec<usize>,
    authored_lines: usize,
    erased: Vec<ErasedLine>,
}

/// The line's content behind its quote prefixes and indentation.
fn behind_prefix(line: &str) -> &str {
    let mut rest = line;
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        match trimmed.strip_prefix('>') {
            Some(after) => rest = after.strip_prefix(' ').unwrap_or(after),
            None => return trimmed,
        }
    }
}

/// The run of quote prefixes an erased line keeps, so quote ownership around it stays.
fn quote_scaffold(line: &str) -> Option<&str> {
    let mut end = 0;
    let mut rest = line;
    loop {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        let Some(after) = trimmed.strip_prefix('>') else {
            break;
        };
        rest = after.strip_prefix(' ').unwrap_or(after);
        end = line.len() - rest.len();
    }
    (end > 0).then(|| &line[..end])
}

fn classify(rest: &str) -> Option<Kind> {
    if rest.starts_with("%%") {
        return (!rest.starts_with("%%%") && !own_lines::is_region_marker_text(rest))
            .then_some(Kind::Comment);
    }
    if rest.starts_with("[^") && parse_footnote_def_line(rest).is_some() {
        return None;
    }
    if parse_link_def_line(rest).is_some() {
        return Some(Kind::LinkDef);
    }
    detect_abbreviation_def(rest).map(|_| Kind::Abbreviation)
}

thread_local! {
    /// The last source asked about and whether it erases a line; the event
    /// route and the tree parser ask in turn about the same document.
    static LAST_ANSWER: std::cell::RefCell<Option<(String, bool)>> =
        const { std::cell::RefCell::new(None) };
}

/// Whether any line of `source` is erased before layout.
#[cfg(any(test, feature = "experimental-event-parser"))]
pub(in crate::parse) fn erases_lines(source: &str) -> bool {
    if !source.contains("%%") && !source.contains("]:") {
        return false;
    }
    if let Some(answer) = LAST_ANSWER.with(|cell| match &*cell.borrow() {
        Some((text, answer)) if text == source => Some(*answer),
        _ => None,
    }) {
        return answer;
    }
    Transparent::detect(source).is_some()
}

impl Transparent {
    pub(in crate::parse) fn detect(source: &str) -> Option<Self> {
        if !source.contains("%%") && !source.contains("]:") {
            return None;
        }
        let found = Self::detect_lines(source);
        LAST_ANSWER.with(|cell| *cell.borrow_mut() = Some((source.to_owned(), found.is_some())));
        found
    }

    fn detect_lines(source: &str) -> Option<Self> {
        let normalized = normalize_source(source);
        let authored: Vec<&str> = normalized.split('\n').collect();
        let mut erased = Vec::new();
        for (index, owner, _) in own_lines::traced_leaves(normalized.as_ref()) {
            let Some(line) = authored.get(index) else {
                continue;
            };
            let rest = behind_prefix(line).trim_end_matches([' ', '\t']);
            if let Some(kind) = classify(rest) {
                erased.push(ErasedLine {
                    index,
                    kind,
                    owner,
                    lead: line[..line.len() - behind_prefix(line).len()]
                        .chars()
                        .count(),
                    rest: rest.to_owned(),
                    text: (*line).to_owned(),
                    width: line.chars().count(),
                });
            }
        }
        if erased.is_empty() {
            return None;
        }
        let mut visible = String::with_capacity(normalized.len());
        let mut lines = Vec::with_capacity(authored.len());
        let mut next = erased.iter().map(|line| line.index).peekable();
        for (index, line) in authored.iter().enumerate() {
            let text = if next.peek() == Some(&index) {
                next.next();
                match quote_scaffold(line) {
                    Some(scaffold) => scaffold,
                    None => continue,
                }
            } else {
                line
            };
            if !lines.is_empty() {
                visible.push('\n');
            }
            visible.push_str(text);
            lines.push(index);
        }
        // A document ending in a newline keeps it, whichever line was erased.
        if normalized.ends_with('\n')
            && !visible.ends_with('\n')
            && !lines.is_empty()
            && lines.last() != Some(&(authored.len() - 1))
        {
            visible.push('\n');
            lines.push(authored.len() - 1);
        }
        Some(Transparent {
            visible,
            lines,
            authored_lines: authored.len(),
            erased,
        })
    }

    /// Each erased line as 1-based line, authored owner and whether it is a comment.
    pub(in crate::parse) fn erased_lines(&self) -> impl Iterator<Item = (usize, &str, bool)> {
        self.erased.iter().map(|line| {
            (
                line.index + 1,
                line.owner.as_str(),
                line.kind == Kind::Comment,
            )
        })
    }

    /// Visible 1-based line to authored 1-based line.
    pub(in crate::parse) fn authored_line(&self, line: usize) -> usize {
        match self.lines.get(line.wrapping_sub(1)) {
            Some(index) => index + 1,
            None => self.authored_lines + line.saturating_sub(self.lines.len()),
        }
    }

    /// Register erased link definitions. The last definition of a label wins,
    /// visible or erased. Returns the keys whose `line` is already authored.
    pub(in crate::parse) fn register_link_defs(
        &self,
        defs: &mut BTreeMap<String, LinkDef>,
    ) -> std::collections::HashSet<String> {
        let mut keys = std::collections::HashSet::new();
        for line in self.erased.iter().filter(|line| line.kind == Kind::LinkDef) {
            let Some((label, _, mut def)) = parse_link_def_line_with_value(&line.rest) else {
                continue;
            };
            let key = label_key(label);
            let earlier = |existing: &LinkDef| {
                let at = existing.line.map(|l| {
                    if keys.contains(&key) {
                        l
                    } else {
                        self.authored_line(l + 1) - 1
                    }
                });
                at.map_or(true, |at| at < line.index)
            };
            if defs.get(&key).is_some_and(|existing| !earlier(existing)) {
                continue;
            }
            def.line = Some(line.index);
            def.raw_label = Some(label.to_owned());
            defs.insert(key.clone(), def);
            keys.insert(key);
        }
        keys
    }

    /// Move every position of the visible parse onto the authored lines.
    pub(in crate::parse) fn remap_positions(
        &self,
        children: &mut Vec<BlockNode>,
        notes: &mut BTreeMap<String, Vec<BlockNode>>,
        note_pos: &mut BTreeMap<String, Pos>,
    ) {
        struct Remap<'a>(&'a Transparent);
        impl SubtreeVisitor for Remap<'_> {
            fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
                for block in blocks {
                    visit_block_children(block, self);
                }
            }
            fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
                for inline in inlines {
                    crate::include_walk::visit_inline_children(inline, self);
                }
            }
            fn position(&mut self, pos: &mut Pos) {
                pos.start_line = self.0.authored_line(pos.start_line);
                pos.end_line = self.0.authored_line(pos.end_line);
            }
        }
        let mut remap = Remap(self);
        remap.blocks(children);
        for blocks in notes.values_mut() {
            remap.blocks(blocks);
        }
        for pos in note_pos.values_mut() {
            remap.position(pos);
        }
        own_lines::remap_active_edit_lines(|line| self.authored_line(line));
    }

    /// Put the erased comments and abbreviation definitions back into the tree.
    pub(in crate::parse) fn restore(
        &self,
        children: &mut Vec<BlockNode>,
        notes: &mut BTreeMap<String, Vec<BlockNode>>,
        note_pos: &BTreeMap<String, Pos>,
    ) {
        for line in &self.erased {
            let pos = Pos {
                start_line: line.index + 1,
                end_line: line.index + 1,
                start_column: line.lead + 1,
                end_column: line.width + 1,
                start_offset: 0,
                end_offset: 0,
                file: None,
            };
            match line.kind {
                Kind::LinkDef => {}
                Kind::Abbreviation => {
                    let Some(mut def) = detect_abbreviation_def(&line.rest) else {
                        continue;
                    };
                    def.pos = Some(pos);
                    let at = position_index(children, (line.index + 1, line.lead + 1));
                    children.insert(at, BlockNode::AbbreviationDef(def));
                }
                Kind::Comment => {
                    let content = line.rest[2..]
                        .strip_prefix(' ')
                        .unwrap_or(&line.rest[2..])
                        .trim_end_matches([' ', '\t'])
                        .to_owned();
                    let node = BlockNode::Comment(Comment {
                        block: false,
                        delimited: false,
                        content,
                        pos: Some(pos.clone()),
                    });
                    // The walk names the note that owns the line; its body is a block list of its own.
                    let note_line = line.owner.split('/').find_map(|segment| {
                        segment
                            .strip_prefix("footnote:")
                            .and_then(|n| n.parse::<usize>().ok())
                    });
                    let note = note_line.and_then(|start| {
                        note_pos
                            .iter()
                            .find(|(_, at)| at.start_line == start)
                            .map(|(label, _)| label.clone())
                    });
                    let mut inside = InsideSpan {
                        transparent: self,
                        line,
                        placed: false,
                    };
                    inside.blocks(children);
                    for blocks in notes.values_mut() {
                        inside.blocks(blocks);
                    }
                    if inside.placed {
                        continue;
                    }
                    let blocks = match note.and_then(|label| notes.get_mut(&label)) {
                        Some(blocks) => blocks,
                        None => &mut *children,
                    };
                    place_comment(blocks, node, &pos, &line.owner);
                }
            }
        }
    }
}

/// An erased line the visible parse reads inside a `%%%` span is payload of
/// that span, as the authored line reads it there.
struct InsideSpan<'a> {
    transparent: &'a Transparent,
    line: &'a ErasedLine,
    placed: bool,
}

impl SubtreeVisitor for InsideSpan<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for i in candidates(blocks, (self.line.index + 1, 0)) {
            let block = &mut blocks[i];
            if self.placed {
                return;
            }
            if let BlockNode::Comment(comment) = block {
                let at = self.line.index + 1;
                if let Some(pos) = comment
                    .pos
                    .as_ref()
                    .filter(|pos| comment.block && pos.start_line < at && at <= pos.end_line)
                {
                    // Content lines follow the opener; an opener's tail is the first one.
                    let lead = usize::from(
                        !comment.content.is_empty()
                            && self
                                .transparent
                                .erased
                                .iter()
                                .all(|e| e.index + 1 != pos.start_line + 1)
                            && comment.content.split('\n').count()
                                > (pos.start_line + 1..pos.end_line)
                                    .filter(|n| {
                                        !self.transparent.erased.iter().any(|e| e.index + 1 == *n)
                                    })
                                    .count(),
                    );
                    let before = (pos.start_line + 1..at)
                        .filter(|n| !self.transparent.erased.iter().any(|e| e.index + 1 == *n))
                        .count();
                    let strip = self.line.lead.min(pos.start_column - 1);
                    let text: String = self
                        .line
                        .text
                        .chars()
                        .skip(strip)
                        .collect::<String>()
                        .trim_end_matches([' ', '\t'])
                        .to_owned();
                    let mut lines: Vec<String> = if comment.content.is_empty() {
                        Vec::new()
                    } else {
                        comment.content.split('\n').map(str::to_owned).collect()
                    };
                    let index = (lead + before).min(lines.len());
                    lines.insert(index, text);
                    comment.content = lines.join("\n");
                    self.placed = true;
                    return;
                }
            }
            if !matches!(block, BlockNode::Paragraph(_) | BlockNode::Heading(_)) {
                visit_block_children(block, self);
            }
        }
    }
}

fn at_or_before(pos: &Pos, at: (usize, usize)) -> bool {
    (pos.start_line, pos.start_column) <= at
}

fn contains(pos: &Pos, at: (usize, usize)) -> bool {
    at_or_before(pos, at) && at < (pos.end_line, pos.end_column)
}

/// Where a node starting at `at` goes among `blocks`, by source order.
fn position_index(blocks: &[BlockNode], at: (usize, usize)) -> usize {
    blocks.partition_point(|block| {
        crate::ast_json::block_pos(block).map_or(true, |pos| at_or_before(pos, at))
    })
}

/// The blocks that can hold `at`: the last one starting at or before it, and
/// the comments and definitions restored after it. Blocks stand in source
/// order, so this keeps each lookup logarithmic.
fn candidates(blocks: &[BlockNode], at: (usize, usize)) -> Vec<usize> {
    let mut found = Vec::new();
    for j in (0..position_index(blocks, at)).rev() {
        found.push(j);
        if !matches!(
            blocks[j],
            BlockNode::Comment(_) | BlockNode::AbbreviationDef(_)
        ) {
            break;
        }
    }
    found
}

fn place_comment(blocks: &mut Vec<BlockNode>, node: BlockNode, pos: &Pos, owner: &str) {
    let at = (pos.start_line, pos.start_column);
    let mut node = Some(node);
    let mut after = AfterParagraph {
        at,
        node: &mut node,
    };
    after.blocks(blocks);
    let Some(node) = node else {
        return;
    };
    let Some(node) = place_in_owner(blocks, node, pos, owner) else {
        return;
    };
    let siblings = containing_blocks(blocks, at);
    let index = position_index(siblings, at);
    siblings.insert(index, node);
}

/// A paragraph crossing the erased line keeps it beside itself, after the paragraph.
struct AfterParagraph<'a> {
    at: (usize, usize),
    node: &'a mut Option<BlockNode>,
}

impl SubtreeVisitor for AfterParagraph<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for i in candidates(blocks, self.at) {
            if self.node.is_none() {
                return;
            }
            let Some(pos) = crate::ast_json::block_pos(&blocks[i]) else {
                continue;
            };
            if !contains(pos, self.at) {
                continue;
            }
            if matches!(blocks[i], BlockNode::Paragraph(_)) {
                let mut j = i + 1;
                while j < blocks.len()
                    && matches!(
                        blocks[j],
                        BlockNode::Comment(_) | BlockNode::AbbreviationDef(_)
                    )
                    && crate::ast_json::block_pos(&blocks[j])
                        .is_some_and(|pos| at_or_before(pos, self.at))
                {
                    j += 1;
                }
                if let Some(node) = self.node.take() {
                    blocks.insert(j, node);
                }
                return;
            }
            if !matches!(blocks[i], BlockNode::Heading(_)) {
                visit_block_children(&mut blocks[i], self);
            }
        }
    }
}

/// The owner the ownership walk named for the erased line, when it is a quote or an item.
/// Hands the node back when no such owner holds it.
fn place_in_owner(
    blocks: &mut [BlockNode],
    node: BlockNode,
    pos: &Pos,
    owner: &str,
) -> Option<BlockNode> {
    let Some(last) = owner.rsplit('/').next() else {
        return Some(node);
    };
    let Some((kind, line)) = last.split_once(':') else {
        return Some(node);
    };
    let Ok(line) = line.parse::<usize>() else {
        return Some(node);
    };
    if kind != "quote" && kind != "item" {
        return Some(node);
    }
    let nth = owner.split('/').filter(|segment| *segment == last).count();
    let mut seen = 0;
    let mut node = Some(node);
    owner_walk(
        blocks,
        kind == "quote",
        line,
        nth,
        &mut seen,
        &mut node,
        pos,
    );
    node
}

fn extend_end(target: &mut Option<Pos>, pos: &Pos) {
    if let Some(target) = target {
        if (target.end_line, target.end_column) < (pos.end_line, pos.end_column) {
            target.end_line = pos.end_line;
            target.end_column = pos.end_column;
        }
    }
}

/// Returns whether the node was placed in this subtree.
fn owner_walk(
    blocks: &mut [BlockNode],
    quote: bool,
    line: usize,
    nth: usize,
    seen: &mut usize,
    node: &mut Option<BlockNode>,
    pos: &Pos,
) -> bool {
    let at = (pos.start_line, pos.start_column);
    for i in candidates(blocks, (line, usize::MAX)) {
        let block = &mut blocks[i];
        if node.is_none() {
            return true;
        }
        let placed = match block {
            BlockNode::BlockQuote(q) => {
                if quote && q.pos.as_ref().is_some_and(|p| p.start_line == line) {
                    *seen += 1;
                    if *seen == nth {
                        let index = position_index(&q.children, at);
                        q.children.insert(index, node.take().expect("node"));
                        extend_end(&mut q.pos, pos);
                        return true;
                    }
                }
                let placed = owner_walk(&mut q.children, quote, line, nth, seen, node, pos);
                if placed {
                    extend_end(&mut q.pos, pos);
                }
                placed
            }
            BlockNode::List(list) => {
                let mut placed = false;
                for item in &mut list.items {
                    if !quote && item.pos.as_ref().is_some_and(|p| p.start_line == line) {
                        *seen += 1;
                        if *seen == nth {
                            let index = position_index(&item.children, at);
                            item.children.insert(index, node.take().expect("node"));
                            extend_end(&mut item.pos, pos);
                            placed = true;
                            break;
                        }
                    }
                    if owner_walk(&mut item.children, quote, line, nth, seen, node, pos) {
                        extend_end(&mut item.pos, pos);
                        placed = true;
                        break;
                    }
                }
                if placed {
                    extend_end(&mut list.pos, pos);
                }
                placed
            }
            BlockNode::Div(d) => owner_walk(&mut d.children, quote, line, nth, seen, node, pos),
            BlockNode::Admonition(a) => {
                owner_walk(&mut a.children, quote, line, nth, seen, node, pos)
            }
            BlockNode::Directive(d) => {
                owner_walk(&mut d.children, quote, line, nth, seen, node, pos)
            }
            BlockNode::DefinitionList(list) => list.items.iter_mut().any(|item| {
                item.definitions.iter_mut().any(|definition| {
                    owner_walk(&mut definition.children, quote, line, nth, seen, node, pos)
                })
            }),
            _ => false,
        };
        if placed {
            return true;
        }
    }
    node.is_none()
}

/// The deepest block sequence whose container holds `at`, as (block, child list) steps.
fn containing_path(blocks: &[BlockNode], at: (usize, usize), path: &mut Vec<(usize, usize)>) {
    let holds = |pos: &Option<Pos>| pos.as_ref().is_some_and(|pos| contains(pos, at));
    let Some(index) = candidates(blocks, at).into_iter().find(|&i| {
        let block = &blocks[i];
        crate::ast_json::block_pos(block).is_some_and(|pos| contains(pos, at))
            && !matches!(block, BlockNode::Paragraph(_) | BlockNode::Heading(_))
    }) else {
        return;
    };
    let (child, inner): (usize, &[BlockNode]) = match &blocks[index] {
        BlockNode::List(list) => match list.items.iter().position(|item| holds(&item.pos)) {
            Some(item) => (item, &list.items[item].children),
            None => return,
        },
        BlockNode::DefinitionList(list) => {
            let mut found = None;
            for (n, definition) in list
                .items
                .iter()
                .flat_map(|item| item.definitions.iter())
                .enumerate()
            {
                if holds(&definition.pos) {
                    found = Some((n, definition.children.as_slice()));
                    break;
                }
            }
            match found {
                Some(found) => found,
                None => return,
            }
        }
        BlockNode::BlockQuote(q) => (0, &q.children),
        BlockNode::Div(d) => (0, &d.children),
        BlockNode::Admonition(a) => (0, &a.children),
        BlockNode::Directive(d) => (0, &d.children),
        _ => return,
    };
    path.push((index, child));
    containing_path(inner, at, path);
}

fn containing_blocks(blocks: &mut Vec<BlockNode>, at: (usize, usize)) -> &mut Vec<BlockNode> {
    let mut path = Vec::new();
    containing_path(blocks, at, &mut path);
    let mut current = blocks;
    for (index, child) in path {
        current = match &mut current[index] {
            BlockNode::List(list) => &mut list.items[child].children,
            BlockNode::DefinitionList(list) => {
                &mut list
                    .items
                    .iter_mut()
                    .flat_map(|item| item.definitions.iter_mut())
                    .nth(child)
                    .expect("definition")
                    .children
            }
            BlockNode::BlockQuote(q) => &mut q.children,
            BlockNode::Div(d) => &mut d.children,
            BlockNode::Admonition(a) => &mut a.children,
            BlockNode::Directive(d) => &mut d.children,
            _ => unreachable!("containing_path steps into containers only"),
        };
    }
    current
}
