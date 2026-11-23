//! The findings the 0.1 lint pass still supplied, read off the 0.2 tree.
//!
//! Each record carries what a 0.1 `LintWarning` carried, so `lint_core` skips,
//! converts and deduplicates it exactly as it did the 0.1 warning.

use crate::view::V;

pub(super) struct Warning {
    pub(super) rule: &'static str,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) message: String,
}

pub(super) struct Node<'a> {
    pub(super) value: V<'a>,
    pub(super) parent: Option<usize>,
    /// Only these two parent slots affect a legacy rule.
    pub(super) slot: Slot,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    Children,
    Caption,
    Other,
}

impl Slot {
    pub(super) fn of(key: &str) -> Self {
        match key {
            "children" => Self::Children,
            "caption" => Self::Caption,
            _ => Self::Other,
        }
    }
}

impl Node<'_> {
    fn kind(&self) -> &str {
        self.value.ty().as_str().unwrap_or("")
    }
}

#[cfg(test)]
#[test]
fn legacy_walk_record_is_compact() {
    assert!(std::mem::size_of::<Node<'static>>() <= 48);
}

fn bounds(node: V<'_>) -> Option<(usize, usize)> {
    node.offsets()
}

fn ancestors<'n, 'a>(all: &'n [Node<'a>], at: usize) -> impl Iterator<Item = &'n Node<'a>> {
    std::iter::successors(all.get(at).and_then(|n| n.parent), |&i| {
        all.get(i).and_then(|n| n.parent)
    })
    .filter_map(|i| all.get(i))
}

/// The findings for `all`, the typed nodes with each subtree after its node, visited in `order`.
pub(super) fn warnings(source: &str, all: &[Node<'_>], order: &[u32]) -> Vec<Warning> {
    let mut out = Vec::new();
    for &at in order {
        let at = at as usize;
        if all
            .get(at)
            .is_some_and(|node| node.kind() == "caption_number")
        {
            panel_number(source, all, at, &mut out);
        }
    }
    out
}

fn push(out: &mut Vec<Warning>, rule: &'static str, (start, end): (usize, usize)) {
    out.push(Warning {
        rule,
        start,
        end,
        message: String::new(),
    });
}

/// A `#` among the top-level inlines of a panel caption.
fn panel_number(source: &str, all: &[Node<'_>], at: usize, out: &mut Vec<Warning>) {
    let mut chain = ancestors(all, at);
    let (Some(paragraph), Some(caption), Some(panel), Some(group)) =
        (chain.next(), chain.next(), chain.next(), chain.next())
    else {
        return;
    };
    let Some(node) = all.get(at) else { return };
    if node.slot == Slot::Children
        && paragraph.kind() == "paragraph"
        && paragraph.slot == Slot::Children
        && caption.kind() == "caption"
        && caption.slot == Slot::Caption
        && group.kind() == "figure_group"
        && panel.slot == Slot::Children
        && (panel.value.get("caption").is_some() || panel.kind() == "table")
        && let Some(span) = bounds(node.value)
        && bounds(caption.value)
            .and_then(|(from, _)| source.get(from..span.0))
            .is_some_and(|before| !before.contains('\n'))
    {
        push(out, "figure-group-panel-number", span);
    }
}
