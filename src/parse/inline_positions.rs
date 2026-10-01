use super::document_column;
use crate::ast::Pos;

pub(crate) struct InlineAnchor<'a> {
    /// Original codepoint column for each expanded line-block character.
    pub(super) columns: Option<&'a [Vec<Option<usize>>]>,
    pub(super) lines: &'a [Option<(usize, isize)>],
    /// Byte offsets in the text at which a new anchor SEGMENT begins without a
    /// newline being there to mark it.
    ///
    /// A newline needs no list. A TABLE CELL rebuilt across a `+` continuation
    /// has none - its fragments are joined by a manufactured space - yet each
    /// fragment sits on a different source line, so without this the whole cell
    /// reads as one and every later position lands on the wrong column.
    ///
    /// Each break opens a segment, so `lines` is indexed by segment rather than
    /// by newline count.
    ///
    /// A BREAK IS A GAP, which a newline is not. The source holds characters
    /// between two segments that the text does not - a closing pipe, a line
    /// break, a `+` and an opening pipe - so a span reaching across one would
    /// not select its own text. Any node that crosses a break is therefore left
    /// unplaced: absent beats wrong (PART 12 section 4). A newline needs no such
    /// rule because it is IN the text, so a span across it still selects itself.
    ///
    /// Offsets are ASCENDING and each one sits IN FRONT OF A CHARACTER, never
    /// at end of text - a segment with nothing in it would have no position to
    /// give.
    pub(super) breaks: &'a [usize],
}

impl<'a> InlineAnchor<'a> {
    pub(super) fn lines(lines: &'a [Option<(usize, isize)>]) -> Self {
        Self {
            lines,
            breaks: &[],
            columns: None,
        }
    }
}

pub(super) struct InlinePositionMap<'a> {
    columns: Option<&'a [Vec<Option<usize>>]>,
    pub(super) unmapped_prefix: Option<Vec<usize>>,
    lines: &'a [Option<(usize, isize)>],
    byte_line: Vec<usize>,
    byte_column: Vec<usize>,
    /// Whether the segments are separated by GAPS rather than by newlines -
    /// see `InlineAnchor::breaks`. A span across a gap carries no position.
    gapped: bool,
}

impl<'a> InlinePositionMap<'a> {
    pub(super) fn new(text: &str, anchor: InlineAnchor<'a>) -> Self {
        let mut byte_line = vec![0usize; text.len() + 1];
        let mut byte_column = vec![0usize; text.len() + 1];
        let mut line = 0usize;
        let mut column = 0usize;
        let mut breaks = anchor.breaks.iter().copied().peekable();
        for (byte, ch) in text.char_indices() {
            // A break OPENS a segment, so it is applied BEFORE the character it
            // sits in front of rather than after the one behind it.
            while breaks.peek() == Some(&byte) {
                breaks.next();
                line += 1;
                column = 0;
            }
            byte_line[byte] = line;
            byte_column[byte] = column;
            for idx in byte + 1..byte + ch.len_utf8() {
                byte_line[idx] = line;
                byte_column[idx] = column;
            }
            if ch == '\n' {
                line += 1;
                column = 0;
            } else {
                column += 1;
            }
        }
        byte_line[text.len()] = line;
        byte_column[text.len()] = column;
        let unmapped_prefix = anchor.columns.map(|columns| {
            let mut prefix = vec![0; text.len() + 1];
            for (byte, ch) in text.char_indices() {
                let missing = ch != '\n'
                    && columns
                        .get(byte_line[byte])
                        .and_then(|line| line.get(byte_column[byte]))
                        .copied()
                        .flatten()
                        .is_none();
                for index in byte + 1..=byte + ch.len_utf8() {
                    prefix[index] = prefix[byte] + usize::from(missing);
                }
            }
            prefix
        });
        Self {
            columns: anchor.columns,
            unmapped_prefix,
            lines: anchor.lines,
            byte_line,
            byte_column,
            gapped: !anchor.breaks.is_empty(),
        }
    }

    pub(super) fn pos(&self, start: usize, end: usize) -> Option<Pos> {
        if start > end || end > self.byte_line.len().saturating_sub(1) {
            return None;
        }
        let start_line_idx = *self.byte_line.get(start)?;
        let end_line_idx = *self.byte_line.get(end)?;
        if self.gapped && start_line_idx != end_line_idx {
            return None;
        }
        for idx in start_line_idx..=end_line_idx {
            self.lines.get(idx).copied().flatten()?;
        }
        let (start_line, start_stripped) = self.lines.get(start_line_idx).copied().flatten()?;
        let (end_line, end_stripped) = self.lines.get(end_line_idx).copied().flatten()?;
        let (start_column, end_column) = if let Some(columns) = self.columns {
            let start_column = columns
                .get(start_line_idx)?
                .get(self.byte_column[start])
                .copied()
                .flatten()?;
            let end_column = if self.byte_column[end] == 0 {
                0
            } else {
                columns
                    .get(end_line_idx)?
                    .get(self.byte_column[end] - 1)
                    .copied()
                    .flatten()?
                    + 1
            };
            (start_column, end_column)
        } else {
            (self.byte_column[start], self.byte_column[end])
        };
        let start_column = document_column(start_stripped, start_column);
        let end_column = document_column(end_stripped, end_column);
        if self.columns.is_some() && start_line == end_line && start_column == end_column {
            return None;
        }
        Some(Pos {
            start_line,
            end_line,
            start_column,
            end_column,
            start_offset: 0,
            end_offset: 0,
            file: None,
        })
    }
}
