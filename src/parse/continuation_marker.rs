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
}

fn refusal_name(reason: Refusal) -> &'static str {
    match reason {
        Refusal::WrongColumn => "wrong-column",
        Refusal::Document => "document",
        Refusal::Blank => "blank",
        Refusal::Left => "left",
        Refusal::Right => "right",
        Refusal::Sibling => "sibling",
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
    static LETTER: RefCell<Option<char>> = const { RefCell::new(None) };
}

pub(super) struct Guard(Option<HashMap<usize, Decision>>, Option<char>);

impl Guard {
    pub(super) fn install(owned: Option<&OwnedSource>) -> Self {
        let decisions = owned
            .filter(|o| !o.accepted.is_empty())
            .map(|o| o.accepted.iter().copied().collect());
        if let Some(owned) = owned {
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
        Self(
            DECISIONS.with(|slot| slot.replace(decisions)),
            LETTER.with(|slot| slot.replace(letter)),
        )
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        DECISIONS.with(|slot| slot.replace(self.0.take()));
        LETTER.with(|slot| slot.replace(self.1.take()));
    }
}

/// Whether `decision` can answer: the document holds an accepted `+`.
pub(super) fn active() -> bool {
    DECISIONS.with(|slot| slot.borrow().is_some())
}

pub(super) fn decision(source_line: usize) -> Option<Decision> {
    DECISIONS.with(|slot| slot.borrow().as_ref()?.get(&source_line).copied())
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

/// A reader may keep as payload a `+` the walk read as a letter.
pub(super) fn restore_payloads(doc: &mut crate::ast::Document) {
    if LETTER.with(|slot| slot.borrow().is_none()) {
        return;
    }
    let mut restore = |block: &mut crate::ast::BlockNode| {
        if let crate::ast::BlockNode::CodeBlock(crate::ast::CodeBlock { content, .. })
        | crate::ast::BlockNode::RawBlock(crate::ast::RawBlock { content, .. }) = block
        {
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
