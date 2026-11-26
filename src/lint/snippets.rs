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

pub(super) fn scan(source: &str, tree: V<'_>, opts: &LintOptions, out: &mut Vec<Diagnostic>) {
    let unused = active("unused-snippet", opts);
    let ids = active("snippet-definition-id", opts) && source.contains("{$");
    if !unused && !ids {
        return;
    }
    let Some(children) = tree.get("children").and_then(V::as_array) else {
        return;
    };
    let positions = Source::new(source);
    let mut used = HashSet::new();
    if unused {
        used_names(tree, &mut used);
    }
    let mut seen = HashSet::new();
    for index in 0..children.len() {
        let Some(child) = children.get(index) else {
            continue;
        };
        let Some((start, column)) = start_of(child) else {
            continue;
        };
        match child.get("type").and_then(V::as_str) {
            Some("snippet_definition") if unused => {
                let name = child.get("name").and_then(V::as_str).unwrap_or("");
                if !seen.insert(name) || used.contains(name) {
                    continue;
                }
                let pos = positions.pos(start + 1, start + 2 + name.len());
                let mut d = Diagnostic::nth("unused-snippet", 0, &[("name", name)], None, Some(pos));
                d.path = Some(format!("/children/{index}"));
                out.push(d);
            }
            Some("paragraph") if ids && column == 1 => {
                let line = source.get(start..).unwrap_or("");
                let line = line.split('\n').next().unwrap_or("");
                if !line.starts_with("{$") {
                    continue;
                }
                let Some(m) = crate::snippet::marker(line) else {
                    continue;
                };
                let value = line.get(m.end..).unwrap_or("");
                if !m.has_id || !value.starts_with(' ') || value.trim().is_empty() {
                    continue;
                }
                let pos = positions.pos(start, start + m.end);
                let mut d = Diagnostic::nth("snippet-definition-id", 0, &[], None, Some(pos));
                d.path = Some(format!("/children/{index}"));
                out.push(d);
            }
            _ => {}
        }
    }
}
