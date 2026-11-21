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
    pub valign: Option<&'static str>,
    /// A span mark `<` or `^` (C2-B-172).
    pub span: Option<char>,
    pub pieces: Pieces,
    /// The text between the pipes (C2-A-149).
    pub start: usize,
    pub end: usize,
    /// A continuation row joined a fragment into this cell (C2-A-158).
    pub joined: bool,
}

/// The source pieces of a cell: almost always one, kept without a heap block.
#[derive(Debug, Clone)]
pub enum Pieces {
    One(Option<Piece>),
    Many(Vec<Piece>),
}

impl Default for Pieces {
    fn default() -> Self {
        Self::One(None)
    }
}

impl Pieces {
    pub fn push(&mut self, p: Piece) {
        match self {
            Self::One(slot @ None) => *slot = Some(p),
            Self::One(Some(first)) => *self = Self::Many(vec![*first, p]),
            Self::Many(v) => v.push(p),
        }
    }

    /// Drops the first piece.
    pub fn remove_first(&mut self) {
        match self {
            Self::One(slot) => *slot = None,
            Self::Many(v) if !v.is_empty() => {
                v.remove(0);
            }
            Self::Many(_) => {}
        }
    }

    /// Moves the pieces of `other` to the end, leaving it empty.
    pub fn append(&mut self, other: &mut Self) {
        for p in std::mem::take(other).iter() {
            self.push(*p);
        }
    }
}

impl std::ops::Deref for Pieces {
    type Target = [Piece];
    fn deref(&self) -> &[Piece] {
        match self {
            Self::One(p) => p.as_slice(),
            Self::Many(v) => v,
        }
    }
}

impl std::ops::DerefMut for Pieces {
    fn deref_mut(&mut self) -> &mut [Piece] {
        match self {
            Self::One(p) => p.as_mut_slice(),
            Self::Many(v) => v,
        }
    }
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
    let mut out = Vec::new();
    each_separator(r, |p| out.push(p));
    out
}

/// Calls `f` with the offset of each separator pipe of [`separators`].
fn each_separator(r: &str, mut f: impl FnMut(usize)) {
    let b = r.as_bytes();
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' => i += 2,
            b'`' => i = skip_code(b, i),
            b'|' => {
                f(i);
                i += 1;
            }
            _ => i += 1,
        }
    }
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
    let mut cells = Vec::new();
    let mut start = s;
    each_separator(area, |p| {
        cells.push(parse_cell(text, start, s + p));
        start = s + p + 1;
    });
    cells.push(parse_cell(text, start, e));
    cells
}

fn parse_cell(text: &str, start: usize, end: usize) -> Cell {
    let mut cell = Cell {
        start,
        end,
        ..Cell::default()
    };
    let mut i = start;
    if text.as_bytes().get(i) == Some(&b'=')
        && i < end
        && (i + 1 == end
            || matches!(
                text.as_bytes().get(i + 1),
                Some(b' ' | b'\t' | b'{' | b'<' | b'>' | b'?' | b'~')
            ))
    {
        cell.header = true;
        i += 1;
    }
    if text.as_bytes().get(i) == Some(&b'{') {
        if let Some((block, mark, valign, after)) = cell_brace(text, i, end) {
            cell.attrs = block;
            cell.align = mark;
            cell.valign = valign;
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
type CellBrace = (
    Option<AttrBlock>,
    Option<&'static str>,
    Option<&'static str>,
    usize,
);

fn cell_brace(text: &str, i: usize, end: usize) -> Option<CellBrace> {
    let b = text.as_bytes();
    let mark = match b.get(i + 1) {
        Some(b'<') => Some("left"),
        Some(b'>') => Some("right"),
        Some(b'~') => Some("center"),
        _ => None,
    };
    if mark.is_some() || b.get(i + 1) == Some(&b'?') {
        let mut prefix_end = i + 2;
        let valign = match b.get(prefix_end) {
            Some(b'^') => Some("top"),
            Some(b'~') => Some("middle"),
            Some(b'v') => Some("bottom"),
            _ => None,
        };
        if valign.is_some() {
            prefix_end += 1;
        }
        if mark.is_none() && valign.is_none() {
            return None;
        }
        if b.get(prefix_end) == Some(&b'}') {
            return Some((None, mark, valign, prefix_end + 1));
        }
        let inner = format!("{{{}", text.get(prefix_end..end)?);
        let (block, e) = parse_glued(&inner, 0)?;
        return Some((Some(block), mark, valign, prefix_end + e - 1));
    }
    let (block, e) = parse_glued(text, i)?;
    if e > end || block.empty {
        return None;
    }
    Some((Some(block), None, None, e))
}

/// Joins the fragments of a continuation row into the row above (C2-B-184).
/// `frags` are the fragment areas.
pub fn join_continuation(row: &mut Row, text: &str, frags: &[(usize, usize)]) {
    row.joined = true;
    for (col, &(s, e)) in frags.iter().enumerate() {
        let (a, z) = trim(text, s, e);
        let header = row
            .cells
            .iter()
            .all(|cell| cell.header || cell.span == Some('<'));
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
                header,
                joined: true,
                pieces: Pieces::One(Some(Piece {
                    start: a,
                    end: z.max(a),
                    join: Join::Line,
                })),
                ..Cell::default()
            }),
        }
    }
}
