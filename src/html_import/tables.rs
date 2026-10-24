use super::{list_table_of, trim_edge_whitespace, CellAlignment, Importer};
use super::{HtmlImportDiagnosticCode, HtmlImportError, HtmlImportSeverity};
use crate::ast::*;
use markup5ever_rcdom::Handle;
use std::collections::{BTreeMap, BTreeSet};
/// HTML's own maxima for the two span attributes, and the largest value read as
/// a number at all before it defaults.
const MAX_COLSPAN: usize = 1000;
const MAX_ROWSPAN: usize = 65534;
const MAX_SAFE_SPAN: u64 = 9_007_199_254_740_991;

/// One imported cell, with the two spans resolved but not yet laid out.
struct BuiltCell {
    cell: TableCell,
    colspan: usize,
    rowspan: usize,
}

/// A table's `<thead>` / `<tbody>` / `<tfoot>`, collected on the way THROUGH it
/// rather than read back off its rows.
///
/// The two vectors are parallel and a row's section id indexes both. Deriving
/// the list from the rows was the shape that missed a section with no rows -
/// `<tbody id="empty"></tbody>` is one of the table's sections and its
/// attributes are as lost as any other's, but no row points at it.
struct Sections {
    /// The section's tag name, which is what says whether it has a slot at all.
    tags: Vec<String>,
    /// The section's own attributes and the path to report them at. Taken by
    /// `row_groups` when it places them on a body group, so what is left is
    /// exactly what nothing holds.
    attrs: Vec<Option<(Attrs, String)>>,
}

impl<'a> Importer<'a> {
    /// A `colspan` / `rowspan` value, by HTML's rules: a non-negative integer,
    /// clamped to the attribute's own maximum, defaulting when it is not one.
    ///
    /// The clamp is not decoration. Each unit of a span becomes a CELL below, so
    /// an unclamped `colspan="1000000000"` is a 30-byte input asking for a
    /// billion of them; the generated cells are charged to `max_nodes` on top of
    /// this, so the two together bound what a table can cost.
    fn span_count(cell: &Handle, name: &str, max: usize, min: usize) -> usize {
        let Some(raw) = Self::attr(cell, name) else {
            return 1;
        };
        let raw = raw.trim();
        if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
            return 1;
        }
        // The same cutoff carve-js reaches through `Number.isSafeInteger`, named
        // rather than inherited, so a value past it defaults on every engine
        // instead of clamping on one and defaulting on another.
        match raw.parse::<u64>() {
            // Clamped in `u64` and converted after, so a value above `usize` on
            // a 32-bit target reaches the maximum rather than truncating to
            // something under it.
            Ok(value) if value <= MAX_SAFE_SPAN => value.clamp(min as u64, max as u64) as usize,
            _ => 1,
        }
    }

    /// Drop a body cell's alignment where the HEAD already says it.
    ///
    /// A header cell's marker run is the COLUMN's default: the renderer reads
    /// it off the leading header rows and every cell below inherits what it
    /// does not state. So a body cell repeating its column's value spells a
    /// thing the document already says, and the shortest source that renders
    /// the same table is the one without it - which is also the source a round
    /// trip has to come back to, or `|= h |` over `| a |` grows a marker on
    /// every body row on its first pass through HTML.
    ///
    /// ONLY WHERE THE VALUE AGREES, and only per axis. A body cell that differs
    /// from its column keeps its own run, because that is the only thing that
    /// overrides the default - and a cell agreeing on the horizontal while
    /// stating its own vertical keeps the vertical alone, which is what `?`
    /// exists to spell.
    ///
    /// THE COLUMN WALK IS [`Self::span_grid`]'s, because the renderer resolves
    /// a column by POSITION IN THE ROW'S CELL ARRAY and that array is what
    /// `span_grid` lays out: a carried rowspan mark occupies an index, so the
    /// cells after it shift right by one, and a walk that ignored the marks
    /// would compare a body cell against a column it does not sit in. The mark
    /// bookkeeping is therefore copied rather than re-derived - the marks a row
    /// OPENS do not age at the end of that same row, which is the off-by-one
    /// that would let the row under a rowspan read the wrong column.
    ///
    /// A HEADER CELL SEEDS ITS OWN INDEX ONLY, not the columns a colspan
    /// carries it across, because that is what THIS engine's renderer reads:
    /// the continuation cell at the next index states no alignment and
    /// `table_column_defaults` finds none there. Seeding the span would drop a
    /// body alignment the re-render could not put back, which is the loss this
    /// whole ruling is against.
    fn drop_inherited_cell_alignment(built: &mut [Vec<BuiltCell>], leading_header_rows: usize) {
        if leading_header_rows == 0 {
            return;
        }
        let mut columns: Vec<CellAlignment> = Vec::new();
        let mut carried: BTreeMap<usize, usize> = BTreeMap::new();
        for (r, row) in built.iter_mut().enumerate() {
            let mut opened: Vec<(usize, usize)> = Vec::new();
            let mut column = 0usize;
            for entry in row.iter_mut() {
                // Every index this cell occupies, skipping the ones a rowspan
                // above already holds - the same walk `span_grid` does when it
                // places the continuation marks.
                let mut covered: Vec<usize> = Vec::new();
                for _ in 0..entry.colspan.max(1) {
                    while carried.contains_key(&column) {
                        column += 1;
                    }
                    covered.push(column);
                    column += 1;
                }
                let Some(&own) = covered.first() else {
                    continue;
                };
                if r < leading_header_rows {
                    if columns.len() <= own {
                        columns.resize(own + 1, CellAlignment::default());
                    }
                    // The LAST header row to state an axis wins, which is how
                    // `table_column_defaults` folds several of them.
                    if entry.cell.align.is_some() {
                        columns[own].align = entry.cell.align;
                    }
                    if entry.cell.valign.is_some() {
                        columns[own].valign = entry.cell.valign;
                    }
                } else {
                    let slot = columns.get(own).copied().unwrap_or_default();
                    if entry.cell.align.is_some() && slot.align == entry.cell.align {
                        entry.cell.align = None;
                    }
                    if entry.cell.valign.is_some() && slot.valign == entry.cell.valign {
                        entry.cell.valign = None;
                    }
                }
                if entry.rowspan > 1 {
                    opened.extend(covered.into_iter().map(|index| (index, entry.rowspan - 1)));
                }
            }
            carried = carried
                .into_iter()
                .filter_map(|(index, left)| (left > 1).then_some((index, left - 1)))
                .chain(opened)
                .collect();
        }
    }

    /// The imported cells, laid out with the continuation cells Carve spells `^`
    /// (this cell continues the one above) and `<` (it continues the one to its
    /// left).
    ///
    /// The model already carried both - `TableCell::span` is in PART 12 and the
    /// HTML renderer derives `rowspan` / `colspan` from a run of them - and the
    /// import simply threw them away: a spanning cell was written as an ordinary
    /// one and the row came up short, so `<td colspan="2">` produced a 1-cell
    /// row under a 2-column header, with `table-degraded` as the only trace.
    ///
    /// The renderer resolves a continuation by POSITION IN THE ROW'S CELL ARRAY,
    /// not by grid column - `^` attaches to the nearest row above whose cell at
    /// the same index is not itself a continuation - so a carried span occupies
    /// ONE array slot however many columns it covers, and the cells after it in
    /// the row shift left by the rest. Placing the carried marks first and
    /// filling the row's own cells around them is what keeps those indexes
    /// aligned.
    fn span_grid(
        &mut self,
        trs: &[(Handle, Option<usize>)],
        built: Vec<Vec<BuiltCell>>,
        row_attrs: &[Option<Attrs>],
        path: &str,
        depth: usize,
    ) -> Result<Vec<TableRow>, HtmlImportError> {
        fn continuation(span: TableCellSpan, header: bool) -> TableCell {
            TableCell {
                colspan: None,
                rowspan: None,
                header,
                span: Some(span),
                align: None,
                valign: None,
                attrs: None,
                children: Vec::new(),
                blocks: None,
                pos: None,
            }
        }
        let mut carried: Vec<(usize, usize)> = Vec::new();
        let mut rows = Vec::with_capacity(built.len());
        for (r, source_cells) in built.into_iter().enumerate() {
            let marks: BTreeSet<usize> = carried.iter().map(|&(index, _)| index).collect();
            let mut cells: Vec<TableCell> = Vec::new();
            let mut opened: Vec<(usize, usize)> = Vec::new();
            for BuiltCell {
                cell,
                colspan,
                rowspan,
            } in source_cells
            {
                while marks.contains(&cells.len()) {
                    self.enter(depth)?;
                    cells.push(continuation(TableCellSpan::Rowspan, false));
                }
                // Every slot the cell occupies in THIS row, so a rowspan can
                // carry a mark into each of them.
                let mut covered = vec![cells.len()];
                let header = cell.header;
                cells.push(cell);
                for _ in 1..colspan {
                    while marks.contains(&cells.len()) {
                        self.enter(depth)?;
                        cells.push(continuation(TableCellSpan::Rowspan, false));
                    }
                    covered.push(cells.len());
                    self.enter(depth)?;
                    cells.push(continuation(TableCellSpan::Colspan, header));
                }
                // A cell spanning BOTH ways carries a mark for each column it
                // covers, not one for its origin. The renderer resolves a `^`
                // against the cell at the SAME INDEX above it, so a single mark
                // left the next rowspan in the row resolving against a column it
                // does not own: `<td colspan="2" rowspan="2">A</td>` beside
                // `<td rowspan="2">B</td>` wrote `| ^ |  | ^ |` over them, which
                // renders a cell the table does not have and reports inventing
                // it.
                if rowspan > 1 {
                    opened.extend(covered.into_iter().map(|index| (index, rowspan - 1)));
                }
            }
            // A mark past the end of this row's own cells. Placing it still costs
            // nothing - it is a cell the span already owns - but a GAP before it
            // does: the index has to be kept, and an empty cell there is one the
            // source did not have. Only that invention is reported.
            let furthest = marks.iter().next_back().copied();
            let mut invented = false;
            if let Some(furthest) = furthest {
                while cells.len() <= furthest {
                    self.enter(depth)?;
                    if marks.contains(&cells.len()) {
                        cells.push(continuation(TableCellSpan::Rowspan, false));
                    } else {
                        cells.push(TableCell {
                            colspan: None,
                            rowspan: None,
                            header: false,
                            span: None,
                            align: None,
                            valign: None,
                            attrs: None,
                            children: Vec::new(),
                            blocks: None,
                            pos: None,
                        });
                        invented = true;
                    }
                }
            }
            if invented {
                self.diag(
                    HtmlImportDiagnosticCode::TableDegraded,
                    "Filled a row that is shorter than the spans reaching into it, with a cell the source did not have".into(),
                    HtmlImportSeverity::Warning,
                    &format!("{path}/tr[{}]", r + 1),
                    &trs[r].0,
                );
            }
            rows.push(TableRow {
                cells,
                // The `<tr>`'s own attributes, which `TableRow::attrs` has a
                // slot for and the writer spells on the closing pipe.
                attrs: row_attrs.get(r).cloned().flatten(),
                continuation_lines: None,
                pos: None,
            });
            carried = carried
                .into_iter()
                .filter_map(|(index, left)| (left > 1).then_some((index, left - 1)))
                .chain(opened)
                .collect();
        }
        Ok(rows)
    }

    /// The `<colgroup>` children of a table, which carry COLUMN structure Carve
    /// has nowhere to put.
    ///
    /// Only `<colgroup>` is looked for, and a `<col>` is not. Every `<col>` is
    /// inside one after parsing - "in table" insertion mode answers a `col`
    /// start tag by inserting an implied `<colgroup>` first, so a run of bare
    /// `<col>`s arrives as one wrapper holding all of them - and reporting the
    /// wrapper covers its children the way one report covers a dropped subtree
    /// everywhere else. A `| "col"` arm here matched nothing on any input, which
    /// is the check that cannot fail (carve#755).
    fn column_groups(h: &Handle, path: &str) -> Vec<(Handle, String)> {
        h.children
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, c)| Self::tag(c).as_deref() == Some("colgroup"))
            .map(|(i, c)| (c.clone(), format!("{path}/colgroup[{}]", i + 1)))
            .collect()
    }

    pub(super) fn table(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
    ) -> Result<BlockNode, HtmlImportError> {
        // Each row remembers the `<thead>` / `<tbody>` / `<tfoot>` it is in, by
        // an id minted when the walk enters one. A rowspan stops at its ROW
        // GROUP in HTML, so the group is not bookkeeping here: it is what says
        // how far down a span reaches.
        //
        // The sections themselves are collected here too, on the way through,
        // rather than read back off the rows afterwards: a section with NO rows
        // is one of the table's sections as well, and deriving the list from the
        // rows leaves a `<tbody id="empty"></tbody>` unread and its attributes
        // unreported.
        fn rows(
            h: &Handle,
            path: &str,
            section: Option<usize>,
            section_tags: &mut Vec<String>,
            section_nodes: &mut Vec<(Handle, String)>,
            out: &mut Vec<(Handle, Option<usize>)>,
        ) {
            if Importer::tag(h).as_deref() == Some("tr") {
                out.push((h.clone(), section));
                return;
            }
            let own = match Importer::tag(h).as_deref() {
                Some(tag @ ("thead" | "tbody" | "tfoot")) => {
                    section_tags.push(tag.to_owned());
                    section_nodes.push((h.clone(), path.to_owned()));
                    Some(section_tags.len() - 1)
                }
                _ => section,
            };
            for (i, c) in h.children.borrow().iter().enumerate() {
                let tag = Importer::tag(c).unwrap_or_default();
                rows(
                    c,
                    &format!("{path}/{tag}[{}]", i + 1),
                    own,
                    section_tags,
                    section_nodes,
                    out,
                );
            }
        }
        // `<caption>` is a DIRECT child of the table and carries the table's own
        // caption, which `Table::caption` has a slot for and Carve spells `^ text`
        // after the rows. The row walk below looks only for `tr`, so before this
        // the element was skipped and the caption left the document silently -
        // pandoc emits exactly this shape for every captioned table.
        let captions: Vec<(usize, Handle)> = h
            .children
            .borrow()
            .iter()
            .enumerate()
            .filter(|(_, c)| Importer::tag(c).as_deref() == Some("caption"))
            .map(|(i, c)| (i, c.clone()))
            .collect();
        // The PARSER keeps the first `^ ` line and reads the second as a
        // paragraph, so a table that arrives with two captions loses one either
        // way. Reported rather than dropped in silence, and by the same rule the
        // parser uses, so the import and a re-read of its own output agree on
        // which one survives.
        for (i, c) in captions.iter().skip(1) {
            self.diag(
                HtmlImportDiagnosticCode::TableDegraded,
                "Dropped a second <caption>: a table has one caption, and the first one wins"
                    .into(),
                HtmlImportSeverity::Warning,
                &format!("{path}/caption[{}]", i + 1),
                c,
            );
        }
        // THE ELEMENT, not its children (carve-rs#1257). Reading `children` off
        // it here meant the `<caption>` was the only caption slot in this
        // importer whose own attributes were never looked at, so an `onclick`
        // on it was stripped with nothing said - the failure mode the report
        // exists to prevent, and the one both other engines already reported.
        // `caption_inlines` is where the answer already lived.
        let caption_node: Option<(usize, Handle)> = captions.first().map(|(i, c)| (*i, c.clone()));
        let mut trs = Vec::new();
        let mut section_tags: Vec<String> = Vec::new();
        let mut section_nodes: Vec<(Handle, String)> = Vec::new();
        rows(
            h,
            path,
            None,
            &mut section_tags,
            &mut section_nodes,
            &mut trs,
        );
        // The attributes of the SECTIONS, read once and in document order,
        // parallel to `section_tags` so a row's section id indexes both. Only a
        // `<tbody>` has a slot for them - the body group `row_groups` states -
        // so `row_groups` TAKES what it places out of this and whatever is left
        // is reported below. Nothing read them before: a `<tbody id="totals">`
        // fell into the empty `attrs` slot with no diagnostic at all
        // (carve#1210).
        let mut sections = Sections {
            tags: section_tags,
            attrs: Vec::with_capacity(section_nodes.len()),
        };
        for (node, section_path) in &section_nodes {
            let own = self
                .attrs(node, section_path)
                .map(|a| (a, section_path.clone()));
            sections.attrs.push(own);
        }
        // A `<colgroup>` and its `<col>` children state COLUMN structure, and
        // Carve has no column model to put it in - not a narrower slot, none at
        // all. Dropped as it always was, said out loud now, because a loss the
        // reader is never told about is the one they cannot work around.
        for (cg, p) in Self::column_groups(h, path) {
            self.diag(
                HtmlImportDiagnosticCode::ElementDropped,
                "Dropped <colgroup>: Carve has no column model, and a table's columns are only the cells its rows carry".into(),
                HtmlImportSeverity::Warning,
                &p,
                &cg,
            );
        }
        let source_cells = |tr: &Handle| -> Vec<Handle> {
            tr.children
                .borrow()
                .iter()
                .filter(|n| matches!(Self::tag(n).as_deref(), Some("td" | "th")))
                .cloned()
                .collect()
        };
        // PART 10 SST9 gives every `th` a `scope` from its POSITION: `col` in
        // the leading run of all-header rows, `row` for a header cell below it.
        // Read off the SOURCE rows, because the run ENDS at the first row
        // carrying a `td` and the built rows carry continuation cells that are
        // neither.
        let leading_header_rows = trs
            .iter()
            .take_while(|(tr, _)| {
                let cells = source_cells(tr);
                !cells.is_empty() && cells.iter().all(|n| Self::tag(n).as_deref() == Some("th"))
            })
            .count();
        // How many rows are left in each row's own group, INCLUDING it. Computed
        // once: asking per cell means scanning the whole table for each one,
        // which is quadratic in the row count.
        let mut totals: BTreeMap<Option<usize>, usize> = BTreeMap::new();
        for (_, section) in &trs {
            *totals.entry(*section).or_insert(0) += 1;
        }
        let mut seen: BTreeMap<Option<usize>, usize> = BTreeMap::new();
        let remaining_in_group: Vec<usize> = trs
            .iter()
            .map(|(_, section)| {
                let index = seen.entry(*section).or_insert(0);
                let left = totals.get(section).copied().unwrap_or(1) - *index;
                *index += 1;
                left
            })
            .collect();
        let list_form = self.opts.list_table_for_block_cells
            && trs
                .iter()
                .any(|(tr, _)| source_cells(tr).iter().any(Self::holds_blocks));
        let mut own_alignment: Vec<Vec<(Option<TableAlign>, Option<TableVerticalAlign>)>> =
            Vec::new();
        let mut built: Vec<Vec<BuiltCell>> = Vec::with_capacity(trs.len());
        for (r, (tr, _)) in trs.iter().enumerate() {
            let mut row = Vec::new();
            for (c, cell) in source_cells(tr).iter().enumerate() {
                let p = format!(
                    "{path}/tr[{}]/{}[{}]",
                    r + 1,
                    Self::tag(cell).unwrap(),
                    c + 1
                );
                let colspan = Self::span_count(cell, "colspan", MAX_COLSPAN, 1);
                // A rowspan stops at its ROW GROUP in HTML, and `rowspan="0"`
                // means exactly "to the end of it". Both are resolved against the
                // group the row is actually in, so a `<tfoot>` below the body is
                // not swallowed by a cell the layout stops at the body's last
                // row.
                let declared = Self::span_count(cell, "rowspan", MAX_ROWSPAN, 0);
                let left = remaining_in_group[r];
                let mut rowspan = if declared == 0 {
                    left
                } else {
                    declared.min(left)
                };
                // And it stops at the head the RENDERER will synthesize. Carve
                // derives the head from the leading run of all-header rows, so a
                // span reaching out of that run lands in a `<thead>` with its
                // other rows in the `<tbody>` - which browsers clip, making the
                // written table say something the source table did not. Clipped
                // here instead, where it can be reported: the alternative is a
                // document that claims a grid it does not render.
                if r < leading_header_rows && r + rowspan > leading_header_rows {
                    self.diag(
                        HtmlImportDiagnosticCode::TableDegraded,
                        "Clipped a rowspan at the header rows: Carve derives the head from the leading header rows, and a span leaving them crosses a boundary browsers clip anyway".into(),
                        HtmlImportSeverity::Warning,
                        &p,
                        cell,
                    );
                    rowspan = leading_header_rows - r;
                }
                // THE MARKER RUN IS THE CELL'S OWN FIELD, so a mapped
                // `text-align` / `vertical-align` lands here rather than in the
                // attribute block `attrs` builds (markup-carve/carve#1745,
                // markup-carve/carve#1746).
                let mut alignment = self.cell_style_alignment(cell);
                let cell_attrs = self.attrs(cell, &p);
                let (children, blocks) = if list_form {
                    (
                        Vec::new(),
                        Some(self.blocks(&cell.children.borrow(), &p, depth + 1)?),
                    )
                } else {
                    self.cell_depth += 1;
                    let children = self.inlines(&cell.children.borrow(), &p, depth + 1);
                    self.cell_depth -= 1;
                    // A cell's edges are block edges: `fmt` drops their space.
                    (trim_edge_whitespace(children?), None)
                };
                let header = Self::tag(cell).as_deref() == Some("th");
                let empty = children.is_empty() && blocks.as_ref().is_none_or(Vec::is_empty);
                if self.writing && !list_form && empty && header {
                    if alignment.align == Some(TableAlign::Left)
                        || alignment.valign == Some(TableVerticalAlign::Top)
                    {
                        self.diag(
                            HtmlImportDiagnosticCode::StyleUnmapped,
                            "Dropped alignment on an empty header cell: without a delimiter row, Carve source cannot spell its column default".into(),
                            HtmlImportSeverity::Warning,
                            &p,
                            cell,
                        );
                        if alignment.align == Some(TableAlign::Left) {
                            alignment.align = None;
                        }
                        if alignment.valign == Some(TableVerticalAlign::Top) {
                            alignment.valign = None;
                        }
                    }
                }
                row.push(BuiltCell {
                    cell: TableCell {
                        colspan: None,
                        rowspan: None,
                        header,
                        span: None,
                        align: alignment.align,
                        valign: alignment.valign,
                        attrs: cell_attrs,
                        children,
                        blocks,
                        pos: None,
                    },
                    colspan,
                    rowspan,
                });
            }
            if list_form {
                own_alignment.push(row.iter().map(|b| (b.cell.align, b.cell.valign)).collect());
            }
            built.push(row);
        }
        self.table_from_cells(
            h,
            path,
            depth,
            attrs,
            trs,
            sections,
            section_nodes,
            source_cells,
            caption_node,
            list_form,
            leading_header_rows,
            own_alignment,
            built,
        )
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn table_from_cells(
        &mut self,
        h: &Handle,
        path: &str,
        depth: usize,
        attrs: Option<Attrs>,
        trs: Vec<(Handle, Option<usize>)>,
        mut sections: Sections,
        section_nodes: Vec<(Handle, String)>,
        source_cells: fn(&Handle) -> Vec<Handle>,
        caption_node: Option<(usize, Handle)>,
        list_form: bool,
        leading_header_rows: usize,
        own_alignment: Vec<Vec<(Option<TableAlign>, Option<TableVerticalAlign>)>>,
        mut built: Vec<Vec<BuiltCell>>,
    ) -> Result<BlockNode, HtmlImportError> {
        Self::drop_inherited_cell_alignment(&mut built, leading_header_rows);
        // A `<tr>`'s own attributes have a slot - `TableRow::attrs`, which the
        // writer spells on the closing pipe and every renderer emits on the
        // `<tr>` - and went in silence before this. Reading them at all also
        // puts a `<tr>` on the ordinary attribute path, so an unsupported one
        // reports the way it does on any other element.
        let row_attrs: Vec<Option<Attrs>> = (0..trs.len())
            .map(|r| {
                let p = format!("{path}/tr[{}]", r + 1);
                let own = self.attrs(&trs[r].0, &p);
                // A list-table row is an outer item, and no renderer reads
                // attributes there.
                match own {
                    Some(own) if list_form => {
                        self.diag(
                            HtmlImportDiagnosticCode::AttributeDropped,
                            format!(
                                "Dropped {} on <tr>: a list table row has no attribute slot",
                                Self::attr_names(&own).join(", ")
                            ),
                            HtmlImportSeverity::Info,
                            &p,
                            &trs[r].0,
                        );
                        None
                    }
                    own => own,
                }
            })
            .collect();
        let mut result = self.span_grid(&trs, built, &row_attrs, path, depth)?;

        // A `scope` equal to the positional default carries no information the
        // renderer cannot reproduce, and importing it would write this engine's
        // own output back as if the author had written it. A value the default
        // cannot explain - `colgroup`, `rowgroup`, which have no marker
        // spelling - is the only way to get it, so it stays (carve-rs#944).
        for (r, row) in result.iter_mut().enumerate() {
            for cell in row.cells.iter_mut() {
                if !cell.header {
                    continue;
                }
                let default = if r < leading_header_rows
                    && trs[r]
                        .1
                        .and_then(|id| sections.tags.get(id))
                        .map(String::as_str)
                        != Some("tfoot")
                {
                    "col"
                } else {
                    "row"
                };
                let Some(attrs) = cell.attrs.as_mut() else {
                    continue;
                };
                if attrs.key_values.get("scope").map(String::as_str) == Some(default) {
                    attrs.key_values.remove("scope");
                }
            }
        }
        let header_rows: Vec<bool> = trs
            .iter()
            .map(|(tr, _)| {
                let cells = source_cells(tr);
                !cells.is_empty() && cells.iter().all(|n| Self::tag(n).as_deref() == Some("th"))
            })
            .collect();
        let unspellable_before = self.unspellable.len();
        if self.writing && trs.is_empty() {
            self.diag(
                HtmlImportDiagnosticCode::TableDegraded,
                "Dropped a rowless table, including its attributes and caption: Carve source cannot spell a table without rows".into(),
                HtmlImportSeverity::Warning,
                path,
                h,
            );
        }
        let mut row_groups = self.row_groups(
            h,
            &trs,
            &header_rows,
            &result,
            leading_header_rows,
            path,
            &mut sections,
        );
        if list_form && row_groups.is_some() {
            self.unspellable.push((
                h.clone(),
                path.to_owned(),
                "ListTable cannot preserve this explicit row-group partition".into(),
                HtmlImportDiagnosticCode::StructureUnspellable,
            ));
        }
        // A list table has no slot for the grouping on either exit, so the AST
        // exit reports what only the writing exit reports for a pipe table.
        if list_form && !self.writing {
            for (node, lost_path, message, code) in self.unspellable.split_off(unspellable_before) {
                self.diag(
                    code,
                    message,
                    HtmlImportSeverity::Warning,
                    &lost_path,
                    &node,
                );
            }
        }
        // Report section attributes that could not be assigned to the partition.
        let sections_with_rows: BTreeSet<usize> = trs.iter().filter_map(|(_, s)| *s).collect();
        for (id, slot) in sections.attrs.iter().enumerate() {
            let Some((own, own_path)) = slot else {
                continue;
            };
            let tag = sections.tags.get(id).map(String::as_str).unwrap_or("tbody");
            // Unclaimed section attributes have no slot in the retained partition.
            let reason = match tag {
                "thead" => "the head cannot be represented in the retained row partition",
                "tfoot" => "the foot cannot be represented in the retained row partition",
                _ if sections_with_rows.contains(&id) => {
                    "the row grouping this body belongs to was not kept, and nothing else holds it"
                }
                _ => "the empty body could not be retained in the row partition",
            };
            self.diag(
                HtmlImportDiagnosticCode::AttributeDropped,
                format!(
                    "Dropped {} on <{tag}>: {reason}",
                    Self::attr_names(own).join(", ")
                ),
                HtmlImportSeverity::Warning,
                own_path,
                &section_nodes[id].0,
            );
        }
        if self.writing && !list_form {
            if let Some(groups) = &mut row_groups {
                let mut cursor = 0;
                let mut keep_count = |count: &mut usize| {
                    let end = cursor + *count;
                    *count = result[cursor..end]
                        .iter()
                        .filter(|row| !Self::blank_table_row(row))
                        .count();
                    cursor = end;
                };
                keep_count(&mut groups.head_rows);
                for body in &mut groups.bodies {
                    keep_count(&mut body.head_rows);
                    keep_count(&mut body.body_rows);
                }
                keep_count(&mut groups.foot_rows);
            }
        }
        let last_dropped = if self.writing && !list_form {
            self.drop_blank_rows(&trs, &mut result, path)
        } else {
            None
        };
        if !list_form {
            if let Some(groups) = &row_groups {
                let mut source_attrs = attrs.clone().unwrap_or_default();
                crate::table_source_metadata::add_row_groups(&mut source_attrs, groups);
                let mut core = groups.clone();
                core.head_attrs = None;
                core.foot_attrs = None;
                for body in &mut core.bodies {
                    body.attrs = None;
                }
                if crate::table_source_metadata::row_groups(&source_attrs, result.len()).as_ref()
                    != Some(&core)
                {
                    self.unspellable.push((h.clone(), path.to_owned(), "The retained table attributes cannot preserve its explicit row-group partition".into(), HtmlImportDiagnosticCode::StructureUnspellable));
                }
            }
        }
        let mut caption = match caption_node {
            Some((index, node)) => Some(self.caption_inlines(
                &node,
                &format!("{path}/caption[{}]", index + 1),
                depth,
                "caption",
            )?),
            None => None,
        };
        if let Some(r) = last_dropped.filter(|_| result.is_empty()) {
            // A lone `^ c` line reads as a paragraph, so the caption goes too.
            if caption.take().is_some() {
                self.diag(
                    HtmlImportDiagnosticCode::ElementDropped,
                    "Dropped a caption whose table has no row left".into(),
                    HtmlImportSeverity::Warning,
                    &format!("{path}/tr[{}]", r + 1),
                    &trs[r].0,
                );
            }
        }
        if self.writing && trs.is_empty() {
            self.unspellable.truncate(unspellable_before);
        } else if self.writing && result.is_empty() {
            if let Some(own) = &attrs {
                let names = Self::attr_names(own);
                if !names.is_empty() {
                    self.diag(
                        HtmlImportDiagnosticCode::AttributeDropped,
                        format!(
                            "Dropped {} on <table>: no row survives source conversion",
                            names.join(", ")
                        ),
                        HtmlImportSeverity::Warning,
                        path,
                        h,
                    );
                }
            }
        }
        if list_form {
            // A list-table row is the list of its cells, so a row with none has
            // no spelling.
            let mut rows = Vec::with_capacity(result.len());
            let mut alignment = Vec::with_capacity(result.len());
            for (r, (row, own)) in result.into_iter().zip(own_alignment).enumerate() {
                if row.cells.is_empty() {
                    self.diag(
                        HtmlImportDiagnosticCode::StructureUnspellable,
                        "Dropped a row with no cells: a list table row is the list of its cells"
                            .into(),
                        HtmlImportSeverity::Warning,
                        &format!("{path}/tr[{}]", r + 1),
                        &trs[r].0,
                    );
                    continue;
                }
                rows.push(row);
                alignment.push(own);
            }
            return Ok(BlockNode::Admonition(list_table_of(
                rows,
                &alignment,
                caption,
                attrs,
                self.writing,
            )));
        }
        Ok(BlockNode::Table(Table {
            source_format: None,
            attrs,
            caption,
            short_caption: None,
            columns: Vec::new(),
            rows: result,
            row_groups: row_groups.map(Box::new),
            separator_widths: Self::reads_separator_widths(h),
            pos: None,
        }))
    }

    /// A cell that holds blocks a pipe-table cell would flatten: a list, a code
    /// block, a quotation, a table, a definition list, or a second paragraph.
    fn holds_blocks(cell: &Handle) -> bool {
        // Iterative: the scan runs before the walk that enforces the depth limit.
        let mut paragraphs = 0;
        let mut stack: Vec<Handle> = cell.children.borrow().iter().cloned().collect();
        while let Some(node) = stack.pop() {
            let Some(tag) = Self::tag(&node) else {
                continue;
            };
            match tag.as_str() {
                "ul" | "ol" | "pre" | "blockquote" | "table" | "dl" => return true,
                "p" => {
                    paragraphs += 1;
                    if paragraphs > 1 {
                        return true;
                    }
                }
                _ => {}
            }
            stack.extend(node.children.borrow().iter().cloned());
        }
        false
    }

    /// Drop every row with no cells, which Carve cannot spell. A row of empty
    /// cells is a row (CARVE-P9-082) and is kept. Returns the source index of
    /// the last row dropped. Writing exit only: the tree renders such a row, and
    /// only the writer has no spelling for it (carve-rs#1735). The writing exit
    /// adjusts group counts before removing the rows.
    fn blank_table_row(row: &TableRow) -> bool {
        row.cells.is_empty()
    }

    fn drop_blank_rows(
        &mut self,
        trs: &[(Handle, Option<usize>)],
        rows: &mut Vec<TableRow>,
        path: &str,
    ) -> Option<usize> {
        let mut last = None;
        let mut source_index = 0;
        rows.retain(|row| {
            let r = source_index;
            source_index += 1;
            let Some((tr, _)) = trs.get(r) else {
                return true;
            };
            if !Self::blank_table_row(row) {
                return true;
            }
            self.diag(
                HtmlImportDiagnosticCode::StructureUnspellable,
                "Dropped a row with no cells: Carve has no spelling for it".into(),
                HtmlImportSeverity::Warning,
                &format!("{path}/tr[{}]", r + 1),
                tr,
            );
            last = Some(r);
            false
        });
        last
    }

    /// `<thead>` / `<tbody>` / `<tfoot>` to `Table::row_groups`, when the
    /// partition says something a reader cannot derive (carve#1210 D1, ruled as
    /// (b); pandoc-carve#61 implements the same rule and carve-js followed).
    // The table element joins the six it already took, so that the one
    // diagnostic in here can report at the element it is about rather than at
    // whatever the walk was holding.
    #[allow(clippy::too_many_arguments)]
    fn row_groups(
        &mut self,
        node: &Handle,
        trs: &[(Handle, Option<usize>)],
        header_rows: &[bool],
        rows: &[TableRow],
        leading_header_rows: usize,
        path: &str,
        sections: &mut Sections,
    ) -> Option<TableRowGroups> {
        let section_of = |index: usize| -> &str {
            trs[index]
                .1
                .and_then(|id| sections.tags.get(id))
                .map(String::as_str)
                .unwrap_or("tbody")
        };
        // The head is a PREFIX of `rows` and the foot a SUFFIX, which is what
        // the field can express. A `<thead>` that is not first, or a `<tfoot>`
        // with rows after it, is a table this cannot describe.
        let head_rows = (0..trs.len())
            .find(|&i| section_of(i) != "thead")
            .unwrap_or(trs.len());
        let mut foot_rows = 0;
        while foot_rows < trs.len() - head_rows && section_of(trs.len() - 1 - foot_rows) == "tfoot"
        {
            foot_rows += 1;
        }
        let middle = head_rows..trs.len() - foot_rows;
        if middle
            .clone()
            .any(|i| matches!(section_of(i), "thead" | "tfoot"))
        {
            self.diag(
                HtmlImportDiagnosticCode::TableDegraded,
                "Dropped the row grouping of a table whose <thead> or <tfoot> is not at the edge of its rows: the head is a prefix of the rows and the foot a suffix".into(),
                HtmlImportSeverity::Warning,
                path,
                node,
            );
            return None;
        }

        let mut take_section_attrs = |tag: &str| {
            let matching: Vec<usize> = sections
                .tags
                .iter()
                .enumerate()
                .filter_map(|(i, name)| (name == tag).then_some(i))
                .collect();
            if matching.len() == 1 {
                sections.attrs[matching[0]].take().map(|(attrs, own_path)| {
                    self.unspellable.push((node.clone(), own_path, format!("Dropped rowGroups.{} because Carve source cannot spell section attributes", if tag == "thead" { "headAttrs" } else { "footAttrs" }), HtmlImportDiagnosticCode::StructureUnspellable));
                    attrs
                })
            } else {
                None
            }
        };
        let head_attrs = take_section_attrs("thead");
        let foot_attrs = take_section_attrs("tfoot");
        let body_paths: Vec<Option<String>> = sections
            .attrs
            .iter()
            .map(|slot| slot.as_ref().map(|(_, path)| path.clone()))
            .collect();
        let mut origins: Vec<bool> = Vec::new();
        let headers: Vec<Vec<bool>> = rows
            .iter()
            .map(|row| {
                origins.resize(row.cells.len(), false);
                row.cells
                    .iter()
                    .enumerate()
                    .map(|(column, cell)| {
                        let header = if cell.span == Some(TableCellSpan::Rowspan) {
                            origins[column]
                        } else {
                            cell.header
                        };
                        if cell.span.is_none() {
                            origins[column] = cell.header;
                        }
                        header
                    })
                    .collect()
            })
            .collect();
        let mut bodies: Vec<TableBodyGroup> = Vec::new();
        let mut body_sections = Vec::new();
        let mut index = middle.start;
        while index < middle.end {
            let section = trs[index].1;
            let start = index;
            while index < middle.end && trs[index].1 == section {
                index += 1;
            }
            let mut group_head = 0;
            while start + group_head < index && header_rows[start + group_head] {
                group_head += 1;
            }
            // A group whose rows are ALL header rows is an intermediate header
            // with nothing under it, which is what the counts say and not
            // something to reinterpret.
            let body_start = start + group_head;
            let row_head_columns = if body_start < index {
                Self::row_head_columns(rows, &headers, body_start..index)
            } else {
                0
            };
            // The `<tbody>`'s own attributes: the body group is where the
            // exchanged model puts them, and a body is the only section with a
            // slot. TAKEN here rather than after the `derivable` return below,
            // because a body carrying any is never derivable - that is what the
            // clause says - so the field is returned whenever one was taken, and
            // the only return that skips this point is the one before the loop.
            let own = section
                .and_then(|id| sections.attrs.get_mut(id))
                .and_then(Option::take)
                .map(|(a, _)| a);
            body_sections.push(section);
            bodies.push(TableBodyGroup {
                head_rows: group_head,
                body_rows: index - body_start,
                row_head_columns: (row_head_columns > 0).then_some(row_head_columns),
                attrs: own,
            });
        }

        let mut sections_with_rows = vec![false; sections.tags.len()];
        for (_, section) in trs {
            if let Some(id) = section {
                sections_with_rows[*id] = true;
            }
        }
        let mut empty_bodies = Vec::new();
        for (id, has_rows) in sections_with_rows.into_iter().enumerate() {
            if sections.tags[id] != "tbody" || has_rows {
                continue;
            }
            let attrs = sections.attrs[id].take().map(|(attrs, _)| attrs);
            empty_bodies.push((
                id,
                TableBodyGroup {
                    head_rows: 0,
                    body_rows: 0,
                    row_head_columns: None,
                    attrs,
                },
            ));
        }
        if !empty_bodies.is_empty() {
            let mut empty_bodies = empty_bodies.into_iter().peekable();
            let mut merged_bodies = Vec::new();
            let mut merged_sections = Vec::new();
            for (body, section) in bodies.into_iter().zip(body_sections) {
                if let Some(id) = section {
                    while empty_bodies.peek().is_some_and(|(empty, _)| *empty < id) {
                        let (empty, body) = empty_bodies.next().unwrap();
                        merged_bodies.push(body);
                        merged_sections.push(Some(empty));
                    }
                }
                merged_bodies.push(body);
                merged_sections.push(section);
            }
            for (empty, body) in empty_bodies {
                merged_bodies.push(body);
                merged_sections.push(Some(empty));
            }
            bodies = merged_bodies;
            body_sections = merged_sections;
        }

        // No `<thead>` at all: the leading run of header rows is what every
        // renderer reads as the head, so it is counted as one here too. Without
        // this, the ORDINARY table - a header row and some data rows, with only
        // the implicit `<tbody>` the HTML parser inserts - comes out with an
        // intermediate header and no head, which is a different statement about
        // the same table and puts the field on nearly every document.
        //
        // ONE body only. With a second one, the header-only first body is a
        // BOUNDARY the field exists to record, and absorbing it away leaves a
        // single ordinary body that the derivation reproduces.
        let mut head_rows = head_rows;
        if head_attrs.is_none()
            && head_rows == 0
            && bodies.len() == 1
            && leading_header_rows > 0
            && bodies[0].head_rows > 0
        {
            let absorbed = leading_header_rows.min(bodies[0].head_rows);
            head_rows = absorbed;
            bodies[0].head_rows -= absorbed;
            // A group carrying attributes is not empty, whatever its counts say:
            // dropping it here would drop them with it.
            if bodies[0].head_rows == 0
                && bodies[0].body_rows == 0
                && bodies[0].row_head_columns.is_none()
                && bodies[0].attrs.is_none()
            {
                bodies.clear();
            }
        }

        for (index, body) in bodies.iter().enumerate() {
            if body.attrs.is_some() {
                let own_path = body_sections[index]
                    .and_then(|id| body_paths[id].clone())
                    .unwrap_or_else(|| path.to_owned());
                self.unspellable.push((node.clone(), own_path, format!("Dropped rowGroups.bodies[{index}].attrs because Carve source cannot spell section attributes"), HtmlImportDiagnosticCode::StructureUnspellable));
            }
        }
        let derivable = head_attrs.is_none()
            && foot_attrs.is_none()
            && head_rows == leading_header_rows
            && foot_rows == 0
            && bodies.len() <= 1
            // A single body's row-head count is read off its cells' header
            // flags, so it states nothing the rows do not (PART 12 §15).
            && bodies.iter().all(|b| b.head_rows == 0 && b.body_rows > 0 && b.attrs.is_none());
        if derivable {
            return None;
        }
        Some(TableRowGroups {
            head_attrs,
            foot_attrs,
            head_rows,
            bodies,
            foot_rows,
        })
    }

    /// Leading COLUMNS that are header cells in every row of the group.
    ///
    /// Counted over the expanded grid rather than over the source cells, because
    /// columns and cells are not the same thing: `<th colspan="2">` is one
    /// element and two columns, and a `<th rowspan="2">` leaves the row below it
    /// starting with a data ELEMENT while a header still occupies the column.
    ///
    /// A `<` needs no resolution: `span_grid` builds a colspan continuation
    /// carrying its ORIGIN's header flag. A `^` is built with the flag cleared,
    /// because the cell it continues is in another row, so that one is resolved
    /// upward.
    ///
    /// One SLOT is one column here, which is what makes the count a simple walk.
    /// It is true because `span_grid` carries a mark into each column a spanning
    /// cell covers rather than one for its origin, so no `^` ever stands for
    /// more than one column. carve-js carries the single mark and needs the
    /// width of the origin to undo it; a width lookup here could not change an
    /// answer, so there is none.
    fn row_head_columns(
        rows: &[TableRow],
        headers: &[Vec<bool>],
        group: std::ops::Range<usize>,
    ) -> usize {
        let leading = |r: usize| -> usize {
            let cells = &rows[r].cells;
            let mut columns = 0;
            while columns < cells.len() && headers[r][columns] {
                columns += 1;
            }
            // An all-header row would say every column is a row head, which is
            // what an intermediate HEADER row is, not a row-head column.
            if columns == cells.len() {
                0
            } else {
                columns
            }
        };
        group.map(leading).min().unwrap_or(0)
    }
}
