//! The ownership walk's verdict on each lone `+` (PART 9 §17 L3): readers ask it
//! what an accepted `+` hosts. A refused one is a letter in their source, so they
//! never meet it.

use std::cell::RefCell;
use std::collections::HashMap;

use super::own_lines::OwnedSource;
use super::{note_lint_event, LintEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    List,
    Quote,
    Description,
    Footnote,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    WrongColumn,
    Document,
    Blank,
    Left,
    Right,
    Sibling,
    Claimed,
}

fn refusal_name(reason: Refusal) -> &'static str {
    match reason {
        Refusal::WrongColumn => "wrong-column",
        Refusal::Document => "document",
        Refusal::Blank => "blank",
        Refusal::Left => "left",
        Refusal::Right => "right",
        Refusal::Sibling => "sibling",
        Refusal::Claimed => "claimed",
    }
}

fn host(kind: Option<Kind>, quote_depth: usize) -> &'static str {
    match kind {
        Some(Kind::Quote) => "block quote",
        None if quote_depth > 0 => "block quote",
        Some(Kind::Description) => "description body",
        Some(Kind::Footnote) => "footnote",
        _ => "list item",
    }
}

/// An accepted `+`.
#[derive(Clone, Copy, Debug)]
pub(super) struct Decision {
    pub owner: Option<Kind>,
    pub owner_column: Option<usize>,
    pub first_block: bool,
    pub quote_depth: usize,
}

/// A refused `+`, for the `continuation-marker-refused` lint.
#[derive(Clone, Copy, Debug)]
pub(super) struct Refused {
    pub line: usize,
    pub column: usize,
    pub host: Option<Kind>,
    pub quote_depth: usize,
    pub reason: Refusal,
    pub first_block: bool,
}

pub(super) fn kind_of(kind: super::own_lines::Kind) -> Option<Kind> {
    use super::own_lines::Kind as Frame;
    match kind {
        Frame::Item => Some(Kind::List),
        Frame::Quote => Some(Kind::Quote),
        Frame::Dd => Some(Kind::Description),
        Frame::Note => Some(Kind::Footnote),
        _ => None,
    }
}

thread_local! {
    static DECISIONS: RefCell<Option<HashMap<usize, Decision>>> = const { RefCell::new(None) };
    static ATTACHED_QUOTES: RefCell<Option<HashMap<usize, usize>>> = const { RefCell::new(None) };
    static LETTER: RefCell<Option<char>> = const { RefCell::new(None) };
    static CONSUMED: RefCell<(Vec<usize>, Option<char>)> = const { RefCell::new((Vec::new(), None)) };
}

pub(super) struct Guard(
    Option<HashMap<usize, Decision>>,
    Option<HashMap<usize, usize>>,
    Option<char>,
    (Vec<usize>, Option<char>),
);

impl Guard {
    pub(super) fn install(owned: Option<&OwnedSource>) -> Self {
        let decisions = owned
            .filter(|o| !o.accepted.is_empty())
            .map(|o| o.accepted.iter().copied().collect());
        if let Some(owned) = owned {
            for &(line, column, width, end_of_document) in &owned.unclosed_comments {
                note_lint_event(LintEvent::CommentFence {
                    line,
                    column,
                    width,
                    end_of_document,
                });
            }
            for refused in &owned.refused {
                note_lint_event(LintEvent::Marker {
                    line: refused.line,
                    column: refused.column + 1,
                    host: host(refused.host, refused.quote_depth),
                    reason: refusal_name(refused.reason),
                    first_block: refused.first_block,
                });
            }
        }
        let letter = owned.and_then(|o| o.letter);
        let consumed = owned
            .map(|o| (o.consumed.clone(), o.consumed_mark))
            .unwrap_or_default();
        Self(
            DECISIONS.with(|slot| slot.replace(decisions)),
            ATTACHED_QUOTES.with(|slot| {
                slot.replace(owned.map(|o| o.attached_quotes.iter().copied().collect()))
            }),
            LETTER.with(|slot| slot.replace(letter)),
            CONSUMED.with(|slot| slot.replace(consumed)),
        )
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        DECISIONS.with(|slot| slot.replace(self.0.take()));
        ATTACHED_QUOTES.with(|slot| slot.replace(self.1.take()));
        LETTER.with(|slot| slot.replace(self.2.take()));
        CONSUMED.with(|slot| slot.replace(std::mem::take(&mut self.3)));
    }
}

/// Whether `decision` can answer: the document holds an accepted `+`.
pub(super) fn active() -> bool {
    DECISIONS.with(|slot| slot.borrow().is_some())
}

pub(super) fn decision(source_line: usize) -> Option<Decision> {
    DECISIONS.with(|slot| slot.borrow().as_ref()?.get(&source_line).copied())
}

pub(super) fn attached_quote_host_column(source_line: usize) -> Option<usize> {
    ATTACHED_QUOTES.with(|slot| slot.borrow().as_ref()?.get(&source_line).copied())
}

/// Text with every refused `+` back as itself.
pub(super) fn restore_letter(text: &str) -> std::borrow::Cow<'_, str> {
    match LETTER.with(|slot| *slot.borrow()) {
        Some(letter) if text.contains(letter) => text.replace(letter, "+").into(),
        _ => text.into(),
    }
}

/// The whole line is one refused `+`.
pub(super) fn is_refused_letter(line: &str) -> bool {
    let line = line.trim_end_matches([' ', '\t']);
    LETTER
        .with(|slot| *slot.borrow())
        .is_some_and(|letter| line.strip_prefix(letter) == Some(""))
}

/// The whole line is the placeholder of a consumed `+`.
pub(super) fn is_consumed_marker(line: &str) -> bool {
    CONSUMED.with(|slot| slot.borrow().1).is_some_and(|mark| {
        line.strip_prefix("%%")
            .is_some_and(|rest| rest.trim().strip_prefix(mark) == Some(""))
    })
}

/// A reader may keep as payload a `+` the walk read as a letter or consumed.
pub(super) fn restore_payloads(doc: &mut crate::ast::Document) {
    let consumed = CONSUMED
        .with(|slot| slot.borrow().1)
        .map(|mark| format!("%% {mark}"));
    if LETTER.with(|slot| slot.borrow().is_none()) && consumed.is_none() {
        return;
    }
    let mut restore = |block: &mut crate::ast::BlockNode| {
        if let crate::ast::BlockNode::CodeBlock(crate::ast::CodeBlock { content, .. })
        | crate::ast::BlockNode::RawBlock(crate::ast::RawBlock { content, .. }) = block
        {
            if let Some(placeholder) = consumed.as_deref().filter(|p| content.contains(p)) {
                *content = content.replace(placeholder, "+");
            }
            if let std::borrow::Cow::Owned(restored) = restore_letter(content) {
                *content = restored;
            }
        }
    };
    crate::includes::walk_blocks_mut(&mut doc.children, &mut restore);
    for body in doc.footnote_defs.values_mut() {
        crate::includes::walk_blocks_mut(body, &mut restore);
    }
}

/// A consumed marker renders nothing and is no node of the tree (carve#5013).
pub(super) fn drop_consumed_markers(doc: &mut crate::ast::Document) {
    use crate::ast::{BlockNode, Pos};
    let (lines, mark) = CONSUMED.with(|slot| slot.borrow().clone());
    if lines.is_empty() {
        return;
    }
    let mark = mark.map(String::from);
    let keep = |block: &BlockNode| {
        !matches!(block, BlockNode::Comment(c)
            if !c.block && !c.delimited
                && (mark.as_deref() == Some(c.content.trim())
                    || (c.content.is_empty()
                        && c.pos.as_ref().is_some_and(|pos| lines.contains(&pos.start_line)))))
    };
    fn sweep(blocks: &mut Vec<BlockNode>, keep: &impl Fn(&BlockNode) -> bool) {
        blocks.retain(|block| keep(block));
        for block in blocks.iter_mut() {
            match block {
                BlockNode::BlockQuote(b) => {
                    let ends_at_child = !b.fenced;
                    sweep_container(
                        &mut b.children,
                        b.pos.as_mut().filter(|_| ends_at_child),
                        keep,
                    );
                }
                BlockNode::Div(b) => sweep(&mut b.children, keep),
                BlockNode::Section(b) => sweep(&mut b.children, keep),
                BlockNode::Admonition(b) => sweep(&mut b.children, keep),
                BlockNode::Directive(b) => sweep(&mut b.children, keep),
                BlockNode::FigureGroup(b) => sweep(&mut b.children, keep),
                BlockNode::ExtensionCarrier(b) => sweep(&mut b.children, keep),
                BlockNode::Figure(figure) => {
                    if let crate::ast::FigureTarget::BlockQuote(quote) = &mut *figure.target {
                        let ends_at_child = !quote.fenced;
                        let pos = quote.pos.as_mut().filter(|_| ends_at_child);
                        sweep_container(&mut quote.children, pos, keep);
                    }
                }
                BlockNode::List(list) => {
                    let old = list.items.iter().rev().find_map(|item| item.pos.clone());
                    for item in &mut list.items {
                        sweep_container(&mut item.children, item.pos.as_mut(), keep);
                    }
                    let new = list.items.iter().rev().find_map(|item| item.pos.clone());
                    reend(list.pos.as_mut(), old, new);
                }
                BlockNode::DefinitionList(list) => {
                    let old = definition_list_end(list);
                    for item in &mut list.items {
                        for body in &mut item.definitions {
                            sweep_container(&mut body.children, body.pos.as_mut(), keep);
                        }
                    }
                    let new = definition_list_end(list);
                    reend(list.pos.as_mut(), old, new);
                }
                _ => {}
            }
        }
    }
    /// Sweep a container that ends at its last placed child (CARVE-P12-014).
    /// Its end was taken with the markers still in place, so a container that
    /// ended on one now ends at its last kept child. An empty host retains
    /// its sole consumed-marker extent (CARVE-P12-014, carve#5246).
    fn sweep_container(
        children: &mut Vec<BlockNode>,
        pos: Option<&mut Pos>,
        keep: &impl Fn(&BlockNode) -> bool,
    ) {
        let old = super::positions::furthest_block_pos(children);
        sweep(children, keep);
        let new = super::positions::furthest_block_pos(children);
        reend(pos, old, new);
    }
    /// Move `pos`'s end from `old` to `new` where `old` was what it ended on.
    fn reend(pos: Option<&mut Pos>, old: Option<Pos>, new: Option<Pos>) {
        let (Some(pos), Some(old), Some(new)) = (pos, old, new) else {
            return;
        };
        if pos.end_offset == old.end_offset && new.end_offset < old.end_offset {
            pos.end_line = new.end_line;
            pos.end_column = new.end_column;
            pos.end_offset = new.end_offset;
        }
    }
    fn definition_list_end(list: &crate::ast::DefinitionList) -> Option<Pos> {
        list.items
            .iter()
            .flat_map(|item| item.definitions.iter().filter_map(|def| def.pos.clone()))
            .max_by_key(|pos| pos.end_offset)
    }
    sweep(&mut doc.children, &keep);
    for (label, body) in doc.footnote_defs.iter_mut() {
        sweep(body, &keep);
        // A definition's extent ends with its body, not on a dropped marker.
        let last = body.last().and_then(crate::ast_json::block_pos);
        if let (Some(pos), Some(last)) = (doc.footnote_def_pos.get_mut(label), last) {
            if lines.contains(&pos.end_line) {
                pos.end_line = last.end_line;
                pos.end_column = last.end_column;
                pos.end_offset = last.end_offset;
            }
        }
    }
}
