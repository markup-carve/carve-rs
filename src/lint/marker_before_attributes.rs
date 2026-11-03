//! `emphasis-marker-before-attributes` (carve#3992): a bare closer glued to an
//! attribute block reads two ways. Report the text reading, and the attaching
//! one when the span also holds a literal same marker. `*a*{.x}` is canonical.

use super::LintWarning;
use crate::ast::Document;
use serde_json::Value;

const MARKERS: [char; 5] = ['*', '/', '_', '~', '='];

fn span_name(marker: char) -> &'static str {
    match marker {
        '*' => "strong",
        '/' => "emphasis",
        '_' => "underline",
        '~' => "strikethrough",
        _ => "highlight",
    }
}

struct Source {
    cps: Vec<char>,
    line_starts: Vec<usize>,
}

impl Source {
    fn line_of(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&start| start <= offset) - 1
    }

    fn escaped(&self, i: usize) -> bool {
        self.cps[..i]
            .iter()
            .rev()
            .take_while(|&&c| c == '\\')
            .count()
            % 2
            == 1
    }

    /// End (exclusive) of a valid attribute block opening at `i`.
    fn block_end(&self, i: usize) -> Option<usize> {
        if self.cps.get(i) != Some(&'{') {
            return None;
        }
        let mut quote = None;
        let mut j = i + 1;
        while j < self.cps.len() {
            let ch = self.cps[j];
            if ch == '\\' {
                j += 2;
                continue;
            }
            if let Some(q) = quote {
                if ch == q {
                    quote = None;
                }
            } else if ch == '"' || ch == '\'' {
                quote = Some(ch);
            } else if ch == '{' {
                return None;
            } else if ch == '}' {
                let payload: String = self.cps[i + 1..j].iter().collect();
                return crate::parse::is_valid_attr_payload(&payload).then_some(j + 1);
            }
            j += 1;
        }
        None
    }
}

fn span(node: &Value) -> Option<(usize, usize)> {
    let pos = node.get("pos")?;
    if pos.get("file").is_some() {
        return None;
    }
    Some((
        pos.get("startOffset")?.as_u64()? as usize,
        pos.get("endOffset")?.as_u64()? as usize,
    ))
}

fn walk(value: &Value, visit: &mut dyn FnMut(&Value)) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| walk(item, visit)),
        Value::Object(node) => {
            visit(value);
            for (key, child) in node {
                if key != "pos" && key != "attrs" {
                    walk(child, visit);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn collect(
    source: &str,
    doc: &Document,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    if !MARKERS.iter().any(|m| source.contains(&format!("{m}{{"))) {
        return;
    }
    let Ok(json) = serde_json::from_str::<Value>(&crate::ast_json::to_json(doc)) else {
        return;
    };
    let cps: Vec<char> = source.chars().collect();
    let mut line_starts = vec![0];
    for (i, &ch) in cps.iter().enumerate() {
        if ch == '\n' || (ch == '\r' && cps.get(i + 1) != Some(&'\n')) {
            line_starts.push(i + 1);
        }
    }
    let src = Source { cps, line_starts };
    let mut report = |at: usize, end: usize, message: String| {
        let line = src.line_of(at);
        let start = to_byte(at);
        out.push(LintWarning {
            file: None,
            included_by: Vec::new(),
            line: line + 1,
            column: at - src.line_starts[line] + 1,
            rule: "emphasis-marker-before-attributes",
            message,
            start,
            end: to_byte(end).max(start),
        });
    };
    let mut visit = |node: &Value| {
        let Some((start, end)) = span(node) else {
            return;
        };
        let end = end.min(src.cps.len());
        if node.get("type").and_then(Value::as_str) == Some("text") {
            for i in start..end.saturating_sub(1) {
                let ch = src.cps[i];
                if !MARKERS.contains(&ch) || src.escaped(i) {
                    continue;
                }
                let Some(block) = src.block_end(i + 1).filter(|&b| b <= end) else {
                    continue;
                };
                report(i, block, format!(
                    "This {ch} closes no span, so it and the attribute block after it stay text. \
                     Brace the span you mean ({{{ch}text{ch}}}{{…}}) to attach the block, or escape the {ch} to keep both as text."
                ));
            }
            return;
        }
        let Some(&marker) = src.cps.get(start) else {
            return;
        };
        if node.get("attrs").is_none() || !MARKERS.contains(&marker) || src.escaped(start) {
            return;
        }
        let empty = Value::Array(Vec::new());
        let children = node.get("children").unwrap_or(&empty);
        let from = children
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(span)
            .fold(start + 1, |from, (_, child_end)| from.max(child_end));
        let Some(closer) = (from..end)
            .find(|&j| src.cps[j] == marker && !src.escaped(j) && src.cps.get(j + 1) == Some(&'{'))
        else {
            return;
        };
        // Only a marker the span holds as literal text counts, not one in code or a nested construct.
        let mut literal = false;
        walk(children, &mut |inner| {
            if literal || inner.get("type").and_then(Value::as_str) != Some("text") {
                return;
            }
            if let Some((a, b)) = span(inner) {
                literal = (a..b.min(closer)).any(|j| src.cps[j] == marker && !src.escaped(j));
            }
        });
        if !literal {
            return;
        }
        let opener = src.line_of(start);
        let block = src.block_end(closer + 1).unwrap_or(closer + 1);
        report(closer, block, format!(
            "This {marker} closes the {} span that opens at line {}, column {}, so the attribute block after it attaches to that whole span, which also holds a literal {marker}. \
             Brace the span you mean ({{{marker}text{marker}}}{{…}}) or escape the inner {marker} to make the pairing explicit.",
            span_name(marker),
            opener + 1,
            start - src.line_starts[opener] + 1,
        ));
    };
    if let Some(children) = json.get("children") {
        walk(children, &mut visit);
    }
    if let Some(Value::Object(defs)) = json.get("footnoteDefs") {
        for body in defs.values() {
            walk(body, &mut visit);
        }
    }
}
