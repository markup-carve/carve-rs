use std::collections::{BTreeMap, BTreeSet};

use super::LintWarning;
use crate::ast::*;
use crate::ast_json::block_pos;
use crate::render_depth::{push_block_children, push_inline_children};

struct Line<'a> {
    text: &'a str,
    start: usize,
    codepoint_starts: Option<Vec<usize>>,
}

impl<'a> Line<'a> {
    fn new(text: &'a str, start: usize) -> Self {
        Self {
            text,
            start,
            codepoint_starts: (!text.is_ascii())
                .then(|| text.char_indices().map(|(byte, _)| byte).collect()),
        }
    }

    fn column(&self, offset: usize) -> usize {
        self.codepoint_starts.as_ref().map_or(offset + 1, |starts| {
            starts.partition_point(|byte| *byte < offset) + 1
        })
    }
}

fn lines(source: &str) -> Vec<Line<'_>> {
    let mut result = Vec::new();
    let mut start = 0;
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if matches!(bytes[i], b'\r' | b'\n') {
            result.push(Line::new(&source[start..i], start));
            if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    result.push(Line::new(&source[start..], start));
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
            file: None,
            included_by: Vec::new(),
            line,
            column: row.column(offset),
            start: row.start + offset,
            end: row.start + (offset + len).min(row.text.len()),
            rule,
            message: message.into(),
        });
    }
}

fn region_keyword(body: &str) -> Option<bool> {
    let keyword = body.strip_prefix('#')?;
    let keyword = if keyword.get(..3)?.eq_ignore_ascii_case("end") {
        &keyword[3..]
    } else {
        keyword
    };
    let prefix = keyword.get(..6)?;
    if !prefix.eq_ignore_ascii_case("region") {
        return None;
    }
    if keyword
        .as_bytes()
        .get(6)
        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    {
        return None;
    }
    Some(body.starts_with("#region") || body.starts_with("#endregion"))
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
            r"^(?:[ \t]*> ?|[ \t]*(?:[-*]|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)])(?:\{[^{}\r\n]*\})? +(?:\[[ xX_?>/!-]\] +)?|[ \t]*(?:: |\[\^[^\]\r\n]+\]: +))",
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
    marker_line_note: bool,
    paragraph_lines: BTreeSet<usize>,
    text_starts: BTreeSet<(usize, usize)>,
}
struct Fence {
    first: usize,
    last: usize,
    column: usize,
    width: usize,
    bare: bool,
}

struct DefinitionHost {
    first: usize,
    last: usize,
    column: usize,
    quotes: usize,
}

fn quote_prefix(text: &str) -> (usize, usize) {
    let mut at = 0;
    let mut depth = 0;
    loop {
        let rest = &text[at..];
        let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let Some(after) = rest[spaces..].strip_prefix('>') else {
            break;
        };
        if !after.is_empty() && !after.starts_with(' ') {
            break;
        }
        at += spaces + 1 + usize::from(after.starts_with(' '));
        depth += 1;
    }
    (at, depth)
}

fn authored_first_nonspace(text: &str) -> usize {
    let at = quote_prefix(text).0;
    at + text[at..].len() - text[at..].trim_start_matches([' ', '\t']).len()
}

fn separator_candidate(text: &str) -> Option<(usize, usize, usize)> {
    let (prefix, depth) = quote_prefix(text);
    let rest = &text[prefix..];
    let at = prefix + rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let tail = &text[at..];
    let len = if tail.starts_with("::") {
        2
    } else if tail.starts_with(':') {
        1
    } else {
        return None;
    };
    (tail.len() == len || matches!(tail.as_bytes()[len], b' ' | b'\t')).then_some((at, len, depth))
}

fn collect_description_separators(
    rows: &[Line<'_>],
    hosts: &[DefinitionHost],
    attached: &BTreeSet<usize>,
    ignored: &BTreeSet<usize>,
    payload: &BTreeSet<usize>,
    out: &mut Vec<LintWarning>,
) {
    for (index, row) in rows.iter().enumerate() {
        let line = index + 1;
        if ignored.contains(&line) || payload.contains(&line) || attached.contains(&line) {
            continue;
        }
        let Some((at, len, depth)) = separator_candidate(row.text) else {
            continue;
        };
        let mut previous = index;
        while previous > 0 && row_is_blank_or_quote(rows[previous - 1].text) {
            previous -= 1;
        }
        if previous == 0 || quote_prefix(rows[previous - 1].text).1 != depth {
            continue;
        }
        let column = visual(&row.text[..at]);
        let Some(host) = hosts
            .iter()
            .filter(|host| {
                host.first <= previous
                    && previous <= host.last
                    && host.quotes == depth
                    && host.column > column
            })
            .max_by_key(|host| host.column)
        else {
            continue;
        };
        let name = if len == 2 {
            "term marker"
        } else {
            "description separator"
        };
        emit(out, rows, line, at, len, "description-separator-below-content-column", &format!("This {name} starts at column {column}, below the column {} of the definition list above, so it adds no entry to that list. Indent it to column {} to continue the list.", host.column, host.column));
    }
}

fn row_is_blank_or_quote(text: &str) -> bool {
    text.bytes().all(|byte| matches!(byte, b' ' | b'\t' | b'>'))
}

pub(super) fn collect(
    source: &str,
    doc: &Document,
    to_byte: &dyn Fn(usize) -> usize,
    caption_looking: bool,
    out: &mut Vec<LintWarning>,
) -> bool {
    let rows = lines(source);
    collect_ordered_splits(doc, &rows, out);
    let mut scan = BlockScan::new(source, doc, &rows, to_byte, out);
    collect_authoring(&rows, doc, &scan, caption_looking, out);
    collect_description_separators(
        &rows,
        &scan.definition_hosts,
        &scan.attached,
        &scan.ignored,
        &scan.payload,
        out,
    );
    collect_habits(
        source,
        &rows,
        &scan.text_runs,
        &scan.emphasis_runs,
        &scan.opaque_runs,
        out,
    );
    scan.items.sort_by_key(|i| (i.first, i.content));
    let list_lines = ListScan::new(&rows, &scan, out).run(out);
    LineRules::new(&rows, &scan, &list_lines).run(out);
    collect_marker_text(&rows, &scan.starts, &list_lines, to_byte, out);
    let recovered_metadata = collect_container_fences(&rows, &mut scan.fences, out);
    collect_version(source, doc, &rows, out);
    recovered_metadata
}

fn collect_authoring(
    rows: &[Line<'_>],
    doc: &Document,
    scan: &BlockScan<'_>,
    caption_looking: bool,
    out: &mut Vec<LintWarning>,
) {
    let mut paragraphs = BTreeMap::new();
    let mut headings = Vec::new();
    let mut table_rows = BTreeMap::new();
    let mut blocks: Vec<_> = doc.children.iter().map(|b| (b, 0)).collect();
    let mut inlines = Vec::new();
    while let Some((block, depth)) = blocks.pop() {
        match block {
            BlockNode::Paragraph(p) => {
                if let Some(pos) = &p.pos {
                    for line in pos.start_line..=pos.end_line {
                        paragraphs.insert(line, pos.end_line);
                    }
                }
            }
            BlockNode::Heading(h) => {
                if let Some(pos) = &h.pos {
                    headings.push((pos.start_line, h.level));
                }
            }
            BlockNode::Table(t) if t.source_format.is_none() => {
                let width = t.rows.first().map_or(0, |r| r.cells.len());
                for (index, row) in t.rows.iter().enumerate() {
                    if let Some(pos) = &row.pos {
                        table_rows.insert(pos.start_line, (index, width, row.cells.len(), row));
                    }
                }
            }
            _ => {}
        }
        push_block_children(block, depth, &mut blocks, &mut inlines);
    }
    headings.sort_unstable();
    let mut last_level = 0;
    for &(line, level) in &headings {
        if last_level > 0 && level > last_level + 1 {
            if let Some(row) = rows.get(line - 1) {
                let at = row.text.find('#').unwrap_or(0);
                emit(out, rows, line, at, level as usize, "heading-level-skip", &format!("This heading skips from level {last_level} to {level}; use the next heading level for a continuous outline."));
            }
        }
        last_level = level;
    }
    let heading_lines: BTreeSet<_> = headings.into_iter().map(|(line, _)| line).collect();
    let mut list_item_lines = BTreeSet::new();
    for item in &scan.items {
        list_item_lines.insert(item.first);
    }
    // Footnote bodies sit outside `children`; only the folded-marker rule reads their paragraphs.
    let mut folded_paragraphs = paragraphs.clone();
    let mut note_blocks: Vec<_> = doc
        .footnote_defs
        .values()
        .flatten()
        .map(|b| (b, 0))
        .collect();
    let mut note_inlines = Vec::new();
    while let Some((block, depth)) = note_blocks.pop() {
        if let BlockNode::Paragraph(p) = block {
            if let Some(pos) = &p.pos {
                for line in pos.start_line..=pos.end_line {
                    folded_paragraphs.insert(line, pos.end_line);
                }
            }
        }
        push_block_children(block, depth, &mut note_blocks, &mut note_inlines);
    }
    for (&line, &(index, width, count, row)) in &table_rows {
        // A cell a `+` line appended past the row's last column starts with that
        // line's cell_wrap (T6), and the row is reported on that line.
        let appended_by = row
            .cells
            .iter()
            .skip(1)
            .rev()
            .map_while(|cell| match cell.children.first() {
                Some(InlineNode::CellWrap(wrap)) if cell.span.is_none() => Some(wrap.line),
                _ => None,
            })
            .last();
        if let Some(plus_line) = appended_by.map(|offset| line + offset) {
            if let Some(source) = rows.get(plus_line - 1) {
                let at = source.text.find('+').unwrap_or(0);
                emit(out, rows, plus_line, at, 1, "table-row-cell-count", "This continuation line appends a cell past the last column of its table row. Add the cell to the row line or remove it.");
            }
        } else if index > 0 && count != width {
            let at = rows[line - 1].text.find('|').unwrap_or(0);
            emit(out, rows, line, at, 1, "table-row-cell-count", &format!("This table row has {count} cells; the header has {width}. Add or remove a cell to match the table."));
        }
    }
    let gap = regex::Regex::new(r"\[[^\]]+\][ \t]+\([ \t]*[^)\s][^)]*\)").unwrap();
    let underline =
        regex::Regex::new(r"(^|[^[:alnum:]])_([[:alpha:]][[:alnum:]_ -]*?)_($|[^[:alnum:]])")
            .unwrap();
    let tag = regex::Regex::new(r"</?[A-Za-z][\w:-]*(?:\s+[^<>]*)?/?>").unwrap();
    let alert = regex::Regex::new(r"\[!(?:NOTE|TIP|IMPORTANT|WARNING|CAUTION)\]").unwrap();
    let list = regex::Regex::new(r"^(?:[ \t]*> ?)*[ \t]*(?:[-*]|[0-9]+[.)])[ \t]+\S").unwrap();
    let definition = regex::Regex::new(r"^[ \t]*:[ \t]+\S").unwrap();
    let caption = regex::Regex::new(r"(?i)^(?:\*?[ \t]*(?:Caption|Table|Tabelle|Figure|Image)(?:[ \t]+[0-9]+)?[ \t]*:|:[ \t]+\S)").unwrap();
    let opener = regex::Regex::new(r"(^|[\s(\[{])([*/])\w").unwrap();
    for (index, row) in rows.iter().enumerate() {
        let line = index + 1;
        if scan.ignored.contains(&line) {
            continue;
        }
        let view = container_view(row.text);
        let prefix = row.text.len() - view.len();
        let trimmed = view.trim_start_matches([' ', '\t']);
        let at = row.text.len() - trimmed.len();
        let is_paragraph = paragraphs.contains_key(&line);
        let is_table = table_rows.contains_key(&line);
        if index > 0
            && list.is_match(row.text)
            && !list_item_lines.contains(&line)
            && folded_paragraphs.contains_key(&line)
            && folded_paragraphs.get(&(line - 1)) == folded_paragraphs.get(&line)
            && !list.is_match(rows[index - 1].text)
            && !rows[index - 1].text.trim().is_empty()
        {
            emit(out, rows, line, authored_first_nonspace(row.text), 1, "list-marker-without-blank-line", "This list marker is part of the preceding paragraph; add a blank line before the list.");
        }
        if is_paragraph {
            if trimmed.starts_with('|') && !trimmed.trim_end_matches([' ', '\t']).ends_with('|') {
                let previous_table_shaped = index > 0
                    && paragraphs.contains_key(&(line - 1))
                    && container_view(rows[index - 1].text)
                        .trim_start()
                        .starts_with('|')
                    && !container_view(rows[index - 1].text)
                        .trim_end_matches([' ', '\t'])
                        .ends_with('|');
                if !previous_table_shaped || table_rows.contains_key(&(line - 1)) {
                    emit(out, rows, line, at, 1, "table-shaped-paragraph", "This table-shaped line lacks a closing pipe and renders as paragraph text; close the table row.");
                }
            }
            if index > 0
                && definition.is_match(&row.text[quote_prefix(row.text).0..])
                && paragraphs.contains_key(&(line - 1))
                && !rows[index - 1].text.trim_start().starts_with(":: ")
            {
                emit(out, rows, line, authored_first_nonspace(row.text), 1, "markdown-definition-paragraph", "This markdown definition remains paragraph text; use a :: term line before the definition.");
            }
            if caption_looking
                && caption.is_match(row.text[quote_prefix(row.text).0..].trim_start())
                && ((line.saturating_sub(2)..=line.saturating_sub(1)).chain((line + 1)..=line + 2))
                    .any(|near| {
                        table_rows.contains_key(&near)
                            || rows
                                .get(near.saturating_sub(1))
                                .is_some_and(|r| r.text.contains("!["))
                    })
            {
                emit(out, rows, line, authored_first_nonspace(row.text), 1, "caption-looking-paragraph", "This caption-looking paragraph sits next to a table or image; use a ^ caption line.");
            }
        }
        if !is_paragraph && !is_table && !heading_lines.contains(&line) {
            continue;
        }
        for m in gap.find_iter(view) {
            let start = prefix + m.start();
            if source_visible(row, start, m.end() - m.start(), scan) {
                emit(
                    out,
                    rows,
                    line,
                    start,
                    m.len(),
                    "link-gap-before-destination",
                    "The gap after this link label keeps the link as text; write ]( with no space.",
                );
            }
        }
        for caps in underline.captures_iter(view) {
            let m = caps.get(2).unwrap();
            let start = prefix + m.start() - 1;
            if source_visible(row, start, m.len() + 2, scan) {
                emit(
                    out,
                    rows,
                    line,
                    start,
                    m.len() + 2,
                    "markdown-italic-underline",
                    "This markdown italic spelling renders as underline; use /word/ for italic.",
                );
            }
        }
        if let Some(m) = tag.find(view) {
            let start = prefix + m.start();
            if source_visible(row, start, m.len(), scan) {
                emit(out, rows, line, start, m.len(), "bare-html-tag", "This bare HTML tag is escaped as text; use a raw HTML block if markup is intended.");
            }
        }
        if row.text.trim_start().starts_with('>') {
            if let Some(m) = alert.find(view) {
                let start = prefix + m.start();
                if source_visible(row, start, m.len(), scan) {
                    emit(out, rows, line, start, m.len(), "github-alert-marker", "This github alert marker stays literal in a Carve quote; use an admonition fence.");
                }
            }
        }
        if is_paragraph {
            for caps in opener.captures_iter(view) {
                let marker = caps.get(2).unwrap();
                let start = prefix + marker.start();
                let delimiter = marker.as_str();
                if !source_visible(row, start, 1, scan) {
                    continue;
                }
                let end_line = paragraphs[&line];
                let closed = has_visible(row, prefix + marker.end(), delimiter, scan)
                    || ((line + 1)..=end_line).any(|next| {
                        rows.get(next - 1).is_some_and(|next_row| {
                            let from = next_row.text.len() - container_view(next_row.text).len();
                            has_visible(next_row, from, delimiter, scan)
                        })
                    });
                if !closed {
                    emit(
                        out,
                        rows,
                        line,
                        start,
                        1,
                        "unclosed-inline-emphasis",
                        &format!(
                            "This {} opener is unclosed in its paragraph; add a closer.",
                            if delimiter == "*" {
                                "strong"
                            } else {
                                "emphasis"
                            }
                        ),
                    );
                }
            }
        }
        if is_paragraph || is_table {
            let dashed = trimmed.starts_with('|')
                && trimmed.contains("--")
                && trimmed
                    .bytes()
                    .all(|b| matches!(b, b'|' | b' ' | b'\t' | b':' | b'-' | b'x'));
            let valid = trimmed.split('|').skip(1).all(|cell| {
                let cell = cell.trim_matches(' ');
                cell.is_empty()
                    || (cell.len() >= 3 && cell.trim_matches(':').bytes().all(|b| b == b'-'))
            });
            let dash_cell = is_table
                && trimmed
                    .split('|')
                    .any(|cell| cell.len() >= 3 && cell.bytes().all(|b| b == b'-'));
            if (dashed && !valid) || dash_cell {
                emit(out, rows, line, at, 1, "table-separator-near-miss", "This table separator spelling renders as table content; correct the dash cells.");
            }
        }
    }
}

fn has_visible(row: &Line<'_>, from: usize, delimiter: &str, scan: &BlockScan<'_>) -> bool {
    row.text[from..]
        .match_indices(delimiter)
        .any(|(at, _)| source_visible(row, from + at, delimiter.len(), scan))
}

fn source_visible(row: &Line<'_>, start: usize, len: usize, scan: &BlockScan<'_>) -> bool {
    let absolute = row.start + start;
    let end = absolute + len;
    if overlaps(&scan.opaque_runs, absolute, end) {
        return false;
    }
    if start > 0
        && row.text[..start]
            .bytes()
            .rev()
            .take_while(|b| *b == b'\\')
            .count()
            % 2
            == 1
    {
        return false;
    }
    true
}

#[derive(Default)]
struct BlockScan<'d> {
    ignored: BTreeSet<usize>,
    // `%%` line comments: the one hidden place a mis-cased `%% #Region` is checked.
    line_comments: BTreeSet<usize>,
    paragraphs: BTreeSet<usize>,
    headings: BTreeSet<usize>,
    block_runs: BTreeMap<(char, usize), Vec<(usize, usize)>>,
    starts: Vec<(Pos, &'d str)>,
    items: Vec<Item>,
    fences: Vec<Fence>,
    definition_hosts: Vec<DefinitionHost>,
    descriptions: Vec<(usize, usize, usize)>,
    attached: BTreeSet<usize>,
    payload: BTreeSet<usize>,
    text_runs: Vec<(usize, usize)>,
    emphasis_runs: Vec<(usize, usize)>,
    opaque_runs: Vec<(usize, usize)>,
}

struct BlockWalker<'d, 'r> {
    rows: &'r [Line<'r>],
    marker: regex::Regex,
    colon: regex::Regex,
    note_marker: regex::Regex,
    scan: BlockScan<'d>,
}

impl<'d> BlockScan<'d> {
    fn new(
        source: &str,
        doc: &'d Document,
        rows: &[Line<'_>],
        to_byte: &dyn Fn(usize) -> usize,
        out: &mut Vec<LintWarning>,
    ) -> Self {
        let mut blocks: Vec<_> = doc.children.iter().map(|b| (b, 0)).collect();
        for body in doc.footnote_defs.values() {
            blocks.extend(body.iter().map(|b| (b, 1)));
        }
        let mut inlines = Vec::new();
        let mut walker = BlockWalker {
            rows,
            marker: regex::Regex::new(
                r"^([ \t]*)([-*]|(?:[0-9]+|[ivxlcdm]+|[IVXLCDM]+|[a-zA-Z])[.)])(\{[^{}\r\n]*\})?( +)(?:\[[ xX_?>/!-]\] +)?",
            )
            .unwrap(),
            colon: regex::Regex::new(r"^(:{3,})(?:[ \t]|$)").unwrap(),
            note_marker: regex::Regex::new(r"^\[\^[^\]\r\n]+\]:").unwrap(),
            scan: BlockScan::default(),
        };
        if let Some(pos) = doc
            .frontmatter_raw
            .as_ref()
            .and_then(|front| front.pos.as_ref())
        {
            walker.scan.ignored.extend(pos.start_line..=pos.end_line);
        }
        while let Some((block, depth)) = blocks.pop() {
            walker.visit(block, depth, to_byte, out);
            push_block_children(block, depth, &mut blocks, &mut inlines);
        }
        let mut scan = walker.scan;
        scan.scan_inlines(inlines, source, to_byte);
        scan
    }

    fn scan_inlines(
        &mut self,
        mut inlines: Vec<(&InlineNode, usize)>,
        source: &str,
        to_byte: &dyn Fn(usize) -> usize,
    ) {
        let mut text_spans = Vec::new();
        let mut emphasis_spans = Vec::new();
        let mut opaque_spans = Vec::new();
        while let Some((node, depth)) = inlines.pop() {
            let opaque = matches!(
                node,
                InlineNode::Code(_)
                    | InlineNode::Math(_)
                    | InlineNode::RawInline(_)
                    | InlineNode::LiteralInline(_)
                    | InlineNode::Comment(_)
                    | InlineNode::CrossRef(_)
                    | InlineNode::AutoLink(_)
                    | InlineNode::Image(_)
            );
            if opaque {
                if let Some(pos) = node.pos() {
                    self.payload.extend((pos.start_line + 1)..=pos.end_line);
                    opaque_spans.push((to_byte(pos.start_offset), to_byte(pos.end_offset)));
                }
            }
            if let InlineNode::Link(link) = node {
                if let Some(pos) = &link.pos {
                    let end = to_byte(pos.end_offset);
                    let after_label = link
                        .children
                        .iter()
                        .filter_map(InlineNode::pos)
                        .map(|child| to_byte(child.end_offset))
                        .max()
                        .unwrap_or_else(|| to_byte(pos.start_offset));
                    if let Some(tail) = source.get(after_label..end) {
                        if let Some(offset) = tail.find("](") {
                            opaque_spans.push((after_label + offset + 2, end));
                        }
                    }
                }
            }
            if let InlineNode::Emphasis(emphasis) = node {
                if let Some(pos) = &emphasis.pos {
                    emphasis_spans.push((to_byte(pos.start_offset), to_byte(pos.end_offset)));
                }
            }
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
        self.text_runs = merged_spans(text_spans);
        self.emphasis_runs = merged_spans(emphasis_spans);
        self.opaque_runs = merged_spans(opaque_spans);
    }
}

fn merged_spans(mut spans: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in spans {
        if let Some(last) = merged.last_mut().filter(|last| last.1 >= start) {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    merged
}

fn in_text(text_runs: &[(usize, usize)], start: usize, end: usize) -> bool {
    let i = text_runs.partition_point(|span| span.0 <= start);
    i > 0 && text_runs[i - 1].1 >= end
}

fn overlaps(spans: &[(usize, usize)], start: usize, end: usize) -> bool {
    let i = spans.partition_point(|span| span.1 <= start);
    spans.get(i).is_some_and(|span| span.0 < end)
}

fn byte_at_column(text: &str, column: usize) -> usize {
    text.char_indices()
        .nth(column.saturating_sub(1))
        .map_or(text.len(), |(at, _)| at)
}

impl<'d> BlockWalker<'d, '_> {
    fn visit(
        &mut self,
        block: &'d BlockNode,
        depth: usize,
        to_byte: &dyn Fn(usize) -> usize,
        out: &mut Vec<LintWarning>,
    ) {
        if let Some(pos) = block_pos(block) {
            self.record_block_run(block, pos);
            self.record_lines(block, pos);
            self.record_colon_fence(block, pos);
        }
        match block {
            BlockNode::Directive(d) if d.kind == "footnotes" && depth > 0 => {
                out.push(super::warning(d.pos.clone(), to_byte, "footnotes-placement-in-container", "This contained footnotes marker does not place the endnotes. Move it to document level.".into()));
            }
            BlockNode::List(list) => {
                for item in &list.items {
                    if let Some(item) = self.list_item(item) {
                        self.scan.items.push(item);
                    }
                }
            }
            BlockNode::DefinitionList(list) => self.record_definition_list(list),
            _ => {}
        }
    }

    fn record_block_run(&mut self, block: &BlockNode, pos: &Pos) {
        let kind = match block {
            BlockNode::BlockQuote(_) => '>',
            BlockNode::Table(_) => '|',
            _ => return,
        };
        let Some(row) = self.rows.get(pos.start_line - 1) else {
            return;
        };
        let at = byte_at_column(row.text, pos.start_column);
        let column = visual(&row.text[..at]);
        for line in pos.start_line..=pos.end_line {
            self.scan
                .block_runs
                .entry((kind, line))
                .or_default()
                .push((pos.start_line, column));
        }
    }

    fn record_lines(&mut self, block: &'d BlockNode, pos: &Pos) {
        let scan = &mut self.scan;
        match block {
            BlockNode::Comment(c) if !c.block => {
                scan.ignored.extend(pos.start_line..=pos.end_line);
                scan.line_comments.extend(pos.start_line..=pos.end_line);
            }
            BlockNode::CodeBlock(_) | BlockNode::RawBlock(_) | BlockNode::Comment(_) => {
                scan.ignored.extend(pos.start_line..=pos.end_line);
            }
            BlockNode::Paragraph(p) => {
                scan.paragraphs.extend(pos.start_line..=pos.end_line);
                if let Some(InlineNode::Text(t)) = p.children.first() {
                    if let Some(p) = &t.pos {
                        scan.starts.push((p.clone(), t.value.as_str()));
                    }
                }
            }
            BlockNode::Heading(_) => {
                scan.headings.insert(pos.end_line);
            }
            BlockNode::Figure(f) if matches!(&*f.target, FigureTarget::CodeBlock(_)) => {
                scan.ignored.extend(pos.start_line..=pos.end_line);
            }
            _ => {}
        }
    }

    fn record_colon_fence(&mut self, block: &BlockNode, pos: &Pos) {
        if !(matches!(
            block,
            BlockNode::Div(_)
                | BlockNode::Directive(_)
                | BlockNode::Admonition(_)
                | BlockNode::FigureGroup(_)
                | BlockNode::LineBlock(_)
        ) || matches!(block, BlockNode::BlockQuote(q) if q.fenced))
        {
            return;
        }
        let rows = self.rows;
        for ln in pos.start_line..=pos.end_line.min(rows.len()) {
            let row = &rows[ln - 1];
            let view = container_view(if ln == 1 {
                row.text.trim_start_matches('\u{feff}')
            } else {
                row.text
            })
            .trim_start_matches([' ', '\t']);
            if let Some(m) = self.colon.captures(view) {
                self.scan.fences.push(Fence {
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

    fn list_item(&self, item: &ListItem) -> Option<Item> {
        let p = item.pos.as_ref()?;
        let row = self.rows.get(p.start_line - 1)?;
        let at = byte_at_column(row.text, p.start_column);
        let m = self.marker.captures(&row.text[at..])?;
        let base = visual(&row.text[..at]) + visual(&m[1]);
        let content = base + m[2].chars().count() + m[4].len();
        let mut rest = &row.text[at..];
        let mut deepest = visual(&row.text[..at]);
        while let Some(m) = self.marker.captures(rest) {
            deepest += visual(&m[1]) + m[2].chars().count() + m[4].len();
            rest = &rest[m.get(0).unwrap().end()..];
        }
        let marker_tail = &row.text[at + m.get(0).unwrap().end()..];
        let mut paragraph_lines = BTreeSet::new();
        let mut text_starts = BTreeSet::new();
        for child in &item.children {
            let BlockNode::Paragraph(paragraph) = child else {
                continue;
            };
            if let Some(pos) = &paragraph.pos {
                paragraph_lines.extend((pos.start_line + 1)..=pos.end_line);
            }
            let mut pending: Vec<_> = paragraph
                .children
                .iter()
                .map(|inline| (inline, 0))
                .collect();
            while let Some((inline, depth)) = pending.pop() {
                if let InlineNode::Text(text) = inline {
                    if let Some(pos) = &text.pos {
                        text_starts.insert((pos.start_line, pos.start_column));
                    }
                }
                push_inline_children(inline, depth, &mut pending);
            }
        }
        Some(Item {
            first: p.start_line,
            last: p.end_line,
            base,
            content,
            quotes: row.text[..at].matches('>').count(),
            deepest,
            marker_line_note: self.note_marker.is_match(marker_tail),
            paragraph_lines,
            text_starts,
        })
    }

    fn record_definition_list(&mut self, list: &DefinitionList) {
        let rows = self.rows;
        let scan = &mut self.scan;
        for item in &list.items {
            for def in &item.definitions {
                let Some(pos) = def.pos.as_ref() else {
                    continue;
                };
                let Some(row) = rows.get(pos.start_line.saturating_sub(1)) else {
                    continue;
                };
                let at = byte_at_column(row.text, pos.start_column);
                let rest = &row.text[at..];
                let width = rest.len() - rest[1..].trim_start_matches([' ', '\t']).len();
                if rest.starts_with(':') && width > 1 {
                    let last = def
                        .children
                        .iter()
                        .filter_map(|child| block_pos(child).map(|p| p.end_line))
                        .fold(pos.end_line, usize::max);
                    scan.descriptions
                        .push((pos.start_line, last, visual(&row.text[..at + width])));
                }
            }
            scan.attached.extend(
                item.terms
                    .iter()
                    .filter_map(|term| term.pos.as_ref().map(|pos| pos.start_line)),
            );
            scan.attached.extend(
                item.definitions
                    .iter()
                    .filter_map(|def| def.pos.as_ref().map(|pos| pos.start_line)),
            );
        }
        if let (Some(pos), Some(term)) = (
            list.pos.as_ref(),
            list.items
                .first()
                .and_then(|item| item.terms.first())
                .and_then(|term| term.pos.as_ref()),
        ) {
            if let Some(row) = rows.get(term.start_line.saturating_sub(1)) {
                let at = byte_at_column(row.text, term.start_column);
                if row.text[at..].starts_with("::") {
                    scan.definition_hosts.push(DefinitionHost {
                        first: pos.start_line,
                        last: pos.end_line,
                        column: visual(&row.text[..at]),
                        quotes: quote_prefix(row.text).1,
                    });
                }
            }
        }
    }
}

fn collect_habits(
    source: &str,
    rows: &[Line<'_>],
    text_runs: &[(usize, usize)],
    emphasis_runs: &[(usize, usize)],
    opaque_runs: &[(usize, usize)],
    out: &mut Vec<LintWarning>,
) {
    let habits = [
        (
            "markdown-strong-double-star",
            regex::Regex::new(r"\*\*([^*\s](?:[^*]*[^*\s])?)\*\*").unwrap(),
            "Djot/Markdown `**strong**` is not Carve bold — Carve bold is a single `*`, so this renders with literal asterisks.",
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
        let mut column_byte = 0;
        let mut column_points = 0;
        let mut column_line = 0;
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
            let authored = |start, end| {
                (in_text(text_runs, start, end)
                    || (*rule == "markdown-strong-double-star"
                        && in_text(emphasis_runs, start, end)))
                    && !overlaps(opaque_runs, start, end)
            };
            if !authored(m.start(), m.start() + width) || !authored(m.end() - width, m.end()) {
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
            if index != column_line {
                column_line = index;
                column_byte = rows[index].start;
                column_points = 0;
            }
            column_points += source[column_byte..m.start()].chars().count();
            column_byte = m.start();
            out.push(LintWarning {
                file: None,
                included_by: Vec::new(),
                line: index + 1,
                column: column_points + 1,
                start: m.start(),
                end: m.end(),
                rule,
                message: (*message).into(),
            });
        }
    }
}

struct ListScan<'s, 'r> {
    rows: &'s [Line<'r>],
    scan: &'s BlockScan<'s>,
    marker_shaped: regex::Regex,
    block_opener: regex::Regex,
    folded: BTreeSet<usize>,
    list_lines: BTreeSet<usize>,
    open_fence: Option<(usize, char, usize)>,
    previous_run: Option<(char, Option<usize>)>,
    active: Vec<&'s Item>,
    next_item: usize,
    ended: Option<&'s Item>,
}

#[derive(Clone, Copy)]
struct ListRow<'s> {
    ln: usize,
    index: usize,
    view: &'s str,
    at: usize,
    column: usize,
    containing: Option<&'s Item>,
    owner: Option<&'s Item>,
}

impl<'s, 'r> ListScan<'s, 'r> {
    fn new(rows: &'s [Line<'r>], scan: &'s BlockScan<'s>, out: &[LintWarning]) -> Self {
        let folded: BTreeSet<_> = out
            .iter()
            .filter(|w| w.rule == "definition-term-block-folded")
            .map(|w| w.line)
            .collect();
        Self {
            rows,
            scan,
            marker_shaped: regex::Regex::new(
                r"^(?:[-+*](?: +\[[ xX\-_>?/!]\])?|\([0-9A-Za-z]+\)|[0-9A-Za-z]+[.)]|\.)[ \t]",
            )
            .unwrap(),
            block_opener: regex::Regex::new(r"^(?:#{1,6} +\S|>(?: |$)|`{3,}|~{3,}|::(?: |$)|:{3,}(?: |$)|!\[|\[[^\]]+\]: +\S|(?:-{3,}|\*{3,}|_{3,})[ \t]*$|\{[^{}]+\}[ \t]*$|\|.*\|[ \t]*$)").unwrap(),
            list_lines: folded.clone(),
            folded,
            open_fence: None,
            previous_run: None,
            active: Vec::new(),
            next_item: 0,
            ended: None,
        }
    }

    fn run(mut self, out: &mut Vec<LintWarning>) -> BTreeSet<usize> {
        let rows = self.rows;
        for (index, row) in rows.iter().enumerate() {
            let line = self.row(index, row);
            self.check_folded_marker(&line, out);
            self.check_separated_marker(&line, out);
            self.check_block_opener(&line, out);
        }
        self.list_lines
    }

    fn row(&mut self, index: usize, row: &'s Line<'r>) -> ListRow<'s> {
        let ln = index + 1;
        let items = &self.scan.items;
        while self.next_item < items.len() && items[self.next_item].first < ln {
            self.active.push(&items[self.next_item]);
            self.next_item += 1;
        }
        let ended = &mut self.ended;
        self.active.retain(|item| {
            if item.last < ln {
                if ended.map_or(true, |old| {
                    item.last > old.last
                        || (item.last == old.last
                            && (item.first < old.first
                                || (item.first == old.first && item.content > old.content)))
                }) {
                    *ended = Some(item);
                }
                false
            } else {
                true
            }
        });
        let containing = self.active.iter().copied().max_by_key(|i| i.content);
        let owner = containing.or(self.ended);
        let (view, at) = quoted_view(row.text, owner.map_or(0, |i| i.quotes));
        ListRow {
            ln,
            index,
            view,
            at,
            column: visual(&row.text[..at]),
            containing,
            owner,
        }
    }

    fn check_folded_marker(&self, l: &ListRow<'_>, out: &mut Vec<LintWarning>) {
        let ln = l.ln;
        let column = l.column;
        if self.scan.ignored.contains(&ln) || !self.marker_shaped.is_match(l.view) {
            return;
        }
        let marker_column = self.rows[l.index].column(l.at);
        if let Some(host) = self
            .active
            .iter()
            .copied()
            .filter(|item| {
                item.paragraph_lines.contains(&ln)
                    && item.text_starts.contains(&(ln, marker_column))
                    && item.base < column
                    && column < item.content
            })
            .max_by_key(|item| item.content)
        {
            let len = self.marker_shaped.find(l.view).unwrap().len() - 1;
            emit(out, self.rows, ln, l.at, len, "list-marker-below-content-column", &format!("This list marker starts at column {column}, below the list item's content column {}, so it folds into the item's paragraph as text. Indent it to column {} to nest it under the item, or start it at column {} to begin a new item at the item's own level.", host.content, host.content, host.base));
        }
    }

    fn check_separated_marker(&self, l: &ListRow<'_>, out: &mut Vec<LintWarning>) {
        let (ln, index, column) = (l.ln, l.index, l.column);
        let rows = self.rows;
        let items = &self.scan.items;
        if !(l.containing.is_none()
            && ln > 1
            && !self.scan.ignored.contains(&ln)
            && self.marker_shaped.is_match(l.view)
            && quoted_view(rows[index - 1].text, l.owner.map_or(0, |i| i.quotes))
                .0
                .trim()
                .is_empty()
            && !out
                .iter()
                .any(|w| w.rule == "ordered-list-split-by-indent" && w.line == ln))
        {
            return;
        }
        let host = items
            .iter()
            .filter(|i| i.first == ln)
            .min_by_key(|i| i.base)
            .and_then(|opened| {
                items
                    .iter()
                    .filter(|i| {
                        i.first < ln
                            && i.quotes == opened.quotes
                            && i.base < opened.base
                            && opened.base < i.content
                            && rows[i.last..index]
                                .iter()
                                .all(|r| quoted_view(r.text, i.quotes).0.trim().is_empty())
                    })
                    .max_by_key(|i| i.content)
            });
        if let Some(host) = host {
            let len = self.marker_shaped.find(l.view).unwrap().len() - 1;
            emit(out, rows, ln, l.at, len, "list-marker-below-content-column", &format!("This list marker starts at column {column} after a blank line, below the content column {} of the list item above, so it starts a separate list instead of nesting. Indent it to column {} to nest it under the item, or start it at column {} to put it at the item's own level.", host.content, host.content, host.base));
        }
    }

    fn check_block_opener(&mut self, l: &ListRow<'s>, out: &mut Vec<LintWarning>) {
        let ListRow {
            ln,
            index,
            view,
            at,
            column,
            containing,
            owner,
        } = *l;
        let rows = self.rows;
        let run = view
            .chars()
            .next()
            .filter(|c| matches!(c, '|' | '>'))
            .filter(|_| self.block_opener.is_match(view))
            .map(|kind| (kind, owner.map(|i| i.first)));
        let block_run = run
            .and_then(|(kind, _)| self.scan.block_runs.get(&(kind, ln)))
            .and_then(|runs| {
                runs.iter()
                    .filter(|(_, column)| *column >= owner.map_or(0, |i| i.content))
                    .map(|(start, _)| *start)
                    .min()
            });
        let continuing_run = run.is_some_and(|(kind, _)| kind == '|')
            && column > owner.map_or(0, |i| i.content)
            && run == self.previous_run
            || block_run.is_some_and(|start| start < ln);
        self.previous_run =
            run.filter(|(kind, _)| *kind == '|' && column > owner.map_or(0, |i| i.content));
        if self.continue_fence(l) {
            return;
        }
        if self.folded.contains(&ln) || column == 0 || !self.block_opener.is_match(view) {
            return;
        }
        if let Some(note) = containing
            .filter(|item| item.marker_line_note && column == item.content + 2)
            .filter(|_| {
                rows[..index]
                    .iter()
                    .rev()
                    .find(|previous| !previous.text.trim().is_empty())
                    .is_some_and(|previous| {
                        let (previous_view, previous_at) =
                            quoted_view(previous.text, owner.map_or(0, |item| item.quotes));
                        previous_view.starts_with("%%")
                            && visual(&previous.text[..previous_at]) == column
                    })
            })
        {
            if let Some(ch) = view.chars().next().filter(|ch| matches!(ch, '`' | '~')) {
                self.open_fence = Some((
                    note.first,
                    ch,
                    view.chars().take_while(|c| *c == ch).count(),
                ));
                return;
            }
        }
        if owner.is_some_and(|i| i.deepest > i.content && i.deepest == column) {
            return;
        }
        let fence_char = view
            .chars()
            .next()
            .filter(|ch| matches!(ch, '`' | '~' | ':'))
            .filter(|ch| view.chars().take_while(|c| c == ch).count() >= 3);
        if self.scan.ignored.contains(&ln)
            && fence_char.is_none()
            && !(view.starts_with('>') && block_run == Some(ln))
        {
            self.list_lines.insert(ln);
            return;
        }
        let adjacent = self.ended.filter(|i| {
            rows[i.last..index]
                .iter()
                .all(|r| quoted_view(r.text, i.quotes).0.is_empty())
        });
        let candidate = containing.or(adjacent);
        // A description inside the item is the innermost container, so its content column places the block.
        if candidate.is_some_and(|i| {
            self.scan
                .descriptions
                .iter()
                .any(|&(first, last, content)| {
                    first >= i.first
                        && first < ln
                        && ln <= last
                        && content > i.content
                        && column >= content
                })
        }) {
            return;
        }
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
            emit(out, rows, ln, at, len, rule, &format!("This block opener does not use the list item's content column {content}. Align it with that column, or escape it to keep literal text."));
            self.list_lines.insert(ln);
        }
        if let (Some(item), Some(ch)) = (candidate, fence_char) {
            if column >= item.content {
                self.open_fence = Some((
                    item.first,
                    ch,
                    view.chars().take_while(|&c| c == ch).count(),
                ));
            }
        }
    }

    fn continue_fence(&mut self, l: &ListRow<'_>) -> bool {
        let Some((owner_line, ch, width)) = self.open_fence else {
            return false;
        };
        if l.containing.is_some_and(|i| i.first == owner_line) {
            let run = l.view.chars().take_while(|&c| c == ch).count();
            if run >= width && l.view[run..].trim().is_empty() {
                self.open_fence = None;
            }
            self.list_lines.insert(l.ln);
            return true;
        }
        self.open_fence = None;
        false
    }
}

struct LineRules<'s, 'r> {
    rows: &'s [Line<'r>],
    scan: &'s BlockScan<'s>,
    list_lines: &'s BTreeSet<usize>,
    trailing: regex::Regex,
    raw: regex::Regex,
    fence_run: regex::Regex,
    include: regex::Regex,
}

impl<'s, 'r> LineRules<'s, 'r> {
    fn new(rows: &'s [Line<'r>], scan: &'s BlockScan<'s>, list_lines: &'s BTreeSet<usize>) -> Self {
        Self {
            rows,
            scan,
            list_lines,
            trailing: regex::Regex::new(r"(?:^|\s)(\{\s*[.#][^{}]*\})\s*$").unwrap(),
            raw: regex::Regex::new(r"^([ \t]*)(`{3,}|~{3,})[ \t]*raw[ \t]+\S+").unwrap(),
            fence_run: regex::Regex::new(r"^(`{3,}|~{3,})").unwrap(),
            include: regex::Regex::new(r"\{\{([^{}]*)\}\}").unwrap(),
        }
    }

    fn run(&self, out: &mut Vec<LintWarning>) {
        for (index, row) in self.rows.iter().enumerate() {
            let ln = index + 1;
            collect_bidi_controls(ln, row, out);
            self.check_region_marker(ln, row, out);
            if self.scan.ignored.contains(&ln) {
                continue;
            }
            self.check_visible_line(ln, row, out);
        }
    }

    fn check_region_marker(&self, ln: usize, row: &Line<'_>, out: &mut Vec<LintWarning>) {
        let trimmed = row.text.trim_start_matches([' ', '\t']);
        let percent = trimmed.starts_with("%%") && !trimmed.starts_with("%%%");
        let body = if percent {
            trimmed[2..].trim_start_matches([' ', '\t'])
        } else if let Some(rest) = trimmed.strip_prefix("<!--") {
            rest.trim_start_matches([' ', '\t'])
        } else if let Some(rest) = trimmed.strip_prefix("/*") {
            rest.trim_start_matches([' ', '\t'])
        } else if let Some(rest) = trimmed
            .strip_prefix("//")
            .or_else(|| trimmed.strip_prefix("--"))
        {
            rest.trim_start_matches([' ', '\t'])
        } else {
            trimmed
        };
        if let Some(lowercase) = region_keyword(body) {
            if (!percent || !lowercase)
                && (!self.scan.ignored.contains(&ln)
                    || (percent && self.scan.line_comments.contains(&ln)))
            {
                emit(
                    out,
                    self.rows,
                    ln,
                    row.text.len() - trimmed.len(),
                    trimmed.len(),
                    "region-marker-renders-as-text",
                    "This line renders as text and is not a region marker for a Carve include. \
                     Use \"%% #region name\".",
                );
            }
        }
    }

    fn check_visible_line(&self, ln: usize, row: &Line<'_>, out: &mut Vec<LintWarning>) {
        let rows = self.rows;
        if self.scan.headings.contains(&ln) {
            if let Some(m) = self.trailing.captures(row.text).and_then(|m| m.get(1)) {
                emit(out,rows,ln,m.start(),m.len(),"heading-trailing-attribute","A heading's trailing attribute block is literal text. Move it to its own line above the heading.");
            }
        }
        let view = container_view(row.text).trim_start_matches([' ', '\t']);
        let at = row.text.len() - view.len();
        if let Some(m) = self.raw.captures(view) {
            let at = at + m[1].len();
            emit(
                out,
                rows,
                ln,
                at,
                row.text.len() - at,
                "raw-block-syntax",
                "Use a fence followed by =FORMAT for a raw block; raw FORMAT does not open one.",
            );
        } else if self.scan.paragraphs.contains(&ln) {
            self.check_paragraph_fence(ln, row, view, at, out);
        }
        if row.text.starts_with('>') && row.text.len() > 1 && !row.text.starts_with("> ") {
            emit(
                out,
                rows,
                ln,
                0,
                row.text.len(),
                "blockquote-marker-without-space",
                "A blockquote marker must be bare or followed by a space.",
            );
        }
        for (start, end, part) in crate::parse::lint_unpadded_cell_runs(view) {
            use crate::parse::UnpaddedCellRun;
            let message = match part {
                UnpaddedCellRun::Kind => "The table kind marker has no terminating space, so it is literal cell content. Add a space after the marker run to make it a header.".to_string(),
                UnpaddedCellRun::Align => format!("The table alignment run {:?} has no terminating space, so it is literal cell content. Add a space after the run to make it alignment.", &view[start..end]),
                UnpaddedCellRun::Attrs => "The table cell attribute block has no terminating space, so it is literal cell content. Add a space after the attribute block to apply it.".to_string(),
            };
            emit(
                out,
                rows,
                ln,
                at + start,
                end - start,
                "table-marker-run-padding",
                &message,
            );
        }
        for (start, len) in crate::parse::lint_reversed_cell_markers(view) {
            emit(out, rows, ln, at + start, len, "table-marker-run-padding", "Write the alignment marker before the cell attribute block, and end the marker run with a space.");
        }
        for m in self.include.captures_iter(row.text) {
            let inner = &m[1];
            let value = inner.trim_matches([' ', '\t']);
            let missing = value.is_empty()
                || (inner.starts_with([' ', '\t']) && value.starts_with(['#', '@']));
            let m = m.get(0).unwrap();
            if missing
                && in_text(
                    &self.scan.text_runs,
                    row.start + m.start(),
                    row.start + m.end(),
                )
            {
                emit(
                    out,
                    rows,
                    ln,
                    m.start(),
                    m.len(),
                    "empty-include-path",
                    "This include-shaped text has no path. Add a path or remove the braces.",
                );
            }
        }
    }

    fn check_paragraph_fence(
        &self,
        ln: usize,
        row: &Line<'_>,
        view: &str,
        at: usize,
        out: &mut Vec<LintWarning>,
    ) {
        let Some(m) = self.fence_run.find(view) else {
            return;
        };
        let run = m.as_str();
        if !crate::parse::lint_opens_code_fence(view) && !view[run.len()..].contains(run) {
            emit(out,self.rows,ln,at,view.len(),"fence-opener-fallback","This fence has an invalid info string and parses as paragraph content. Use a language, optional quoted title, and optional label.");
        } else if at > 0
            && row.text.starts_with([' ', '\t'])
            && container_view(row.text).starts_with([' ', '\t'])
            && !self.list_lines.contains(&ln)
            && !view[run.len()..].contains(run)
        {
            emit(out,self.rows,ln,at,run.len(),"fence-delimiter-indentation","This fence is indented past its container's content column and does not open a code block.");
        }
    }
}

fn collect_bidi_controls(ln: usize, row: &Line<'_>, out: &mut Vec<LintWarning>) {
    for (column, (at, ch)) in row.text.char_indices().enumerate() {
        if matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            out.push(LintWarning { file: None, included_by: Vec::new(),
                line: ln,
                column: column + 1,
                start: row.start + at,
                end: row.start + at + ch.len_utf8(),
                rule: "bidi-control-in-source",
                message: "This bidi control is preserved by canonical Carve but removed from presentation output.".into(),
            });
        }
    }
}

fn collect_marker_text(
    rows: &[Line<'_>],
    starts: &[(Pos, &str)],
    list_lines: &BTreeSet<usize>,
    to_byte: &dyn Fn(usize) -> usize,
    out: &mut Vec<LintWarning>,
) {
    let title = regex::Regex::new(r#"^:{3,}[ \t]+[A-Za-z_][\w-]*[ \t]+([^"\[ \t].*)$"#).unwrap();
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
            rows,
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
}

fn collect_container_fences(
    rows: &[Line<'_>],
    fences: &mut [Fence],
    out: &mut Vec<LintWarning>,
) -> bool {
    let mut recovered_metadata = false;
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
            recovered_metadata = true;
            emit(out, rows, fence.first, row.text.len() - view.len(), view.len(),
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
                        rows,
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
                rows,
                fence.first,
                fence.column,
                (row.text.len() - fence.column).min(fence.width),
                "unclosed-container-fence",
                "This colon-fenced container has no closer. Add a bare fence of the same width.",
            );
        }
    }
    recovered_metadata
}

fn collect_version(source: &str, doc: &Document, rows: &[Line<'_>], out: &mut Vec<LintWarning>) {
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
    let Some((value, at)) = declared else {
        return;
    };
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
            rows,
            index + 1,
            at - rows[index].start,
            value.len(),
            "carve-version-unsupported",
            "The declared Carve version is newer than or unrecognized by this engine.",
        );
    }
}

fn collect_ordered_splits(doc: &Document, rows: &[Line<'_>], out: &mut Vec<LintWarning>) {
    let ordered_marker =
        regex::Regex::new(r"^(?:[ \t]*> ?)*[ \t]*(\([0-9A-Za-z]+\)|[0-9A-Za-z]+[.)]|\.)[ \t]")
            .unwrap();
    let first_marker = |list: &List| {
        let line = list.items.first()?.pos.as_ref()?.start_line;
        let row = rows.get(line - 1)?;
        let marker = ordered_marker.captures(row.text)?.get(1)?;
        let offset = marker.start();
        Some((line, offset, visual(&row.text[..offset])))
    };
    let mut containers: Vec<&[BlockNode]> = vec![&doc.children];
    containers.extend(doc.footnote_defs.values().map(Vec::as_slice));
    while let Some(blocks) = containers.pop() {
        for pair in blocks.windows(2) {
            let (BlockNode::List(before), BlockNode::List(after)) = (&pair[0], &pair[1]) else {
                continue;
            };
            if !before.ordered
                || !after.ordered
                || before.bare_marker
                || after.bare_marker
                || before.ol_type != after.ol_type
                || before.delim != after.delim
                || after.start.unwrap_or(1) != before.start.unwrap_or(1) + before.items.len()
            {
                continue;
            }
            let (Some(before_pos), Some(after_pos)) = (&before.pos, &after.pos) else {
                continue;
            };
            if rows
                .get(before_pos.end_line..after_pos.start_line.saturating_sub(1))
                .map_or(true, |between| {
                    between.iter().any(|row| {
                        let mut rest = row.text;
                        while let Some(after_quote) =
                            rest.trim_start_matches([' ', '\t']).strip_prefix('>')
                        {
                            rest = after_quote.strip_prefix(' ').unwrap_or(after_quote);
                        }
                        !rest.trim().is_empty()
                    })
                })
            {
                continue;
            }
            let (Some((_, _, first)), Some((line, offset, second))) =
                (first_marker(before), first_marker(after))
            else {
                continue;
            };
            if first == second {
                continue;
            }
            let start = before.start.unwrap_or(1);
            let next = after.start.unwrap_or(1);
            let suffix = if before.ol_type.is_some() {
                ".".to_string()
            } else {
                format!(", or write {{start={start}}} on the line above and use the auto-numbered . marker.")
            };
            emit(out, rows, line, offset, 1, "ordered-list-split-by-indent", &format!("This ordered list continues the numbering of the list above ({start}, then {next}), but its marker starts at column {second} and the list above starts at column {first}, so they are two lists. Start the markers at the same column{suffix}"));
        }
        for block in blocks {
            match block {
                BlockNode::List(list) => {
                    containers.extend(list.items.iter().map(|item| item.children.as_slice()))
                }
                BlockNode::BlockQuote(node) => containers.push(&node.children),
                BlockNode::Admonition(node) => containers.push(&node.children),
                BlockNode::Directive(node) => containers.push(&node.children),
                BlockNode::Div(node) => containers.push(&node.children),
                BlockNode::Section(node) => containers.push(&node.children),
                BlockNode::LineBlock(node) => containers.push(&node.children),
                BlockNode::FigureGroup(node) => containers.push(&node.children),
                BlockNode::DefinitionList(node) => containers.extend(
                    node.items
                        .iter()
                        .flat_map(|item| item.definitions.iter())
                        .map(|def| def.children.as_slice()),
                ),
                BlockNode::Table(node) => containers.extend(
                    node.rows
                        .iter()
                        .flat_map(|row| &row.cells)
                        .filter_map(|cell| cell.blocks.as_deref()),
                ),
                BlockNode::Figure(node) => match &*node.target {
                    FigureTarget::BlockQuote(quote) => containers.push(&quote.children),
                    FigureTarget::Table(table) => containers.extend(
                        table
                            .rows
                            .iter()
                            .flat_map(|row| &row.cells)
                            .filter_map(|cell| cell.blocks.as_deref()),
                    ),
                    _ => {}
                },
                BlockNode::BlockExtension(node) => {
                    containers.push(std::slice::from_ref(&node.fallback))
                }
                BlockNode::ExtensionCarrier(node) => containers.push(&node.children),
                _ => {}
            }
        }
    }
}
