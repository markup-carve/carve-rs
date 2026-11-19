//! An opt-in walk of the resolved tree and its source origins.

mod validate;
pub(super) use validate::validate;

use super::{Error, Input, Tree};
use crate::block::events::{Event, LeafKind};
use crate::block::table::{Row, TableData};
use serde_json::{Map, Value, json};

pub(super) fn build(source: &str, tree: &Tree, input: Input<'_>) -> Result<Value, Error> {
    let document =
        super::deep_json(&tree.json).map_err(|error| Error::option("tree", error.to_string()))?;
    let (encoding, original) = match input {
        Input::Bytes(bytes) if std::str::from_utf8(bytes).is_err() => ("base64", base64(bytes)),
        _ => ("utf-8", source.to_string()),
    };
    let mut walk = Walk {
        source,
        nodes: Vec::new(),
        offsets: match input {
            Input::Bytes(bytes) if std::str::from_utf8(bytes).is_err() => {
                Some(decoded_offsets(bytes))
            }
            _ => None,
        },
        tables: if source.contains('|') {
            crate::block::scan(&crate::source::Source::new(source))
                .events
                .into_iter()
                .filter_map(|event| {
                    if let Event::Leaf {
                        kind: LeafKind::Table(data),
                        start,
                        ..
                    } = event
                    {
                        Some((start, *data))
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            Vec::new()
        },
    };
    walk.visit(
        &document,
        "",
        Frame {
            start: 0,
            end: source.len(),
            quotes: 0,
            blocks: true,
            cell: false,
            caption: false,
            open: false,
        },
    );
    Ok(json!({
        "version": 2, "encoding": encoding, "source": original,
        "lineEndings": line_endings(source), "bom": source.starts_with('\u{feff}'),
        "nodes": walk.nodes,
    }))
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let a = group[0];
        let b = *group.get(1).unwrap_or(&0);
        let c = *group.get(2).unwrap_or(&0);
        out.push(ALPHABET[(a >> 2) as usize] as char);
        out.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        out.push(if group.len() > 1 {
            ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        out.push(if group.len() > 2 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn decoded_offsets(bytes: &[u8]) -> Vec<usize> {
    let mut map = vec![0; bytes.len() + 1];
    let (mut original, mut decoded) = (0, 0);
    for chunk in bytes.utf8_chunks() {
        for at in 0..chunk.valid().len() {
            map[original + at] = decoded + at;
        }
        original += chunk.valid().len();
        decoded += chunk.valid().len();
        for at in 0..chunk.invalid().len() {
            map[original + at] = decoded;
        }
        original += chunk.invalid().len();
        if !chunk.invalid().is_empty() {
            decoded += '\u{fffd}'.len_utf8();
        }
    }
    map[bytes.len()] = decoded;
    map
}

fn line_endings(source: &str) -> &'static str {
    let bytes = source.as_bytes();
    let (mut lf, mut crlf, mut cr) = (false, false, false);
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'\r' {
            if bytes.get(at + 1) == Some(&b'\n') {
                crlf = true;
                at += 1;
            } else {
                cr = true;
            }
        } else if bytes[at] == b'\n' {
            lf = true;
        }
        at += 1;
    }
    match (lf, crlf, cr) {
        (false, false, false) => "none",
        (true, false, false) => "lf",
        (false, true, false) => "crlf",
        (false, false, true) => "cr",
        _ => "mixed",
    }
}

#[derive(Clone, Copy)]
struct Frame {
    start: usize,
    end: usize,
    quotes: usize,
    blocks: bool,
    cell: bool,
    caption: bool,
    open: bool,
}

struct Walk<'a> {
    source: &'a str,
    nodes: Vec<Value>,
    tables: Vec<(usize, TableData)>,
    offsets: Option<Vec<usize>>,
}

impl Walk<'_> {
    fn visit(&mut self, node: &Value, path: &str, frame: Frame) {
        let Some(ty) = node.get("type").and_then(Value::as_str) else {
            return;
        };
        let pos = node.get("pos");
        let byte_start = pos
            .and_then(|p| p.get("startOffset"))
            .and_then(Value::as_u64)
            .map(|n| n as usize);
        let byte_end = pos
            .and_then(|p| p.get("endOffset"))
            .and_then(Value::as_u64)
            .map(|n| n as usize);
        let start = byte_start.map(|n| self.text_offset(n));
        let end = byte_end.map(|n| self.text_offset(n));
        let mut inner = frame;
        if let (Some(start), Some(end)) = (start, end) {
            if start <= end
                && end <= self.source.len()
                && is_block(ty)
                && (!frame.caption || ty == "caption")
                && !frame.cell
            {
                let mut entry = Map::new();
                entry.insert("path".into(), path.into());
                entry.insert("startByte".into(), byte_start.unwrap_or(start).into());
                entry.insert("endByte".into(), byte_end.unwrap_or(end).into());
                if required_blanks(ty, frame) {
                    entry.insert(
                        "blankLinesBefore".into(),
                        self.blank_before(start, frame.start, frame.quotes).into(),
                    );
                    let after = if ty == "block_quote"
                        && !self.source[start..end].trim_start().starts_with(":::")
                    {
                        self.marked_blank_end(end, frame.quotes + 1)
                    } else {
                        end
                    };
                    entry.insert(
                        "blankLinesAfter".into(),
                        self.blank_after(after, frame.end, frame.quotes, frame.open)
                            .into(),
                    );
                }
                self.facts(ty, node, start, end, frame.quotes, &mut entry);
                self.nodes.push(Value::Object(entry));
                inner.start = start;
                inner.end = end;
                inner.quotes += usize::from(
                    ty == "block_quote" && !self.source[start..end].trim_start().starts_with(":::"),
                );
                inner.open = ty == "block_quote";
            }
        }
        if matches!(ty, "link" | "image") && node.get("key").is_some() {
            if let (Some(start), Some(end)) = (start, end) {
                self.raw_ref(
                    ty,
                    path,
                    start,
                    end,
                    byte_start.unwrap_or(start),
                    byte_end.unwrap_or(end),
                    frame.quotes,
                );
            }
        }
        if ty == "table"
            && let (Some(start), Some(end)) = (start, end)
        {
            self.table_rows(node, path, start, end);
        }
        if ty == "include" {
            return;
        }
        for slot in ["title", "children", "caption"] {
            let Some(value) = node.get(slot) else {
                continue;
            };
            let child_frame = Frame {
                blocks: slot == "children" && block_children(ty),
                cell: frame.cell || ty == "table_cell",
                caption: frame.caption || ty == "caption" || slot == "caption",
                ..inner
            };
            if let Some(children) = value.as_array() {
                for (index, child) in children.iter().enumerate() {
                    self.visit(child, &format!("{path}/{slot}/{index}"), child_frame);
                }
            } else if value.is_object() {
                self.visit(value, &format!("{path}/{slot}"), child_frame);
            }
        }
    }

    fn text_offset(&self, byte: usize) -> usize {
        self.offsets
            .as_ref()
            .and_then(|map| map.get(byte).copied())
            .unwrap_or(byte)
    }

    fn facts(
        &self,
        ty: &str,
        node: &Value,
        start: usize,
        end: usize,
        quotes: usize,
        entry: &mut Map<String, Value>,
    ) {
        let source = self.source;
        let line = &source[start..line_end(source, start).min(end)];
        let trimmed = line.trim_start_matches([' ', '\t']);
        let mark_at = start + line.len() - trimmed.len();
        let column = column_of(source, mark_at, quotes);
        match ty {
            "heading" => {
                let raw = if trimmed.starts_with('#') {
                    trimmed.bytes().take_while(|b| *b == b'#').count()
                } else if trimmed.starts_with('=') {
                    trimmed.bytes().take_while(|b| *b == b'=').count()
                } else {
                    0
                };
                if raw > 0 {
                    marker(entry, &trimmed[..raw], column);
                }
            }
            "thematic_break" => marker(entry, trimmed.trim_end(), column),
            "list" => {
                if node.pointer("/marker/kind").and_then(Value::as_str) == Some("ordered") {
                    entry.insert("bare".into(), trimmed.starts_with('.').into());
                }
            }
            "list_item" | "definition_description" | "footnote" => {
                let mark_len = if ty == "list_item" {
                    list_marker_len(trimmed)
                } else if ty == "definition_description" {
                    1
                } else {
                    trimmed.find(":").map_or(0, |n| n + 1)
                };
                if mark_len > 0 && mark_len <= trimmed.len() {
                    marker(entry, &trimmed[..mark_len], column);
                    if ty == "footnote" {
                        let rest = &trimmed[mark_len..];
                        entry.insert(
                            "metadataSeparatorRaw".into(),
                            format!(
                                ":{}",
                                &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
                            )
                            .into(),
                        );
                    }
                    let mut content = mark_at + mark_len;
                    while source
                        .as_bytes()
                        .get(content)
                        .is_some_and(|b| *b == b' ' || *b == b'\t')
                    {
                        content += 1;
                    }
                    if ty == "list_item" && source[content..].starts_with('[') {
                        if let Some(close) = source[content..].find(']') {
                            content += close + 1;
                        }
                        while source
                            .as_bytes()
                            .get(content)
                            .is_some_and(|b| *b == b' ' || *b == b'\t')
                        {
                            content += 1;
                        }
                    }
                    if content < line_end(source, start) {
                        let content_col = column_of(source, content, quotes);
                        entry.insert("contentColumn".into(), content_col.into());
                        entry.insert(
                            "continuationStyle".into(),
                            continuation_style(source, start, end, content_col, quotes).into(),
                        );
                    } else {
                        entry.insert("continuationStyle".into(), "marker_line".into());
                    }
                }
            }
            "definition_term" => {
                if trimmed.starts_with("::") {
                    marker(entry, "::", column);
                    let rest = &trimmed[2..];
                    entry.insert(
                        "metadataSeparatorRaw".into(),
                        rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()].into(),
                    );
                }
            }
            "block_quote" => {
                entry.insert("fenced".into(), trimmed.starts_with(":::").into());
                if trimmed.starts_with(":::") {
                    fence(entry, source, start, end, ':');
                }
            }
            "code_block" | "raw_block" | "frontmatter" | "line_block" | "comment" | "div"
            | "admonition" | "directive" => {
                if let Some(open) = opener(source, start, end) {
                    fence(entry, source, open, end, source.as_bytes()[open] as char);
                }
                if ty == "div"
                    && node
                        .pointer("/attrs/classes")
                        .and_then(Value::as_array)
                        .is_some_and(|a| a.iter().any(|v| v == "hardbreaks"))
                {
                    entry.insert(
                        "hardBreakOpener".into(),
                        trimmed.starts_with("::: \\").into(),
                    );
                }
            }
            "link_reference_definition" | "abbreviation_def" => {
                if let Some(close) = trimmed.find(":") {
                    marker(entry, &trimmed[..=close], column);
                    let rest = &trimmed[close + 1..];
                    entry.insert(
                        "metadataSeparatorRaw".into(),
                        format!(
                            ":{}",
                            &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
                        )
                        .into(),
                    );
                }
            }
            "caption" => {
                entry.insert("attachment".into(), "preceding".into());
            }
            "table" => {
                let open = opener(source, start, end);
                if let Some(open) = open.filter(|&at| source[at..].starts_with("```")) {
                    if source[start..open].contains("header-rows=") {
                        let head = node
                            .get("children")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .filter(|group| {
                                group.get("type").and_then(Value::as_str) == Some("table_head")
                            })
                            .map(|group| {
                                group
                                    .get("children")
                                    .and_then(Value::as_array)
                                    .map_or(0, Vec::len)
                            })
                            .sum::<usize>();
                        entry.insert("headerRows".into(), head.into());
                    }
                }
                entry.insert(
                    "sourceFormat".into(),
                    if let Some(open) = open.filter(|&at| source[at..].starts_with("```")) {
                        if source[open..line_end(source, open)].contains("tsv") {
                            "tsv"
                        } else {
                            "csv"
                        }
                    } else {
                        "pipe"
                    }
                    .into(),
                );
            }
            "include" => {
                let mut spellings = Map::new();
                let raw = &source[start..end];
                let options = node.get("options").and_then(Value::as_object);
                let mut at = 0;
                while let Some(found) = raw[at..].find('@') {
                    let key_start = at + found + 1;
                    let Some(colon) = raw[key_start..].find(':') else {
                        break;
                    };
                    let key_end = key_start + colon;
                    let key = &raw[key_start..key_end];
                    let value_start = key_end + 1;
                    if options.is_some_and(|o| o.contains_key(key)) {
                        let bytes = raw.as_bytes();
                        let mut stop = value_start;
                        if bytes.get(stop) == Some(&b'"') {
                            stop += 1;
                            while stop < bytes.len() {
                                if bytes[stop] == b'\\' {
                                    stop += 2;
                                    continue;
                                }
                                if bytes[stop] == b'"' {
                                    stop += 1;
                                    break;
                                }
                                stop += 1;
                            }
                        } else {
                            while stop < bytes.len()
                                && !matches!(bytes[stop], b' ' | b'\t' | b'\r' | b'\n' | b'}')
                            {
                                stop += 1;
                            }
                        }
                        if let Some(value) = raw.get(value_start..stop) {
                            spellings.insert(key.into(), value.into());
                        }
                    }
                    at = value_start;
                    if at >= raw.len() {
                        break;
                    }
                }
                if !spellings.is_empty() {
                    entry.insert("optionSpelling".into(), Value::Object(spellings));
                }
            }
            _ => {}
        }
        if ty == "comment" && quotes > 0 && node.get("form").and_then(Value::as_str) == Some("line")
        {
            let line = &source[line_start(source, start)..start];
            if let Some(mark) = line.find('>') {
                let at = line_start(source, start) + mark;
                entry.insert(
                    "markerColumn".into(),
                    column_of(source, at, quotes - 1).into(),
                );
            }
        }
    }

    fn blank_before(&self, start: usize, floor: usize, quotes: usize) -> usize {
        let mut at = line_start(self.source, start);
        let floor = line_start(self.source, floor);
        let mut count = 0;
        while at > floor {
            let prev = line_start(self.source, at - 1);
            if !blank_inside(self.source, prev, line_end(self.source, prev), quotes) {
                break;
            }
            count += 1;
            at = prev;
        }
        count
    }

    fn blank_after(&self, end: usize, limit: usize, quotes: usize, open: bool) -> usize {
        let mut at = match self.source[end..].find('\n') {
            Some(n) => end + n + 1,
            None => return 0,
        };
        let mut count = 0;
        while (at < limit || open)
            && at < self.source.len()
            && blank_inside(self.source, at, line_end(self.source, at), quotes)
        {
            count += 1;
            at = match self.source[at..].find('\n') {
                Some(n) => at + n + 1,
                None => break,
            };
        }
        count
    }

    fn marked_blank_end(&self, end: usize, quotes: usize) -> usize {
        let mut at = match self.source[end..].find('\n') {
            Some(n) => end + n + 1,
            None => return end,
        };
        let mut last = end;
        while at < self.source.len()
            && blank_inside(self.source, at, line_end(self.source, at), quotes)
        {
            last = line_end(self.source, at);
            at = match self.source[at..].find('\n') {
                Some(n) => at + n + 1,
                None => break,
            };
        }
        last
    }

    fn raw_ref(
        &mut self,
        ty: &str,
        path: &str,
        start: usize,
        end: usize,
        byte_start: usize,
        byte_end: usize,
        quotes: usize,
    ) {
        if end > self.source.len() || start >= end {
            return;
        }
        let from = start + usize::from(ty == "image");
        let spelling = &self.source[from..end];
        let mut raw = String::new();
        for (index, line) in spelling.split('\n').enumerate() {
            if index > 0 {
                raw.push('\n');
            }
            let mut rest = line;
            for _ in 0..quotes {
                rest = rest.trim_start_matches([' ', '\t']);
                if let Some(after) = rest.strip_prefix('>') {
                    rest = after.strip_prefix(' ').unwrap_or(after);
                }
            }
            raw.push_str(rest);
        }
        if let Some(last) = raw.rfind(']') {
            raw.truncate(last + 1);
        }
        self.nodes
            .push(json!({"path":path,"startByte":byte_start,"endByte":byte_end,"rawRef":raw}));
    }

    fn table_rows(&mut self, table: &Value, path: &str, start: usize, end: usize) {
        let Some((_, data)) = self
            .tables
            .iter()
            .find(|(_, data)| {
                data.rows
                    .first()
                    .is_some_and(|r| r.start >= start && r.start < end)
                    && data.rows.last().is_some_and(|r| r.end <= end)
            })
            .cloned()
        else {
            return;
        };
        let mut row_index = 0;
        for (group_index, group) in table
            .get("children")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let Some(rows) = group.get("children").and_then(Value::as_array) else {
                continue;
            };
            for (index, node) in rows.iter().enumerate() {
                if node.get("type").and_then(Value::as_str) != Some("table_row") {
                    continue;
                }
                let Some(row) = data.rows.get(row_index) else {
                    return;
                };
                row_index += 1;
                let row_path = format!("{path}/children/{group_index}/children/{index}");
                let lines = self.source[row.start..row.end]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count();
                if lines > 0 {
                    self.nodes.push(json!({"path":row_path,"startByte":row.start,"endByte":row.end,"continuationLines":lines}));
                }
                let Some(cells) = node.get("children").and_then(Value::as_array) else {
                    continue;
                };
                for (column, cell) in cells.iter().enumerate() {
                    let Some(origin) = row.cells.get(column) else {
                        continue;
                    };
                    let cell_path = format!("{row_path}/children/{column}");
                    self.table_cell(origin, row, cell, &cell_path);
                }
            }
        }
    }

    fn table_cell(
        &mut self,
        origin: &crate::block::table::Cell,
        row: &Row,
        node: &Value,
        path: &str,
    ) {
        if !origin.joined {
            return;
        }
        let Some(inlines) = node
            .pointer("/children/0/children")
            .and_then(Value::as_array)
        else {
            return;
        };
        let Some(first) = inlines.first() else {
            return;
        };
        let mut wraps = Vec::new();
        let mut offset = 0;
        for (index, piece) in origin.pieces.iter().enumerate() {
            let line = self.source[row.start..piece.start]
                .bytes()
                .filter(|b| *b == b'\n')
                .count();
            if line > 0 {
                if index > 0 && offset > 0 && piece.join == crate::inline::Join::Cell {
                    offset += 1;
                }
                wraps.push(json!({"line":line,"path":format!("{path}/children/0/children/0"),"offset":offset}));
            }
            offset += self
                .source
                .get(piece.start..piece.end)
                .unwrap_or("")
                .trim_end_matches([' ', '\t'])
                .len();
        }
        if wraps.is_empty() {
            return;
        }
        let last = origin.pieces.last().map_or(origin.end, |p| p.end);
        let cell_end = self.source[last..line_end(self.source, last)]
            .find('|')
            .map_or(last, |n| last + n);
        self.nodes
            .push(json!({"path":path,"startByte":origin.start,"endByte":cell_end,"wraps":wraps}));
        let _ = first;
    }
}

fn is_block(ty: &str) -> bool {
    matches!(
        ty,
        "paragraph"
            | "heading"
            | "thematic_break"
            | "list"
            | "list_item"
            | "definition_list"
            | "definition_term"
            | "definition_description"
            | "footnote"
            | "block_quote"
            | "div"
            | "admonition"
            | "directive"
            | "code_block"
            | "raw_block"
            | "frontmatter"
            | "line_block"
            | "comment"
            | "table"
            | "caption"
            | "include"
            | "link_reference_definition"
            | "abbreviation_def"
    )
}

fn block_children(ty: &str) -> bool {
    matches!(
        ty,
        "document"
            | "block_quote"
            | "div"
            | "admonition"
            | "directive"
            | "list"
            | "list_item"
            | "definition_list"
            | "definition_description"
            | "footnote"
    )
}

fn required_blanks(ty: &str, frame: Frame) -> bool {
    ty == "caption"
        || (!frame.cell
            && !frame.caption
            && (frame.blocks
                || matches!(
                    ty,
                    "list_item" | "definition_term" | "definition_description"
                )))
}

fn opener(source: &str, start: usize, end: usize) -> Option<usize> {
    let mut at = start;
    while at < end {
        let line = &source[at..line_end(source, at).min(end)];
        let trim = line.trim_start_matches([' ', '\t', '>']);
        if trim.starts_with("```")
            || trim.starts_with(":::")
            || trim.starts_with("%%%")
            || trim.starts_with("---")
        {
            return Some(at + line.len() - trim.len());
        }
        at = source[at..].find('\n').map_or(end, |n| at + n + 1);
    }
    None
}

fn marker(entry: &mut Map<String, Value>, raw: &str, column: usize) {
    entry.insert("markerRaw".into(), raw.into());
    entry.insert("markerColumn".into(), column.into());
}

fn line_start(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .rfind('\n')
        .map_or(0, |n| n + 1)
}
fn line_end(source: &str, offset: usize) -> usize {
    let feed = source[offset.min(source.len())..]
        .find('\n')
        .map_or(source.len(), |n| offset + n);
    if feed > offset && source.as_bytes().get(feed - 1) == Some(&b'\r') {
        feed - 1
    } else {
        feed
    }
}
fn blank_inside(source: &str, start: usize, end: usize, quotes: usize) -> bool {
    let mut text = source[start..end].trim();
    for _ in 0..quotes {
        if let Some(rest) = text.strip_prefix('>') {
            text = rest.trim_start();
        } else {
            return false;
        }
    }
    text.is_empty()
}
fn column_of(source: &str, offset: usize, quotes: usize) -> usize {
    let line = &source[line_start(source, offset)..offset];
    let mut chars = line.char_indices().peekable();
    let mut column: usize = 0;
    let mut origin = 0;
    for _ in 0..quotes {
        while let Some((_, ch)) = chars.peek().copied() {
            if ch == ' ' || ch == '\t' {
                column = advance(column, ch);
                chars.next();
            } else {
                break;
            }
        }
        if chars.peek().is_none_or(|(_, ch)| *ch != '>') {
            break;
        }
        chars.next();
        column += 1;
        if chars.peek().is_some_and(|(_, ch)| *ch == ' ') {
            chars.next();
            column += 1;
        }
        origin = column;
    }
    for (_, ch) in chars {
        column = advance(column, ch);
    }
    column.saturating_sub(origin)
}

fn advance(column: usize, ch: char) -> usize {
    if ch == '\t' {
        column + 4 - column % 4
    } else if ch == '\u{feff}' && column == 0 {
        column
    } else {
        column + 1
    }
}
fn list_marker_len(line: &str) -> usize {
    if line.starts_with(['-', '*']) {
        return 1;
    }
    if line.starts_with('.') {
        return 1;
    }
    line.find(['.', ')']).map_or(0, |n| n + 1)
}
fn continuation_style(
    source: &str,
    start: usize,
    end: usize,
    column: usize,
    quotes: usize,
) -> &'static str {
    let first = match source[start..end].find('\n') {
        Some(n) => start + n + 1,
        None => return "marker_line",
    };
    let mut indent = false;
    for line in source[first..end].split('\n') {
        let line_start = first + source[first..end].find(line).unwrap_or(0);
        if line.trim().is_empty() {
            continue;
        }
        let at = line_start + line.len() - line.trim_start_matches([' ', '\t', '>']).len();
        let at_col = column_of(source, at, quotes);
        if at_col < column {
            return "lazy";
        }
        indent = true;
    }
    if indent { "indent" } else { "marker_line" }
}
fn fence(entry: &mut Map<String, Value>, source: &str, start: usize, end: usize, marker: char) {
    let line = &source[start..line_end(source, start).min(end)];
    let len = line.chars().take_while(|c| *c == marker).count();
    if len < 3 {
        return;
    }
    entry.insert("openerRaw".into(), line[..len].into());
    let last = &source[line_start(source, end)..end];
    let trimmed = last.trim().trim_start_matches('>').trim_start();
    entry.insert(
        "closerRaw".into(),
        if trimmed.starts_with(marker) && trimmed.chars().all(|c| c == marker) {
            trimmed
        } else {
            ""
        }
        .into(),
    );
}
