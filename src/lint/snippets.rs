//! C2-N-096 and C2-B-236: the snippet rules, which read the authored tree.

use super::{LintOptions, active};
use crate::diagnostic::Diagnostic;
use crate::source::Source;
use crate::view::V;
use std::collections::HashSet;

/// The names every authored use names, those inside definitions included (C2-N-096).
fn used_names<'a>(v: V<'a>, out: &mut HashSet<&'a str>) {
    if let Some(arr) = v.as_array() {
        for at in 0..arr.len() {
            if let Some(item) = arr.get(at) {
                used_names(item, out);
            }
        }
        return;
    }
    if v.get("type").and_then(V::as_str) == Some("snippet_use") {
        if let Some(name) = v.get("name").and_then(V::as_str) {
            out.insert(name);
        }
        return;
    }
    v.each(&mut |key, x| {
        if key != "pos" && x.is_container() {
            used_names(x, out);
        }
    });
}

fn start_of(node: V<'_>) -> Option<(usize, u64)> {
    let pos = node.get("pos")?;
    let start = usize::try_from(pos.get("startOffset")?.as_u64()?).ok()?;
    Some((start, pos.get("startColumn")?.as_u64()?))
}

/// The offset behind the attribute lines at the start of `text`, and whether one holds an id.
fn attribute_lines(text: &str) -> (usize, bool) {
    let (mut at, mut id) = (0, false);
    while let Some(rest) = text.get(at..) {
        let line = rest.split('\n').next().unwrap_or("");
        let line = line.strip_suffix('\r').unwrap_or(line);
        let block = line
            .starts_with('{')
            .then(|| crate::attr::parse_glued(line, 0))
            .flatten()
            .filter(|(b, end)| {
                !b.empty && line.get(*end..).is_some_and(crate::chars::is_blank_line)
            });
        let (Some((block, _)), Some(next)) = (block, rest.find('\n')) else {
            break;
        };
        id |= crate::snippet::holds_id(&block);
        at += next + 1;
    }
    (at, id)
}

/// C2-B-236: the start and end of the marker of a definition line that an id
/// made text, an id in the marker or in the attribute lines above it.
fn id_marker(text: &str) -> Option<(usize, usize)> {
    let (at, line_id) = attribute_lines(text);
    let line = text.get(at..)?.split('\n').next()?;
    if !line.starts_with("{$") {
        return None;
    }
    let m = crate::snippet::marker(line)?;
    let value = line.get(m.end..)?;
    ((m.has_id || line_id) && value.starts_with(' ') && !value.trim().is_empty())
        .then_some((at, at + m.end))
}

struct Scan<'s, 'v> {
    positions: Source<'s>,
    used: HashSet<&'v str>,
    seen: HashSet<&'v str>,
    unused: bool,
    ids: bool,
}

impl<'v> Scan<'_, 'v> {
    fn child(&mut self, source: &str, child: V<'v>, index: usize, out: &mut Vec<Diagnostic>) {
        let Some((start, column)) = start_of(child) else {
            return;
        };
        match child.get("type").and_then(V::as_str) {
            Some("snippet_definition") if self.unused => {
                let name = child.get("name").and_then(V::as_str).unwrap_or("");
                if !self.seen.insert(name) || self.used.contains(name) {
                    return;
                }
                let pos = self.positions.pos(start + 1, start + 2 + name.len());
                let mut d =
                    Diagnostic::nth("unused-snippet", 0, &[("name", name)], None, Some(pos));
                d.path = Some(format!("/children/{index}"));
                out.push(d);
            }
            Some("paragraph") if self.ids && column == 1 => {
                let Some((from, end)) = id_marker(source.get(start..).unwrap_or("")) else {
                    return;
                };
                let pos = self.positions.pos(start + from, start + end);
                let mut d = Diagnostic::nth("snippet-definition-id", 0, &[], None, Some(pos));
                d.path = Some(format!("/children/{index}"));
                out.push(d);
            }
            _ => {}
        }
    }
}

pub(super) fn scan(source: &str, tree: V<'_>, opts: &LintOptions, out: &mut Vec<Diagnostic>) {
    // Both rules read a definition marker, which starts with `{$`.
    if !source.contains("{$") {
        return;
    }
    let unused = active("unused-snippet", opts);
    let ids = active("snippet-definition-id", opts);
    let Some(children) = tree.get("children").and_then(V::as_array) else {
        return;
    };
    let mut scan = Scan {
        positions: Source::new(source),
        used: HashSet::new(),
        seen: HashSet::new(),
        unused,
        ids,
    };
    if unused {
        used_names(tree, &mut scan.used);
    }
    for index in 0..children.len() {
        if let Some(child) = children.get(index) {
            scan.child(source, child, index, out);
        }
    }
}
