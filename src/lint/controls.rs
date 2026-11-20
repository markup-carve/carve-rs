//! C2-B-215 and C2-B-216 suppression from parsed line comment nodes.

use super::V;
use crate::diagnostic::Diagnostic;
use std::collections::HashMap;

struct Control {
    line: u32,
    kind: &'static str,
    ids: Vec<String>,
}

pub(super) fn filter(source: &str, findings: Vec<Diagnostic>, tree: V<'_>) -> Vec<Diagnostic> {
    let mut controls = collect(source, tree);
    controls.sort_by_key(|control| control.line);
    let mut line_scoped: HashMap<u32, Vec<&Control>> = HashMap::new();
    for control in &controls {
        let target = match control.kind {
            "disable-line" => control.line,
            "disable-next-line" => control.line + 1,
            _ => continue,
        };
        line_scoped.entry(target).or_default().push(control);
    }
    let mut state = HashMap::new();
    let mut index = 0;
    let mut output = Vec::new();
    for finding in findings {
        let Some(line) = finding.pos.map(|pos| pos.start_line) else {
            output.push(finding);
            continue;
        };
        while let Some(control) = controls.get(index).filter(|control| control.line <= line) {
            update_state(&mut state, control);
            index += 1;
        }
        let in_line_scope = line_scoped.get(&line).is_some_and(|scoped| {
            scoped.iter().any(|control| {
                control.ids.is_empty() || control.ids.iter().any(|id| id == finding.code)
            })
        });
        if !in_line_scope
            && !state
                .get(finding.code)
                .or_else(|| state.get("*"))
                .copied()
                .unwrap_or(false)
        {
            output.push(finding);
        }
    }
    output
}

fn update_state(state: &mut HashMap<String, bool>, control: &Control) {
    if !matches!(control.kind, "disable" | "enable") {
        return;
    }
    if control.ids.is_empty() {
        state.clear();
        state.insert(String::from("*"), control.kind == "disable");
    } else {
        for id in &control.ids {
            state.insert(id.clone(), control.kind == "disable");
        }
    }
}

fn collect(source: &str, root: V<'_>) -> Vec<Control> {
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(value) = pending.pop() {
        if let Some(items) = value.as_array() {
            pending.extend(items);
        } else if value.is_object() {
            if value.ty() == "comment" && value.at("form") == "line" {
                if let Some(control) = parse(source, value) {
                    out.push(control);
                }
            }
            value.each(&mut |_, child| pending.push(child));
        }
    }
    out
}

fn parse(source: &str, comment: V<'_>) -> Option<Control> {
    let pos = comment.pos()?;
    let start = usize::try_from(pos.at("startOffset").as_u64()?).ok()?;
    let end = usize::try_from(pos.at("endOffset").as_u64()?).ok()?;
    let raw = source.get(start..end)?;
    let mark = raw.find("%%")?;
    if !raw.get(mark + 2..)?.starts_with(' ') || raw.get(mark + 3..)?.starts_with(' ') {
        return None;
    }
    let content = comment.at("value").as_str()?;
    let rest = content.strip_prefix("carve-lint-")?;
    parse_kind(rest, pos)
}

fn parse_kind(rest: &str, pos: V<'_>) -> Option<Control> {
    for kind in ["disable-next-line", "disable-line", "disable", "enable"] {
        if let Some(tail) = rest.strip_prefix(kind) {
            if !tail.is_empty() && !tail.starts_with([' ', ',']) {
                return None;
            }
            let ids = tail
                .split([' ', ','])
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect();
            return Some(Control {
                line: u32::try_from(pos.at("startLine").as_u64()?).ok()?,
                kind,
                ids,
            });
        }
    }
    None
}
