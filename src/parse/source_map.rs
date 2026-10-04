use crate::ast::Pos;

enum LineOrigin {
    Mapped(usize),
    Unmapped,
    Synthetic,
}

pub(super) struct SourceLine {
    pub(super) text: String,
    origin: LineOrigin,
    pub(super) stripped: Option<isize>,
}

impl SourceLine {
    pub(super) fn source_line(&self) -> Option<usize> {
        match self.origin {
            LineOrigin::Mapped(line) => Some(line),
            LineOrigin::Unmapped | LineOrigin::Synthetic => None,
        }
    }

    pub(super) fn is_synthetic(&self) -> bool {
        matches!(self.origin, LineOrigin::Synthetic)
    }
}

#[derive(Default)]
pub(super) struct LineBuffer {
    pub(super) lines: Vec<SourceLine>,
}

/// Keep earlier unmapped lines aligned when a mapped source gains its first origin.
fn pad_line_map(line_map: &mut Vec<Option<usize>>, lines: usize, source_line: Option<usize>) {
    if source_line.is_some() && line_map.len() + 1 < lines {
        line_map.resize(lines - 1, None);
    }
}

impl LineBuffer {
    pub(super) fn push_at(&mut self, text: String, origin: Option<usize>, stripped: Option<isize>) {
        self.lines.push(SourceLine {
            text,
            origin: origin.map_or(LineOrigin::Unmapped, LineOrigin::Mapped),
            stripped,
        });
    }

    pub(super) fn push_synthetic_blank(&mut self) {
        self.lines.push(SourceLine {
            text: String::new(),
            origin: LineOrigin::Synthetic,
            stripped: None,
        });
    }

    pub(super) fn into_source(self) -> MappedSource {
        let ends_in_authored_blank = self
            .lines
            .last()
            .is_some_and(|line| !line.is_synthetic() && line.text.is_empty());
        let mapped = self.lines.iter().any(|line| line.source_line().is_some());
        let mut source =
            String::with_capacity(self.lines.iter().map(|line| line.text.len() + 1).sum());
        let mut line_map = if mapped {
            Vec::with_capacity(self.lines.len())
        } else {
            Vec::new()
        };
        let mut col_map = Vec::with_capacity(self.lines.len());
        for (index, line) in self.lines.into_iter().enumerate() {
            if index > 0 {
                source.push('\n');
            }
            source.push_str(&line.text);
            if mapped {
                line_map.push(line.source_line());
            }
            col_map.push(line.stripped);
        }
        if ends_in_authored_blank {
            source.push('\n');
        }
        MappedSource {
            col_map,
            source,
            line_map,
            authored_base_at_start: false,
            item_marker_at_start: false,
            reached: Vec::new(),
            sublists_carry_authored_base: false,
        }
    }
}

#[derive(Clone)]
pub(super) struct MappedSource {
    pub(super) source: String,
    pub(super) line_map: Vec<Option<usize>>,
    /// Codepoints stripped from the front of each line - a blockquote marker, a list
    /// indent, a container prefix. Without it a column in `source` cannot be
    /// mapped back to a column in the document, which is why nested blocks
    /// could not carry a position (spec PART 12 section 4).
    pub(super) col_map: Vec<Option<isize>>,
    /// The collector consumed an authored blank immediately before this
    /// chunk's first line, making an over-indented opener a block boundary.
    pub(super) authored_base_at_start: bool,
    /// The first line is the authored content of an enclosing item marker.
    pub(super) item_marker_at_start: bool,
    /// Did each line REACH the enclosing container's content column?
    ///
    /// The collector leaves a line one column PAST the column and a line BELOW
    /// it with the SAME single residual column - `- - x` over `   # H` and over
    /// ` # H` both arrive here as `- x` / ` # H` - so the re-parse cannot tell
    /// the container's own authored base from a descendant's lazy line without
    /// being told (markup-carve/carve#1896).
    ///
    /// Recorded by the collectors alone. `false` is the conservative reading
    /// everywhere else, and a short vector reads as `false` beyond its end.
    pub(super) reached: Vec<bool>,
    /// Whether an over-indented sublist in this source carries its own authored
    /// base, so its span begins AT ITS MARKER rather than at the placing indent.
    ///
    /// A footnote body and a definition body both rebase over-indented blocks
    /// with sublists included (`rebase_overindented_blocks(.., true)`), so a
    /// list marker written past the body's own content column establishes a base
    /// of its own (PART 9 §24 C3): its span starts on the marker. Everywhere
    /// else - the document, a block quote, a list item - the run between the
    /// container's content column and the marker is the indentation that PLACES
    /// the marker, and PART 12 section 4 puts it INSIDE the span. Anchoring every
    /// list at its marker diverges from carve-js (markup-carve/carve#1797).
    /// This flag keeps the marker anchor to the two bodies that earn it
    /// (markup-carve/carve#1980, converging with carve-js's
    /// `sublistsCarryAuthoredBase`).
    pub(super) sublists_carry_authored_base: bool,
}

impl MappedSource {
    /// Start a mapped line with its stripped codepoint count.
    pub(super) fn new_line_at(
        line: String,
        source_line: Option<usize>,
        stripped: Option<isize>,
    ) -> Self {
        MappedSource {
            source: line,
            line_map: source_line.into_iter().map(Some).collect(),
            col_map: vec![stripped],
            authored_base_at_start: false,
            item_marker_at_start: false,
            reached: vec![false],
            sublists_carry_authored_base: false,
        }
    }

    /// Lines in `source`, counted the way `rebase_overindented_blocks` counts
    /// them so `reached` can be kept exactly as long.
    fn line_count(&self) -> usize {
        if self.source.is_empty() {
            0
        } else {
            self.source.matches('\n').count() + 1
        }
    }

    fn pad_reached(&mut self) {
        let lines = self.line_count();
        self.reached.resize(lines, false);
    }

    /// Append a line, recording how many codepoints were stripped from its
    /// front by the enclosing container.
    pub(super) fn push_newline_at(
        &mut self,
        line: String,
        source_line: Option<usize>,
        stripped: Option<isize>,
    ) {
        if !self.source.is_empty() {
            self.source.push('\n');
        }
        self.source.push_str(&line);
        let lines = self.line_count();
        pad_line_map(&mut self.line_map, lines, source_line);
        if source_line.is_some() || !self.line_map.is_empty() {
            self.line_map.push(source_line);
        }
        self.col_map.push(stripped);
        self.pad_reached();
    }

    pub(super) fn append(&mut self, other: MappedSource) {
        if other.source.is_empty() {
            return;
        }
        let mut other_reached = other.reached;
        other_reached.resize(other.source.matches('\n').count() + 1, false);
        self.pad_reached();
        if !self.source.is_empty() {
            self.source.push('\n');
        } else {
            self.item_marker_at_start = other.item_marker_at_start;
        }
        self.source.push_str(&other.source);
        self.line_map.extend(other.line_map);
        self.col_map.extend(other.col_map);
        self.reached.extend(other_reached);
    }
}

pub(super) fn compose_mapped_source(
    mut child: MappedSource,
    parent: &MappedSource,
) -> MappedSource {
    for child_index in 0..child.line_map.len() {
        let Some(local_line) = child.line_map[child_index] else {
            continue;
        };
        let parent_index = local_line.saturating_sub(1);
        child.line_map[child_index] = parent.line_map.get(parent_index).copied().flatten();
        if let Some(column) = child.col_map.get_mut(child_index) {
            *column = match (*column, parent.col_map.get(parent_index).copied().flatten()) {
                (Some(child_column), Some(parent_column)) => Some(child_column + parent_column),
                _ => None,
            };
        }
    }
    child
}

pub(super) fn first_mapped_line(source: &MappedSource) -> Option<usize> {
    source.line_map.iter().flatten().copied().min()
}

pub(super) fn map_pos_through_source(pos: &mut Pos, parent: &MappedSource) {
    let map_boundary = |line: usize, column: usize| {
        let index = line.saturating_sub(1);
        let mapped_line = parent.line_map.get(index).copied().flatten()?;
        let mapped_column = parent.col_map.get(index).copied().flatten()?;
        Some((
            mapped_line,
            (column as isize + mapped_column).max(1) as usize,
        ))
    };
    if let Some((line, column)) = map_boundary(pos.start_line, pos.start_column) {
        pos.start_line = line;
        pos.start_column = column;
    }
    if let Some((line, column)) = map_boundary(pos.end_line, pos.end_column) {
        pos.end_line = line;
        pos.end_column = column;
    }
}

pub(super) fn remap_source(source: String, original: &MappedSource) -> MappedSource {
    let source_line_count = source.lines().count();
    if source_line_count <= original.line_map.len() {
        return MappedSource {
            source,
            line_map: original.line_map[..source_line_count].to_vec(),
            col_map: original.col_map[..source_line_count.min(original.col_map.len())].to_vec(),
            authored_base_at_start: original.authored_base_at_start,
            item_marker_at_start: original.item_marker_at_start,
            reached: original.reached[..source_line_count.min(original.reached.len())].to_vec(),
            sublists_carry_authored_base: original.sublists_carry_authored_base,
        };
    }
    MappedSource {
        line_map: (1..=source_line_count).map(Some).collect(),
        // Top-level source: nothing has been stripped, so every column in this
        // text is a column in the document.
        col_map: vec![Some(0); source_line_count],
        authored_base_at_start: false,
        item_marker_at_start: false,
        reached: Vec::new(),
        sublists_carry_authored_base: false,
        source,
    }
}
