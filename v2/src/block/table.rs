//! Pipe tables (C2-B-166 to C2-B-187, `table-marks.json`): cell split, cell
//! prefixes, span marks, continuation rows and the head of a table.

use crate::attr::{AttrBlock, parse_glued};
use crate::inline::{Join, Piece};

/// One cell as written.
#[derive(Debug, Clone, Default)]
pub struct Cell {
    pub header: bool,
    pub attrs: Option<AttrBlock>,
    /// Alignment mark of the cell prefix (C2-B-182).
    pub align: Option<&'static str>,
    /// A span mark `<` or `^` (C2-B-172).
    pub span: Option<char>,
    pub pieces: Vec<Piece>,
    /// The text between the pipes (C2-A-149).
    pub start: usize,
    pub end: usize,
    /// A continuation row joined a fragment into this cell (C2-A-158).
    pub joined: bool,
}

/// One row as written, with the comment-only lines that follow it.
#[derive(Debug, Clone, Default)]
pub struct Row {
    pub cells: Vec<Cell>,
    pub start: usize,
    pub end: usize,
    pub attrs: Option<AttrBlock>,
    pub joined: bool,
    /// `(start, end, value)` of comment-only lines below the row (C2-A-086).
    pub comments: Vec<(usize, usize, String)>,
}

/// A pipe table as the block pass collected it.
#[derive(Debug, Clone, Default)]
pub struct TableData {
    pub rows: Vec<Row>,
}

/// Byte offsets of the separator pipes of a row: not escaped, not between a
/// backtick run and its matching run on the same line (C2-B-169).
#[must_use]
pub fn separators(r: &str) -> Vec<usize> {
    let b = r.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' => i += 2,
            b'`' => i = skip_code(b, i),
            b'|' => {
                out.push(i);
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// Past a backtick run and its closing run of the same length, or past the
/// opening run alone when no closer follows on the line.
fn skip_code(b: &[u8], i: usize) -> usize {
    let n = b
        .get(i..)
        .map_or(0, |r| r.iter().take_while(|&&c| c == b'`').count());
    let mut j = i + n;
    while j < b.len() {
        if b.get(j) == Some(&b'`') {
            let m = b
                .get(j..)
                .map_or(0, |r| r.iter().take_while(|&&c| c == b'`').count());
            if m == n {
                return j + m;
            }
            j += m;
        } else {
            j += 1;
        }
    }
    i + n
}

const fn is_pad(b: u8) -> bool {
    b == b' ' || b == b'\t'
}

/// Trims padding from a cell area; returns the content range.
fn trim(text: &str, s: usize, e: usize) -> (usize, usize) {
    let b = text.as_bytes();
    let mut a = s;
    let mut z = e;
    while a < z && b.get(a).copied().is_some_and(is_pad) {
        a += 1;
    }
    while z > a && b.get(z - 1).copied().is_some_and(is_pad) {
        z -= 1;
    }
    (a, z)
}

/// Splits a row line (`text` is the whole source, `s..e` the cell area
/// between the outer pipes, `seps` the absolute separator offsets inside it).
#[must_use]
pub fn parse_cells(text: &str, s: usize, e: usize) -> Vec<Cell> {
    let area = text.get(s..e).unwrap_or("");
    let mut bounds = vec![s];
    bounds.extend(separators(area).into_iter().map(|p| s + p + 1));
    let mut cells = Vec::new();
    for (k, &start) in bounds.iter().enumerate() {
        let end = bounds.get(k + 1).map_or(e, |n| n - 1);
        cells.push(parse_cell(text, start, end));
    }
    cells
}

fn parse_cell(text: &str, start: usize, end: usize) -> Cell {
    let mut cell = Cell {
        start,
        end,
        ..Cell::default()
    };
    let mut i = start;
    if text.as_bytes().get(i) == Some(&b'=') && i < end {
        cell.header = true;
        i += 1;
    }
    if text.as_bytes().get(i) == Some(&b'{') {
        if let Some((block, mark, after)) = cell_brace(text, i, end) {
            cell.attrs = block;
            cell.align = mark;
            i = after;
        }
    }
    let (a, z) = trim(text, i, end);
    match text.get(a..z) {
        Some("<") => cell.span = Some('<'),
        Some("^") => cell.span = Some('^'),
        _ => {}
    }
    if a < z || cell.span.is_none() {
        cell.pieces.push(Piece {
            start: a,
            end: z,
            join: Join::Line,
        });
    }
    cell
}

/// The brace block of a cell prefix: an alignment mark first (C2-B-182),
/// then the parts of an attribute block.
fn cell_brace(
    text: &str,
    i: usize,
    end: usize,
) -> Option<(Option<AttrBlock>, Option<&'static str>, usize)> {
    let b = text.as_bytes();
    let mark = match b.get(i + 1) {
        Some(b'<') => Some("left"),
        Some(b'>') => Some("right"),
        Some(b'~') => Some("center"),
        _ => None,
    };
    if mark.is_some() {
        if b.get(i + 2) == Some(&b'}') {
            return Some((None, mark, i + 3));
        }
        let inner = format!("{{{}", text.get(i + 2..end)?);
        let (block, e) = parse_glued(&inner, 0)?;
        return Some((Some(block), mark, i + 2 + e - 1));
    }
    let (block, e) = parse_glued(text, i)?;
    if e > end || block.empty {
        return None;
    }
    Some((Some(block), None, e))
}

/// Joins the fragments of a continuation row into the row above (C2-B-184).
/// `frags` are the fragment areas.
pub fn join_continuation(row: &mut Row, text: &str, frags: &[(usize, usize)]) {
    row.joined = true;
    for (col, &(s, e)) in frags.iter().enumerate() {
        let (a, z) = trim(text, s, e);
        match row.cells.get_mut(col) {
            // C2-B-184: an empty fragment adds nothing to a cell.
            Some(_) if a >= z => {}
            Some(cell) => {
                cell.joined = true;
                if cell.pieces.is_empty() {
                    cell.pieces.push(Piece {
                        start: a,
                        end: z,
                        join: Join::Line,
                    });
                } else {
                    cell.pieces.push(Piece {
                        start: a,
                        end: z,
                        join: Join::Cell,
                    });
                }
            }
            // C2-B-176: a fragment past the last column forms a cell, even an empty one.
            None => row.cells.push(Cell {
                start: s,
                end: e,
                joined: true,
                pieces: vec![Piece {
                    start: a,
                    end: z.max(a),
                    join: Join::Line,
                }],
                ..Cell::default()
            }),
        }
    }
}
