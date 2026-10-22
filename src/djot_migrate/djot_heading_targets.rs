use std::collections::{HashMap, HashSet};

use crate::ast::BlockNode;
use crate::extension::{HeadingIdOptions, SmartTypographyMode};
use crate::include_walk::{visit_block_children, SubtreeVisitor};

struct HeadingText(Vec<String>);

impl SubtreeVisitor for HeadingText {
    fn blocks(&mut self, blocks: &mut Vec<BlockNode>) {
        for block in blocks {
            if let BlockNode::Heading(heading) = block {
                self.0.push(crate::render::plain_inlines_typography(
                    &heading.children,
                    SmartTypographyMode::Source,
                ));
            }
            visit_block_children(block, self);
        }
    }
}

pub(super) fn from_carve(carve: &str) -> HashMap<String, String> {
    let mut actual = crate::parse(&carve.replace("\0U\0", "\0"));
    let ids = crate::document_ids::assigned_heading_ids(&actual, HeadingIdOptions::PLAIN);
    let mut text = HeadingText(Vec::new());
    text.blocks(&mut actual.children);
    let mut occupied: HashSet<char> = carve.chars().collect();
    for heading in &text.0 {
        occupied.extend(heading.chars());
    }
    let marker = [(0xE000, 0xF8FF), (0xF0000, 0xFFFFD), (0x100000, 0x10FFFD)]
        .into_iter()
        .flat_map(|(first, last)| first..=last)
        .filter_map(char::from_u32)
        .find(|character| !occupied.contains(character));
    let Some(marker) = marker else {
        return HashMap::new();
    };
    let marker = marker.to_string();
    let mut marked = crate::parse(&carve.replace("\0U\0", &marker).replace('\0', &marker));
    let mut marked_text = HeadingText(Vec::new());
    marked_text.blocks(&mut marked.children);
    if marked_text.0.len() != text.0.len() || ids.len() != text.0.len() {
        return HashMap::new();
    }
    let mut targets = HashMap::new();
    for (text, id) in marked_text.0.into_iter().zip(ids) {
        let text = text.replace(&marker, "\0");
        let key = text
            .trim()
            .split([' ', '\t', '\r', '\n'])
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        targets.entry(key).or_insert_with(|| format!("#{id}"));
    }
    targets
}
