use std::collections::HashMap;

use crate::ast::BlockNode;

fn indent(body: &str) -> usize {
    body.len() - body.trim_start_matches(' ').len()
}

fn blank(body: &str) -> bool {
    body.trim().is_empty()
}

struct Layout<'a> {
    lines: Vec<&'a str>,
    parts: Vec<(usize, &'a str)>,
    in_fence: Vec<bool>,
}

fn layout(carve: &str) -> Layout<'_> {
    let quote = cached_regex!(r"^(?:[ \t]*>[ \t]?)*").unwrap();
    let fence_line = cached_regex!(r"^[ \t]*(`{3,}|~{3,})(.*)$").unwrap();
    let lines: Vec<&str> = carve.split('\n').collect();
    let parts: Vec<(usize, &str)> = lines
        .iter()
        .map(|line| {
            let prefix = quote.find(line).map_or(0, |m| m.end());
            (line[..prefix].matches('>').count(), &line[prefix..])
        })
        .collect();
    let mut in_fence = vec![false; lines.len()];
    let mut fence: Option<(u8, usize, usize)> = None;
    for (index, &(depth, body)) in parts.iter().enumerate() {
        let run = fence_line.captures(body).map(|caps| {
            (
                caps[1].as_bytes()[0],
                caps[1].len(),
                caps[2].trim().is_empty(),
            )
        });
        // A fence ends with the quote that holds it.
        if fence.is_some_and(|(_, _, at)| depth < at) {
            fence = None;
        }
        if let Some((open, width, _)) = fence {
            in_fence[index] = true;
            if run.is_some_and(|(ch, len, bare)| ch == open && len >= width && bare) {
                fence = None;
            }
            continue;
        }
        fence = run.map(|(ch, len, _)| (ch, len, depth));
    }
    Layout {
        lines,
        parts,
        in_fence,
    }
}

fn strip_quotes(line: &str, depth: usize) -> Option<(&str, &str)> {
    let level = cached_regex!(r"^[ \t]*>[ \t]?").unwrap();
    let mut rest = line;
    for _ in 0..depth {
        rest = &rest[level.find(rest)?.end()..];
    }
    Some((&line[..line.len() - rest.len()], rest))
}

fn marker_kind(marker: &str) -> String {
    let marker = cached_regex!(r"[0-9]+").unwrap().replace(marker, "1");
    let marker = cached_regex!(r"[a-z]+").unwrap().replace(&marker, "a");
    cached_regex!(r"[A-Z]+")
        .unwrap()
        .replace(&marker, "A")
        .into_owned()
}

// A blank line just above the item that a sibling item stands above loosens the list anyway.
fn loose_before(parts: &[(usize, &str)], owner: usize, owner_indent: usize) -> bool {
    let depth = parts[owner].0;
    if owner == 0 || !blank(parts[owner - 1].1) {
        return false;
    }
    for &(line_depth, line) in parts[..owner - 1].iter().rev() {
        if line_depth < depth {
            return false;
        }
        if line_depth > depth || blank(line) {
            continue;
        }
        return indent(line) >= owner_indent;
    }
    false
}

struct SubListBlank {
    first: usize,
    last: usize,
    below: usize,
    end: usize,
}

// What the scan reads from a line's body at one quote depth.
#[derive(Clone)]
struct Facts {
    blank: bool,
    indent: usize,
    marker: Option<(usize, String)>,
    div: Option<bool>,
    plus: bool,
    comment: bool,
}

fn facts(body: &str) -> Facts {
    let marker = super::sub_list_marker()
        .captures(body)
        .map(|caps| (caps[0].len(), marker_kind(&caps[2])));
    let div = cached_regex!(r"^ *(:{3,})[ \t]*(\S?)")
        .unwrap()
        .captures(body)
        .map(|caps| caps[2].is_empty());
    let trimmed = body.trim();
    Facts {
        blank: trimmed.is_empty(),
        indent: indent(body),
        marker,
        div,
        plus: trimmed == "+",
        comment: trimmed.starts_with("%%"),
    }
}

// Facts at a line's own depth are computed once, so a scan per item stays linear in lines.
struct Lines<'a> {
    lines: &'a [&'a str],
    parts: &'a [(usize, &'a str)],
    own: Vec<Facts>,
}

impl Lines<'_> {
    fn at(&self, at: usize, depth: usize) -> Option<std::borrow::Cow<'_, Facts>> {
        let line_depth = self.parts[at].0;
        if line_depth < depth {
            return None;
        }
        if line_depth == depth {
            return Some(std::borrow::Cow::Borrowed(&self.own[at]));
        }
        strip_quotes(self.lines[at], depth).map(|(_, body)| std::borrow::Cow::Owned(facts(body)))
    }
}

/// djot.js charges a blank read while an item's sub-list is open to the sub-list,
/// so a block after it keeps the outer item tight (carve#5197). Carve would read
/// the blank as loosening the item, so it goes; a block that would then fold into
/// the sub-list's paragraph comes back behind a `+` at the marker column.
pub(super) fn drop_blanks_after_sub_lists(carve: String) -> String {
    let Layout {
        lines,
        parts,
        in_fence,
    } = layout(&carve);
    let scan = Lines {
        lines: &lines,
        parts: &parts,
        own: parts.iter().map(|(_, body)| facts(body)).collect(),
    };
    let mut replaced: HashMap<usize, Option<String>> = HashMap::new();
    let mut probes: HashMap<String, Option<usize>> = HashMap::new();
    let mut cached = |source: String| -> Option<usize> {
        *probes
            .entry(source)
            .or_insert_with_key(|source| single_item_children(source))
    };
    for owner in 0..lines.len() {
        if in_fence[owner] {
            continue;
        }
        let depth = parts[owner].0;
        let Some(column) = scan.own[owner].marker.as_ref().map(|(column, _)| *column) else {
            continue;
        };
        let marker_column = scan.own[owner].indent;
        let Some(blanks) = sub_list_blanks(&scan, &in_fence, owner, depth, column) else {
            continue;
        };
        let Some(last_run) = blanks.last() else {
            continue;
        };
        if loose_before(&parts, owner, marker_column)
            || super::separated_from_a_sibling(&parts, &in_fence, last_run.last, marker_column)
        {
            continue;
        }
        for run in &blanks {
            if (run.first..run.end).any(|at| replaced.contains_key(&at)) {
                continue;
            }
            edit(
                &lines,
                run,
                depth,
                marker_column,
                column,
                &mut cached,
                &mut replaced,
            );
        }
    }
    if replaced.is_empty() {
        return carve;
    }
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| match replaced.get(&index) {
            Some(value) => value.clone(),
            None => Some((*line).to_owned()),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// The item's blank runs that djot.js charges to an open sub-list, or None when
// another blank at the item's level loosens it anyway.
fn sub_list_blanks(
    scan: &Lines<'_>,
    in_fence: &[bool],
    owner: usize,
    depth: usize,
    column: usize,
) -> Option<Vec<SubListBlank>> {
    let mut runs: Vec<SubListBlank> = Vec::new();
    let mut nested: Option<(usize, String)> = None;
    let mut pending: Option<usize> = None;
    let mut previous_blank = false;
    // djot.js emits a blank line for a div's closing fence, so a line directly after one can loosen the list.
    let mut open_divs = 0usize;
    let mut div_closed = false;
    let count = scan.lines.len();
    for (at, &fenced) in in_fence.iter().enumerate().skip(owner + 1) {
        let Some(line) = scan.at(at, depth) else {
            break;
        };
        if fenced {
            pending = None;
            previous_blank = false;
            continue;
        }
        if line.blank {
            pending.get_or_insert(at);
            previous_blank = true;
            div_closed = false;
            continue;
        }
        let was_blank = previous_blank;
        previous_blank = false;
        let line_indent = line.indent;
        if line_indent < column {
            if div_closed || line.marker.is_none() && !was_blank {
                return None;
            }
            break;
        }
        // A new list takes the div's blank line; any other line is loosened by it.
        if div_closed && line.marker.is_none() {
            return None;
        }
        div_closed = false;
        if let Some(bare) = line.div {
            if line_indent == column {
                if bare && open_divs > 0 {
                    open_divs -= 1;
                    div_closed = open_divs == 0;
                } else {
                    open_divs += 1;
                }
            }
        }
        if line.plus {
            return None;
        }
        if nested
            .as_ref()
            .is_some_and(|(content, _)| line_indent >= *content)
        {
            pending = None;
            continue;
        }
        if let Some((content, kind)) = &line.marker {
            if let Some(first) = pending {
                let (_, nested_kind) = nested.as_ref()?;
                if kind != nested_kind {
                    runs.push(SubListBlank {
                        first,
                        last: at - 1,
                        below: at,
                        end: at,
                    });
                }
            }
            nested = Some((*content, kind.clone()));
            pending = None;
            continue;
        }
        if line_indent != column {
            return None;
        }
        if let Some(first) = pending {
            nested.as_ref()?;
            // A comment stands where the importer hoisted a definition and its blanks; keep the blank.
            if !line.comment {
                runs.push(SubListBlank {
                    first,
                    last: at - 1,
                    below: at,
                    end: at,
                });
            }
        }
        nested = None;
        pending = None;
    }
    for run in &mut runs {
        let mut end = run.below + 1;
        while end < count {
            if !in_fence[end] {
                match scan.at(end, depth) {
                    Some(line) if !line.blank && line.indent >= column => {}
                    _ => break,
                }
            }
            end += 1;
        }
        run.end = end;
    }
    Some(runs)
}

// The probe returns the child count of the single item `- x` holds, or None when
// the source is not that one item, so Carve's own rules decide the fold.
fn edit(
    lines: &[&str],
    run: &SubListBlank,
    depth: usize,
    marker_column: usize,
    column: usize,
    probe: &mut impl FnMut(String) -> Option<usize>,
    replaced: &mut HashMap<usize, Option<String>>,
) {
    let Some(block) = lines[run.below..run.end]
        .iter()
        .map(|line| strip_quotes(line, depth))
        .collect::<Option<Vec<_>>>()
    else {
        return;
    };
    let shifted: Vec<String> = block
        .iter()
        .map(|(_, body)| format!("  {}", &body[column.min(indent(body))..]))
        .collect();
    let shifted_text = shifted.join("\n");
    let Some(blocks) = probe(format!("- x\n\n{shifted_text}\n")) else {
        return;
    };
    let children = 1 + blocks;
    if probe(format!("- x\n  - y\n{shifted_text}\n")) == Some(children) {
        for at in run.first..=run.last {
            replaced.insert(at, None);
        }
        return;
    }
    // Only the first line moves to the marker column; the rest stay where they are.
    let mut moved = vec![shifted[0][2..].to_owned()];
    moved.extend(shifted[1..].iter().cloned());
    if probe(format!("- x\n  - y\n+\n{}\n", moved.join("\n"))) != Some(children) {
        return;
    }
    let (quote, body) = block[0];
    let pad = " ".repeat(marker_column);
    replaced.insert(run.first, Some(format!("{quote}{pad}+")));
    for at in run.first + 1..=run.last {
        replaced.insert(at, None);
    }
    replaced.insert(
        run.below,
        Some(format!("{quote}{pad}{}", &body[column.min(indent(body))..])),
    );
}

fn single_item_children(source: &str) -> Option<usize> {
    let document = crate::parse(source);
    match document.children.as_slice() {
        [BlockNode::List(list)] if list.items.len() == 1 => Some(list.items[0].children.len()),
        _ => None,
    }
}
