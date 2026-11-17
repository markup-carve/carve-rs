//! The section of an included file (C2-N-027 to C2-N-031).

use crate::Arena;
use crate::ast::{self, Document, Node};
use crate::options::HeadingIds;
use crate::resolve::order::addr;

/// C2-N-029: the block lists of the file's body below `n`; a footnote, a
/// caption and a table hold none.
fn inner<'r, 'a>(n: &'r Node<'a>) -> Vec<&'r [Node<'a>]> {
    match n {
        Node::List(l) => l.children.iter().filter_map(Node::children).collect(),
        Node::DefinitionList(d) => d
            .children
            .iter()
            .filter(|c| matches!(c, Node::DefinitionDescription(_)))
            .filter_map(Node::children)
            .collect(),
        Node::Footnote(_) | Node::Include(_) => Vec::new(),
        _ if super::expand::holds_blocks(n) => n.children().into_iter().collect(),
        _ => Vec::new(),
    }
}

fn inner_mut<'r, 'a>(n: &'r mut Node<'a>, k: usize) -> Option<&'r mut [Node<'a>]> {
    match n {
        Node::List(l) => l.children.get_mut(k)?.children_mut(),
        Node::DefinitionList(d) => d.children.get_mut(k)?.children_mut(),
        _ => n.children_mut(),
    }
}

fn is_definition(n: &Node<'_>) -> bool {
    matches!(
        n,
        Node::LinkReferenceDefinition(_)
            | Node::Footnote(_)
            | Node::AbbreviationDef(_)
            | Node::CitationDefinition(_)
    )
}

/// C2-N-029: a definition, a comment, frontmatter or an include selects nothing.
fn selectable(n: &Node<'_>) -> bool {
    !is_definition(n)
        && !matches!(
            n,
            Node::Comment(_) | Node::Frontmatter(_) | Node::Include(_)
        )
}

/// The path to the first block for which `test` holds, a container before
/// the blocks inside it: per step, the index in the list and the inner list.
fn find(
    list: &[Node<'_>],
    test: &dyn Fn(&Node<'_>) -> bool,
    path: &mut Vec<(usize, usize)>,
) -> bool {
    for (at, n) in list.iter().enumerate() {
        if test(n) {
            path.push((at, 0));
            return true;
        }
        let lists = inner(n);
        for (k, l) in lists.iter().enumerate() {
            let k = match n {
                Node::List(_) => k,
                Node::DefinitionList(d) => d
                    .children
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| matches!(c, Node::DefinitionDescription(_)))
                    .nth(k)
                    .map_or(k, |(i, _)| i),
                _ => 0,
            };
            path.push((at, k));
            if find(l, test, path) {
                return true;
            }
            path.pop();
        }
    }
    false
}

fn placeholder(arena: &Arena) -> Node<'_> {
    Node::SoftBreak(arena.alloc(ast::SoftBreak::default()))
}

/// C2-N-027 to C2-N-031: the selected blocks, then every definition of the
/// file that the selection did not take; `None` when nothing matches.
pub(super) fn select<'a>(
    doc: &mut Document<'a>,
    name: &str,
    mode: HeadingIds,
    arena: &'a Arena,
) -> Option<&'a mut [Node<'a>]> {
    let ids = crate::resolve::heading_ids_alone(doc, mode, arena);
    let mut path = Vec::new();
    let heading = |n: &Node<'_>| matches!(n, Node::Heading(_)) && ids.get(&addr(n)) == Some(&name);
    if !find(doc.children, &heading, &mut path) {
        path.clear();
        let block = |n: &Node<'_>| selectable(n) && n.attrs().and_then(|a| a.id) == Some(name);
        if !find(doc.children, &block, &mut path) {
            return None;
        }
    }
    let ((at, _), steps) = path.split_last()?;
    let mut list: &mut [Node<'a>] = &mut *doc.children;
    for &(i, k) in steps {
        list = inner_mut(list.get_mut(i)?, k)?;
    }
    let level = match list.get(*at)? {
        Node::Heading(h) => Some(h.level),
        _ => None,
    };
    let end = level.map_or(at + 1, |lv| {
        list.iter()
            .enumerate()
            .skip(at + 1)
            .find(|(_, n)| matches!(n, Node::Heading(h) if h.level <= lv))
            .map_or(list.len(), |(i, _)| i)
    });
    let mut out: Vec<Node<'a>> = list
        .get_mut(*at..end)?
        .iter_mut()
        .map(|n| std::mem::replace(n, placeholder(arena)))
        .collect();
    for n in doc.children.iter_mut().filter(|n| is_definition(n)) {
        out.push(std::mem::replace(n, placeholder(arena)));
    }
    Some(arena.nodes(&mut out))
}
