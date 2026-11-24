//! C2-W-146, C2-W-092: a caret and a space that start a line of paragraph, term or line block text stay
//! text that `unattached-caption` reports, so the caret takes a backslash.
use super::node::{N, children, for_each_slot, is_break, text, ty};
use crate::view::Arr;

/// The text nodes of `group` whose first byte is such a caret.
pub(super) fn caret_lines<'t>(group: Arr<'t>, found: &mut Vec<N<'t>>) {
    for node in group {
        walk(node, found);
    }
}

fn walk<'t>(node: N<'t>, found: &mut Vec<N<'t>>) {
    match ty(node) {
        "paragraph" => scan_lines(children(node), true, found),
        // A term's first line starts behind its marker; its later lines read like paragraph text (C2-B-069).
        "definition_term" => scan_lines(children(node), false, found),
        "line" => test_line_start(children(node), 0, found),
        _ => {}
    }
    for_each_slot(node, true, &mut |kids, _, _| {
        for child in kids {
            walk(child, found);
        }
    });
}

fn scan_lines<'t>(nodes: Arr<'t>, line_start: bool, found: &mut Vec<N<'t>>) {
    let mut start = line_start;
    for (at, node) in nodes.iter().enumerate() {
        match ty(node) {
            "soft_break" | "hard_break" => start = true,
            "text" => {
                if start {
                    test_line_start(nodes, at, found);
                }
                if !text(node).is_empty() {
                    start = false;
                }
            }
            // Any other node writes a mark or a body first; a break inside it starts a line of the same paragraph.
            _ => {
                scan_lines(children(node), false, found);
                start = false;
            }
        }
    }
}

fn test_line_start<'t>(nodes: Arr<'t>, at: usize, found: &mut Vec<N<'t>>) {
    let Some(node) = nodes.get(at).filter(|n| ty(*n) == "text") else {
        return;
    };
    let Some(rest) = text(node).strip_prefix("^ ") else {
        return;
    };
    let rest = rest.trim_start_matches([' ', '\t']);
    let behind = nodes.get(at + 1);
    if !rest.is_empty() || (behind.is_some() && !is_break(behind)) {
        found.push(node);
    }
}
