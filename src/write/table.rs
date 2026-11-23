//! Tables as Carve source (C2-W-061 to C2-W-068, C2-W-118, C2-W-119, C2-W-136):
//! a pipe table, with the continuation rows the sidecar records, or a delimited table.

use super::attrs::{BlockParts, brace_block, longest_bare_run, plain_block};
use super::inline::{IN_CELL, line_comment_text, write_inlines};
use super::node::{N, children, has, id, list, s, text, ty};
use super::state::{Writer, emit, lose, lose_key};
use super::width_data::{GCB, INCB_CONSONANT, INCB_EXTEND, INCB_LINKER, PICT, WIDE};
use crate::ast::Node as AstNode;
use crate::view::{Arr, V};

/// A plain pipe table whose emitted rows are their own read-back certificate.
/// The restricted alphabet cannot start a block or inline construct, and the
/// exact row spelling below fixes the table parser's cell boundaries.
macro_rules! plain_meta {
    ($n:expr) => {
        $n.attrs.is_none()
            && $n.data.is_none()
            && $n.dir.is_none()
            && $n.lang.is_none()
            && $n.metadata.is_none()
    };
}

pub(super) fn certifies_plain_rows(w: &Writer<'_>, group: Arr<'_>) -> bool {
    if w.align_tables
        || !w.abbreviations.is_empty()
        || !w.extensions.is_empty()
        || !w.sp.escapes.is_empty()
        || group.len() != 1
    {
        return false;
    }
    let Some(V::Node(AstNode::Table(table))) = group.first() else {
        return false;
    };
    certifies_plain_table(w, table)
}

fn certifies_plain_table(w: &Writer<'_>, table: &crate::ast::Table<'_>) -> bool {
    if !plain_meta!(table)
        || table.caption.is_some()
        || table.children.len() != 1
        || table.columns.is_empty()
        || !table.columns.iter().all(plain_column)
    {
        return false;
    }
    let Some(AstNode::TableBody(body)) = table.children.first() else {
        return false;
    };
    if !plain_meta!(body)
        || body.row_head_columns.is_some()
        || body.children.is_empty()
        || body.children.len() != w.lines.len()
    {
        return false;
    }
    body.children.iter().zip(&w.lines).all(|(node, written)| {
        let AstNode::TableRow(row) = node else {
            return false;
        };
        plain_row(row, table.columns.len(), written)
    })
}

fn plain_column(node: &AstNode<'_>) -> bool {
    let AstNode::TableColumn(c) = node else {
        return false;
    };
    plain_meta!(c) && c.align.is_none() && c.valign.is_none() && c.width.is_none()
}

fn plain_row(row: &crate::ast::TableRow<'_>, ncols: usize, written: &str) -> bool {
    if !plain_meta!(row) || row.children.len() != ncols {
        return false;
    }
    let Some(mut rest) = written.strip_prefix('|') else {
        return false;
    };
    for node in row.children.iter() {
        let AstNode::TableCell(cell) = node else {
            return false;
        };
        let Some(next) = plain_cell(cell, rest) else {
            return false;
        };
        rest = next;
    }
    rest.is_empty()
}

fn plain_cell_options(cell: &crate::ast::TableCell<'_>) -> bool {
    plain_meta!(cell)
        && !cell.header
        && cell.align.is_none()
        && cell.valign.is_none()
        && cell.span.is_none()
        && cell.colspan.is_none()
        && cell.rowspan.is_none()
        && cell.scope.is_none()
        && cell.header_ids.is_none()
}

fn plain_cell<'a>(cell: &crate::ast::TableCell<'_>, written: &'a str) -> Option<&'a str> {
    if !plain_cell_options(cell) || cell.children.len() != 1 {
        return None;
    }
    let AstNode::CellParagraph(paragraph) = cell.children.first()? else {
        return None;
    };
    if !plain_meta!(paragraph) || paragraph.children.len() > 1 {
        return None;
    }
    let value = match paragraph.children.first() {
        None => "",
        Some(AstNode::Text(t)) if plain_meta!(t) && !t.value.is_empty() => t.value,
        _ => return None,
    };
    let value = value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric())
        .then_some(value)?;
    consume_plain_cell(written, value)
}

fn consume_plain_cell<'a>(written: &'a str, value: &str) -> Option<&'a str> {
    let rest = written.strip_prefix(' ')?.strip_prefix(value)?;
    let rest = if value.is_empty() {
        rest
    } else {
        rest.strip_prefix(' ')?
    };
    rest.strip_prefix('|')
}

const MIN_RUN: usize = 3;

fn refs(nodes: Arr<'_>) -> Vec<V<'_>> {
    nodes.iter().collect()
}

fn rows_of<'t>(table: V<'t>, kind: &str) -> Vec<V<'t>> {
    children(table)
        .iter()
        .filter(|g| ty(g) == kind)
        .flat_map(|g| children(g).iter().filter(|r| ty(r) == "table_row"))
        .collect()
}

fn columns(table: V<'_>) -> Arr<'_> {
    list(table, "columns").unwrap_or_else(Arr::empty)
}

/// C2-B-212: a width as a percentage with the fewest digits that read back.
fn percentage_text(width: f64) -> String {
    for digits in 1..17 {
        let text = plain_decimal(round_to(width * 100.0, digits));
        if text
            .parse::<f64>()
            .is_ok_and(|v| (v / 100.0).to_bits() == width.to_bits())
        {
            return text;
        }
    }
    plain_decimal(width * 100.0)
}

fn round_to(value: f64, digits: usize) -> f64 {
    format!("{value:.*e}", digits.saturating_sub(1))
        .parse()
        .unwrap_or(value)
}

fn plain_decimal(value: f64) -> String {
    let text = format!("{value}");
    text.strip_suffix(".0").map_or(text.clone(), str::to_string)
}

fn column_entry(column: V<'_>, field: &str) -> String {
    match field {
        "align" => s(column, "align").unwrap_or("").to_string(),
        "valign" => match s(column, "valign") {
            Some("baseline") | None => String::new(),
            Some(v) => v.to_string(),
        },
        _ => column
            .get("width")
            .and_then(V::as_f64)
            .map_or(String::new(), percentage_text),
    }
}

fn column_list(table: V<'_>, field: &str) -> Option<String> {
    let cols = columns(table);
    let last = cols
        .iter()
        .rposition(|c| !column_entry(c, field).is_empty())?;
    Some(
        cols.iter()
            .take(last + 1)
            .map(|c| column_entry(c, field))
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn is_header_row(row: V<'_>) -> bool {
    ty(row) == "table_row"
        && children(row).iter().any(|c| super::node::flag(c, "header"))
        && children(row)
            .iter()
            .all(|c| super::node::flag(c, "header") || has(c, "span"))
}

fn leading_header_rows(body: V<'_>) -> usize {
    children(body)
        .iter()
        .filter(|r| ty(r) == "table_row")
        .take_while(|r| is_header_row(*r))
        .count()
}

fn bodies_of(table: V<'_>) -> Vec<V<'_>> {
    children(table)
        .iter()
        .filter(|g| ty(g) == "table_body")
        .collect()
}

fn partitioned(bodies: &[V<'_>]) -> bool {
    bodies.len() > 1
        || bodies.iter().any(|b| {
            !children(b).iter().any(|r| ty(r) == "table_row")
                || leading_header_rows(*b) > 0
                || has(b, "rowHeadColumns")
        })
}

fn body_keys(table: V<'_>) -> Vec<(String, String)> {
    let bodies = bodies_of(table);
    if !partitioned(&bodies) {
        return Vec::new();
    }
    let header: Vec<usize> = bodies.iter().map(|b| leading_header_rows(*b)).collect();
    let data: Vec<String> = bodies
        .iter()
        .zip(&header)
        .map(|(b, h)| (children(b).iter().filter(|r| ty(r) == "table_row").count() - h).to_string())
        .collect();
    let mut keys = vec![("body-rows".to_string(), data.join(","))];
    if header.iter().any(|&h| h > 0) {
        let h: Vec<String> = header.iter().map(ToString::to_string).collect();
        keys.push(("body-header-rows".into(), h.join(",")));
    }
    if bodies.iter().any(|b| has(b, "rowHeadColumns")) {
        let c: Vec<String> = bodies
            .iter()
            .map(|b| {
                b.get("rowHeadColumns")
                    .map_or(String::new(), |v| v.to_string())
            })
            .collect();
        keys.push(("body-header-cols".into(), c.join(",")));
    }
    keys
}

/// The keys of the attribute line that the tree holds in the columns and groups (C2-W-136).
fn table_keys(
    mut w: Option<&mut Writer<'_>>,
    table: V<'_>,
    first: Vec<(String, String)>,
) -> BlockParts {
    let mut keys = first;
    let aligns = column_list(table, "align");
    let valigns = column_list(table, "valign");
    if let Some(w) = w.as_deref_mut() {
        for c in columns(table) {
            if s(c, "valign") == Some("baseline") {
                lose(w, c, Some("valign"));
            }
        }
    }
    let widths = column_list(table, "width");
    let foot = rows_of(table, "table_foot").len();
    if let Some(a) = aligns {
        keys.push(("aligns".into(), a));
    }
    if let Some(v) = valigns {
        keys.push(("valigns".into(), v));
    }
    if let Some(wd) = widths {
        keys.push(("widths".into(), wd));
    }
    if foot > 0 {
        keys.push(("footer-rows".into(), foot.to_string()));
    }
    keys.extend(body_keys(table));
    let mut skip_keys = Vec::new();
    for (index, (key, _)) in super::node::key_values(table).into_iter().enumerate() {
        if !keys.iter().any(|(k, _)| k == key) {
            continue;
        }
        skip_keys.push(key.to_string());
        if let Some(w) = w.as_deref_mut() {
            lose_key(w, table, index, key);
        }
    }
    BlockParts {
        keys,
        skip_keys,
        ..BlockParts::default()
    }
}

fn cell_content<'t>(w: &mut Writer<'t>, cell: N<'t>, flags: u32) -> String {
    let blocks = children(cell);
    if blocks.len() > 1 || blocks.first().is_some_and(|b| ty(b) != "cell_paragraph") {
        lose(w, cell, None);
    }
    let mut content = String::new();
    for (at, b) in blocks.iter().enumerate() {
        if at > 0 {
            content.push(' ');
        }
        content.push_str(&write_inlines(w, &refs(children(b)), flags | IN_CELL));
    }
    if content == "^" || content == "<" {
        format!("\\{content}")
    } else {
        content
    }
}

fn cell_marks(w: &mut Writer<'_>, cell: V<'_>) -> String {
    let horizontal = match s(cell, "align") {
        Some("left") => "<",
        Some("right") => ">",
        Some("center") => "~",
        _ => "",
    };
    let valign = s(cell, "valign");
    let vertical = match valign {
        Some("top") => "^",
        Some("middle") => "~",
        Some("bottom") => "v",
        _ => "",
    };
    if valign.is_some() && vertical.is_empty() {
        lose(w, cell, Some("valign"));
    }
    if !vertical.is_empty() && horizontal.is_empty() {
        format!("?{vertical}")
    } else {
        format!("{horizontal}{vertical}")
    }
}

fn cell_prefix(w: &mut Writer<'_>, cell: V<'_>, bare: bool) -> String {
    let mark = cell_marks(w, cell);
    let head = if super::node::flag(cell, "header") && !bare {
        "="
    } else {
        ""
    };
    let block = brace_block(
        cell,
        &BlockParts {
            mark,
            pipes: true,
            ..BlockParts::default()
        },
    );
    if head.is_empty() {
        block
    } else {
        head.to_string() + &block
    }
}

fn segment(prefix: &str, content: &str) -> String {
    let mut out = String::with_capacity(prefix.len() + content.len() + 2);
    out.push_str(prefix);
    out.push(' ');
    if !content.is_empty() {
        out.push_str(content);
        out.push(' ');
    }
    out
}

fn fragment_lines(w: &Writer<'_>, row: V<'_>, cell: V<'_>, count: usize) -> Option<Vec<i64>> {
    let wraps = w.fact(cell)?.wraps.as_ref()?;
    let below = w.fact(row).and_then(|f| f.continuation_lines).unwrap_or(0);
    if wraps.len() != count {
        return None;
    }
    let mut lines = Vec::new();
    for wrap in wraps {
        let previous = lines.last().copied().unwrap_or(0);
        if wrap.row.unwrap_or(0) != 0 || wrap.line < 1 || wrap.line > below || wrap.line <= previous
        {
            return None;
        }
        lines.push(wrap.line);
    }
    Some(lines)
}

struct WrittenCell {
    prefix: String,
    first: String,
    extra: Vec<String>,
    places: Vec<i64>,
}

struct WrittenRow {
    lines: Vec<Vec<String>>,
    attribute: String,
    direct: Option<String>,
}

fn written_cell<'t>(
    w: &mut Writer<'t>,
    row: V<'_>,
    cell: N<'t>,
    flags: u32,
    head: bool,
    bare: bool,
) -> WrittenCell {
    let span = match s(cell, "span") {
        Some("rowspan") => Some("^"),
        Some("colspan") => Some("<"),
        _ => None,
    };
    if let Some(mark) = span {
        return WrittenCell {
            prefix: if head { "=".into() } else { String::new() },
            first: mark.into(),
            extra: Vec::new(),
            places: Vec::new(),
        };
    }
    let content = cell_content(w, cell, flags);
    if !content.contains('\n') {
        return WrittenCell {
            prefix: cell_prefix(w, cell, bare),
            first: content,
            extra: Vec::new(),
            places: Vec::new(),
        };
    }
    let mut parts: Vec<String> = content.split('\n').map(str::to_string).collect();
    if let Some(places) = fragment_lines(w, row, cell, parts.len() - 1) {
        return WrittenCell {
            prefix: cell_prefix(w, cell, bare),
            first: parts.remove(0),
            extra: parts,
            places,
        };
    }
    let wraps = w
        .fact(cell)
        .and_then(|f| f.wraps.as_ref())
        .map_or(0, Vec::len);
    let skip = usize::from(wraps == parts.len() - 1 && parts.first().is_some_and(String::is_empty));
    let joined = if parts.len() == 1 && skip == 0 {
        parts.pop().unwrap_or_default()
    } else {
        parts.get(skip..).unwrap_or(&[]).join(" ")
    };
    WrittenCell {
        prefix: cell_prefix(w, cell, bare),
        first: joined,
        extra: Vec::new(),
        places: Vec::new(),
    }
}

fn written_row<'t>(
    w: &mut Writer<'t>,
    row: N<'t>,
    flags: u32,
    head: bool,
    bare: bool,
) -> WrittenRow {
    let cells: Vec<WrittenCell> = children(row)
        .iter()
        .map(|c| written_cell(w, row, c, flags, head, bare))
        .collect();
    let mut below = w.fact(row).and_then(|f| f.continuation_lines).unwrap_or(0);
    for c in &cells {
        below = below.max(c.places.last().copied().unwrap_or(0));
    }
    let count = usize::try_from(below).unwrap_or(0) + 1;
    let attribute = plain_block(row);
    if count == 1 && !w.align_tables {
        let size = cells
            .iter()
            .map(|cell| cell.prefix.len() + cell.first.len() + 3)
            .sum::<usize>()
            + attribute.len()
            + 1;
        let mut body = String::with_capacity(size);
        body.push('|');
        for cell in &cells {
            body.push_str(&cell.prefix);
            body.push(' ');
            if !cell.first.is_empty() {
                body.push_str(&cell.first);
                body.push(' ');
            }
            body.push('|');
        }
        if cells.is_empty() {
            body.push('|');
        }
        body.push_str(&attribute);
        return WrittenRow {
            lines: Vec::new(),
            attribute: String::new(),
            direct: Some(body),
        };
    }
    let mut lines = Vec::with_capacity(count);
    lines.push(
        cells
            .iter()
            .map(|c| segment(&c.prefix, &c.first))
            .collect::<Vec<_>>(),
    );
    if count > 1 {
        lines.resize(count, vec![" ".to_string(); cells.len()]);
    }
    for (column, cell) in cells.iter().enumerate() {
        for (at, &line) in cell.places.iter().enumerate() {
            let line = usize::try_from(line).unwrap_or(0);
            if let Some(slot) = lines.get_mut(line).and_then(|l| l.get_mut(column)) {
                *slot = segment("", cell.extra.get(at).map_or("", String::as_str));
            }
        }
    }
    WrittenRow {
        lines,
        attribute,
        direct: None,
    }
}

fn align_columns(rows: &mut [WrittenRow]) {
    let mut widths: Vec<usize> = Vec::new();
    for row in rows.iter() {
        for line in &row.lines {
            for (column, part) in line.iter().enumerate() {
                let width = display_width(part);
                match widths.get_mut(column) {
                    Some(w) => *w = (*w).max(width),
                    None => widths.push(width),
                }
            }
        }
    }
    for row in rows.iter_mut() {
        for line in &mut row.lines {
            for (column, part) in line.iter_mut().enumerate() {
                let pad = widths
                    .get(column)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(display_width(part));
                part.push_str(&" ".repeat(pad));
            }
        }
    }
}

fn emit_row(w: &mut Writer<'_>, prefix: &str, row: &WrittenRow) {
    if let Some(body) = &row.direct {
        emit(w, prefix, body);
        return;
    }
    for (at, line) in row.lines.iter().enumerate() {
        let mut body = String::with_capacity(
            line.iter().map(|p| p.len() + 1).sum::<usize>() + 1 + row.attribute.len(),
        );
        body.push(if at == 0 { '|' } else { '+' });
        for (k, part) in line.iter().enumerate() {
            if k > 0 {
                body.push('|');
            }
            body.push_str(part);
        }
        body.push('|');
        if at == 0 {
            body.push_str(&row.attribute);
        }
        emit(w, prefix, &body);
    }
}

enum Entry<'t> {
    Row(WrittenRow),
    Comment(N<'t>),
}

fn table_entries<'t>(w: &mut Writer<'t>, table: N<'t>, flags: u32) -> Vec<Entry<'t>> {
    let mut entries = Vec::new();
    let split = partitioned(&bodies_of(table));
    for group in children(table) {
        if ty(group) == "comment" {
            entries.push(Entry::Comment(group));
            continue;
        }
        let head = ty(group) == "table_head";
        let mut bare = if split && ty(group) == "table_body" {
            leading_header_rows(group)
        } else {
            0
        };
        for row in children(group) {
            if ty(row) == "table_row" {
                let is_bare = bare > 0;
                bare = bare.saturating_sub(1);
                entries.push(Entry::Row(written_row(w, row, flags, head, is_bare)));
            } else {
                entries.push(Entry::Comment(row));
            }
        }
    }
    entries
}

fn ends_with_group_comment(table: V<'_>) -> bool {
    children(table).last().is_some_and(|g| {
        ty(g) != "comment" && children(g).last().is_some_and(|r| ty(r) == "comment")
    })
}

fn pipe_table<'t>(w: &mut Writer<'t>, table: N<'t>, prefix: &str, flags: u32) {
    let parts = table_keys(Some(w), table, Vec::new());
    let block = brace_block(table, &parts);
    if w.prewritten == Some(id(table)) {
        w.prewritten = None;
    } else if !block.is_empty() {
        emit(w, prefix, &block);
    }
    let mut entries = table_entries(w, table, flags);
    let closing = ends_with_group_comment(table);
    if closing {
        trim_last_row(&mut entries);
    }
    if w.align_tables {
        align_entries(&mut entries);
    }
    for e in &entries {
        match e {
            Entry::Row(r) => emit_row(w, prefix, r),
            Entry::Comment(c) => {
                let line = line_comment_text(w, *c, text(c));
                emit(w, prefix, &line);
            }
        }
    }
    if closing {
        emit(w, prefix, "+ |");
    }
}

/// Drops the blank trailing lines of the last row.
fn trim_last_row(entries: &mut [Entry<'_>]) {
    let Some(Entry::Row(last)) = entries
        .iter_mut()
        .rev()
        .find(|e| matches!(e, Entry::Row(_)))
    else {
        return;
    };
    while last.lines.len() > 1
        && last
            .lines
            .last()
            .is_some_and(|l| l.iter().all(|p| p.trim().is_empty()))
    {
        last.lines.pop();
    }
}

fn align_entries(entries: &mut Vec<Entry<'_>>) {
    let mut rows: Vec<WrittenRow> = Vec::new();
    let mut kinds = Vec::new();
    for e in entries.drain(..) {
        match e {
            Entry::Row(r) => {
                rows.push(r);
                kinds.push(None);
            }
            Entry::Comment(c) => kinds.push(Some(c)),
        }
    }
    align_columns(&mut rows);
    let mut rows = rows.into_iter();
    for k in kinds {
        match k {
            Some(c) => entries.push(Entry::Comment(c)),
            None => {
                if let Some(r) = rows.next() {
                    entries.push(Entry::Row(r));
                }
            }
        }
    }
}

fn is_plain_cell(cell: V<'_>, header: bool) -> bool {
    if super::node::flag(cell, "header") != header
        || has(cell, "span")
        || has(cell, "align")
        || has(cell, "valign")
        || has(cell, "attrs")
        || has(cell, "lang")
    {
        return false;
    }
    let Some(block) = children(cell).single() else {
        return false;
    };
    if ty(block) != "cell_paragraph" {
        return false;
    }
    let kids = children(block);
    match kids.len() {
        0 => true,
        1 => kids
            .first()
            .is_some_and(|first| ty(first) == "text" && !has(first, "attrs")),
        _ => false,
    }
}

/// The row count and whether a delimited form is required, if the table can use one.
fn delimited_shape(table: V<'_>) -> Option<(usize, bool)> {
    let mut seen = [0usize; 3];
    let mut rows = 0;
    let mut needs = false;
    for group in children(table) {
        let fixed = delimited_group(group, &mut seen)?;
        let mut leading = true;
        for row in children(group) {
            rows += 1;
            needs |= delimited_row(row, fixed, &mut leading)?;
        }
    }
    // C2-L-036: a table without rows is delimited, whatever its bodies.
    (rows == 0 || seen[0] <= 1).then_some((rows, needs))
}

/// Whether the cells of `group` are header cells: `None` for a body, whose leading header rows
/// `body-header-rows` spells (C2-B-230). `header-rows`, `body-rows` and `footer-rows` spell the
/// head, the bodies and one foot in this order (C2-B-227); `seen` counts each.
fn delimited_group(group: V<'_>, seen: &mut [usize; 3]) -> Option<Option<bool>> {
    let (slot, fixed) = match ty(group) {
        "table_head" if seen[1] == 0 && seen[2] == 0 => (0, Some(true)),
        "table_body" if seen[2] == 0 => (1, None),
        "table_foot"
            if seen[2] == 0
                && !children(group).is_empty()
                && !has(group, "attrs")
                && !has(group, "lang") =>
        {
            (2, Some(false))
        }
        _ => return None,
    };
    *seen.get_mut(slot)? += 1;
    Some(fixed)
}

/// Whether a plain `row` needs the delimited form, or `None` where that form cannot hold it.
fn delimited_row(row: V<'_>, fixed: Option<bool>, leading: &mut bool) -> Option<bool> {
    if ty(row) != "table_row" || has(row, "attrs") {
        return None;
    }
    let cells = children(row);
    let header = fixed.unwrap_or_else(|| {
        *leading = *leading
            && cells
                .first()
                .is_some_and(|c| super::node::flag(c, "header"));
        *leading
    });
    let mut needs = false;
    for cell in cells {
        if !is_plain_cell(cell, header) {
            return None;
        }
        needs |= cell_needs_delimited(cell);
    }
    Some(needs)
}

fn first_value(cell: V<'_>) -> Option<&str> {
    children(children(cell).first()?)
        .first()
        .and_then(|n| s(n, "value"))
}

fn cell_needs_delimited(cell: V<'_>) -> bool {
    let Some(v) = first_value(cell).filter(|v| !v.is_empty()) else {
        return false;
    };
    v.contains('\n') || v.starts_with([' ', '\t']) || v.ends_with([' ', '\t'])
}

fn header_rows(w: &Writer<'_>, table: V<'_>) -> Option<usize> {
    let head = rows_of(table, "table_head").len();
    let all = head + rows_of(table, "table_body").len() + rows_of(table, "table_foot").len();
    let first = usize::from(all != 0);
    if head != first {
        return Some(head);
    }
    let recorded = w.fact(table).and_then(|f| f.header_rows);
    (recorded == i64::try_from(head).ok()).then_some(head)
}

/// C2-W-066: whether the table is written in a delimited form, given the format its sidecar
/// records, where a cell keeps the spaces and tabs at its ends as data.
pub(super) fn writes_delimited(table: V<'_>, recorded: Option<&str>) -> bool {
    delimited_shape(table)
        .is_some_and(|(rows, needs)| rows == 0 || needs || matches!(recorded, Some("csv" | "tsv")))
}

fn table_format(w: &Writer<'_>, table: V<'_>) -> String {
    let Some((rows, needs_delimited)) = delimited_shape(table) else {
        return "pipe".into();
    };
    if let Some(f) = w.fact(table).and_then(|f| f.source_format.as_deref()) {
        if f == "csv" || f == "tsv" {
            return f.into();
        }
    }
    if rows == 0 || needs_delimited {
        "csv"
    } else {
        "pipe"
    }
    .into()
}

fn delimited_cell(cell: V<'_>, separator: &str) -> String {
    let v = first_value(cell).unwrap_or("");
    if !v.contains(separator) && !v.contains('"') && !v.contains('\n') {
        return v.to_string();
    }
    format!("\"{}\"", v.replace('"', "\"\""))
}

fn delimited_body(table: V<'_>, separator: &str) -> Vec<String> {
    let mut body = Vec::new();
    for group in children(table) {
        if ty(group) == "comment" {
            continue;
        }
        for row in children(group) {
            body.push(if ty(row) == "table_row" {
                children(row)
                    .iter()
                    .map(|c| delimited_cell(c, separator))
                    .collect::<Vec<_>>()
                    .join(separator)
            } else {
                String::new()
            });
        }
    }
    // An empty body after the last row does not end the fence body.
    let one_cell = children(table)
        .iter()
        .rev()
        .take_while(|g| ty(g) != "comment")
        .find_map(|g| children(g).last())
        .is_some_and(|r| ty(r) == "table_row" && children(r).len() == 1);
    if body.last().is_some_and(String::is_empty) && one_cell {
        if let Some(last) = body.last_mut() {
            *last = "\"\"".into();
        }
    }
    body
}

fn delimited_keys(w: Option<&mut Writer<'_>>, table: V<'_>, declared: Option<usize>) -> BlockParts {
    let mut first = vec![("table".to_string(), String::new())];
    if let Some(d) = declared {
        first.push(("header-rows".into(), d.to_string()));
    }
    table_keys(w, table, first)
}

fn delimited_table(w: &mut Writer<'_>, table: V<'_>, prefix: &str, format: &str) {
    let separator = if format == "tsv" { "\t" } else { "," };
    let declared = header_rows(w, table);
    let parts = delimited_keys(Some(w), table, declared);
    let block = brace_block(table, &parts);
    if w.prewritten == Some(id(table)) {
        w.prewritten = None;
    } else {
        emit(w, prefix, &block);
    }
    let body = delimited_body(table, separator);
    let joined = body.join("\n");
    let lines: Vec<&str> = joined.split('\n').collect();
    let run = "`".repeat(MIN_RUN.max(longest_bare_run(lines.iter().copied(), b'`') + 1));
    emit(w, prefix, &format!("{run}{format}"));
    if !body.is_empty() {
        for line in lines {
            emit(w, prefix, line);
        }
    }
    emit(w, prefix, &run);
}

pub fn write_table<'t>(w: &mut Writer<'t>, table: N<'t>, prefix: &str, flags: u32) {
    let format = table_format(w, table);
    if format == "pipe" {
        pipe_table(w, table, prefix, flags);
    } else {
        delimited_table(w, table, prefix, &format);
    }
}

/// C2-W-122: the same table attribute line before an intervening comment as before the table itself.
pub fn table_attribute(w: &Writer<'_>, table: V<'_>) -> String {
    if table_format(w, table) == "pipe" {
        return brace_block(table, &table_keys(None, table, Vec::new()));
    }
    brace_block(table, &delimited_keys(None, table, header_rows(w, table)))
}

fn property(ranges: &[u32], cp: u32, stride: usize) -> u32 {
    let count = ranges.len() / stride;
    let (mut low, mut high) = (0usize, count);
    while low < high {
        let mid = usize::midpoint(low, high);
        let at = mid * stride;
        let (start, end) = (
            ranges.get(at).copied().unwrap_or(0),
            ranges.get(at + 1).copied().unwrap_or(0),
        );
        if cp < start {
            high = mid;
        } else if cp > end {
            low = mid + 1;
        } else {
            return if stride == 3 {
                ranges.get(at + 2).copied().unwrap_or(0)
            } else {
                1
            };
        }
    }
    0
}

const CR: u32 = 1;
const LF: u32 = 2;
const CONTROL: u32 = 3;
const EXTEND: u32 = 4;
const ZWJ: u32 = 5;
const REGIONAL: u32 = 6;
const PREPEND: u32 = 7;
const SPACING: u32 = 8;
const L: u32 = 9;
const V: u32 = 10;
const T: u32 = 11;
const LV: u32 = 12;
const LVT: u32 = 13;

#[derive(Default)]
struct Ctx {
    before: u32,
    indicators: u32,
    linker: bool,
    pict: bool,
    pict_before_joiner: bool,
}

fn joins_hangul(before: u32, at: u32) -> bool {
    match before {
        L => matches!(at, L | V | LV | LVT),
        LV | V => matches!(at, V | T),
        LVT | T => at == T,
        _ => false,
    }
}

fn breaks_before(c: &Ctx, cp: u32, gcb: u32) -> bool {
    let before = c.before;
    if before == CR && gcb == LF {
        return false;
    }
    let control = |g: u32| matches!(g, CR | LF | CONTROL);
    if control(before) || control(gcb) {
        return true;
    }
    let joins = matches!(gcb, EXTEND | ZWJ | SPACING)
        || before == PREPEND
        || (c.linker && property(INCB_CONSONANT, cp, 2) != 0)
        || (before == ZWJ && c.pict_before_joiner && property(PICT, cp, 2) != 0)
        || (before == REGIONAL && gcb == REGIONAL && c.indicators % 2 == 1);
    !joins_hangul(before, gcb) && !joins
}

fn advance(c: &mut Ctx, cp: u32, gcb: u32) {
    let linker = property(INCB_LINKER, cp, 2) != 0;
    if linker || property(INCB_EXTEND, cp, 2) == 0 {
        c.linker = linker;
    }
    c.pict_before_joiner = c.pict;
    if gcb != EXTEND {
        c.pict = property(PICT, cp, 2) != 0;
    }
    c.indicators = if gcb == REGIONAL { c.indicators + 1 } else { 0 };
    c.before = gcb;
}

/// C2-W-119: the display width of `text`.
pub fn display_width(text: &str) -> usize {
    if text.bytes().all(|b| b < 0x80 && b != b'\r') {
        return text.len();
    }
    let mut c = Ctx::default();
    let (mut width, mut wide, mut first) = (0, false, true);
    for ch in text.chars() {
        let cp = u32::from(ch);
        let gcb = property(GCB, cp, 3);
        if first || breaks_before(&c, cp, gcb) {
            if !first {
                width += if wide { 2 } else { 1 };
            }
            wide = false;
        }
        first = false;
        if property(WIDE, cp, 2) != 0 {
            wide = true;
        }
        advance(&mut c, cp, gcb);
    }
    if first {
        0
    } else {
        width + if wide { 2 } else { 1 }
    }
}
