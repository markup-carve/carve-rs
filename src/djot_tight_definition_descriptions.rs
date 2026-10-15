use crate::ast::{BlockNode, InlineNode, Paragraph, Pos};
use crate::include_walk::{visit_block_children, SubtreeVisitor};

struct Edit {
    start: usize,
    end: usize,
    value: String,
}
struct Layout<'a> {
    source: &'a str,
    lines: Vec<&'a str>,
    offsets: Vec<usize>,
    indexes: Vec<usize>,
    edits: Vec<Edit>,
}
impl Layout<'_> {
    fn offset(&self, offset: usize) -> usize {
        self.indexes.get(offset).copied().unwrap_or(offset)
    }
    fn clear_loose(&mut self, pos: Option<&Pos>) {
        let Some(pos) = pos.filter(|pos| pos.start_line >= 2) else {
            return;
        };
        let line = pos.start_line - 2;
        if let Some(marker) = cached_regex!(r"\{loose\}[ \t]*$")
            .unwrap()
            .find(self.lines[line])
        {
            self.edits.push(Edit {
                start: self.offsets[line] + marker.start(),
                end: self.offsets[line] + marker.end(),
                value: "%%".into(),
            });
        }
    }
    fn clear_gap(&mut self, before: Option<&Pos>, after: Option<&Pos>, boundary: bool) {
        let (Some(before), Some(after)) = (before, after) else {
            return;
        };
        let prefix = &self.lines[after.start_line - 1]
            [..self.offset(after.start_offset) - self.offsets[after.start_line - 1]];
        let mut inserted = false;
        for line in before.end_line - 1..after.start_line - 1 {
            if self.lines[line]
                .bytes()
                .all(|byte| matches!(byte, b' ' | b'\t' | b'>'))
            {
                self.edits.push(Edit {
                    start: self.offsets[line],
                    end: self.offsets[line]
                        + self.lines[line].len()
                        + usize::from(line + 1 < self.lines.len()),
                    value: if boundary && !inserted {
                        format!("{prefix}\n{prefix}%%\n")
                    } else {
                        String::new()
                    },
                });
                inserted = true;
            }
        }
    }
}
struct Pending<'a> {
    nodes: &'a mut Vec<(BlockNode, bool)>,
    tight: bool,
}
impl SubtreeVisitor for Pending<'_> {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        self.nodes
            .extend(blocks.drain(..).map(|block| (block, self.tight)));
    }
}
fn position(node: &BlockNode) -> Option<&Pos> {
    match node {
        BlockNode::Heading(n) => n.pos.as_ref(),
        BlockNode::Paragraph(n) => n.pos.as_ref(),
        BlockNode::CodeBlock(n) => n.pos.as_ref(),
        BlockNode::List(n) => n.pos.as_ref(),
        BlockNode::BlockQuote(n) => n.pos.as_ref(),
        BlockNode::Table(n) => n.pos.as_ref(),
        BlockNode::Admonition(n) => n.pos.as_ref(),
        BlockNode::Directive(n) => n.pos.as_ref(),
        BlockNode::Div(n) => n.pos.as_ref(),
        BlockNode::Section(n) => n.pos.as_ref(),
        BlockNode::LineBlock(n) => n.pos.as_ref(),
        BlockNode::DefinitionList(n) => n.pos.as_ref(),
        BlockNode::Figure(n) => n.pos.as_ref(),
        BlockNode::FigureGroup(n) => n.pos.as_ref(),
        BlockNode::AbbreviationDef(n) => n.pos.as_ref(),
        BlockNode::LinkReferenceDefinition(n) => n.pos.as_ref(),
        BlockNode::CitationDefinition(n) => n.pos.as_ref(),
        BlockNode::RawBlock(n) => n.pos.as_ref(),
        BlockNode::Comment(n) => n.pos.as_ref(),
        BlockNode::BlockExtension(n) => n.pos.as_ref(),
        BlockNode::ExtensionCarrier(n) => n.pos.as_ref(),
        BlockNode::BlockImage(n) => n.pos.as_ref(),
        BlockNode::ThematicBreak(n) => n.pos.as_ref(),
    }
}
pub(super) fn normalize(source: &str) -> String {
    if !source.contains("::")
        || !cached_regex!(r"(?m)^[ \t>]*(?:[-*+]|[0-9A-Za-z]+[.)]|\([0-9A-Za-z]+\))[ \t]")
            .unwrap()
            .is_match(source)
    {
        return source.into();
    }
    let lines: Vec<_> = source.split('\n').collect();
    let mut offset = 0;
    let offsets = lines
        .iter()
        .map(|line| {
            let start = offset;
            offset += line.len() + 1;
            start
        })
        .collect();
    let indexes = if source.is_ascii() {
        Vec::new()
    } else {
        source
            .char_indices()
            .map(|(at, _)| at)
            .chain(std::iter::once(source.len()))
            .collect()
    };
    let mut layout = Layout {
        source,
        lines,
        offsets,
        indexes,
        edits: Vec::new(),
    };
    let mut doc =
        crate::parse_with_options(source, &crate::Options::default().with_positions(true));
    let mut pending: Vec<_> = doc.children.drain(..).map(|node| (node, false)).collect();
    while let Some((mut node, inherited)) = pending.pop() {
        let mut tight = if let BlockNode::List(list) = &node {
            list.tight
        } else {
            inherited
        };
        if let BlockNode::List(list) = &node {
            let has_definition = list.items.iter().any(|item| {
                item.children
                    .iter()
                    .any(|child| matches!(child, BlockNode::DefinitionList(_)))
            });
            let safe_children = list.items.iter().all(|item| {
                item.children.iter().all(|child| {
                    matches!(
                        child,
                        BlockNode::DefinitionList(_)
                            | BlockNode::Paragraph(_)
                            | BlockNode::Comment(_)
                    )
                })
            });
            let mut previous: Option<&BlockNode> = None;
            let mut gap = false;
            for child in list.items.iter().flat_map(|item| &item.children) {
                if let Some(BlockNode::Paragraph(before)) = previous {
                    if let (Some(before), Some(after)) = (&before.pos, position(child)) {
                        let mut blank = false;
                        for line in before.end_line - 1..after.start_line - 1 {
                            let value = layout.lines[line];
                            if value
                                .bytes()
                                .all(|byte| matches!(byte, b' ' | b'\t' | b'>'))
                            {
                                blank = true;
                            } else if blank && matches!(child, BlockNode::DefinitionList(_)) {
                                let value = value.trim_matches([' ', '\t', '>']);
                                gap |= value.starts_with('{') && value != "{loose}";
                            }
                        }
                        gap |= blank
                            && matches!(child, BlockNode::Paragraph(_) | BlockNode::Comment(_));
                    }
                }
                previous = Some(child);
            }
            if has_definition && gap {
                tight = false;
                if let Some(pos) = list.pos.as_ref().filter(|_| list.tight) {
                    let at = layout.offset(pos.start_offset);
                    let prefix = &layout.lines[pos.start_line - 1]
                        [..at - layout.offsets[pos.start_line - 1]];
                    layout.edits.push(Edit {
                        start: at,
                        end: at,
                        value: format!("{{loose}}\n{prefix}"),
                    });
                }
            }
            if !list.tight && safe_children && has_definition && !gap {
                tight = true;
                layout.clear_loose(list.pos.as_ref());
                for item in &list.items {
                    for pair in item.children.windows(2) {
                        layout.clear_gap(position(&pair[0]), position(&pair[1]), true);
                    }
                }
                for pair in list.items.windows(2) {
                    layout.clear_gap(pair[0].pos.as_ref(), pair[1].pos.as_ref(), false);
                }
            }
        }
        if let BlockNode::DefinitionList(list) = &node {
            if tight {
                if list.loose {
                    layout.clear_loose(list.pos.as_ref());
                }
                for item in &list.items {
                    for description in &item.definitions {
                        if description.len() < 2
                            || !description
                                .iter()
                                .all(|block| matches!(block, BlockNode::Paragraph(_)))
                        {
                            continue;
                        }
                        let (Some(first), Some(last)) = (
                            position(&description[0]),
                            position(description.last().unwrap()),
                        ) else {
                            continue;
                        };
                        let prefix = &layout.lines[first.start_line - 1][..layout
                            .offset(first.start_offset)
                            - layout.offsets[first.start_line - 1]];
                        let continuation = cached_regex!(r":[ \t]+$")
                            .unwrap()
                            .replace(prefix, |caps: &regex::Captures<'_>| {
                                " ".repeat(caps[0].len())
                            });
                        let mut joined = Paragraph::default();
                        for (index, paragraph) in description.iter().enumerate() {
                            if index > 0
                                && !matches!(joined.children.last(), Some(InlineNode::HardBreak(_)))
                            {
                                joined.children.push(InlineNode::soft_break());
                            }
                            if let BlockNode::Paragraph(paragraph) = paragraph {
                                joined.children.extend(paragraph.children.iter().cloned());
                            }
                        }
                        let mut fragment = crate::parse("");
                        fragment.children.push(BlockNode::Paragraph(joined));
                        if let Ok(contents) = crate::render_carve(&fragment) {
                            layout.edits.push(Edit {
                                start: layout.offset(first.start_offset),
                                end: layout.offset(last.end_offset),
                                value: contents
                                    .trim_end_matches('\n')
                                    .replace('\n', &format!("\n{continuation}")),
                            });
                        }
                    }
                }
            }
        }
        visit_block_children(
            &mut node,
            &mut Pending {
                nodes: &mut pending,
                tight,
            },
        );
    }
    layout.edits.sort_by_key(|edit| edit.start);
    let mut result = String::new();
    let mut copied = 0;
    for edit in layout.edits {
        if edit.start < copied {
            continue;
        }
        result.push_str(&layout.source[copied..edit.start]);
        result.push_str(&edit.value);
        copied = edit.end;
    }
    result.push_str(&source[copied..]);
    result
}
