use std::collections::{BTreeMap, BTreeSet};

use super::LintWarning;
use crate::ast::*;
use crate::ast_json::block_pos;
use crate::render_depth::{push_block_children, push_inline_children};

struct Line<'a> {
    text: &'a str,
    start: usize,
}

fn lines(source: &str) -> Vec<Line<'_>> {
    let mut result = Vec::new();
    let mut start = 0;
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if matches!(bytes[i], b'\r' | b'\n') {
            result.push(Line {
                text: &source[start..i],
                start,
            });
            if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    result.push(Line {
        text: &source[start..],
        start,
    });
    result
}

fn emit(
    out: &mut Vec<LintWarning>,
    lines: &[Line<'_>],
    line: usize,
    offset: usize,
    len: usize,
    rule: &'static str,
    message: &str,
) {
    if let Some(row) = lines.get(line.saturating_sub(1)) {
        let offset = offset.min(row.text.len());
        out.push(LintWarning {
            line,
            column: row.text[..offset].chars().count() + 1,
            start: row.start + offset,
            end: row.start + (offset + len).min(row.text.len()),
            rule,
            message: message.into(),
        });
    }
}

fn visual(text: &str) -> usize {
    text.chars().fold(0, |col, ch| {
        if ch == '\t' {
            (col / 4 + 1) * 4
        } else {
            col + 1
        }
    })
}

fn quoted_view(text: &str, quotes: usize) -> (&str, usize) {
    let mut rest = text;
    for _ in 0..quotes {
        let trimmed = rest.trim_start_matches([' ', '\t']);
        if let Some(after) = trimmed.strip_prefix('>') {
            if after.is_empty() || after.starts_with(' ') {
                rest = after.strip_prefix(' ').unwrap_or(after);
            } else {
                break;
            }
        } else {
            break;
        }
    }
    let rest = rest.trim_start_matches([' ', '\t']);
    (rest, text.len() - rest.len())
}

fn container_view(text: &str) -> &str {
    static PREFIX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = PREFIX.get_or_init(|| {
        regex::Regex::new(
            r"^(?:[ \t]*> ?|[ \t]*(?:[-*] |(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)] |: ))",
        )
        .unwrap()
    });
    let mut rest = text;
    while let Some(m) = re.find(rest) {
        rest = &rest[m.end()..];
    }
    rest
}

struct Item {
    first: usize,
    last: usize,
    base: usize,
    content: usize,
    quotes: usize,
    deepest: usize,
}
struct Fence {
    first: usize,
    last: usize,
    column: usize,
    width: usize,
    bare: bool,
}

pub(super) fn collect(
    source: &str,
    doc: &Document,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    let rows = lines(source);
    let mut blocks: Vec<_> = doc.children.iter().map(|b| (b, 0)).collect();
    for body in doc.footnote_defs.values() {
        blocks.extend(body.iter().map(|b| (b, 1)));
    }
    let mut inlines = Vec::new();
    let mut ignored = BTreeSet::new();
    if let Some(pos) = doc
        .frontmatter_raw
        .as_ref()
        .and_then(|front| front.pos.as_ref())
    {
        ignored.extend(pos.start_line..=pos.end_line);
    }
    let mut paragraphs = BTreeSet::new();
    let mut headings = BTreeSet::new();
    let mut block_runs: BTreeMap<(char, usize), Vec<(usize, usize)>> = BTreeMap::new();
    let mut starts = Vec::new();
    let mut items = Vec::new();
    let mut fences = Vec::new();
    let marker = regex::Regex::new(
        r"^([ \t]*)([-*]|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)])(\{[^{}\r\n]*\})?( +)(?:\[[ xX_?>-]\] +)?",
    )
    .unwrap();
    let colon = regex::Regex::new(r"^(:{3,})(?:[ \t]|$)").unwrap();
    while let Some((block, depth)) = blocks.pop() {
        let pos = block_pos(block);
        if let Some(pos) = pos {
            let kind = match block {
                BlockNode::BlockQuote(_) => Some('>'),
                BlockNode::Table(_) => Some('|'),
                _ => None,
            };
            if let (Some(kind), Some(row)) = (kind, rows.get(pos.start_line - 1)) {
                let at = row
                    .text
                    .char_indices()
                    .nth(pos.start_column.saturating_sub(1))
                    .map_or(row.text.len(), |(at, _)| at);
                let column = visual(&row.text[..at]);
                for line in pos.start_line..=pos.end_line {
                    block_runs
                        .entry((kind, line))
                        .or_default()
                        .push((pos.start_line, column));
                }
            }
            match block {
                BlockNode::CodeBlock(_) | BlockNode::RawBlock(_) | BlockNode::Comment(_) => {
                    ignored.extend(pos.start_line..=pos.end_line);
                }
                BlockNode::Paragraph(p) => {
                    paragraphs.extend(pos.start_line..=pos.end_line);
                    if let Some(InlineNode::Text(t)) = p.children.first() {
                        if let Some(p) = &t.pos {
                            starts.push((p.clone(), t.value.as_str()));
                        }
                    }
                }
                BlockNode::Heading(_) => {
                    headings.insert(pos.end_line);
                }
                BlockNode::Figure(f) if matches!(&*f.target, FigureTarget::CodeBlock(_)) => {
                    ignored.extend(pos.start_line..=pos.end_line);
                }
                _ => {}
            }
            if matches!(
                block,
                BlockNode::Div(_)
                    | BlockNode::Directive(_)
                    | BlockNode::Admonition(_)
                    | BlockNode::FigureGroup(_)
                    | BlockNode::LineBlock(_)
            ) || matches!(block, BlockNode::BlockQuote(q) if q.fenced)
            {
                for ln in pos.start_line..=pos.end_line.min(rows.len()) {
                    let row = &rows[ln - 1];
                    let view = container_view(if ln == 1 {
                        row.text.trim_start_matches('\u{feff}')
                    } else {
                        row.text
                    })
                    .trim_start_matches([' ', '\t']);
                    if let Some(m) = colon.captures(view) {
                        fences.push(Fence {
                            first: ln,
                            last: pos.end_line,
                            column: row.text.len() - view.len(),
                            width: m[1].len(),
                            bare: view.trim() == &m[1],
                        });
                        break;
                    }
                    if !row.text.trim_start().starts_with('{') {
                        break;
                    }
                }
            }
        }
        if let BlockNode::Directive(d) = block {
            if d.kind == "footnotes" && depth > 0 {
                out.push(super::warning(d.pos.clone(), to_byte, "footnotes-placement-in-container", "This contained footnotes marker does not place the endnotes. Move it to document level.".into()));
            }
        }
        if let BlockNode::List(list) = block {
            for item in &list.items {
                let Some(p) = &item.pos else {
                    continue;
                };
                let Some(row) = rows.get(p.start_line - 1) else {
                    continue;
                };
                let at = row
                    .text
                    .char_indices()
                    .nth(p.start_column.saturating_sub(1))
                    .map(|(at, _)| at)
                    .unwrap_or(row.text.len());
                let Some(m) = marker.captures(&row.text[at..]) else {
                    continue;
                };
                let base = visual(&row.text[..at]) + visual(&m[1]);
                let content = base + m[2].chars().count() + m[4].len();
                let mut rest = &row.text[at..];
                let mut deepest = visual(&row.text[..at]);
                while let Some(m) = marker.captures(rest) {
                    deepest += visual(&m[1]) + m[2].chars().count() + m[4].len();
                    rest = &rest[m.get(0).unwrap().end()..];
                }
                items.push(Item {
                    first: p.start_line,
                    last: p.end_line,
                    base,
                    content,
                    quotes: row.text[..at].matches('>').count(),
                    deepest,
                });
            }
        }
        push_block_children(block, depth, &mut blocks, &mut inlines);
    }
    let mut text_spans = Vec::new();
    while let Some((node, depth)) = inlines.pop() {
        if matches!(
            node,
            InlineNode::Text(_) | InlineNode::Mention(_) | InlineNode::Tag(_)
        ) {
            if let Some(p) = node.pos() {
                text_spans.push((to_byte(p.start_offset), to_byte(p.end_offset)));
            }
        }
        push_inline_children(node, depth, &mut inlines);
    }
    text_spans.sort_unstable();
    let mut text_runs: Vec<(usize, usize)> = Vec::new();
    for (start, end) in text_spans {
        if let Some(last) = text_runs.last_mut().filter(|last| last.1 == start) {
            last.1 = end;
        } else {
            text_runs.push((start, end));
        }
    }
    let in_text = |start: usize, end: usize| {
        let i = text_runs.partition_point(|span| span.0 <= start);
        i > 0 && text_runs[i - 1].1 >= end
    };
    let habits = [
        (
            "markdown-strong-double-star",
            regex::Regex::new(r"\*\*([^*\s](?:[^*]*[^*\s])?)\*\*").unwrap(),
            "Use single asterisks for Carve strong text.",
        ),
        (
            "markdown-strikethrough-double-tilde",
            regex::Regex::new(r"~~([^~\s](?:[^~]*[^~\s])?)~~").unwrap(),
            "Use single tildes for Carve strikethrough.",
        ),
        (
            "djot-superscript-caret",
            regex::Regex::new(r"\^([^^\s](?:[^^]*[^^\s])?)\^").unwrap(),
            "Use {^text^} for Carve superscript.",
        ),
        (
            "djot-plus-bullet",
            regex::Regex::new(r"(?m)^[ \t]*(\+)[ \t]+\S").unwrap(),
            "Use a dash for a Carve list item.",
        ),
    ];
    let blank_line = regex::Regex::new(r"\n[ \t]*\r?\n").unwrap();
    for (rule, pattern, message) in &habits {
        let mut search = 0;
        while let Some(captures) = pattern.captures_at(source, search) {
            let m = captures
                .get(if *rule == "djot-plus-bullet" { 1 } else { 0 })
                .unwrap();
            search = m.end();
            let slashes = source[..m.start()]
                .bytes()
                .rev()
                .take_while(|b| *b == b'\\')
                .count();
            let width = if rule.starts_with("markdown-") { 2 } else { 1 };
            if slashes % 2 == 1 {
                continue;
            }
            if !in_text(m.start(), m.start() + width) || !in_text(m.end() - width, m.end()) {
                search = m.start() + 1;
                continue;
            }
            if blank_line.is_match(m.as_str()) {
                search = m.start() + 1;
                continue;
            }
            if *rule == "djot-superscript-caret" {
                let before = source[..m.start()].chars().next_back();
                let inner = captures.get(1).unwrap().as_str();
                if matches!(before, Some('{' | '['))
                    || inner.starts_with('[')
                    || inner.ends_with('[')
                    || source[m.end()..].starts_with('}')
                {
                    search = m.start() + 1;
                    continue;
                }
            }
            let index = rows.partition_point(|row| row.start <= m.start()) - 1;
            out.push(LintWarning {
                line: index + 1,
                column: source[rows[index].start..m.start()].chars().count() + 1,
                start: m.start(),
                end: m.end(),
                rule,
                message: (*message).into(),
            });
        }
    }
    let folded: BTreeSet<_> = out
        .iter()
        .filter(|w| w.rule == "definition-term-block-folded")
        .map(|w| w.line)
        .collect();
    let mut list_lines = folded.clone();
    items.sort_by_key(|i| (i.first, i.content));
    let block_opener = regex::Regex::new(r"^(?:#{1,6} +\S|>(?: |$)|`{3,}|~{3,}|::(?: |$)|:{3,}(?: |$)|!\[|\[[^\]]+\]: +\S|(?:-{3,}|\*{3,}|_{3,})[ \t]*$|\{[^{}]+\}[ \t]*$|\|.*\|[ \t]*$)").unwrap();
    let mut open_fence: Option<(usize, char, usize)> = None;
    let mut previous_run = None;
    let mut active: Vec<&Item> = Vec::new();
    let mut next_item = 0;
    let mut ended: Option<&Item> = None;
    for (index, row) in rows.iter().enumerate() {
        let ln = index + 1;
        while next_item < items.len() && items[next_item].first < ln {
            active.push(&items[next_item]);
            next_item += 1;
        }
        active.retain(|item| {
            if item.last < ln {
                if ended.map_or(true, |old| {
                    item.last > old.last
                        || (item.last == old.last
                            && (item.first < old.first
                                || (item.first == old.first && item.content > old.content)))
                }) {
                    ended = Some(item);
                }
                false
            } else {
                true
            }
        });
        let containing = active.iter().copied().max_by_key(|i| i.content);
        let owner = containing.or(ended);
        let (view, at) = quoted_view(row.text, owner.map_or(0, |i| i.quotes));
        let column = visual(&row.text[..at]);
        let run = view
            .chars()
            .next()
            .filter(|c| matches!(c, '|' | '>'))
            .filter(|_| block_opener.is_match(view))
            .map(|kind| (kind, owner.map(|i| i.first)));
        let block_run = run
            .and_then(|(kind, _)| block_runs.get(&(kind, ln)))
            .and_then(|runs| {
                runs.iter()
                    .filter(|(_, column)| *column >= owner.map_or(0, |i| i.content))
                    .map(|(start, _)| *start)
                    .min()
            });
        let continuing_run = run.is_some_and(|(kind, _)| kind == '|')
            && column > owner.map_or(0, |i| i.content)
            && run == previous_run
            || block_run.is_some_and(|start| start < ln);
        previous_run =
            run.filter(|(kind, _)| *kind == '|' && column > owner.map_or(0, |i| i.content));
        if let Some((owner_line, ch, width)) = open_fence {
            if containing.is_some_and(|i| i.first == owner_line) {
                let run = view.chars().take_while(|&c| c == ch).count();
                if run >= width && view[run..].trim().is_empty() {
                    open_fence = None;
                }
                list_lines.insert(ln);
                continue;
            }
            open_fence = None;
        }
        if folded.contains(&ln) || column == 0 || !block_opener.is_match(view) {
            continue;
        }
        if owner.is_some_and(|i| i.deepest > i.content && i.deepest == column) {
            continue;
        }
        let fence_char = view
            .chars()
            .next()
            .filter(|ch| matches!(ch, '`' | '~' | ':'))
            .filter(|ch| view.chars().take_while(|c| c == ch).count() >= 3);
        if ignored.contains(&ln)
            && fence_char.is_none()
            && !(view.starts_with('>') && block_run == Some(ln))
        {
            list_lines.insert(ln);
            continue;
        }
        let adjacent = ended.filter(|i| {
            rows[i.last..index]
                .iter()
                .all(|r| quoted_view(r.text, i.quotes).0.is_empty())
        });
        let candidate = containing.or(adjacent);
        let rule = candidate.and_then(|i| {
            if column > i.content && !continuing_run {
                Some("list-item-block-overindented")
            } else if containing.is_none() && column > i.base && column < i.content {
                Some("list-item-body-detached")
            } else {
                None
            }
        });
        if let Some(rule) = rule {
            let len = view.find(char::is_whitespace).unwrap_or(view.len());
            let content = candidate.unwrap().content;
            emit(out, &rows, ln, at, len, rule, &format!("This block opener does not use the list item's content column {content}. Align it with that column, or escape it to keep literal text."));
            list_lines.insert(ln);
        }
        if let (Some(item), Some(ch)) = (candidate, fence_char) {
            if column >= item.content {
                open_fence = Some((
                    item.first,
                    ch,
                    view.chars().take_while(|&c| c == ch).count(),
                ));
            }
        }
    }
    let trailing = regex::Regex::new(r"(?:^|\s)(\{\s*[.#][^{}]*\})\s*$").unwrap();
    let raw = regex::Regex::new(r"^([ \t]*)(`{3,}|~{3,})[ \t]*raw[ \t]+\S+").unwrap();
    let fence_run = regex::Regex::new(r"^(`{3,}|~{3,})").unwrap();
    let title = regex::Regex::new(r#"^:{3,}[ \t]+[A-Za-z_][\w-]*[ \t]+([^"\[ \t].*)$"#).unwrap();
    let include = regex::Regex::new(r"\{\{([^{}]*)\}\}").unwrap();
    for (index, row) in rows.iter().enumerate() {
        let ln = index + 1;
        for (at, ch) in row.text.char_indices() {
            if matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                emit(out, &rows, ln, at, ch.len_utf8(), "bidi-control-in-source", "This bidi control is preserved by canonical Carve but removed from presentation output.");
            }
        }
        if ignored.contains(&ln) {
            continue;
        }
        if headings.contains(&ln) {
            if let Some(m) = trailing.captures(row.text).and_then(|m| m.get(1)) {
                emit(out,&rows,ln,m.start(),m.len(),"heading-trailing-attribute","A heading's trailing attribute block is literal text. Move it to its own line above the heading.");
            }
        }
        let view = container_view(row.text).trim_start_matches([' ', '\t']);
        let at = row.text.len() - view.len();
        if let Some(m) = raw.captures(view) {
            let at = at + m[1].len();
            emit(
                out,
                &rows,
                ln,
                at,
                row.text.len() - at,
                "raw-block-syntax",
                "Use a fence followed by =FORMAT for a raw block; raw FORMAT does not open one.",
            );
        } else if paragraphs.contains(&ln) {
            if let Some(m) = fence_run.find(view) {
                let run = m.as_str();
                if !crate::parse::lint_opens_code_fence(view) && !view[run.len()..].contains(run) {
                    emit(out,&rows,ln,at,view.len(),"fence-opener-fallback","This fence has an invalid info string and parses as paragraph content. Use a language, optional quoted title, and optional label.");
                } else if at > 0
                    && row.text.starts_with([' ', '\t'])
                    && container_view(row.text).starts_with([' ', '\t'])
                    && !list_lines.contains(&ln)
                    && !view[run.len()..].contains(run)
                {
                    emit(out,&rows,ln,at,run.len(),"fence-delimiter-indentation","This fence is indented past its container's content column and does not open a code block.");
                }
            }
        }
        if row.text.starts_with('>') && row.text.len() > 1 && !row.text.starts_with("> ") {
            emit(
                out,
                &rows,
                ln,
                0,
                row.text.len(),
                "blockquote-marker-without-space",
                "A blockquote marker must be bare or followed by a space.",
            );
        }
        for (start, len) in crate::parse::lint_reversed_cell_markers(view) {
            emit(out, &rows, ln, at + start, len, "table-cell-attribute-before-marker", "Write the alignment marker before the cell attribute block, and end the marker run with a space.");
        }
        for m in include.captures_iter(row.text) {
            let inner = &m[1];
            let value = inner.trim_matches([' ', '\t']);
            let missing = value.is_empty()
                || (inner.starts_with([' ', '\t']) && value.starts_with(['#', '@']));
            let m = m.get(0).unwrap();
            if missing && in_text(row.start + m.start(), row.start + m.end()) {
                emit(
                    out,
                    &rows,
                    ln,
                    m.start(),
                    m.len(),
                    "empty-include-path",
                    "This include-shaped text has no path. Add a path or remove the braces.",
                );
            }
        }
    }
    for (pos, text) in starts {
        if list_lines.contains(&pos.start_line) {
            continue;
        }
        let view = text.trim_start_matches([' ', '\t']);
        if !(view.starts_with(":::") || view.starts_with("{#") || view.starts_with("{.")) {
            continue;
        }
        let Some(row) = rows.get(pos.start_line - 1) else {
            continue;
        };
        let at = to_byte(pos.start_offset).saturating_sub(row.start);
        let rule = if title.is_match(row.text.trim_start()) {
            "fence-title-syntax"
        } else {
            "block-marker-as-text"
        };
        emit(
            out,
            &rows,
            pos.start_line,
            at,
            if rule == "fence-title-syntax" {
                view.len()
            } else if view.starts_with(':') {
                view.bytes().take_while(|&b| b == b':').count()
            } else {
                2
            },
            rule,
            if rule == "fence-title-syntax" {
                "A fence title must use straight double quotes; put attributes on a separate preceding line."
            } else {
                "This block-shaped marker parsed as text. Check its syntax and container indentation."
            },
        );
    }
    fences.sort_by_key(|f| (f.first, std::cmp::Reverse(f.last)));
    let mut closed = vec![false; fences.len()];
    let mut claimed = BTreeSet::new();
    for (index, fence) in fences.iter().enumerate().rev() {
        let last = (fence.first..fence.last.min(rows.len())).rev().find(|&i| {
            let v = container_view(rows[i].text).trim();
            !v.is_empty() && !v.starts_with("^ ")
        });
        if let Some(i) = last {
            let view = container_view(rows[i].text).trim_start_matches([' ', '\t']);
            if view.trim_end() == ":".repeat(fence.width)
                && rows[i].text.len() - view.len() == fence.column
                && claimed.insert(i)
            {
                closed[index] = true;
            }
        }
    }
    for (index, fence) in fences.iter().enumerate() {
        let Some(row) = rows.get(fence.first - 1) else {
            continue;
        };
        let view = container_view(if fence.first == 1 {
            row.text.trim_start_matches('\u{feff}')
        } else {
            row.text
        })
        .trim_start_matches([' ', '\t']);
        if crate::parse::lint_invalid_container_metadata(view) {
            emit(out, &rows, fence.first, row.text.len() - view.len(), view.len(),
                "fence-title-syntax", "Invalid container metadata was dropped. Use a straight-double-quoted title or a bracketed label; the container and its children are preserved.");
        }
        if fence.bare && !closed[index] {
            if let Some(parent) = fences[..index]
                .iter()
                .rev()
                .find(|p| p.first < fence.first && p.last >= fence.last && p.column == fence.column)
            {
                if parent.width != fence.width {
                    emit(
                        out,
                        &rows,
                        fence.first,
                        fence.column,
                        fence.width,
                        "colon-fence-length-mismatch",
                        &format!(
                            "This bare fence has {} colons; the enclosing fence requires {}.",
                            fence.width, parent.width
                        ),
                    );
                }
            }
        }
        if !closed[index] {
            emit(
                out,
                &rows,
                fence.first,
                fence.column,
                (row.text.len() - fence.column).min(fence.width),
                "unclosed-container-fence",
                "This colon-fenced container has no closer. Add a bare fence of the same width.",
            );
        }
    }
    let version = regex::Regex::new(r"^[ \t]*carve-version[ \t]*:[ \t]*(\S+)[ \t]*$").unwrap();
    let declared = doc
        .frontmatter_raw
        .as_ref()
        .and_then(|front| front.pos.as_ref())
        .and_then(|pos| {
            (pos.start_line..=pos.end_line.min(rows.len())).find_map(|ln| {
                version
                    .captures(rows[ln - 1].text)
                    .and_then(|m| m.get(1))
                    .map(|m| (m.as_str().to_owned(), rows[ln - 1].start + m.start()))
            })
        })
        .or_else(|| {
            crate::read_stamp(source).map(|stamp| {
                let at = source.rfind(&stamp.version).unwrap_or(0);
                (stamp.version, at)
            })
        });
    if let Some((value, at)) = declared {
        let declared: Option<Vec<u64>> = value.split('.').map(|v| v.parse().ok()).collect();
        let implemented: Vec<u64> = crate::SPEC_VERSION
            .split('.')
            .map(|v| v.parse().unwrap())
            .collect();
        let unsupported = declared.map_or(true, |parts| {
            (0..parts.len().max(implemented.len()))
                .map(|i| parts.get(i).copied().unwrap_or(0))
                .cmp(
                    (0..parts.len().max(implemented.len()))
                        .map(|i| implemented.get(i).copied().unwrap_or(0)),
                )
                .is_gt()
        });
        if unsupported {
            let index = rows
                .partition_point(|row| row.start <= at)
                .saturating_sub(1);
            emit(
                out,
                &rows,
                index + 1,
                at - rows[index].start,
                value.len(),
                "carve-version-unsupported",
                "The declared Carve version is newer than or unrecognized by this engine.",
            );
        }
    }
}
