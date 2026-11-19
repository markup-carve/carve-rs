//! `listTable` (carve-js#5070): `::: list-table`, a nested bullet list
//! written as a table.

use super::{Bundle, Claim, Engine, Form, Matcher, declaration, key_values, with_attrs};
use crate::api::{Channel, Error, Extension, RenderContext, Renderer};
use serde_json::{Value, json};
use std::fmt::Write as _;

const MAX_ROWS: usize = 10_000;
const MAX_CELLS: usize = 100_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Left,
    Up,
}

struct Cell {
    item: Value,
    mark: Option<Mark>,
    rowspan: usize,
    colspan: usize,
    skip: bool,
}

fn children(node: &Value) -> &[Value] {
    node.get("children")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
}

fn is(node: Option<&Value>, ty: &str) -> bool {
    node.and_then(|n| n.get("type")).and_then(Value::as_str) == Some(ty)
}

/// The cells of a row item: the items of its leading list, the blocks after
/// that list appended to the last cell.
fn row_cells(row: &Value) -> Option<Vec<Value>> {
    let (first, rest) = children(row).split_first()?;
    if !is(Some(first), "list") {
        return None;
    }
    let mut items = children(first).to_vec();
    let last = items.last_mut()?;
    if let Some(c) = last.get_mut("children").and_then(Value::as_array_mut) {
        c.extend(rest.iter().cloned());
    }
    Some(items)
}

fn table_list(body: &[Value]) -> Option<&Value> {
    let [list] = body else { return None };
    if !is(Some(list), "list") {
        return None;
    }
    let rows = children(list);
    if rows.is_empty() || rows.len() > MAX_ROWS {
        return None;
    }
    let (mut total, mut width) = (0, 0);
    for row in rows {
        let cells = row_cells(row)?.len();
        total += cells;
        width = width.max(cells);
        if total > MAX_CELLS || width * rows.len() > MAX_CELLS {
            return None;
        }
    }
    Some(list)
}

fn claim(claim: &Claim<'_>) -> Option<Value> {
    if claim.form != Form::Div {
        return None;
    }
    let body = children(claim.node);
    let list = table_list(body)?;
    let title = claim
        .node
        .get("title")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let mut node = json!({
        "type": "block_extension",
        "name": "carve.listTable",
        "version": "1",
        "children": [],
        "slots": {"rows": [list], "caption": title},
        "fallback": body,
    });
    if let Some(label) = claim.node.get("groupLabel") {
        if let Some(map) = node.as_object_mut() {
            map.insert("props".into(), json!({ "label": label }));
        }
    }
    Some(node)
}

/// The value of the authored key `name` (lowercase), ASCII case ignored.
fn attr_value<'a>(attrs: Option<&'a Value>, name: &str) -> Option<&'a str> {
    attrs?
        .get("keyValues")?
        .as_array()?
        .iter()
        .find(|kv| {
            kv.get(0)
                .and_then(Value::as_str)
                .is_some_and(|k| k.to_lowercase() == name)
        })?
        .get(1)?
        .as_str()
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn header_count(attrs: Option<&Value>, key: &str) -> usize {
    match attr_value(attrs, key) {
        None => 0,
        Some("") => 1,
        Some(v) => js_number(v)
            .filter(|n| n.fract() == 0.0 && (0.0..=10_000.0).contains(n))
            .map_or(0, |n| n as usize),
    }
}

/// `Number(value)` for the decimal forms a count is written in.
#[allow(clippy::cast_precision_loss)]
fn js_number(value: &str) -> Option<f64> {
    let t = value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if t.is_empty() {
        return Some(0.0);
    }
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let (Some(head), Some(digits)) = (t.get(..2), t.get(2..)) {
            if !digits.is_empty() && head.eq_ignore_ascii_case(prefix) {
                return u64::from_str_radix(digits, radix).ok().map(|n| n as f64);
            }
        }
    }
    if t.bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-'))
    {
        return t.parse::<f64>().ok();
    }
    None
}

fn mark(item: &Value) -> Option<Mark> {
    if item.get("attrs").is_some() {
        return None;
    }
    let [para] = children(item) else { return None };
    if !is(Some(para), "paragraph") {
        return None;
    }
    let [text] = children(para) else { return None };
    if !is(Some(text), "text") {
        return None;
    }
    match text.get("value").and_then(Value::as_str)?.trim() {
        "<" => Some(Mark::Left),
        "^" => Some(Mark::Up),
        _ => None,
    }
}

fn body_boundaries(row_heads: &[bool], start: usize, end: usize) -> Vec<usize> {
    let mut boundaries = vec![start];
    let mut has_data = false;
    for (row, &head) in row_heads.iter().enumerate().take(end).skip(start) {
        if head && has_data {
            boundaries.push(row);
            has_data = false;
        }
        if !head {
            has_data = true;
        }
    }
    boundaries.push(end);
    boundaries
}

fn group_ids(count: usize, head: usize, foot: usize, boundaries: &[usize]) -> Vec<usize> {
    (0..count)
        .map(|row| {
            if row < head {
                0
            } else if row >= count - foot {
                boundaries.len()
            } else {
                boundaries
                    .iter()
                    .position(|&end| end > row)
                    .unwrap_or(usize::MAX)
            }
        })
        .collect()
}

/// Merges `<` cells into the cell on their left and `^` cells into the cell
/// above in the same row group. Every index is a row or cell already walked.
#[allow(clippy::indexing_slicing)]
fn resolve_spans(rows: &mut [Vec<Cell>], groups: &[usize]) {
    let mut above: Vec<(usize, usize)> = Vec::new();
    for row in 0..rows.len() {
        let same_group = row > 0 && groups[row] == groups[row - 1];
        let mut current: Vec<(usize, usize)> = Vec::with_capacity(rows[row].len());
        for col in 0..rows[row].len() {
            let left = col.checked_sub(1).map(|c| current[c]);
            let up = above.get(col).copied();
            let owner = match rows[row][col].mark {
                Some(Mark::Left) if left.is_some_and(|l| Some(l) != up) => {
                    let (r, c) = left.unwrap_or((row, col));
                    rows[row][col].skip = true;
                    rows[r][c].colspan += 1;
                    (r, c)
                }
                Some(Mark::Up)
                    if same_group && up.is_some_and(|(r, c)| rows[r][c].colspan == 1) =>
                {
                    let (r, c) = up.unwrap_or((row, col));
                    rows[row][col].skip = true;
                    rows[r][c].rowspan += 1;
                    (r, c)
                }
                _ => (row, col),
            };
            current.push(owner);
        }
        above = current;
    }
}

struct Table<'a> {
    rows: Vec<Vec<Cell>>,
    row_heads: Vec<bool>,
    attrs: Option<&'a Value>,
    head: usize,
    head_cols: usize,
    width: usize,
}

fn alignment<'a>(value: Option<&str>, allowed: &[&'a str]) -> Option<&'a str> {
    let folded = value?.trim().to_lowercase();
    allowed.iter().copied().find(|a| *a == folded)
}

const CELL_KEYS: &[&str] = &[
    "rowspan",
    "colspan",
    "scope",
    "header",
    "header-row",
    "align",
    "valign",
];
const TABLE_KEYS: &[&str] = &[
    "header-rows",
    "header-cols",
    "footer-rows",
    "aligns",
    "valigns",
    "widths",
];

fn without_keys(attrs: Option<&Value>, consumed: &[&str]) -> Vec<Value> {
    key_values(attrs)
        .into_iter()
        .filter(|kv| {
            kv.get(0)
                .and_then(Value::as_str)
                .is_none_or(|k| !consumed.contains(&k.to_lowercase().as_str()))
        })
        .collect()
}

/// The authored attributes of `attrs` without the consumed keys and without
/// a source line.
fn author_attrs(attrs: Option<&Value>, consumed: &[&str], cx: &RenderContext<'_>) -> String {
    let Some(a) = attrs else {
        return String::new();
    };
    let node = with_attrs(
        &json!({ "attrs": a }),
        super::classes(Some(a)),
        without_keys(Some(a), consumed),
    );
    cx.attributes(&node)
}

/// The source line of a block element, without authored attributes.
fn source_line(node: Option<&Value>, cx: &RenderContext<'_>) -> String {
    match node.and_then(|n| n.get("pos")) {
        Some(pos) if cx.options().source_lines => cx.attributes(&json!({ "pos": pos })),
        _ => String::new(),
    }
}

impl Table<'_> {
    fn cell_style(&self, cell: &Cell, col: usize) -> String {
        let list = |key| {
            attr_value(self.attrs, key)
                .map(|v| v.split(',').collect::<Vec<_>>())
                .unwrap_or_default()
        };
        let (aligns, valigns) = (list("aligns"), list("valigns"));
        let own = cell.item.get("attrs");
        let h = alignment(
            attr_value(own, "align").or_else(|| aligns.get(col).copied()),
            &["left", "right", "center"],
        );
        let v = alignment(
            attr_value(own, "valign").or_else(|| valigns.get(col).copied()),
            &["top", "middle", "bottom"],
        );
        let mut out = String::new();
        if let Some(h) = h {
            let _ = write!(out, "text-align: {h};");
        }
        if h.is_some() && v.is_some() {
            out.push(' ');
        }
        if let Some(v) = v {
            let _ = write!(out, "vertical-align: {v};");
        }
        out
    }

    fn cell(
        &self,
        cell: &Cell,
        col: usize,
        row: usize,
        cx: &RenderContext<'_>,
    ) -> Result<String, Error> {
        if cell.skip {
            return Ok(String::new());
        }
        let own = cell.item.get("attrs");
        let row_head = self.row_heads.get(row).copied().unwrap_or(false);
        let heading = row < self.head
            || row_head
            || col < self.head_cols
            || attr_value(own, "header").is_some();
        let tag = if heading { "th" } else { "td" };
        let mut open = format!("<{tag}");
        if heading {
            let scope = if row < self.head || row_head {
                "col"
            } else {
                "row"
            };
            let _ = write!(open, r#" scope="{scope}""#);
        }
        if cell.rowspan > 1 {
            let _ = write!(open, r#" rowspan="{}""#, cell.rowspan);
        }
        if cell.colspan > 1 {
            let _ = write!(open, r#" colspan="{}""#, cell.colspan);
        }
        open.push_str(&author_attrs(own, CELL_KEYS, cx));
        let style = self.cell_style(cell, col);
        if !style.is_empty() {
            let _ = write!(open, r#" style="{}""#, cx.escape_attribute(&style));
        }
        let blocks = children(&cell.item);
        let content = match blocks {
            [para] if is(Some(para), "paragraph") => {
                cx.render_inlines(para.get("children").unwrap_or(&json!([])))?
            }
            _ => blocks
                .iter()
                .map(|b| cx.render_blocks(&json!([b])))
                .collect::<Result<Vec<_>, _>>()?
                .join("\n"),
        };
        Ok(format!("{open}>{content}</{tag}>"))
    }

    fn row(&self, row: usize, cx: &RenderContext<'_>) -> Result<String, Error> {
        let Some(cells) = self.rows.get(row) else {
            return Ok(String::new());
        };
        let mut out = format!(
            "    <tr{}>",
            source_line(cells.first().map(|c| &c.item), cx)
        );
        for (col, cell) in cells.iter().enumerate() {
            out.push_str(&self.cell(cell, col, row, cx)?);
        }
        out.push_str(&"<td></td>".repeat(self.width.saturating_sub(cells.len())));
        out.push_str("</tr>");
        Ok(out)
    }

    fn group(
        &self,
        start: usize,
        end: usize,
        name: &str,
        cx: &RenderContext<'_>,
    ) -> Result<String, Error> {
        if start >= end {
            return Ok(String::new());
        }
        let rows = (start..end)
            .map(|r| self.row(r, cx))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");
        let line = source_line(
            self.rows
                .get(start)
                .and_then(|r| r.first())
                .map(|c| &c.item),
            cx,
        );
        Ok(format!("  <{name}{line}>\n{rows}\n  </{name}>\n"))
    }
}

fn columns(attrs: Option<&Value>, cx: &RenderContext<'_>) -> String {
    let Some(widths) = attr_value(attrs, "widths") else {
        return String::new();
    };
    let values: Vec<&str> = widths.split(',').map(str::trim).collect();
    let number = |v: &str| {
        let (int, frac) = v.split_once('.').unwrap_or((v, "1"));
        !int.is_empty()
            && int.bytes().all(|b| b.is_ascii_digit())
            && !frac.is_empty()
            && frac.bytes().all(|b| b.is_ascii_digit())
    };
    if !values.iter().all(|v| number(v)) {
        return String::new();
    }
    let cols = values
        .iter()
        .map(|v| format!(r#"    <col style="width: {}%;">"#, cx.escape_attribute(v)))
        .collect::<Vec<_>>()
        .join("\n");
    format!("  <colgroup>\n{cols}\n  </colgroup>\n")
}

struct ListTable;

impl ListTable {
    fn table(node: &Value, cx: &RenderContext<'_>) -> Result<Option<String>, Error> {
        let Some(list) = node
            .pointer("/slots/rows/0")
            .filter(|l| is(Some(l), "list"))
        else {
            return Ok(None);
        };
        let mut rows = Vec::new();
        let mut row_heads = Vec::new();
        for row in children(list) {
            let Some(items) = row_cells(row) else {
                return Ok(None);
            };
            row_heads.push(
                attr_value(items.first().and_then(|i| i.get("attrs")), "header-row").is_some(),
            );
            rows.push(
                items
                    .into_iter()
                    .map(|item| Cell {
                        mark: mark(&item),
                        item,
                        rowspan: 1,
                        colspan: 1,
                        skip: false,
                    })
                    .collect::<Vec<_>>(),
            );
        }
        let attrs = node.get("attrs");
        let count = rows.len();
        let head = header_count(attrs, "header-rows").min(count);
        let foot = header_count(attrs, "footer-rows").min(count - head);
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let body_end = count - foot;
        let boundaries = body_boundaries(&row_heads, head, body_end);
        resolve_spans(&mut rows, &group_ids(count, head, foot, &boundaries));
        let table = Table {
            rows,
            row_heads,
            attrs,
            head,
            head_cols: header_count(attrs, "header-cols"),
            width,
        };
        let label = match node.pointer("/props/label").and_then(Value::as_str) {
            Some(l) if !l.is_empty() => format!("<p class=\"div-label\">{}</p>\n", cx.escape(l)),
            _ => String::new(),
        };
        let title = node
            .pointer("/slots/caption")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let caption = if title.as_array().is_some_and(|t| !t.is_empty()) {
            format!("  <caption>{}</caption>\n", cx.render_inlines(&title)?)
        } else {
            String::new()
        };
        let mut consumed = TABLE_KEYS.to_vec();
        if cx.options().source_lines {
            consumed.push("data-source-line");
        }
        let mut open = author_attrs(attrs, &consumed, cx);
        open.push_str(&source_line(Some(node), cx));
        let mut html = format!("{label}<table{open}>\n{caption}{}", columns(attrs, cx));
        html.push_str(&table.group(0, head, "thead", cx)?);
        for pair in boundaries.windows(2) {
            if let [start, end] = pair {
                html.push_str(&table.group(*start, *end, "tbody", cx)?);
            }
        }
        html.push_str(&table.group(body_end, count, "tfoot", cx)?);
        html.push_str("</table>");
        let indent = "  ".repeat(cx.block_depth());
        Ok(Some(if indent.is_empty() {
            html
        } else {
            html.replace('\n', &format!("\n{indent}"))
        }))
    }
}

impl Renderer for ListTable {
    fn render(&self, node: &Value, cx: &RenderContext<'_>) -> Result<String, Error> {
        match Self::table(node, cx)? {
            Some(html) => Ok(html),
            None => cx.render_blocks(node.get("fallback").unwrap_or(&json!([]))),
        }
    }
}

/// `::: list-table` with its renderer, in both channels.
#[must_use]
pub fn list_table() -> Bundle {
    Bundle {
        extensions: vec![Extension {
            declaration: declaration("carve.listTable", &["rows", "caption"]),
            renderers: vec![
                (Channel::Interactive, Box::new(ListTable)),
                (Channel::BuildTime, Box::new(ListTable)),
            ],
            transform: None,
        }],
        matchers: vec![Matcher {
            extension: "carve.listTable".into(),
            form: Form::Div,
            name: "list-table".into(),
            claimer: Box::new(Engine(claim)),
        }],
    }
}
