//! Table nodes from the rows of the block pass: the head (C2-B-178), spans
//! (C2-B-172 to C2-B-174, C2-B-183), the footer (C2-B-187) and the columns
//! (C2-B-211, C2-B-212, C2-B-220).

use super::Builder;
use crate::ast::{self, Attrs, Node};
use crate::block::table::{Row, TableData};
use crate::inline::ContentKind;

/// Where the cell at a grid place came from.
#[derive(Debug, Clone, Copy)]
struct Origin {
    row: usize,
    col: usize,
}

/// The result of span resolution for one written cell.
#[derive(Debug, Clone, Copy, Default)]
struct Resolved {
    colspan: usize,
    rowspan: usize,
    /// A valid span mark: the cell stays empty (C2-A-092).
    mark: Option<char>,
    /// A failed mark leaves an empty cell (C2-B-173).
    failed: bool,
}

const TABLE_KEYS: [&str; 4] = ["aligns", "widths", "header-rows", "footer-rows"];

impl Builder<'_> {
    pub(super) fn table(
        &self,
        mut data: TableData,
        attrs: Option<Attrs>,
        lang: Option<String>,
        start: usize,
        end: usize,
    ) -> Node {
        // C2-A-086, gap G-012: comments after the last row belong to the table.
        let trailing = data
            .rows
            .last_mut()
            .map(|r| std::mem::take(&mut r.comments))
            .unwrap_or_default();
        let head = head_rows(&data.rows);
        let spans = resolve_spans(&data.rows, head);
        let ncols = data.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
        let n = data.rows.len();
        let foot = footer_count(attrs.as_ref()).min(n - head);
        let (columns, attrs) = columns(ncols, attrs);
        let mut groups: Vec<Node> = Vec::new();
        let bounds = [(0, head, 0u8), (head, n - foot, 1), (n - foot, n, 2)];
        for (lo, hi, kind) in bounds {
            if lo >= hi {
                continue;
            }
            groups.push(self.group(&data.rows, &spans, lo, hi, kind));
        }
        for (s, e, value) in trailing {
            groups.push(self.comment(s, e, value));
        }
        Node::Table(ast::Table {
            children: groups,
            columns,
            attrs,
            lang,
            pos: Some(self.src.pos(start, end)),
            ..Default::default()
        })
    }

    /// C2-B-209, C2-B-221: the head count of a delimited table and its partition.
    pub(super) fn delimited_table(
        value: &str,
        sep: char,
        attrs: Option<Attrs>,
        lang: Option<String>,
        pos: Option<ast::Pos>,
    ) -> Node {
        let declared = attrs
            .as_ref()
            .and_then(|a| a.get("header-rows"))
            .and_then(count);
        let footer = footer_count(attrs.as_ref());
        let (children, ncols) = super::delimited::table(value, sep, declared, footer);
        let attrs = attrs.map(|mut a| {
            a.key_values
                .retain(|(k, _)| !k.eq_ignore_ascii_case("table"));
            a
        });
        let (columns, attrs) = columns(ncols, attrs.filter(|a| !a.is_empty()));
        Node::Table(ast::Table {
            children,
            columns,
            attrs,
            lang,
            pos,
            ..Default::default()
        })
    }

    fn comment(&self, s: usize, e: usize, value: String) -> Node {
        Node::Comment(ast::Comment {
            form: ast::CommentForm::Line,
            value,
            pos: Some(self.src.pos(s, e)),
            ..Default::default()
        })
    }

    fn group(&self, rows: &[Row], spans: &[Vec<Resolved>], lo: usize, hi: usize, kind: u8) -> Node {
        let mut children = Vec::new();
        for (i, row) in rows.iter().enumerate().take(hi).skip(lo) {
            children.push(self.row(
                row,
                spans.get(i).map_or(&[], Vec::as_slice),
                i < lo + (hi - lo),
            ));
            for (s, e, value) in &row.comments {
                children.push(self.comment(*s, *e, value.clone()));
            }
        }
        let first = rows.get(lo).map_or(0, |r| r.start);
        let last = rows.get(hi.saturating_sub(1)).map_or(0, |r| r.end);
        let pos = Some(self.src.pos(first, last));
        match kind {
            0 => Node::TableHead(ast::TableHead {
                children,
                pos,
                ..Default::default()
            }),
            1 => Node::TableBody(ast::TableBody {
                children,
                pos,
                ..Default::default()
            }),
            _ => Node::TableFoot(ast::TableFoot {
                children,
                pos,
                ..Default::default()
            }),
        }
    }

    fn row(&self, row: &Row, spans: &[Resolved], _in_group: bool) -> Node {
        // C2-B-173, gap G-005: "every other cell" needs at least one other cell.
        // A span-mark cell is no cell of its own for this test.
        let others = |i: usize| {
            row.cells
                .iter()
                .enumerate()
                .filter(move |&(j, _)| j != i && !spans.get(j).is_some_and(|s| s.mark.is_some()))
        };
        let all_header = |i: usize| others(i).count() > 0 && others(i).all(|(_, c)| c.header);
        let mut cells = Vec::new();
        for (i, (cell, sp)) in row.cells.iter().zip(spans).enumerate() {
            // Gap G-009: a cell with no content has no position.
            let blank = cell
                .pieces
                .iter()
                .all(|p| crate::block::trim_end(self.src, p.start, p.end) == p.start);
            let positioned = !cell.joined && !blank && sp.mark.is_none() && !sp.failed;
            let cell_positioned = positioned || (sp.failed && !cell.joined);
            let children = if sp.mark.is_some() {
                Vec::new()
            } else {
                let inl = self.inline(ContentKind::Cell, &cell.pieces);
                let ppos = if positioned {
                    cell.pieces.first().map(|p| {
                        self.src.pos(
                            p.start,
                            crate::block::trim_end(
                                self.src,
                                p.start,
                                cell.pieces.last().map_or(p.end, |l| l.end),
                            ),
                        )
                    })
                } else {
                    None
                };
                let inl = if positioned { inl } else { strip_pos(inl) };
                vec![Node::CellParagraph(ast::CellParagraph {
                    children: inl,
                    pos: ppos,
                    ..Default::default()
                })]
            };
            let header = if sp.failed {
                all_header(i)
            } else {
                cell.header
            };
            let (attrs, lang) = super::split_attrs(cell.attrs.clone());
            let span = sp.mark.map(|m| {
                if m == '<' {
                    ast::TableCellSpan::Colspan
                } else {
                    ast::TableCellSpan::Rowspan
                }
            });
            let align = cell
                .align
                .and_then(|a| serde_json::from_value(serde_json::Value::String(a.into())).ok());
            cells.push(Node::TableCell(ast::TableCell {
                children,
                header,
                span,
                colspan: (sp.colspan > 1).then(|| i64::try_from(sp.colspan).unwrap_or(1)),
                rowspan: (sp.rowspan > 1).then(|| i64::try_from(sp.rowspan).unwrap_or(1)),
                align,
                attrs,
                lang,
                pos: cell_positioned.then(|| self.src.pos(cell.start, cell.end)),
                ..Default::default()
            }));
        }
        let (attrs, lang) = super::split_attrs(row.attrs.clone());
        let pos = (!row.joined).then(|| self.src.pos(row.start, row.end));
        Node::TableRow(ast::TableRow {
            children: cells,
            attrs,
            lang,
            pos,
            ..Default::default()
        })
    }
}

fn strip_pos(mut nodes: Vec<Node>) -> Vec<Node> {
    for n in &mut nodes {
        clear_pos(n, 0);
    }
    nodes
}

fn clear_pos(n: &mut Node, depth: usize) {
    if let Some(p) = n.pos_slot() {
        *p = None;
    }
    if depth < crate::tables::DEPTH_CAP {
        n.for_each_child_mut(&mut |c| clear_pos(c, depth + 1));
    }
}

/// C2-B-178: the leading rows made only of header cells (span marks count with them).
fn head_rows(rows: &[Row]) -> usize {
    rows.iter()
        .take_while(|r| {
            r.cells.iter().any(|c| c.header) && r.cells.iter().all(|c| c.header || c.span.is_some())
        })
        .count()
}

fn resolve_spans(rows: &[Row], head: usize) -> Vec<Vec<Resolved>> {
    let mut out: Vec<Vec<Resolved>> = rows
        .iter()
        .map(|r| {
            vec![
                Resolved {
                    colspan: 1,
                    rowspan: 1,
                    ..Resolved::default()
                };
                r.cells.len()
            ]
        })
        .collect();
    let mut grid: Vec<Vec<Option<Origin>>> =
        rows.iter().map(|r| vec![None; r.cells.len()]).collect();
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            let origin = match cell.span {
                Some('<') if c > 0 => grid
                    .get(r)
                    .and_then(|g| g.get(c - 1))
                    .copied()
                    .flatten()
                    .filter(|o| o.row == r),
                Some('^') if r > 0 && !(r == head && head > 0) => grid
                    .get(r - 1)
                    .and_then(|g| g.get(c))
                    .copied()
                    .flatten()
                    .filter(|o| o.col == c),
                _ => None,
            };
            let set = match (cell.span, origin) {
                (Some(m), Some(o)) => {
                    if let Some(res) = out.get_mut(o.row).and_then(|v| v.get_mut(o.col)) {
                        if m == '<' {
                            res.colspan += 1;
                        } else {
                            res.rowspan += 1;
                        }
                    }
                    if let Some(res) = out.get_mut(r).and_then(|v| v.get_mut(c)) {
                        res.mark = Some(m);
                    }
                    o
                }
                (Some(_), None) => {
                    if let Some(res) = out.get_mut(r).and_then(|v| v.get_mut(c)) {
                        res.failed = true;
                    }
                    Origin { row: r, col: c }
                }
                _ => Origin { row: r, col: c },
            };
            if let Some(g) = grid.get_mut(r).and_then(|g| g.get_mut(c)) {
                *g = Some(set);
            }
        }
    }
    out
}

/// C2-B-210: a count is one or more decimal digits; an empty value counts 1.
fn count(v: &str) -> Option<usize> {
    if v.is_empty() {
        return Some(1);
    }
    if v.bytes().all(|b| b.is_ascii_digit()) {
        v.parse().ok()
    } else {
        None
    }
}

/// The columns and the table attributes that are read, not kept (gap G-003).
fn columns(ncols: usize, attrs: Option<Attrs>) -> (Vec<Node>, Option<Attrs>) {
    let mut cols: Vec<ast::TableColumn> = (0..ncols).map(|_| ast::TableColumn::default()).collect();
    let Some(mut a) = attrs else {
        return (cols.into_iter().map(Node::TableColumn).collect(), None);
    };
    if let Some(v) = a.get("aligns") {
        for (i, entry) in v.split(',').enumerate() {
            if let (Some(col), Ok(al)) = (
                cols.get_mut(i),
                serde_json::from_value::<ast::TableColumnAlign>(serde_json::Value::String(
                    entry.trim().into(),
                )),
            ) {
                col.align = Some(al);
                col.authored = Some(true);
            }
        }
    }
    if let Some(v) = a.get("widths") {
        for (i, entry) in v.split(',').enumerate() {
            if let (Some(col), Some(w)) = (cols.get_mut(i), width(entry.trim())) {
                col.width = Some(w);
                col.authored = Some(true);
            }
        }
    }
    a.key_values
        .retain(|(k, _)| !TABLE_KEYS.iter().any(|t| t.eq_ignore_ascii_case(k)));
    let attrs = (!a.is_empty()).then_some(a);
    (cols.into_iter().map(Node::TableColumn).collect(), attrs)
}

/// C2-B-212, C2-B-220: a decimal number greater than 0 and at most 100.
fn width(e: &str) -> Option<f64> {
    let (int, frac) = e.split_once('.').map_or((e, None), |(a, b)| (a, Some(b)));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(int) || frac.is_some_and(|f| !digits(f)) {
        return None;
    }
    let v: f64 = e.parse().ok()?;
    (v > 0.0 && v <= 100.0).then_some(v / 100.0)
}

fn footer_count(attrs: Option<&Attrs>) -> usize {
    attrs
        .and_then(|a| a.get("footer-rows"))
        .and_then(count)
        .unwrap_or(0)
}
