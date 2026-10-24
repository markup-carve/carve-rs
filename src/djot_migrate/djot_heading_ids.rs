use std::collections::{HashMap, HashSet};

use crate::ast::{AttrSlot, Attrs, BlockNode, InlineNode, Pos};
use crate::extension::HeadingIdOptions;
use crate::include_walk::{visit_block_children, visit_inline_children, SubtreeVisitor};

#[derive(Default)]
struct IdNodes {
    position: Option<usize>,
    explicit: Vec<(usize, String)>,
    headings: Vec<(usize, bool, String)>,
}

impl SubtreeVisitor for IdNodes {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks {
            if let BlockNode::Heading(heading) = block {
                if let Some(pos) = &heading.pos {
                    let base = crate::parse::slugify_parse(
                        &crate::render::plain_inlines(&heading.children),
                        HeadingIdOptions::PLAIN,
                    );
                    let authored = heading
                        .attrs
                        .as_ref()
                        .is_some_and(|a| a.order.contains(&AttrSlot::Id));
                    self.headings.push((pos.start_offset, authored, base));
                }
            }
            visit_block_children(block, self);
        }
    }

    fn inlines(&mut self, inlines: &mut Vec<InlineNode>) {
        for inline in inlines {
            visit_inline_children(inline, self);
        }
    }

    fn position(&mut self, pos: &mut Pos) {
        self.position = Some(pos.start_offset);
    }

    fn attributes(&mut self, attrs: &mut Option<Attrs>, _inline: bool) {
        if let (Some(at), Some(id)) = (
            self.position,
            attrs
                .as_ref()
                .filter(|a| a.order.contains(&AttrSlot::Id))
                .and_then(|a| a.id.as_ref()),
        ) {
            self.explicit.push((at, id.clone()));
        }
    }
}

pub(super) fn preserve(source: String) -> String {
    if !source.contains('#') {
        return source;
    }
    let mut doc =
        crate::parse_with_options(&source, &crate::Options::default().with_positions(true));
    let actual = crate::document_ids::assigned_heading_ids(&doc, HeadingIdOptions::PLAIN);
    let mut nodes = IdNodes::default();
    nodes.blocks(&mut doc.children);
    if nodes.headings.len() != actual.len() {
        return source;
    }
    nodes.explicit.sort_by_key(|(at, _)| *at);
    let mut occupied = HashSet::new();
    let mut next_suffix = HashMap::new();
    let mut next_explicit = 0;
    let mut edits = Vec::new();
    for ((at, authored, base), current) in nodes.headings.into_iter().zip(actual) {
        while next_explicit < nodes.explicit.len() && nodes.explicit[next_explicit].0 <= at {
            occupied.insert(nodes.explicit[next_explicit].1.clone());
            next_explicit += 1;
        }
        if authored {
            continue;
        }
        let mut id = base.clone();
        let mut suffix = *next_suffix.get(&base).unwrap_or(&1);
        while occupied.contains(&id) {
            id = format!("{base}-{suffix}");
            suffix += 1;
        }
        next_suffix.insert(base, suffix);
        occupied.insert(id.clone());
        if id != current {
            edits.push((at, id));
        }
    }
    if edits.is_empty() {
        return source;
    }
    let marker = regex::Regex::new(r"(?:[-+*]|[0-9]+[.)])[ \t]*$").unwrap();
    let mut result = String::with_capacity(source.len() + edits.len() * 16);
    let mut cursor = 0;
    let mut positions = source.char_indices();
    let mut char_at = 0;
    for (at, id) in edits {
        let byte_at = loop {
            match positions.next() {
                Some((byte, _)) if char_at == at => {
                    char_at += 1;
                    break byte;
                }
                Some(_) => char_at += 1,
                None => break source.len(),
            }
        };
        result.push_str(&source[cursor..byte_at]);
        let line_start = source[..byte_at].rfind('\n').map_or(0, |i| i + 1);
        let prefix = &source[line_start..byte_at];
        let continuation = marker.replace(prefix, |caps: &regex::Captures<'_>| {
            " ".repeat(caps[0].len())
        });
        result.push_str(&format!("{{#{id}}}\n{continuation}"));
        cursor = byte_at;
    }
    result.push_str(&source[cursor..]);
    result
}
