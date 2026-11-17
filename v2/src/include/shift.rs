//! Heading shifts of an expanded tree (C2-N-048 to C2-N-051).

use crate::ast::Node;
use crate::diagnostic::Diagnostic;
use serde_json::Value;

const MIN_LEVEL: i64 = 1;
const MAX_LEVEL: i64 = 6;

/// The level of the last heading of each open container, innermost last.
struct Walk<'d> {
    levels: Vec<i64>,
    diags: &'d mut Vec<Diagnostic>,
}

/// C2-N-050: include by include in tree order, an include before the includes inside it.
pub(super) fn apply(nodes: &mut [Node<'_>], diags: &mut Vec<Diagnostic>) {
    let mut w = Walk {
        levels: vec![0],
        diags,
    };
    list(&mut w, nodes);
}

fn list(w: &mut Walk<'_>, nodes: &mut [Node<'_>]) {
    for n in nodes.iter_mut() {
        node(w, n);
    }
}

fn node(w: &mut Walk<'_>, n: &mut Node<'_>) {
    match n {
        Node::Heading(h) => {
            if let Some(l) = w.levels.last_mut() {
                *l = h.level;
            }
        }
        // C2-N-049: an expanded include is no container.
        Node::Include(i) => {
            let file = i.file;
            let by = i.options.and_then(|o| o.get("shift")).cloned();
            if let (Some(children), Some(by)) = (i.children.as_deref_mut(), by) {
                shift_include(w, children, &by, file);
            }
            if let Some(children) = i.children.as_deref_mut() {
                list(w, children);
            }
        }
        Node::List(_) | Node::DefinitionList(_) => n.for_each_child_mut(&mut |c| node(w, c)),
        _ if super::expand::holds_blocks(n) || matches!(n, Node::DefinitionDescription(_)) => {
            w.levels.push(0);
            if let Some(children) = n.children_mut() {
                list(w, children);
            }
            w.levels.pop();
        }
        _ => {}
    }
}

fn headings<'r, 'a>(nodes: &'r mut [Node<'a>], out: &mut Vec<&'r mut crate::ast::Heading<'a>>) {
    for n in nodes.iter_mut() {
        if let Node::Heading(h) = n {
            out.push(h);
        } else if let Some(children) = block_children(n) {
            headings(children, out);
        }
    }
}

fn block_children<'r, 'a>(n: &'r mut Node<'a>) -> Option<&'r mut [Node<'a>]> {
    if matches!(n, Node::Include(_)) || super::expand::holds_blocks(n) {
        n.children_mut()
    } else {
        None
    }
}

/// C2-N-048, C2-N-049: adds the shift to every heading among the children, clamped to levels 1 to 6.
fn shift_include(w: &mut Walk<'_>, children: &mut [Node<'_>], by: &Value, file: Option<&str>) {
    let mut found = Vec::new();
    headings(children, &mut found);
    let Some(lowest) = found.iter().map(|h| h.level).min() else {
        return;
    };
    let by = if by.as_str() == Some("auto") {
        let context = w
            .levels
            .iter()
            .rev()
            .find(|&&l| l > 0)
            .copied()
            .unwrap_or(0);
        context + 1 - lowest
    } else {
        by.as_i64().unwrap_or(0)
    };
    if by == 0 {
        return;
    }
    for h in found {
        let level = h.level.saturating_add(by);
        h.level = level.clamp(MIN_LEVEL, MAX_LEVEL);
        if h.level != level {
            let text = crate::resolve::plain::text_of(h.children);
            let shown = h.level.to_string();
            w.diags.push(Diagnostic::new(
                "include-heading-clamp",
                &[("text", text.as_str()), ("level", shown.as_str())],
                file,
                h.pos,
            ));
        }
    }
}
