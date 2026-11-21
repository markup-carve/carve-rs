//! `headingShift` (C2-H-208, C2-H-209): the core step that moves every
//! heading of a render view down by N, before the first document transform.
//! The writers report the first level the shift raised above 6.

use crate::ast::{Node, Pos, owned};

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

/// The first heading above level 6 in document order, the loss a set shift
/// reports once (C2-H-208, C2-H-209): its position and `file_of` it.
pub(crate) fn first_above_6(nodes: &[Node<'_>], file_of: &dyn Fn(&Pos) -> Option<String>) -> Found {
    let mut found = None;
    for n in nodes {
        find(n, file_of, &mut found);
        if found.is_some() {
            break;
        }
    }
    found
}

pub(crate) type Found = Option<(Option<Pos>, Option<String>)>;

fn find(n: &Node<'_>, file_of: &dyn Fn(&Pos) -> Option<String>, found: &mut Found) {
    if found.is_some() {
        return;
    }
    if let Node::Heading(h) = n {
        if h.level > 6 {
            *found = Some((h.pos, h.pos.as_ref().and_then(file_of)));
        }
        return;
    }
    n.for_each_child(&mut |c| find(c, file_of, found));
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

/// The first above-six heading just after shifting an owned render view.
pub(crate) fn first_above_6_owned(nodes: &[owned::Node]) -> Found {
    for node in nodes {
        if let Some(found) = find_owned(node) {
            return Some(found);
        }
    }
    None
}

fn find_owned(node: &owned::Node) -> Found {
    if let owned::Node::Heading(heading) = node {
        return (heading.level > 6).then_some((heading.pos, None));
    }
    let mut found = None;
    node.for_each_child(&mut |child| {
        if found.is_none() {
            found = find_owned(child);
        }
    });
    found
}
