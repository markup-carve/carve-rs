//! `headingShift` (C2-H-208, C2-H-209): the core step that moves every
//! heading of a render view down by N, before the first document transform.
//! The writers report a level the shift raised above 6.

use crate::ast::{Node, owned};

/// The loss of a shift that raises a heading above level 6.
pub const ABOVE_6: &str = "heading-shift-above-6";

/// Adds `shift` to every heading level, at every depth.
pub(crate) fn apply(nodes: &mut [Node<'_>], shift: u8) {
    if shift == 0 {
        return;
    }
    for n in nodes.iter_mut() {
        walk(n, i64::from(shift));
    }
}

fn walk(n: &mut Node<'_>, by: i64) {
    if let Node::Heading(h) = n {
        h.level = h.level.saturating_add(by);
        return;
    }
    n.for_each_child_mut(&mut |c| walk(c, by));
}

/// [`apply`] on an owned tree, the view the transforms of a host receive.
pub(crate) fn apply_owned(nodes: &mut [owned::Node], shift: u8) {
    if shift == 0 {
        return;
    }
    for n in nodes.iter_mut() {
        walk_owned(n, i64::from(shift));
    }
}

fn walk_owned(n: &mut owned::Node, by: i64) {
    if let owned::Node::Heading(h) = n {
        h.level = h.level.saturating_add(by);
        return;
    }
    n.for_each_child_mut(&mut |c| walk_owned(c, by));
}
