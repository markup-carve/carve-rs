//! C2-W-144: a next-line control comment that suppressed a line of the source
//! is written directly above the next line, and one that suppressed nothing
//! keeps a blank line below it, where the tree reads back the same.

use super::check::parse_written;
use super::node::trees_equal;
use super::state::Writer;
use serde_json::Value;
use std::collections::HashSet;

const KEYWORD: &str = "carve-lint-disable-next-line";
const CONTROL: &str = "%% carve-lint-disable-next-line";
/// The parse work of the check, as a multiple of the document length, so that it stays linear.
const BUDGET: usize = 16;

/// `written` with its blank lines below control comments settled; `original`
/// is the source of the tree, when the writer has it.
pub fn keep_above(w: &Writer<'_>, written: String, original: Option<&str>) -> String {
    if !written.contains(CONTROL) {
        return written;
    }
    let tree = parse_written(w, &written);
    let live: Vec<bool> = match original {
        Some(src) => {
            let parsed = parse_written(w, src);
            let lines: Vec<&str> = src.split('\n').collect();
            controls(&parsed)
                .iter()
                .map(|c| suppresses_below(c, &lines))
                .collect()
        }
        None => vec![true; controls(&tree).len()],
    };
    let dropped = drop_blanks(w, written, &tree, &live);
    restore_blanks(w, dropped, &tree, &live)
}

/// Leaves out the blank lines below each control comment of `live` that suppressed a line of the source.
fn drop_blanks(w: &Writer<'_>, written: String, tree: &Value, live: &[bool]) -> String {
    let lines: Vec<&str> = written.split('\n').collect();
    let below: HashSet<usize> = controls(tree)
        .iter()
        .enumerate()
        .filter(|(i, _)| live.get(*i).copied().unwrap_or(false))
        .filter_map(|(_, c)| end_line(c))
        .collect();
    let spans: Vec<(usize, usize)> = blanks_below_controls(&lines)
        .into_iter()
        .filter(|(from, _)| below.contains(from))
        .collect();
    if spans.is_empty() {
        return written;
    }
    let mut budget = BUDGET.saturating_mul(written.len());
    let all = without(&lines, &spans);
    budget = budget.saturating_sub(all.len());
    if same_controls(w, &all, tree, live) {
        return all;
    }
    let mut taken: Vec<(usize, usize)> = Vec::new();
    for span in spans {
        if budget == 0 {
            break;
        }
        let mut trial = taken.clone();
        trial.push(span);
        let candidate = without(&lines, &trial);
        budget = budget.saturating_sub(candidate.len());
        if same_controls(w, &candidate, tree, live) {
            taken = trial;
        }
    }
    without(&lines, &taken)
}

/// Puts a blank line below each control comment that suppressed nothing in the source and would now.
fn restore_blanks(w: &Writer<'_>, written: String, tree: &Value, live: &[bool]) -> String {
    if live.iter().all(|l| *l) {
        return written;
    }
    let lines: Vec<&str> = written.split('\n').collect();
    let parsed = parse_written(w, &written);
    let at: HashSet<usize> = controls(&parsed)
        .iter()
        .enumerate()
        .filter(|(i, c)| !live.get(*i).copied().unwrap_or(true) && suppresses_below(c, &lines))
        .filter_map(|(_, c)| end_line(c))
        .collect();
    if at.is_empty() {
        return written;
    }
    let mut out: Vec<&str> = Vec::with_capacity(lines.len() + at.len());
    let mut previous = "";
    for (index, line) in lines.iter().enumerate() {
        if at.contains(&index) {
            // The blank line keeps the quote marks of the comment line, so that it stays inside the quote.
            out.push(quote_marks(previous));
        }
        out.push(line);
        previous = line;
    }
    let candidate = out.join("\n");
    if same_controls(w, &candidate, tree, live) {
        candidate
    } else {
        written
    }
}

/// Whether `candidate` reads back as `tree` and gives no control comment a line it did not suppress.
fn same_controls(w: &Writer<'_>, candidate: &str, tree: &Value, live: &[bool]) -> bool {
    let parsed = parse_written(w, candidate);
    let (Some(a), Some(b)) = (
        parsed.get("children").and_then(Value::as_array),
        tree.get("children").and_then(Value::as_array),
    ) else {
        return false;
    };
    if a.len() != b.len() || !a.iter().zip(b).all(|(x, y)| trees_equal(x, y)) {
        return false;
    }
    let lines: Vec<&str> = candidate.split('\n').collect();
    let found = controls(&parsed);
    found.len() == live.len()
        && found
            .iter()
            .zip(live)
            .all(|(c, l)| *l || !suppresses_below(c, &lines))
}

/// The runs of blank lines, as line ranges, that stand directly below a next-line control comment.
fn blanks_below_controls(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut at = 0;
    while at + 1 < lines.len() {
        let below = lines.get(at + 1).copied().unwrap_or("");
        if !lines.get(at).copied().is_some_and(is_control_line) || !is_blank(below) {
            at += 1;
            continue;
        }
        let mut end = at + 1;
        while lines.get(end).copied().is_some_and(is_blank) {
            end += 1;
        }
        if end + 1 < lines.len() {
            spans.push((at + 1, end));
        }
        at = end;
    }
    spans
}

fn without(lines: &[&str], spans: &[(usize, usize)]) -> String {
    let dropped: HashSet<usize> = spans.iter().flat_map(|&(from, to)| from..to).collect();
    lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !dropped.contains(i))
        .map(|(_, l)| *l)
        .collect::<Vec<_>>()
        .join("\n")
}

/// C2-B-215: the keyword ends the comment or is followed by a space or a comma.
fn is_control_line(line: &str) -> bool {
    line.match_indices(CONTROL).any(|(i, _)| {
        matches!(
            line[i + CONTROL.len()..].chars().next(),
            None | Some(' ' | ',')
        )
    })
}

/// Blank inside its containers: only spaces, tabs and quote marks, and the CR of a CRLF line ending.
fn is_blank(line: &str) -> bool {
    line.strip_suffix('\r')
        .unwrap_or(line)
        .chars()
        .all(|c| matches!(c, ' ' | '\t' | '>'))
}

/// C2-B-215: the keyword starts the comment, and the comment ends or a space or a comma follows it.
fn is_control_value(value: &str) -> bool {
    value
        .strip_prefix(KEYWORD)
        .is_some_and(|rest| matches!(rest.chars().next(), None | Some(' ' | ',')))
}

fn quote_marks(line: &str) -> &str {
    let mut end = 0;
    for (i, c) in line.char_indices() {
        match c {
            '>' => end = i + 1,
            ' ' | '\t' => {}
            _ => break,
        }
    }
    &line[..end]
}

/// The next-line control comments of a tree, in a fixed order of its nodes.
fn controls(tree: &Value) -> Vec<&Value> {
    let mut found = Vec::new();
    collect(tree, &mut found);
    found
}

fn collect<'v>(v: &'v Value, found: &mut Vec<&'v Value>) {
    match v {
        Value::Array(items) => items.iter().for_each(|x| collect(x, found)),
        Value::Object(m) => {
            let is_control = m.get("type").and_then(Value::as_str) == Some("comment")
                && m.get("form").and_then(Value::as_str) == Some("line")
                && m.get("value")
                    .and_then(Value::as_str)
                    .is_some_and(is_control_value);
            if is_control {
                found.push(v);
            }
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            for key in keys {
                if key != "pos" {
                    collect(&m[key], found);
                }
            }
        }
        _ => {}
    }
}

fn end_line(comment: &Value) -> Option<usize> {
    let n = comment.get("pos")?.get("endLine")?.as_u64()?;
    usize::try_from(n).ok()
}

/// Whether the line below `comment` holds content, so that the comment suppresses findings on it.
fn suppresses_below(comment: &Value, lines: &[&str]) -> bool {
    end_line(comment).is_none_or(|end| lines.get(end).is_some_and(|l| !is_blank(l)))
}
