//! C2-B-215 and C2-B-216 suppression from parsed line comment nodes.

use super::parse_tree_value;
use crate::diagnostic::Diagnostic;
use serde_json::Value;
use std::collections::HashMap;

struct Control {
    line: u32,
    kind: &'static str,
    ids: Vec<String>,
}

pub(super) fn filter(source: &str, findings: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let tree = crate::tree_json(source, &crate::Options::default());
    let Some(tree) = parse_tree_value(&tree) else {
        return findings;
    };
    let mut controls = collect(source, &tree);
    controls.sort_by_key(|control| control.line);
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
        let in_line_scope = controls.iter().any(|control| {
            (control.kind == "disable-line" && control.line == line
                || control.kind == "disable-next-line" && control.line + 1 == line)
                && (control.ids.is_empty() || control.ids.iter().any(|id| id == finding.code))
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

fn collect(source: &str, root: &Value) -> Vec<Control> {
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(value) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(items),
            Value::Object(map) => {
                if value["type"] == "comment" && value["form"] == "line" {
                    if let Some(control) = parse(source, value) {
                        out.push(control);
                    }
                }
                pending.extend(map.values());
            }
            _ => {}
        }
    }
    out
}

fn parse(source: &str, comment: &Value) -> Option<Control> {
    let pos = comment.get("pos")?;
    let start = usize::try_from(pos["startOffset"].as_u64()?).ok()?;
    let end = usize::try_from(pos["endOffset"].as_u64()?).ok()?;
    let raw = source.get(start..end)?;
    let mark = raw.find("%%")?;
    if !raw.get(mark + 2..)?.starts_with(' ') || raw.get(mark + 3..)?.starts_with(' ') {
        return None;
    }
    let content = comment["value"].as_str()?;
    let rest = content.strip_prefix("carve-lint-")?;
    parse_kind(rest, pos)
}

fn parse_kind(rest: &str, pos: &Value) -> Option<Control> {
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
                line: u32::try_from(pos["startLine"].as_u64()?).ok()?,
                kind,
                ids,
            });
        }
    }
    None
}
