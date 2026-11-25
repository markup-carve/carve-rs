//! An opt-in walk of the resolved tree and its source origins.
// Byte indices below come from checked parser positions or bounded byte scans.
#![allow(clippy::indexing_slicing)]

mod validate;
pub(super) use validate::validate;

use super::{Error, Input, Tree};
use crate::block::events::{Event, LeafKind};
use crate::block::table::{Row, TableData};
use serde_json::{Map, Value, json};

pub(super) fn build(source: &str, tree: &Tree, input: Input<'_>) -> Result<Value, Error> {
    let document =
        super::deep_json(tree.json()).map_err(|error| Error::option("tree", error.to_string()))?;
    Ok(build_from(source, &document, input))
}

/// [`build`] from the encoded tree as a value.
pub(super) fn build_from(source: &str, document: &Value, input: Input<'_>) -> Value {
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
        tables: ScannedTables::new(source),
    };
    walk.visit(document, "", Frame::document(source));
    json!({
        "version": 2, "encoding": encoding, "source": original,
        "lineEndings": line_endings(source), "bom": source.starts_with('\u{feff}'),
        "nodes": walk.nodes,
    })
}

/// The layout entries of a parsed UTF-8 document, as the lint model reads them (C2-W-150).
pub(crate) fn entries(source: &str, document: &Value) -> Vec<Value> {
    let mut walk = Walk {
        source,
        nodes: Vec::new(),
        offsets: None,
        tables: ScannedTables::new(source),
    };
    walk.visit(document, "", Frame::document(source));
    walk.nodes
}

/// The source format of the table whose source runs from `start` to `end` (C2-W-149).
pub(crate) fn table_source_format(source: &str, start: usize, end: usize) -> &'static str {
    if start > end || end > source.len() {
        return "pipe";
    }
    source_format(source, opener(source, start, end))
}

fn source_format(source: &str, open: Option<usize>) -> &'static str {
    match open.filter(|&at| source[at..].starts_with("```")) {
        Some(open) if source[open..line_end(source, open)].contains("tsv") => "tsv",
        Some(_) => "csv",
        None => "pipe",
    }
}

/// The reference of a link or image with a key as C2-L-022 bounds it, without the container prefixes of its lines.
pub(crate) fn raw_ref_text(
    source: &str,
    ty: &str,
    start: usize,
    end: usize,
    quotes: usize,
) -> String {
    let from = start + usize::from(ty == "image");
    let spelling = &source[from..end];
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
    if let Some(stop) = reference_label_end(&raw) {
        raw.truncate(stop);
    }
    raw
}

/// The `markerColumn` and `contentColumn` facts of a list item, description or footnote entry, as the
/// layout walk writes them, for the lint model.
pub(crate) fn item_columns(
    source: &str,
    ty: &str,
    start: usize,
    end: usize,
    quotes: usize,
) -> (Option<usize>, Option<usize>) {
    if start > end || end > source.len() {
        return (None, None);
    }
    let first = &source[start..line_end(source, start).min(end)];
    let mark_start = if first.trim_start().starts_with('{') && line_end(source, start) < end {
        line_end(source, start) + 1
    } else {
        start
    };
    let line = &source[mark_start..line_end(source, mark_start).min(end)];
    let trimmed = line.trim_start_matches([' ', '\t']);
    let mark_at = mark_start + line.len() - trimmed.len();
    let mark_len = match ty {
        "list_item" => list_marker_len(trimmed),
        "definition_description" => 1,
        _ => trimmed.find(':').map_or(0, |n| n + 1),
    };
    if mark_len == 0 || mark_len > trimmed.len() {
        return (None, None);
    }
    let marker_content = skip_layout_space(source, mark_at + mark_len);
    let content = item_content_start(ty, source, marker_content);
    let line_end = line_end(source, start);
    let content = if content < line_end {
        Some(content)
    } else {
        (ty == "list_item" && marker_content < line_end).then_some(marker_content)
    };
    (
        Some(column_of(source, mark_at, quotes)),
        content.map(|at| column_of(source, at, quotes)),
    )
}

/// The `blankLinesAfter` fact of a block whose parent is not a quote, as the layout walk writes it.
pub(crate) fn blank_lines_after(source: &str, end: usize, limit: usize, quotes: usize) -> usize {
    let walk = Walk {
        source,
        nodes: Vec::new(),
        tables: ScannedTables::default(),
        offsets: None,
    };
    walk.blank_after(end.min(source.len()), limit, quotes, false)
}

/// The tables of a source as the block scan reads them, with the first row start of each that has
/// rows, so a table node finds its scanned table by binary search instead of a scan (carve#6489).
#[derive(Default)]
pub(crate) struct ScannedTables {
    tables: Vec<(usize, TableData)>,
    /// (first row start, index) of the tables with rows, in scan order.
    firsts: Vec<(usize, usize)>,
    sorted: bool,
}

impl ScannedTables {
    pub(crate) fn new(source: &str) -> Self {
        let tables = tables(source);
        let firsts: Vec<(usize, usize)> = tables
            .iter()
            .enumerate()
            .filter_map(|(at, (_, data))| Some((data.rows.first()?.start, at)))
            .collect();
        let sorted = firsts.windows(2).all(|pair| pair[0].0 <= pair[1].0);
        Self {
            tables,
            firsts,
            sorted,
        }
    }

    /// The first scanned table whose rows lie in the table node that spans `start..end`.
    fn find(&self, start: usize, end: usize) -> Option<&TableData> {
        let fits = |&(first, at): &(usize, usize)| {
            let data = &self.tables[at].1;
            (first >= start && first < end && data.rows.last().is_some_and(|r| r.end <= end))
                .then_some(data)
        };
        if !self.sorted {
            return self.firsts.iter().find_map(fits);
        }
        let from = self.firsts.partition_point(|&(first, _)| first < start);
        self.firsts[from..]
            .iter()
            .take_while(|&&(first, _)| first < end)
            .find_map(fits)
    }
}

/// The span of each row with continuation lines of the table node that spans `start..end`, by its
/// group and row index: the row entries [`entries`] gives that table, without building and walking
/// the whole tree (carve#6489).
pub(crate) fn continuation_rows(
    source: &str,
    tables: &ScannedTables,
    table: crate::view::V<'_>,
    start: usize,
    end: usize,
) -> Vec<((usize, usize), (usize, usize))> {
    let mut spans = Vec::new();
    let Some(data) = tables.find(start, end) else {
        return spans;
    };
    let mut row_index = 0;
    let groups = table.get("children").and_then(crate::view::V::as_array);
    for (group_index, group) in groups
        .into_iter()
        .flat_map(crate::view::Arr::iter)
        .enumerate()
    {
        let Some(rows) = group.get("children").and_then(crate::view::V::as_array) else {
            continue;
        };
        for (index, node) in rows.iter().enumerate() {
            if node.ty().as_str() != Some("table_row") {
                continue;
            }
            let Some(row) = data.rows.get(row_index) else {
                return spans;
            };
            row_index += 1;
            if continuation_lines(source, row.start, row.end) > 0 {
                spans.push(((group_index, index), (row.start, row.end)));
            }
        }
    }
    spans
}

fn tables(source: &str) -> Vec<(usize, TableData)> {
    if !source.contains('|') {
        return Vec::new();
    }
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
#[allow(clippy::struct_excessive_bools)]
struct Frame {
    start: usize,
    end: usize,
    quotes: usize,
    blocks: bool,
    cell: bool,
    caption: bool,
    table: bool,
    open: bool,
}

impl Frame {
    const fn document(source: &str) -> Self {
        Self {
            start: 0,
            end: source.len(),
            quotes: 0,
            blocks: true,
            cell: false,
            caption: false,
            table: false,
            open: false,
        }
    }
}

struct Walk<'a> {
    source: &'a str,
    nodes: Vec<Value>,
    tables: ScannedTables,
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
            .and_then(|n| usize::try_from(n).ok());
        let byte_end = pos
            .and_then(|p| p.get("endOffset"))
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok());
        let start = byte_start.map(|n| self.text_offset(n));
        let end = byte_end.map(|n| self.text_offset(n));
        let mut inner = frame;
        if let (Some(start), Some(end)) = (start, end) {
            inner = self.visit_block(
                ty,
                node,
                path,
                start,
                end,
                byte_start.unwrap_or(start),
                byte_end.unwrap_or(end),
                frame,
            );
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
        self.visit_slots(node, path, ty, frame, inner);
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_block(
        &mut self,
        ty: &str,
        node: &Value,
        path: &str,
        start: usize,
        end: usize,
        byte_start: usize,
        byte_end: usize,
        mut frame: Frame,
    ) -> Frame {
        if start > end
            || end > self.source.len()
            || !is_block(ty)
            || frame.cell
            || frame.caption && ty != "caption"
            || ty == "comment"
                && !(frame.blocks || frame.table && table_comment(self.source, start))
        {
            return frame;
        }
        let mut entry = Map::new();
        entry.insert("path".into(), path.into());
        entry.insert("startByte".into(), byte_start.into());
        entry.insert("endByte".into(), byte_end.into());
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
        frame.start = start;
        frame.end = end;
        frame.quotes += usize::from(
            ty == "block_quote" && !self.source[start..end].trim_start().starts_with(":::"),
        );
        frame.open = ty == "block_quote";
        frame
    }

    fn visit_slots(&mut self, node: &Value, path: &str, ty: &str, frame: Frame, inner: Frame) {
        for slot in ["title", "children", "caption"] {
            if ty == "include" && slot == "children" {
                continue;
            }
            let Some(value) = node.get(slot) else {
                continue;
            };
            let child_frame = Frame {
                blocks: slot == "children" && block_children(ty),
                cell: frame.cell || ty == "table_cell",
                caption: frame.caption || ty == "caption" || slot == "caption",
                table: ty == "table_body",
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

    // Each arm measures one independent block family.
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
        let first = &source[start..line_end(source, start).min(end)];
        let mark_start = if first.trim_start().starts_with('{') && line_end(source, start) < end {
            line_end(source, start) + 1
        } else {
            start
        };
        let line = &source[mark_start..line_end(source, mark_start).min(end)];
        let trimmed = line.trim_start_matches([' ', '\t']);
        let mark_at = mark_start + line.len() - trimmed.len();
        let column = column_of(source, mark_at, quotes);
        match ty {
            "heading" => heading_facts(trimmed, column, entry),
            "thematic_break" => marker(entry, trimmed.trim_end(), column),
            "list" => self.list_facts(node, start, entry),
            "list_item" | "definition_description" | "footnote" => self.item_facts(
                ty, node, start, end, quotes, mark_at, column, trimmed, entry,
            ),
            "definition_term" => term_facts(trimmed, column, entry),
            "block_quote" => quote_facts(source, start, end, trimmed, entry),
            "code_block" | "raw_block" | "frontmatter" | "line_block" | "comment" | "div"
            | "admonition" | "directive" => self.fence_facts(ty, node, start, end, entry),
            "link_reference_definition" | "abbreviation_def" => {
                definition_facts(trimmed, column, entry);
            }
            "caption" => {
                entry.insert("attachment".into(), "preceding".into());
            }
            "table" => self.table_facts(node, start, end, entry),
            "include" => self.include_facts(node, start, end, entry),
            _ => {}
        }
        comment_column(ty, node, source, start, quotes, entry);
    }

    #[allow(clippy::too_many_arguments)]
    fn item_facts(
        &self,
        ty: &str,
        node: &Value,
        start: usize,
        end: usize,
        quotes: usize,
        mark_at: usize,
        column: usize,
        trimmed: &str,
        entry: &mut Map<String, Value>,
    ) {
        let source = self.source;
        let mark_len = if ty == "list_item" {
            list_marker_len(trimmed)
        } else if ty == "definition_description" {
            1
        } else {
            trimmed.find(':').map_or(0, |n| n + 1)
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
            let marker_content = skip_layout_space(source, mark_at + mark_len);
            let content = item_content_start(ty, source, marker_content);
            content_facts(
                source,
                node,
                ty,
                start,
                end,
                quotes,
                marker_content,
                content,
                entry,
            );
        }
    }

    fn include_facts(
        &self,
        node: &Value,
        start: usize,
        end: usize,
        entry: &mut Map<String, Value>,
    ) {
        let source = self.source;
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
            let key = raw[key_start..key_end]
                .rsplit('@')
                .next()
                .unwrap_or("")
                .trim();
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

    fn table_facts(&self, node: &Value, start: usize, end: usize, entry: &mut Map<String, Value>) {
        let source = self.source;
        let open = opener(source, start, end);
        if let Some(open) = open.filter(|&at| source[at..].starts_with("```")) {
            if source[start..open].contains("header-rows=") {
                let head = node
                    .get("children")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter(|group| group.get("type").and_then(Value::as_str) == Some("table_head"))
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
        entry.insert("sourceFormat".into(), source_format(source, open).into());
    }

    fn fence_facts(
        &self,
        ty: &str,
        node: &Value,
        start: usize,
        end: usize,
        entry: &mut Map<String, Value>,
    ) {
        let source = self.source;
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
                source[start..end]
                    .lines()
                    .any(|line| line.trim_start().starts_with("::: \\"))
                    .into(),
            );
        }
    }

    fn list_facts(&self, node: &Value, start: usize, entry: &mut Map<String, Value>) {
        if node.pointer("/marker/kind").and_then(Value::as_str) == Some("ordered") {
            let first = node
                .pointer("/children/0/pos/startOffset")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
                .map_or(start, |n| self.text_offset(n));
            entry.insert(
                "bare".into(),
                self.source[first..line_end(self.source, first)]
                    .trim_start()
                    .starts_with(". ")
                    .into(),
            );
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

    #[allow(clippy::too_many_arguments)]
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
        let raw = raw_ref_text(self.source, ty, start, end, quotes);
        self.nodes
            .push(json!({"path":path,"startByte":byte_start,"endByte":byte_end,"rawRef":raw}));
    }

    fn table_rows(&mut self, table: &Value, path: &str, start: usize, end: usize) {
        let Some(mut data) = self.tables.find(start, end).cloned() else {
            return;
        };
        let groups = table
            .get("children")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let kinds: Vec<(u8, usize)> = groups
            .iter()
            .filter_map(|group| {
                let kind = match group.get("type").and_then(Value::as_str)? {
                    "table_head" => 0,
                    "table_body" => 1,
                    "table_foot" => 2,
                    _ => return None,
                };
                let rows = group.get("children").and_then(Value::as_array)?;
                let count = rows
                    .iter()
                    .filter(|row| row.get("type").and_then(Value::as_str) == Some("table_row"))
                    .count();
                Some((kind, count))
            })
            .collect();
        // C2-B-184: a fragment under a span mark is a piece of the cell that covers it.
        crate::tree::table::join_span_pieces(&mut data.rows, &kinds);
        let mut row_index = 0;
        for (group_index, group) in groups.iter().enumerate() {
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
                let lines = continuation_lines(self.source, row.start, row.end);
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
                    self.table_cell(origin, &data.rows, row_index - 1, cell, &cell_path);
                }
            }
        }
    }

    fn table_cell(
        &mut self,
        origin: &crate::block::table::Cell,
        rows: &[Row],
        home: usize,
        node: &Value,
        path: &str,
    ) {
        if !origin.joined {
            return;
        }
        if node
            .pointer("/children/0/children")
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty)
        {
            return;
        }
        let spans_rows = node.get("rowspan").and_then(Value::as_u64).unwrap_or(1) > 1;
        let spans_columns = node.get("colspan").and_then(Value::as_u64).unwrap_or(1) > 1;
        let mut wraps = Vec::new();
        let mut offset = 0;
        let mut at = home;
        for (index, piece) in origin.pieces.iter().enumerate() {
            // A piece moved in from a span mark can stand in a row below the cell's own.
            if rows[at].start > piece.start {
                at = home;
            }
            while at + 1 < rows.len() && rows[at].end < piece.start {
                at += 1;
            }
            let line = continuation_lines(self.source, rows[at].start, piece.start);
            if line > 0 {
                if index > 0 && offset > 0 && piece.join == crate::inline::Join::Cell {
                    offset += 1;
                }
                let mut wrap = json!({"line":line,"path":format!("{path}/children/0/children/0"),"offset":offset});
                // C2-L-020: a wrap in a cell that spans rows names the row of its fragment.
                if spans_rows {
                    wrap["row"] = (at - home).into();
                }
                wraps.push(wrap);
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
        // C2-L-048: by line, and the wraps of one line by row.
        if spans_rows || spans_columns {
            wraps.sort_by_key(|w| w["line"].as_u64());
        }
        // C2-L-013: the range ends at the last piece in the source.
        let last = origin
            .pieces
            .iter()
            .map(|p| p.end)
            .max()
            .unwrap_or(origin.end);
        let cell_end = self.source[last..line_end(self.source, last)]
            .find('|')
            .map_or(last, |n| last + n);
        self.nodes.push(
            json!({"path":path,"startByte":origin.start,"endByte":cell_end.max(origin.end),"wraps":wraps}),
        );
    }
}

fn skip_layout_space(source: &str, mut at: usize) -> usize {
    while source
        .as_bytes()
        .get(at)
        .is_some_and(|b| *b == b' ' || *b == b'\t')
    {
        at += 1;
    }
    at
}

fn item_content_start(ty: &str, source: &str, mut content: usize) -> usize {
    if ty == "list_item" && source[content..].starts_with("{.") {
        if let Some(close) = source[content..].find('}') {
            content += close + 1;
        }
        content = skip_layout_space(source, content);
    }
    // C2-L-051: the task box is item text and leaves the content column alone.
    content
}

#[allow(clippy::too_many_arguments)]
fn content_facts(
    source: &str,
    node: &Value,
    ty: &str,
    start: usize,
    end: usize,
    quotes: usize,
    marker_content: usize,
    content: usize,
    entry: &mut Map<String, Value>,
) {
    let line_end = line_end(source, start);
    if content < line_end || ty == "list_item" && marker_content < line_end {
        let content_col = column_of(
            source,
            if content < line_end {
                content
            } else {
                marker_content
            },
            quotes,
        );
        entry.insert("contentColumn".into(), content_col.into());
        entry.insert(
            "continuationStyle".into(),
            continuation_style(source, node, start, end, content_col, quotes).into(),
        );
    } else {
        entry.insert("continuationStyle".into(), "marker_line".into());
    }
}

fn heading_facts(trimmed: &str, column: usize, entry: &mut Map<String, Value>) {
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

fn definition_facts(trimmed: &str, column: usize, entry: &mut Map<String, Value>) {
    if let Some(close) = trimmed.find(':') {
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

fn term_facts(trimmed: &str, column: usize, entry: &mut Map<String, Value>) {
    if let Some(rest) = trimmed.strip_prefix("::") {
        marker(entry, "::", column);
        entry.insert(
            "metadataSeparatorRaw".into(),
            rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()].into(),
        );
    }
}

fn quote_facts(
    source: &str,
    start: usize,
    end: usize,
    trimmed: &str,
    entry: &mut Map<String, Value>,
) {
    entry.insert("fenced".into(), trimmed.starts_with(":::").into());
    if trimmed.starts_with(":::") {
        fence(entry, source, start, end, ':');
    }
}

fn comment_column(
    ty: &str,
    node: &Value,
    source: &str,
    start: usize,
    quotes: usize,
    entry: &mut Map<String, Value>,
) {
    if ty == "comment" && quotes > 0 && node.get("form").and_then(Value::as_str) == Some("line") {
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

fn reference_label_end(raw: &str) -> Option<usize> {
    fn close(raw: &[u8], from: usize) -> Option<usize> {
        if raw.get(from) != Some(&b'[') {
            return None;
        }
        let mut depth = 1usize;
        let mut at = from + 1;
        while at < raw.len() {
            match raw[at] {
                b'\\' => at += 1,
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(at + 1);
                    }
                }
                _ => {}
            }
            at += 1;
        }
        None
    }
    let bytes = raw.as_bytes();
    let first = close(bytes, 0)?;
    if bytes.get(first) == Some(&b'[') {
        close(bytes, first)
    } else {
        Some(first)
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
            | "figure_group"
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
            | "figure_group"
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
                || frame.table
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
fn table_comment(source: &str, start: usize) -> bool {
    source[line_start(source, start)..start].trim().is_empty()
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
fn continuation_lines(source: &str, start: usize, end: usize) -> usize {
    source[start..end]
        .split('\n')
        .skip(1)
        .filter(|line| line.trim_start_matches([' ', '\t', '>']).starts_with('+'))
        .count()
}
fn continuation_style(
    source: &str,
    node: &Value,
    start: usize,
    end: usize,
    column: usize,
    quotes: usize,
) -> &'static str {
    let first = match source[start..end].find('\n') {
        Some(n) => start + n + 1,
        None => return "marker_line",
    };
    if has_lazy_paragraph(node, source, first, column, quotes) {
        return "lazy";
    }
    let mut later = false;
    let mut line_start = first;
    while line_start < end {
        let line_end = source[line_start..end]
            .find('\n')
            .map_or(end, |n| line_start + n);
        let line = &source[line_start..line_end];
        let next = line_end.saturating_add(1);
        line_start = next;
        if line.trim().is_empty() {
            continue;
        }
        later = true;
        let at = line_end - line.trim_start_matches([' ', '\t', '>']).len();
        if column_of(source, at, quotes) >= column {
            return "indent";
        }
    }
    if later { "none" } else { "marker_line" }
}

fn has_lazy_paragraph(
    node: &Value,
    source: &str,
    first: usize,
    column: usize,
    quotes: usize,
) -> bool {
    fn visit(
        node: &Value,
        source: &str,
        first: usize,
        column: usize,
        quotes: usize,
        paragraph: bool,
    ) -> bool {
        let ty = node.get("type").and_then(Value::as_str).unwrap_or("");
        if ty == "table" {
            return false;
        }
        let paragraph = paragraph || ty == "paragraph";
        if paragraph
            && ty != "paragraph"
            && node.get("children").and_then(Value::as_array).is_none()
            && let Some(at) = node
                .pointer("/pos/startOffset")
                .and_then(Value::as_u64)
                .and_then(|n| usize::try_from(n).ok())
            && at >= first
            && at <= source.len()
            && column_of(source, at, quotes) < column
        {
            return true;
        }
        node.get("children")
            .and_then(Value::as_array)
            .is_some_and(|children| {
                children
                    .iter()
                    .any(|child| visit(child, source, first, column, quotes, paragraph))
            })
    }
    visit(node, source, first, column, quotes, false)
}
fn fence(entry: &mut Map<String, Value>, source: &str, start: usize, end: usize, marker: char) {
    let line = &source[start..line_end(source, start).min(end)];
    let len = line.chars().take_while(|c| *c == marker).count();
    if len < 3 {
        return;
    }
    entry.insert("openerRaw".into(), line[..len].into());
    let mut close = "";
    for candidate in source[start + line.len()..end].lines() {
        let trimmed = candidate.trim().trim_start_matches('>').trim_start();
        if trimmed.starts_with(marker) && trimmed.chars().all(|c| c == marker) {
            close = trimmed;
        }
    }
    entry.insert("closerRaw".into(), close.into());
}
