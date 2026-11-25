//! Table nodes from the rows of the block pass: the head (C2-B-178), spans
//! (C2-B-172 to C2-B-174, C2-B-183), the footer (C2-B-187) and the columns
//! (C2-B-211, C2-B-212, C2-B-220).

use super::Builder;
use crate::Arena;
use crate::ast::{self, Node, owned::Attrs};
use crate::block::table::{Pieces, Row, TableData};
use crate::inline::{ContentKind, Join, Piece};

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
    /// C2-B-173: a failed mark remains the cell's text.
    failed: bool,
    origin: Option<Origin>,
}

/// One value per written cell, row by row, in one list.
struct Grid<T> {
    cells: Vec<T>,
    /// `starts[r]..starts[r + 1]` are the cells of row `r`.
    starts: Vec<usize>,
}

impl<T: Copy> Grid<T> {
    fn new(rows: &[Row], fill: T) -> Self {
        let mut starts = Vec::with_capacity(rows.len() + 1);
        let mut n = 0;
        starts.push(0);
        for r in rows {
            n += r.cells.len();
            starts.push(n);
        }
        Self {
            cells: vec![fill; n],
            starts,
        }
    }

    fn row(&self, r: usize) -> &[T] {
        match (self.starts.get(r), self.starts.get(r + 1)) {
            (Some(&a), Some(&z)) => self.cells.get(a..z).unwrap_or(&[]),
            _ => &[],
        }
    }

    fn get(&self, r: usize, c: usize) -> Option<&T> {
        self.row(r).get(c)
    }

    fn get_mut(&mut self, r: usize, c: usize) -> Option<&mut T> {
        let a = *self.starts.get(r)?;
        let z = *self.starts.get(r + 1)?;
        self.cells.get_mut(a..z)?.get_mut(c)
    }

    fn rows(&self) -> impl Iterator<Item = &[T]> {
        (0..self.starts.len().saturating_sub(1)).map(|r| self.row(r))
    }
}

#[derive(Clone, Copy)]
struct GroupSpan {
    lo: usize,
    hi: usize,
    kind: u8,
    header_rows: usize,
    row_head_columns: Option<i64>,
}

#[derive(Clone, Copy)]
struct SpanBounds {
    head: usize,
    foot_start: usize,
    body_start: bool,
}

#[derive(Clone, Copy)]
struct ConsumedKeys(u8);

impl ConsumedKeys {
    fn has(self, bit: u8) -> bool {
        self.0 & bit != 0
    }
}

impl<'a> Builder<'_, 'a> {
    pub(super) fn table(
        &self,
        mut data: TableData,
        attrs: Option<Attrs>,
        lang: Option<&'a str>,
        start: usize,
        end: usize,
    ) -> Node<'a> {
        // C2-A-086, gap G-012: comments after the last row belong to the table.
        let trailing = data.rows.last_mut().map_or_else(Vec::new, |r| {
            let split = r.comments.partition_point(|(start, _, _)| *start < r.end);
            r.comments.split_off(split)
        });
        let marked = has_marks(&data.rows);
        let head = head_rows(&data.rows, marked);
        let n = data.rows.len();
        let foot = footer_count(attrs.as_ref()).min(n - head);
        let plan = body_plan(attrs.as_ref(), n - head - foot);
        let body_starts = body_starts(head, plan.as_ref());
        let spans = marked.then(|| join_spans(&mut data.rows, head, n - foot, &body_starts));
        let ncols = data.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
        let (columns, attrs) = columns(self.arena, ncols, attrs, false, plan.is_some());
        let mut groups: Vec<Node<'a>> = Vec::new();
        if head > 0 {
            groups.push(self.group(
                &data.rows,
                spans.as_ref(),
                GroupSpan {
                    lo: 0,
                    hi: head,
                    kind: 0,
                    header_rows: 0,
                    row_head_columns: None,
                },
            ));
        }
        if let Some(plan) = &plan {
            let mut lo = head;
            for (index, &count) in plan.data_rows.iter().enumerate() {
                let headers = plan.header_rows.get(index).copied().unwrap_or(0);
                let hi = lo + headers + count;
                groups.push(self.group(
                    &data.rows,
                    spans.as_ref(),
                    GroupSpan {
                        lo,
                        hi,
                        kind: 1,
                        header_rows: headers,
                        row_head_columns: plan.head_cols.get(index).copied().flatten(),
                    },
                ));
                lo = hi;
            }
        } else if head < n - foot {
            groups.push(self.group(
                &data.rows,
                spans.as_ref(),
                GroupSpan {
                    lo: head,
                    hi: n - foot,
                    kind: 1,
                    header_rows: 0,
                    row_head_columns: None,
                },
            ));
        }
        if foot > 0 {
            groups.push(self.group(
                &data.rows,
                spans.as_ref(),
                GroupSpan {
                    lo: n - foot,
                    hi: n,
                    kind: 2,
                    header_rows: 0,
                    row_head_columns: None,
                },
            ));
        }
        for (s, e, value) in trailing {
            groups.push(self.comment(s, e, &value));
        }
        Node::Table(self.arena.alloc(ast::Table {
            children: self.arena.nodes(&mut groups),
            columns,
            attrs: self.at(attrs),
            lang,
            pos: Some(self.src.pos(start, end)),
            ..Default::default()
        }))
    }

    /// C2-B-209, C2-B-221: the head count of a delimited table and its partition.
    pub(super) fn delimited_table(
        &self,
        value: &str,
        sep: char,
        attrs: Option<Attrs>,
        lang: Option<&'a str>,
        pos: Option<ast::Pos>,
    ) -> Node<'a> {
        let declared = attrs
            .as_ref()
            .and_then(|a| a.get("header-rows"))
            .and_then(count);
        let footer = footer_count(attrs.as_ref());
        let (children, ncols, body_valid) =
            super::delimited::table(self.arena, value, sep, declared, footer, attrs.as_ref());
        if let Some(quote) = super::delimited::unclosed_quote_offset(value, sep) {
            let start = pos.map_or(0, |p| p.start());
            let text = self.src.text().get(start..).unwrap_or("");
            let body = text
                .find("```")
                .and_then(|fence| text.get(fence..)?.find('\n').map(|line| fence + line + 1));
            let start = start + body.unwrap_or(0) + quote;
            self.findings.borrow_mut().push(crate::block::Finding {
                code: "delimited-table-malformed",
                message: 0,
                start,
                end: start + 1,
                values: Vec::new(),
            });
        }
        let attrs = attrs.map(|mut a| {
            a.key_values
                .retain(|(k, _)| !k.eq_ignore_ascii_case("table"));
            a
        });
        let (columns, attrs) = columns(
            self.arena,
            ncols,
            attrs.filter(|a| !a.is_empty()),
            true,
            body_valid,
        );
        Node::Table(self.arena.alloc(ast::Table {
            children,
            columns,
            attrs: self.at(attrs),
            lang,
            pos,
            ..Default::default()
        }))
    }

    fn comment(&self, s: usize, e: usize, value: &str) -> Node<'a> {
        Node::Comment(self.arena.alloc(ast::Comment {
            form: ast::CommentForm::Line,
            value: self.s(value),
            pos: Some(self.src.pos(s, e)),
            ..Default::default()
        }))
    }

    fn group(&self, rows: &[Row], spans: Option<&Grid<Resolved>>, span: GroupSpan) -> Node<'a> {
        let GroupSpan {
            lo,
            hi,
            kind,
            header_rows,
            row_head_columns,
        } = span;
        let mut children = Vec::new();
        for (i, row) in rows.iter().enumerate().take(hi).skip(lo) {
            children.push(self.row(row, spans.map(|s| s.row(i)), i < lo + header_rows));
            for (s, e, value) in &row.comments {
                children.push(self.comment(*s, *e, value));
            }
        }
        let first = rows.get(lo).map_or(0, |r| r.start);
        let last = rows.get(hi.saturating_sub(1)).map_or(0, |r| r.end);
        let pos = (lo < hi).then(|| self.src.pos(first, last));
        let children = self.arena.nodes(&mut children);
        match kind {
            0 => Node::TableHead(self.arena.alloc(ast::TableHead {
                children,
                pos,
                ..Default::default()
            })),
            1 => Node::TableBody(self.arena.alloc(ast::TableBody {
                children,
                row_head_columns,
                pos,
                ..Default::default()
            })),
            _ => Node::TableFoot(self.arena.alloc(ast::TableFoot {
                children,
                pos,
                ..Default::default()
            })),
        }
    }

    fn row(&self, row: &Row, spans: Option<&[Resolved]>, force_header: bool) -> Node<'a> {
        let plain = Resolved {
            colspan: 1,
            rowspan: 1,
            ..Resolved::default()
        };
        let cells = self
            .arena
            .fill(row.cells.iter().enumerate().map(|(i, cell)| {
                let sp = spans.and_then(|s| s.get(i)).unwrap_or(&plain);
                // Gap G-009: a cell with no content has no position.
                let blank = cell
                    .pieces
                    .iter()
                    .all(|p| crate::block::trim_end(self.src, p.start, p.end) == p.start);
                let positioned = !cell.joined && !blank && sp.mark.is_none();
                let marked = sp.mark.is_some();
                // C2-A-158: a cell that holds a span mark spans the text between its pipes.
                let cell_positioned = !cell.joined;
                let children = self.cell_children(cell, positioned, marked);
                let header = (cell.header || force_header) && !marked;
                let (attrs, lang) = if marked {
                    (None, None)
                } else {
                    self.split_attrs(cell.attrs.clone())
                };
                let span = sp.mark.map(|m| {
                    if m == '<' {
                        ast::TableCellSpan::Colspan
                    } else {
                        ast::TableCellSpan::Rowspan
                    }
                });
                let align = if marked {
                    None
                } else {
                    cell.align.and_then(ast::TableCellAlign::from_name)
                };
                let valign = if marked {
                    None
                } else {
                    cell.valign.and_then(ast::TableCellValign::from_name)
                };
                Node::TableCell(self.arena.alloc(ast::TableCell {
                    children,
                    header,
                    span,
                    colspan: (sp.colspan > 1).then(|| i64::try_from(sp.colspan).unwrap_or(1)),
                    rowspan: (sp.rowspan > 1).then(|| i64::try_from(sp.rowspan).unwrap_or(1)),
                    align,
                    valign,
                    attrs: self.at(attrs),
                    lang,
                    pos: cell_positioned.then(|| self.src.pos(cell.start, cell.end)),
                    ..Default::default()
                }))
            }));
        let (attrs, lang) = self.split_attrs(row.attrs.clone());
        let pos = (!row.joined).then(|| self.src.pos(row.start, row.end));
        Node::TableRow(self.arena.alloc(ast::TableRow {
            children: cells,
            attrs: self.at(attrs),
            lang,
            pos,
            ..Default::default()
        }))
    }
    #[inline]
    fn cell_children(
        &self,
        cell: &crate::block::table::Cell,
        positioned: bool,
        marked: bool,
    ) -> &'a mut [Node<'a>] {
        if marked {
            &mut []
        } else {
            let inl = self.inline_cell(&cell.pieces);
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
            self.arena
                .one(Node::CellParagraph(self.arena.alloc(ast::CellParagraph {
                    children: inl,
                    pos: ppos,
                    ..Default::default()
                })))
        }
    }

    /// C2-B-184: each physical row fragment is a separate inline content.
    fn inline_cell(&self, pieces: &[Piece]) -> &'a mut [Node<'a>] {
        if pieces.len() <= 1 {
            return self.inline(ContentKind::Cell, pieces);
        }
        let mut out = Vec::new();
        for piece in pieces {
            let nodes = self.inline(ContentKind::Cell, std::slice::from_ref(piece));
            if nodes.is_empty() {
                continue;
            }
            if !out.is_empty() {
                append_text(&mut out, self.arena, " ");
            }
            for node in nodes {
                let node =
                    std::mem::replace(node, Node::Text(self.arena.alloc(ast::Text::default())));
                if let Node::Text(t) = &node {
                    if matches!(out.last(), Some(Node::Text(_))) {
                        append_text(&mut out, self.arena, t.value);
                    } else {
                        out.push(node);
                    }
                } else {
                    out.push(node);
                }
            }
        }
        self.arena.nodes(&mut out)
    }
}

fn append_text<'a>(out: &mut Vec<Node<'a>>, arena: &'a Arena, value: &str) {
    if let Some(Node::Text(t)) = out.last_mut() {
        t.value = arena.str(&format!("{}{value}", t.value));
        t.pos = None;
    } else {
        out.push(Node::Text(arena.alloc(ast::Text {
            value: arena.str(value),
            ..Default::default()
        })));
    }
}

/// C2-B-178: leading rows with a header cell and no other cell.
fn head_rows(rows: &[Row], marked: bool) -> usize {
    if !marked {
        return rows
            .iter()
            .take_while(|row| {
                row.cells.iter().any(|c| c.header) && row.cells.iter().all(|c| c.header)
            })
            .count();
    }
    let provisional = resolve_spans(rows, rows.len(), rows.len(), &[]);
    rows.iter()
        .zip(provisional.rows())
        .take_while(|(row, spans)| {
            let mut has_header = false;
            let mut has_other = false;
            for (cell, span) in row.cells.iter().zip(*spans) {
                if cell.header {
                    has_header = true;
                } else if span.mark.is_none() {
                    has_other = true;
                }
            }
            has_header && !has_other
        })
        .count()
}

fn has_marks(rows: &[Row]) -> bool {
    rows.iter()
        .any(|r| r.cells.iter().any(|c| c.span.is_some()))
}

fn resolve_spans(
    rows: &[Row],
    head: usize,
    foot_start: usize,
    body_starts: &[usize],
) -> Grid<Resolved> {
    let mut out = Grid::new(
        rows,
        Resolved {
            colspan: 1,
            rowspan: 1,
            ..Resolved::default()
        },
    );
    let mut grid: Grid<Option<Origin>> = Grid::new(rows, None);
    let mut starts = vec![false; rows.len()];
    for &row in body_starts {
        if let Some(slot) = starts.get_mut(row) {
            *slot = true;
        }
    }
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            let origin = span_origin(
                cell.span,
                r,
                c,
                SpanBounds {
                    head,
                    foot_start,
                    body_start: starts.get(r).copied().unwrap_or(false),
                },
                &grid,
                &out,
            );
            let set = match (cell.span, origin) {
                (Some(m), Some(o)) => {
                    if let Some(res) = out.get_mut(o.row, o.col) {
                        if m == '<' {
                            res.colspan += 1;
                        } else {
                            res.rowspan += 1;
                        }
                    }
                    if let Some(res) = out.get_mut(r, c) {
                        res.mark = Some(m);
                        res.origin = Some(o);
                    }
                    o
                }
                (Some(_), None) => {
                    if let Some(res) = out.get_mut(r, c) {
                        res.failed = true;
                    }
                    Origin { row: r, col: c }
                }
                _ => Origin { row: r, col: c },
            };
            if let Some(g) = grid.get_mut(r, c) {
                *g = Some(set);
            }
        }
    }
    out
}

/// Resolves the span marks and moves the fragments under a mark into the cell that covers it
/// (C2-B-184).
fn join_spans(
    rows: &mut [Row],
    head: usize,
    foot_start: usize,
    body_starts: &[usize],
) -> Grid<Resolved> {
    let spans = resolve_spans(rows, head, foot_start, body_starts);
    for (row, resolved) in rows.iter_mut().zip(spans.rows()) {
        for (cell, mark) in row.cells.iter_mut().zip(resolved) {
            if mark.mark.is_some() && !cell.pieces.is_empty() {
                cell.pieces.remove_first();
            }
        }
    }
    move_joined_marks(rows, &spans);
    spans
}

/// The cell pieces of a scanned table as the tree holds them: `groups` gives the row count and
/// kind (0 head, 1 body, 2 foot) of each group of the table node.
pub(crate) fn join_span_pieces(rows: &mut [Row], groups: &[(u8, usize)]) {
    if !has_marks(rows) {
        return;
    }
    let mut head = 0;
    let mut foot_start = rows.len();
    let mut body_starts = Vec::new();
    let mut at = 0;
    for &(kind, count) in groups {
        match kind {
            0 => head = at + count,
            1 => body_starts.push(at),
            _ => foot_start = foot_start.min(at),
        }
        at += count;
    }
    join_spans(rows, head, foot_start, &body_starts);
}

fn move_joined_marks(rows: &mut [Row], spans: &Grid<Resolved>) {
    let mut moves: Vec<(Origin, Pieces)> = Vec::new();
    for (r, row) in rows.iter_mut().enumerate() {
        for (c, cell) in row.cells.iter_mut().enumerate() {
            let origin = spans.get(r, c).and_then(|sp| sp.origin);
            if let Some(origin) = origin.filter(|_| !cell.pieces.is_empty()) {
                moves.push((origin, std::mem::take(&mut cell.pieces)));
                cell.joined = false;
            }
        }
    }
    for (origin, mut pieces) in moves {
        if let Some(cell) = rows
            .get_mut(origin.row)
            .and_then(|r| r.cells.get_mut(origin.col))
        {
            if !cell.pieces.is_empty() {
                if let Some(first) = pieces.first_mut() {
                    first.join = Join::Cell;
                }
            }
            cell.pieces.append(&mut pieces);
            cell.joined = true;
        }
    }
}

fn span_origin(
    span: Option<char>,
    r: usize,
    c: usize,
    bounds: SpanBounds,
    grid: &Grid<Option<Origin>>,
    out: &Grid<Resolved>,
) -> Option<Origin> {
    let SpanBounds {
        head,
        foot_start,
        body_start,
    } = bounds;
    match span {
        Some('<') if c > 0 => grid.get(r, c - 1).copied().flatten().filter(|o| o.row == r),
        Some('^') if r > 0 && !(r == head && head > 0) && r != foot_start && !body_start => grid
            .get(r - 1, c)
            .copied()
            .flatten()
            // Gap G-024: as in the oracle, a cell that spans columns takes no row span.
            .filter(|o| o.col == c)
            .filter(|o| out.get(o.row, o.col).is_some_and(|x| x.colspan == 1)),
        _ => None,
    }
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

pub(super) struct BodyPlan {
    pub(super) data_rows: Vec<usize>,
    pub(super) header_rows: Vec<usize>,
    pub(super) head_cols: Vec<Option<i64>>,
}

fn body_counts(value: &str) -> Option<Vec<usize>> {
    if value.is_empty() {
        return Some(Vec::new());
    }
    value
        .split(',')
        .map(|entry| {
            let entry = entry.trim_matches([' ', '\t']);
            (!entry.is_empty() && entry.bytes().all(|b| b.is_ascii_digit()))
                .then(|| entry.parse().ok())?
        })
        .collect()
}

fn body_head_cols(value: &str) -> Option<Vec<Option<i64>>> {
    value
        .split(',')
        .map(|entry| {
            let entry = entry.trim_matches([' ', '\t']);
            if entry.is_empty() {
                Some(None)
            } else if entry.bytes().all(|b| b.is_ascii_digit()) {
                entry.parse().ok().map(Some)
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn body_plan(attrs: Option<&Attrs>, available: usize) -> Option<BodyPlan> {
    let attrs = attrs?;
    let data_rows = body_counts(attrs.get("body-rows")?)?;
    let header_rows = if let Some(value) = attrs.get("body-header-rows") {
        body_counts(value)?
    } else {
        vec![0; data_rows.len()]
    };
    let head_cols = if let Some(value) = attrs.get("body-header-cols") {
        if data_rows.is_empty() && value.is_empty() {
            Vec::new()
        } else {
            body_head_cols(value)?
        }
    } else {
        vec![None; data_rows.len()]
    };
    if data_rows.len() != header_rows.len() || data_rows.len() != head_cols.len() {
        return None;
    }
    let total = data_rows
        .iter()
        .chain(&header_rows)
        .try_fold(0usize, |sum, &n| sum.checked_add(n))?;
    (total == available).then_some(BodyPlan {
        data_rows,
        header_rows,
        head_cols,
    })
}

fn body_starts(head: usize, plan: Option<&BodyPlan>) -> Vec<usize> {
    let mut starts = Vec::new();
    let Some(plan) = plan else { return starts };
    let mut row = head;
    for (&data, &headers) in plan.data_rows.iter().zip(&plan.header_rows) {
        starts.push(row);
        row = row.saturating_add(data).saturating_add(headers);
    }
    starts
}

/// The columns and the table attributes that are valid and consumed (C2-B-227).
fn columns<'a>(
    arena: &'a Arena,
    ncols: usize,
    attrs: Option<Attrs>,
    header_applies: bool,
    body_valid: bool,
) -> (&'a mut [Node<'a>], Option<Attrs>) {
    let mut cols: Vec<ast::TableColumn<'a>> =
        (0..ncols).map(|_| ast::TableColumn::default()).collect();
    let nodes = |cols: Vec<ast::TableColumn<'a>>| -> &'a mut [Node<'a>] {
        let mut v: Vec<Node<'a>> = cols
            .into_iter()
            .map(|col| Node::TableColumn(arena.alloc(col)))
            .collect();
        arena.nodes(&mut v)
    };
    let Some(mut a) = attrs else {
        return (nodes(cols), None);
    };
    let aligns = a.get("aligns").and_then(|v| {
        v.split(',')
            .map(|entry| ast::TableColumnAlign::from_name(entry.trim()))
            .collect::<Option<Vec<_>>>()
    });
    let valigns = a.get("valigns").and_then(|v| {
        v.split(',')
            .map(|entry| ast::TableColumnValign::from_name(entry.trim()))
            .collect::<Option<Vec<_>>>()
    });
    let widths = a.get("widths").and_then(|v| {
        v.split(',')
            .map(|entry| width(entry.trim()))
            .collect::<Option<Vec<_>>>()
    });
    if let Some(values) = aligns.as_ref() {
        for (col, align) in cols.iter_mut().zip(values) {
            col.align = Some(*align);
        }
    }
    if let Some(values) = valigns.as_ref() {
        for (col, valign) in cols.iter_mut().zip(values) {
            col.valign = Some(*valign);
        }
    }
    if let Some(values) = widths.as_ref() {
        for (col, &width) in cols.iter_mut().zip(values) {
            col.width = Some(width);
        }
    }
    let header_valid = header_applies && a.get("header-rows").is_some_and(|v| count(v).is_some());
    let footer_valid = a.get("footer-rows").is_some_and(|v| count(v).is_some());
    let consumed = ConsumedKeys(
        u8::from(aligns.is_some())
            | (u8::from(valigns.is_some()) << 1)
            | (u8::from(widths.is_some()) << 2)
            | (u8::from(header_valid) << 3)
            | (u8::from(footer_valid) << 4)
            | (u8::from(body_valid) << 5),
    );
    a.key_values
        .retain(|(key, _)| !consumed_column_key(key, consumed));
    let attrs = (!a.is_empty()).then_some(a);
    (nodes(cols), attrs)
}

fn consumed_column_key(key: &str, consumed: ConsumedKeys) -> bool {
    if key.eq_ignore_ascii_case("aligns") {
        consumed.has(1)
    } else if key.eq_ignore_ascii_case("valigns") {
        consumed.has(1 << 1)
    } else if key.eq_ignore_ascii_case("widths") {
        consumed.has(1 << 2)
    } else if key.eq_ignore_ascii_case("header-rows") {
        consumed.has(1 << 3)
    } else if key.eq_ignore_ascii_case("footer-rows") {
        consumed.has(1 << 4)
    } else {
        consumed.has(1 << 5)
            && ["body-rows", "body-header-rows", "body-header-cols"]
                .iter()
                .any(|name| key.eq_ignore_ascii_case(name))
    }
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
