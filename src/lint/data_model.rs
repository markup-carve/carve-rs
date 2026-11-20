//! Source and tree records consumed by the pinned 0.2 lint model.

use super::{
    Range,
    data_rules::{Record, Records},
    lines,
};
use crate::source::Source;
use serde_json::{Value, json};
use std::collections::HashMap;

fn parent(path: &str) -> &str {
    path.rsplit_once("/children/").map_or("", |(p, _)| p)
}

fn text_at<'a>(source: &'a str, from: usize, to: usize) -> &'a str {
    source
        .get(from.min(source.len())..to.min(source.len()))
        .unwrap_or("")
}

fn trim_end(text: &str) -> &str {
    text.trim_end_matches([' ', '\t'])
}

fn quoted_start(text: &str, marks: usize) -> usize {
    let bytes = text.as_bytes();
    let mut at = 0;
    while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
        at += 1;
    }
    for _ in 0..marks {
        if bytes.get(at) != Some(&b'>') || !bytes.get(at + 1).is_none_or(|b| *b == b' ') {
            break;
        }
        at += 1;
        if bytes.get(at) == Some(&b' ') {
            at += 1;
        }
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
    }
    at
}

fn quote_marks(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut marks = 0;
    loop {
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'>')
            || !bytes.get(at + 1).is_none_or(|b| *b == b' ' || *b == b'\t')
        {
            break;
        }
        marks += 1;
        at += 1;
    }
    marks
}

fn quote_marks_space(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut at = 0;
    let mut marks = 0;
    loop {
        while bytes.get(at).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            at += 1;
        }
        if bytes.get(at) != Some(&b'>') || bytes.get(at + 1) != Some(&b' ') {
            break;
        }
        marks += 1;
        at += 1;
    }
    marks
}

fn shape(body: &str) -> &'static str {
    if body.is_empty() {
        return "blank";
    }
    let first = body.as_bytes()[0];
    if matches!(first, b'#' | b'=')
        && body.as_bytes().iter().take_while(|b| **b == first).count() <= 6
        && body.as_bytes().iter().take_while(|b| **b == first).count() > 0
        && body
            .as_bytes()
            .get(body.as_bytes().iter().take_while(|b| **b == first).count())
            == Some(&b' ')
    {
        return "heading";
    }
    if first == b'>' && (body.len() == 1 || body.as_bytes().get(1) == Some(&b' ')) {
        return "quote";
    }
    if body.starts_with("%%%") {
        return "comment_block_opener";
    }
    if body.starts_with("%%") {
        return "comment_only";
    }
    if body.starts_with("^ ") {
        return "caption";
    }
    if body.starts_with("{{") && body.ends_with("}}") {
        return "include";
    }
    if body.starts_with('{') && body.ends_with('}') {
        return "attribute_line";
    }
    if body.starts_with(":::") {
        if body
            .trim_end_matches([' ', '\t'])
            .bytes()
            .all(|b| b == b':')
        {
            return "div_bare_run";
        }
        return "div_opener";
    }
    if body.starts_with(":: ") {
        return "term";
    }
    if body.starts_with(": ") {
        return "description";
    }
    if body.starts_with('|') && body.ends_with('|') {
        return "table_row";
    }
    if body.starts_with("```") {
        return "code_fence_opener";
    }
    if body.starts_with("---") || body.starts_with("***") || body.starts_with("___") {
        return "thematic_break";
    }
    if body.starts_with("[^") && body.contains("]:") {
        return "footnote_definition";
    }
    if body.starts_with('[') && body.contains("]:") {
        return "link_definition";
    }
    if matches!(first, b'-' | b'*') && body.get(1..2) == Some(" ") {
        return "bullet";
    }
    if first.is_ascii_alphanumeric()
        && body
            .find(['.', ')'])
            .is_some_and(|at| body.get(at + 1..at + 2) == Some(" "))
    {
        return "ordered";
    }
    "text"
}

fn column_of(source: &Source<'_>, at: usize, quotes: usize) -> usize {
    let (start, _) = source.line_range(source.line_of(at));
    let prefix = source.slice(start, at);
    let mut chars = prefix.chars().peekable();
    let mut column: usize = 0;
    let mut origin: usize = 0;
    for _ in 0..quotes {
        while let Some(ch) = chars.peek().copied() {
            if ch != ' ' && ch != '\t' {
                break;
            }
            column = if ch == '\t' {
                column + 4 - column % 4
            } else {
                column + 1
            };
            chars.next();
        }
        if chars.peek() != Some(&'>') {
            break;
        }
        chars.next();
        column += 1;
        if chars.peek() == Some(&' ') {
            chars.next();
            column += 1;
        }
        origin = column;
    }
    for ch in chars {
        column = if ch == '\t' {
            column + 4 - column % 4
        } else if ch == '\u{feff}' && column == 0 {
            column
        } else {
            column + 1
        };
    }
    column.saturating_sub(origin)
}

fn owner_record(
    path: &str,
    types: &HashMap<String, String>,
    by_path: &HashMap<&str, &Range>,
    source: &Source<'_>,
) -> Value {
    let kind = types.get(path).map_or("document", String::as_str);
    let range = by_path.get(path).copied();
    let mut quotes = 0;
    let mut holder = parent(path);
    while !holder.is_empty() {
        quotes += usize::from(types.get(holder).is_some_and(|kind| kind == "block_quote"));
        holder = parent(holder);
    }
    let marker_column = range.map(|range| column_of(source, range.start, quotes));
    let content_column = range.map_or(0, |range| {
        if matches!(kind, "block_quote" | "document") {
            return 0;
        }
        if matches!(kind, "list_item" | "footnote" | "definition_description") {
            let first = source.line(range.start_line.saturating_sub(1));
            let marker = range
                .start
                .saturating_sub(source.line_range(range.start_line.saturating_sub(1)).0);
            let suffix = first.get(marker..).unwrap_or("");
            let marker_len = match kind {
                "definition_description" => 1,
                "footnote" => suffix.find(':').map_or(0, |i| i + 1),
                _ if suffix.starts_with(['-', '*', '.']) => 1,
                _ => suffix.find(['.', ')']).map_or(0, |i| i + 1),
            };
            let mut at = range.start + marker_len;
            while source
                .text()
                .as_bytes()
                .get(at)
                .is_some_and(|b| matches!(b, b' ' | b'\t'))
            {
                at += 1;
            }
            if kind == "list_item" && source.text().get(at..).is_some_and(|s| s.starts_with("{.")) {
                if let Some(close) = source.text().get(at..).and_then(|s| s.find('}')) {
                    at += close + 1;
                }
                while source
                    .text()
                    .as_bytes()
                    .get(at)
                    .is_some_and(|b| matches!(b, b' ' | b'\t'))
                {
                    at += 1;
                }
            }
            column_of(source, at, quotes)
        } else {
            column_of(source, range.start, quotes)
        }
    });
    json!({"type":kind,"content_column":content_column,"marker_column":marker_column})
}

fn owner_path<'a>(mut path: &'a str, types: &HashMap<String, String>) -> &'a str {
    while types
        .get(path)
        .is_some_and(|kind| kind == "definition_list")
    {
        path = parent(path)
    }
    path
}

fn item_owner_path<'a>(mut path: &'a str, types: &HashMap<String, String>) -> &'a str {
    while types.get(path).is_some_and(|kind| {
        matches!(
            kind.as_str(),
            "admonition" | "directive" | "div" | "figure_group"
        )
    }) {
        path = parent(path)
    }
    if types.get(path).is_some_and(|kind| kind == "list_item") {
        path
    } else {
        ""
    }
}

fn opener_start(source: &str, mut at: usize) -> usize {
    while source.get(at..).is_some_and(|s| s.starts_with('{')) {
        let Some(end) = source.get(at..).and_then(|s| s.find('\n')) else {
            break;
        };
        at += end + 1;
        while source
            .as_bytes()
            .get(at)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'>'))
        {
            at += 1;
        }
    }
    at
}

struct Colon<'a> {
    from: usize,
    end: usize,
    closed: bool,
    owner: &'a str,
    entry: Value,
}

fn boundary_at<'a>(at: usize, by_line: &[Vec<&'a Range>], positions: &Source<'_>) -> &'a str {
    by_line
        .get(positions.line_of(at))
        .into_iter()
        .flatten()
        .filter(|range| range.start <= at && at < range.end)
        .max_by_key(|range| range.path.len())
        .map_or("", |range| range.path.as_str())
}

fn colon_layouts<'a>(
    source: &str,
    ranges: &'a [Range],
    boundaries: &[Vec<&'a Range>],
    positions: &Source<'_>,
) -> Vec<Colon<'a>> {
    let mut colons = Vec::new();
    for range in ranges {
        if !matches!(
            range.kind.as_str(),
            "div" | "directive" | "admonition" | "figure_group" | "line_block"
        ) {
            continue;
        }
        let from = opener_start(source, range.start);
        let width = source
            .as_bytes()
            .get(from..)
            .unwrap_or(&[])
            .iter()
            .take_while(|b| **b == b':')
            .count();
        if width < 3 {
            continue;
        }
        let kind = match range.kind.as_str() {
            "admonition" => "admonition",
            "figure_group" => "figure group",
            "line_block" => "line block",
            _ => {
                if source
                    .get(from + width..)
                    .is_some_and(|s| s.starts_with(" \\"))
                {
                    "hard-break block"
                } else {
                    "div"
                }
            }
        };
        let (line, column) = positions.line_col(from);
        let final_line = positions.line(positions.line_of(range.end.saturating_sub(1)));
        let closer = final_line
            .trim_start_matches([' ', '\t', '>'])
            .trim_end_matches([' ', '\t']);
        colons.push(Colon {
            from,
            end: range.end,
            closed: closer.len() >= width && closer.bytes().all(|byte| byte == b':'),
            owner: boundary_at(from, boundaries, positions),
            entry: json!({"width":width,"line":line,"column":column,"kind":kind,"start":from}),
        });
    }
    colons.sort_by_key(|colon| colon.from);
    colons
}

pub(super) fn records(source: &str, ranges: &[Range]) -> Records {
    let clock = std::time::Instant::now();
    let positions = Source::new(source);
    let raw_lines = lines::data_lines(source, ranges);
    let lines_elapsed = clock.elapsed();
    let mut types = HashMap::new();
    let mut by_path = HashMap::new();
    for range in ranges {
        types.insert(range.path.to_string(), range.kind.clone());
        by_path.insert(range.path.as_str(), range);
    }
    let mut leaf_by_line: Vec<Option<&Range>> = vec![None; raw_lines.len()];
    let mut attached_by_line = vec![false; raw_lines.len()];
    let mut boundaries = vec![Vec::new(); raw_lines.len()];
    for range in ranges {
        if matches!(
            range.kind.as_str(),
            "block_quote" | "list_item" | "footnote" | "definition_description"
        ) {
            let first = range.start_line.saturating_sub(1);
            for owners in boundaries
                .get_mut(first..range.end_line.min(raw_lines.len()))
                .into_iter()
                .flatten()
            {
                owners.push(range);
            }
        }
        if range.has_attrs {
            if let Some(attached) = attached_by_line.get_mut(range.start_line.saturating_sub(1)) {
                *attached = true;
            }
        }
        if !matches!(
            range.kind.as_str(),
            "paragraph"
                | "caption"
                | "definition_term"
                | "heading"
                | "line"
                | "table_cell"
                | "table_row"
                | "code_block"
                | "raw_block"
                | "frontmatter"
                | "comment"
        ) || (range.kind == "comment" && !range.form_block)
        {
            continue;
        }
        let first = range.start_line.saturating_sub(1);
        for place in leaf_by_line
            .get_mut(first..range.end_line.min(raw_lines.len()))
            .into_iter()
            .flatten()
        {
            if place.is_none_or(|known| known.path.len() <= range.path.len()) {
                *place = Some(range);
            }
        }
    }
    let colons = colon_layouts(source, ranges, &boundaries, &positions);
    let mut next_colon = 0;
    let mut active_colons: Vec<&Colon<'_>> = Vec::new();
    let mut base = Vec::with_capacity(raw_lines.len());
    for (index, line) in raw_lines.iter().enumerate() {
        let number = index + 1;
        let end = line.start + line.text.len();
        let leaf = leaf_by_line[index];
        let leaf_kind = leaf.map_or("none", |range| {
            if range.kind == "comment" && range.form_block {
                "comment_block"
            } else {
                range.kind.as_str()
            }
        });
        let leaf_id = leaf.map_or("", |range| range.path.as_str());
        let line_text = if matches!(
            leaf_kind,
            "code_block" | "raw_block" | "frontmatter" | "comment_block"
        ) {
            line.text.as_str()
        } else {
            line.masked.as_str()
        };
        let mut quote_depth = 0;
        let mut holder = parent(leaf_id);
        while !holder.is_empty() {
            quote_depth += usize::from(types.get(holder).is_some_and(|kind| kind == "block_quote"));
            holder = parent(holder);
        }
        let body_start = line.content.unwrap_or_else(|| {
            line.start
                + quoted_start(
                    line_text,
                    if matches!(
                        leaf_kind,
                        "code_block" | "raw_block" | "frontmatter" | "comment_block"
                    ) {
                        quote_depth
                    } else {
                        usize::MAX
                    },
                )
        });
        let from = body_start.saturating_sub(line.start).min(line_text.len());
        let body = trim_end(line_text.get(from..).unwrap_or(""));
        let mut ancestors = Vec::new();
        let mut ancestor = parent(leaf_id);
        while !ancestor.is_empty() {
            if let Some(kind) = types.get(ancestor) {
                ancestors.push(kind.clone());
            }
            ancestor = parent(ancestor);
        }
        let marks = quote_marks(line_text);
        let can_be_lazy = matches!(
            leaf_kind,
            "paragraph" | "caption" | "definition_term" | "heading" | "line" | "table_cell"
        ) && leaf.is_some_and(|range| range.start_line != number);
        let carried_marks = quote_marks_space(line_text);
        let mut lazy = can_be_lazy && carried_marks < quote_depth;
        if can_be_lazy && !lazy {
            let mut holder = parent(leaf_id);
            while !holder.is_empty() {
                if types.get(holder).is_some_and(|kind| {
                    matches!(
                        kind.as_str(),
                        "list_item" | "footnote" | "definition_description"
                    )
                }) {
                    let owner = owner_record(holder, &types, &by_path, &positions);
                    lazy = column_of(&positions, body_start, carried_marks)
                        < owner["content_column"].as_u64().unwrap_or(0) as usize;
                    break;
                }
                holder = parent(holder);
            }
        }
        while colons
            .get(next_colon)
            .is_some_and(|colon| colon.from < body_start)
        {
            active_colons.push(&colons[next_colon]);
            next_colon += 1;
        }
        active_colons.retain(|colon| {
            if colon.closed && colon.end < line.start {
                return false;
            }
            if colon.end >= line.start.saturating_sub(1) {
                return true;
            }
            source.get(colon.end..line.start).is_some_and(|gap| {
                gap.bytes()
                    .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b'>'))
            })
        });
        let colon: Vec<Value> = active_colons
            .iter()
            .rev()
            .filter(|entry| entry.owner == boundary_at(body_start, &boundaries, &positions))
            .map(|entry| entry.entry.clone())
            .collect();
        let closes = body.len() >= 3
            && body.bytes().all(|byte| byte == b':')
            && colon
                .first()
                .is_some_and(|entry| body.len() >= entry["width"].as_u64().unwrap_or(0) as usize);
        base.push(json!({
            "number": number,"start":line.start,"end":end,"text":line_text,"body":body,
            "body_start":body_start,"body_end":body_start + body.len(),"blank":leaf.is_none() && body.is_empty(),
            "leaf":leaf_kind,"leaf_id":leaf_id,"leaf_first":leaf.is_some_and(|range|range.start_line==number),
            "leaf_head_index":leaf.map_or(0,|range|range.start_line),"leaf_last_index":leaf.map_or(0,|range|range.end_line),
            "leaf_start":leaf.map_or(0,|range|range.start),"ancestors":ancestors,"shape":shape(body),
            "quote_prefix":text_at(source,line.start,body_start).contains('>'),"quote_depth":quote_depth,
            "quote_marks":marks,"raw_body":trim_end(text_at(source,body_start,end)),"lazy":lazy,
            "attached_here":attached_by_line[index],
            "closes_container":closes,"open_colon":colon
        }));
    }
    let base_elapsed = clock.elapsed();
    let mut records = Vec::new();
    for (index, line) in raw_lines.iter().enumerate() {
        records.push(Record {
            value: Value::Null,
            path: line.path.clone(),
            line: base[index]["text"].as_str().unwrap_or("").to_owned(),
            line_start: line.start,
            line_index: index,
            over: "line",
        });
    }
    let records_elapsed = clock.elapsed();
    for range in ranges {
        let at = range.start_line.saturating_sub(1);
        let Some(line) = raw_lines.get(at) else {
            continue;
        };
        let is_inline = (range.kind == "comment" && !range.form_block)
            || matches!(
                range.kind.as_str(),
                "text"
                    | "image"
                    | "emphasis"
                    | "strong"
                    | "highlight"
                    | "strike"
                    | "link"
                    | "raw_inline"
                    | "code"
                    | "span"
                    | "subscript"
                    | "superscript"
                    | "autolink"
                    | "reference"
                    | "footnote_ref"
                    | "citation"
                    | "wikilink"
            );
        if is_inline {
            let mut ancestor = parent(range.path.as_str());
            let mut leaf = "";
            while !ancestor.is_empty() {
                if let Some(kind) = types.get(ancestor) {
                    if matches!(
                        kind.as_str(),
                        "paragraph"
                            | "caption"
                            | "definition_term"
                            | "heading"
                            | "line"
                            | "table_cell"
                    ) {
                        leaf = kind;
                        break;
                    }
                }
                ancestor = parent(ancestor);
            }
            if leaf.is_empty() {
                continue;
            }
            let inline = json!({"type":range.kind,"start":range.start,"end":range.end,
                "text":text_at(source,range.start,range.end),"leaf":leaf,
                "parent":types.get(parent(range.path.as_str())).map_or("document",String::as_str)});
            records.push(Record {
                value: inline,
                path: range.path.to_string(),
                line: base[at]["text"].as_str().unwrap_or("").to_owned(),
                line_start: line.start,
                line_index: at,
                over: "inline",
            });
            continue;
        }
        if matches!(
            range.kind.as_str(),
            "soft_break" | "hard_break" | "line_break"
        ) {
            continue;
        }
        let parent_path = parent(range.path.as_str());
        let parent_type = types.get(parent_path).map_or("document", String::as_str);
        let owner = owner_path(parent_path, &types);
        let item_owner = item_owner_path(owner, &types);
        let kind = range.kind.as_str();
        let item_child = parent_type == "list_item" || item_owner != "";
        let opener_rule = matches!(
            kind,
            "admonition"
                | "block_quote"
                | "code_block"
                | "definition_term"
                | "directive"
                | "div"
                | "figure_group"
                | "heading"
                | "line_block"
                | "raw_block"
                | "table"
                | "thematic_break"
        );
        let opener = if range.has_attrs {
            opener_start(source, range.start)
        } else {
            range.start
        };
        let last_index = range.end_line.saturating_sub(1).min(base.len() - 1);
        let index = range
            .path
            .rsplit('/')
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        let previous = if kind == "paragraph" && index > 0 {
            types
                .get(&format!("{}/children/{}", parent_path, index - 1))
                .cloned()
        } else {
            None
        };
        let mut block_quotes = 0;
        let mut holder = parent(range.path.as_str());
        while !holder.is_empty() {
            block_quotes +=
                usize::from(types.get(holder).is_some_and(|kind| kind == "block_quote"));
            holder = parent(holder);
        }
        let block_column = if item_child {
            column_of(&positions, range.start, block_quotes)
        } else {
            0
        };
        let mark_column = if opener_rule || item_child {
            column_of(&positions, opener, block_quotes)
        } else {
            0
        };
        let opener_text = if opener_rule || matches!(kind, "directive" | "list") {
            let end = source
                .get(opener..)
                .and_then(|s| s.find('\n'))
                .map_or(source.len(), |n| opener + n);
            trim_end(text_at(source, opener, end))
        } else {
            ""
        };
        let block_text = if matches!(kind, "line" | "list" | "frontmatter") {
            text_at(source, range.start, range.end)
        } else {
            ""
        };
        let parent_value = if item_child || matches!(kind, "paragraph" | "directive") {
            owner_record(parent_path, &types, &by_path, &positions)
        } else {
            Value::Null
        };
        let owner_value = if opener_rule {
            owner_record(owner, &types, &by_path, &positions)
        } else {
            Value::Null
        };
        let item_owner_value = if opener_rule || item_child {
            owner_record(item_owner, &types, &by_path, &positions)
        } else {
            Value::Null
        };
        let block = json!({"type":range.kind,"path":range.path.as_str(),"start":range.start,"end":range.end,
            "text":block_text,"column":block_column,
            "opener_column":mark_column,"mark_column":mark_column,
            "opener_start":opener,"opener_text":opener_text,
            "after_quote_mark":text_at(source,line.start,range.start).ends_with("> "),"attributed":range.has_attrs,
            "image_line":range.image_line,
            "index":index,"line_index":at,"last_line_index":last_index,"prev_type":previous,
            "owner":owner_value,"item_owner":item_owner_value,"parent":parent_value});
        records.push(Record {
            value: block,
            path: range.path.to_string(),
            line: base[at]["text"].as_str().unwrap_or("").to_owned(),
            line_start: line.start,
            line_index: at,
            over: "block",
        });
    }
    if std::env::var("CARVE_LINT_PROFILE").is_ok() {
        eprintln!(
            "model phases {:?} {:?} {:?} {:?}",
            lines_elapsed,
            base_elapsed - lines_elapsed,
            records_elapsed - base_elapsed,
            clock.elapsed() - records_elapsed
        );
    }
    Records {
        lines: base,
        items: records,
    }
}
