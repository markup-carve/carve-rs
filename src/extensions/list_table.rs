//! Render `::: list-table` blocks as real HTML `<table>` markup, with the table
//! authored as a nested list so cells can hold full block content (paragraphs,
//! lists, code, ...) that the native pipe-table syntax cannot express.
//!
//! Port of the carve-php `ListTableExtension` (PR #195). carve-php keys on a
//! `Div` with class `list-table`; in carve-rs `::: list-table` parses as a
//! [`BlockNode::Admonition`] whose `kind` is `list-table` (the same shape the
//! `details` extension keys on), so this extension claims that admonition kind.
//!
//! Like `details`, this runs as a `before_render` transform: a renderable
//! `list-table` admonition is rewritten into a [`BlockNode::Extension`] carrier
//! whose `render_block_extension` builds the `<table>`. A `list-table` that
//! cannot be rendered as a table (no usable nested list, or a row that yields
//! no cells) is LEFT UNTOUCHED, so the core renderer emits the default
//! `<div class="list-table">` holding the literal nested list and no content is
//! ever silently dropped. The defer decision is made on the pristine AST before
//! any rewrite, so a deferred render is byte-identical to the plain admonition.

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{
    Admonition, AttrSlot, Attrs, BlockExtension, BlockNode, Document, InlineNode, ListItem,
};
use crate::extension::{CarveExtension, RenderContext};

/// The admonition kind this extension claims.
const KIND: &str = "list-table";

/// Sentinel name for the rewritten carrier node.
pub(crate) const CARRIER: &str = "carve-list-table";

/// Render `::: list-table` blocks as real HTML `<table>` markup.
///
/// The table is authored as an outer list where each outer item is a row and
/// each inner item is a cell; cells hold full block content. The caption comes
/// from the quoted title (`::: list-table "Cap"` -> `<caption>Cap</caption>`).
/// `{header-rows=N}` / `{header-cols=N}` block attributes on the PRECEDING line
/// promote rows to `<thead>`/`<th>` and the first N cells of each row to
/// row-header `<th>`. A cell whose sole content is a lone `^` merges with the
/// cell above (rowspan); a lone `<` merges with the cell to the left (colspan),
/// matching Carve's native pipe-table continuation markers. A cell carrying its
/// own attributes is never a span marker (its `^`/`<` is literal) and its
/// attributes carry onto the `<td>`/`<th>`.
///
/// ```
/// use carve::{ListTable, Options};
/// let ext = ListTable::new();
/// let opts = Options::new().with_extension(&ext);
/// let src = "::: list-table \"Cap\"\n- - A\n  - B\n:::";
/// assert_eq!(
///     carve::to_html_with_options(src, &opts),
///     "<table>\n  <caption>Cap</caption>\n  <tbody>\n    <tr><td>A</td><td>B</td></tr>\n  </tbody>\n</table>"
/// );
/// ```
#[derive(Debug, Default, Clone)]
pub struct ListTable;

impl ListTable {
    /// Create a list-table extension.
    pub fn new() -> Self {
        Self
    }
}

impl CarveExtension for ListTable {
    fn name(&self) -> &'static str {
        "list-table"
    }

    fn before_render(&self, mut doc: Document) -> Document {
        rewrite_blocks(&mut doc.children);
        // Footnote bodies live outside the tree but are still rendered, so a
        // list-table inside a footnote def must be rewritten too (mirrors the
        // details extension).
        for blocks in doc.footnote_defs.values_mut() {
            rewrite_blocks(blocks);
        }
        doc
    }

    fn render_block_extension(
        &self,
        node: &BlockExtension,
        ctx: &RenderContext<'_>,
    ) -> Option<String> {
        if node.name != CARRIER {
            return None;
        }
        Some(render_table(node, ctx))
    }
}

/// Rewrite every renderable `list-table` admonition (recursively) into a
/// `carve-list-table` carrier. An admonition that cannot be rendered as a table
/// (see [`is_renderable`]) is left untouched so the core renderer emits the
/// default `<div class="list-table">`.
fn rewrite_blocks(blocks: &mut [BlockNode]) {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Admonition(a) if a.kind == KIND => {
                rewrite_blocks(&mut a.children);
                if is_renderable(a) {
                    *block = BlockNode::Extension(BlockExtension {
                        attrs: a.attrs.take(),
                        name: CARRIER.to_string(),
                        children: std::mem::take(&mut a.children),
                        summary: a.title.take().map(|t| inline_text(&t)),
                    });
                }
            }
            BlockNode::List(l) => {
                for item in &mut l.items {
                    rewrite_blocks(&mut item.children);
                }
            }
            BlockNode::BlockQuote(b) => rewrite_blocks(&mut b.children),
            BlockNode::Admonition(a) => rewrite_blocks(&mut a.children),
            BlockNode::Div(d) => rewrite_blocks(&mut d.children),
            BlockNode::Extension(e) => rewrite_blocks(&mut e.children),
            BlockNode::DefinitionList(dl) => {
                for item in &mut dl.items {
                    for def in &mut item.definitions {
                        rewrite_blocks(def);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Whether a `list-table` admonition can be rendered as a `<table>`.
///
/// Renderable when its sole block child is a list (the table list) and every
/// outer item yields at least one cell. A stray sibling (a paragraph before or
/// after the list), or a row authored as a plain paragraph (`- not-a-cell-row`)
/// with no inner cell list, makes the block defer to the default div renderer
/// so content is never dropped. The check is NON-MUTATING - it inspects the
/// pristine AST so a deferred render is byte-identical to the plain admonition.
fn is_renderable(a: &Admonition) -> bool {
    let Some(BlockNode::List(outer)) = a.children.first() else {
        return false;
    };
    if a.children.len() != 1 {
        return false;
    }
    if outer.items.is_empty() {
        return false;
    }
    // Every row must yield at least one cell.
    outer.items.iter().all(|row| !row_cells(row).is_empty())
}

/// The cell items of a row: the items of every inner [`List`] child of the row
/// item, in document order. carve-rs nests a cell's trailing block content
/// directly inside the cell item (unlike carve-php, which can leave a stray
/// sibling outside the inner list), so a cell's blocks are simply its own
/// children - no extra-block bookkeeping is needed.
fn row_cells(row: &ListItem) -> Vec<&ListItem> {
    let mut cells = Vec::new();
    for child in &row.children {
        if let BlockNode::List(inner) = child {
            for cell in &inner.items {
                cells.push(cell);
            }
        }
    }
    cells
}

/// A single placed cell in the resolved grid.
struct Placed<'a> {
    cell: &'a ListItem,
    rowspan: usize,
    colspan: usize,
    /// A `^`/`<` marker with nothing to merge into: an empty cell, no content.
    empty: bool,
    /// Overlaps a rowspan from above: kept only for span tracking, emits
    /// nothing (its column is covered).
    dropped: bool,
}

/// One resolved grid row.
struct GridRow<'a> {
    /// Placed cells keyed by their starting column.
    cells: BTreeMap<usize, Placed<'a>>,
    /// Columns covered by a colspan body, a rowspan from above, a dropped
    /// overlapping cell, or a consumed `^` marker (the renderer skips them).
    covered: BTreeSet<usize>,
    /// Effective column count of the row (advanced past colspans).
    width: usize,
}

/// Build the `<table>` markup for a `list-table` carrier.
fn render_table(node: &BlockExtension, ctx: &RenderContext<'_>) -> String {
    // The carrier's sole child is the table list (guaranteed by is_renderable).
    let Some(BlockNode::List(outer)) = node.children.first() else {
        // Should not happen (only renderable tables are rewritten); emit an
        // empty table rather than panic.
        return "<table>\n</table>".to_string();
    };

    let rows: Vec<Vec<&ListItem>> = outer.items.iter().map(row_cells).collect();

    let header_rows = attr_count(node.attrs.as_ref(), "header-rows");
    let header_cols = attr_count(node.attrs.as_ref(), "header-cols");

    let grid = resolve_spans(&rows, header_rows);

    let column_count = grid.iter().map(|r| r.width).max().unwrap_or(0);

    let mut lines: Vec<String> = Vec::new();

    if let Some(summary) = node.summary.as_deref() {
        if !summary.trim().is_empty() {
            lines.push(format!("  <caption>{}</caption>", ctx.escape_html(summary)));
        }
    }

    let head_rows = grid.len().min(header_rows);

    if head_rows > 0 {
        let mut thead = String::new();
        for (row_index, placed_row) in grid.iter().take(head_rows).enumerate() {
            thead.push_str(&render_row(
                placed_row,
                row_index,
                header_rows,
                header_cols,
                column_count,
                ctx,
            ));
        }
        lines.push(format!("  <thead>{thead}</thead>"));
    }

    if head_rows < grid.len() {
        let mut tbody = String::new();
        for (offset, placed_row) in grid.iter().skip(head_rows).enumerate() {
            tbody.push_str("    ");
            tbody.push_str(&render_row(
                placed_row,
                offset + head_rows,
                header_rows,
                header_cols,
                column_count,
                ctx,
            ));
            tbody.push('\n');
        }
        let tbody = tbody.trim_end_matches('\n');
        lines.push(format!("  <tbody>\n{tbody}\n  </tbody>"));
    }

    let attrs = table_attrs(node.attrs.as_ref(), ctx);

    format!("<table{attrs}>\n{}\n</table>", lines.join("\n"))
}

/// Render one grid row as a `<tr>...</tr>`.
fn render_row(
    placed_row: &GridRow<'_>,
    row_index: usize,
    header_rows: usize,
    header_cols: usize,
    column_count: usize,
    ctx: &RenderContext<'_>,
) -> String {
    let is_header_row = row_index < header_rows;
    let mut html = String::new();
    for col in 0..column_count {
        match placed_row.cells.get(&col) {
            // A dropped cell overlaps a rowspan from above: emits nothing.
            Some(placed) if placed.dropped => continue,
            Some(placed) => {
                let is_header_cell = is_header_row || col < header_cols;
                let tag = if is_header_cell { "th" } else { "td" };
                let mut attr_html = String::new();
                if placed.rowspan > 1 {
                    attr_html.push_str(&format!(" rowspan=\"{}\"", placed.rowspan));
                }
                if placed.colspan > 1 {
                    attr_html.push_str(&format!(" colspan=\"{}\"", placed.colspan));
                }
                attr_html.push_str(&cell_attrs(placed.cell.attrs.as_ref(), ctx));
                let content = if placed.empty {
                    String::new()
                } else {
                    render_cell(placed.cell, ctx)
                };
                html.push_str(&format!("<{tag}{attr_html}>{content}</{tag}>"));
            }
            None => {
                if placed_row.covered.contains(&col) {
                    continue;
                }
                // A genuinely empty padding column.
                let tag = if is_header_row || col < header_cols {
                    "th"
                } else {
                    "td"
                };
                html.push_str(&format!("<{tag}></{tag}>"));
            }
        }
    }
    format!("<tr>{html}</tr>")
}

/// Render a single cell's content. A cell whose sole child is an attribute-free
/// paragraph collapses to its inline content (no `<p>` wrapper), matching tight
/// list-item / table-cell rendering; otherwise the block children render
/// normally and keep their wrappers.
fn render_cell(cell: &ListItem, ctx: &RenderContext<'_>) -> String {
    if let [BlockNode::Paragraph(p)] = cell.children.as_slice() {
        if p.attrs.is_none() {
            return ctx.render_inlines(&p.children);
        }
    }
    // Render at level 0 (cell content carries no leading indentation, matching
    // carve-php's `renderNodeFragment`) but through `render_blocks_at` so the
    // live document heading-id counter continues across the cell boundary - a
    // duplicate heading slug inside a cell gets its numeric suffix instead of
    // resetting (mirrors the details extension's `render_blocks_at`).
    ctx.render_blocks_at(&cell.children, 0)
        .trim_end_matches('\n')
        .to_string()
}

/// Resolve `^` / `<` span markers into a placed grid, mirroring the pipe-table
/// continuation model so the output matches an equivalent pipe table.
///
/// - A `<` cell folds into the nearest content cell to its LEFT in the same row,
///   growing its colspan. A leading `<` (no cell to the left) becomes its own
///   empty cell.
/// - A `^` cell folds into the cell currently open in its column above, growing
///   its rowspan. A `^` with no cell above becomes an empty cell.
/// - A cell carrying its own attributes is never a bare marker (its `^`/`<` is
///   literal).
/// - `header_rows` clamps rowspans at the header/body boundary: a `^` in a body
///   row whose origin sits in the header rows finds no valid origin and degrades
///   to an empty cell (an HTML cell cannot span row groups reliably).
fn resolve_spans<'a>(rows: &[Vec<&'a ListItem>], header_rows: usize) -> Vec<GridRow<'a>> {
    // Per-column origin of the cell currently open in it: (row_index, start_col).
    let mut column_origin: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut grid: Vec<GridRow<'a>> = Vec::new();
    let mut has_rowspan = false;

    for (row_index, cells) in rows.iter().enumerate() {
        // PASS 0: collapse colspan. A `<` increments the colspan of the most
        // recent entry to its left (unless that entry is itself a `<`); a
        // leading `<` becomes its own empty entry.
        struct Resolved<'a> {
            cell: &'a ListItem,
            marker: Option<char>,
            colspan: usize,
        }
        let mut resolved: Vec<Resolved<'a>> = Vec::new();
        for cell in cells {
            let marker = marker_of(cell);
            if marker == Some('<') {
                if let Some(last) = resolved.last_mut() {
                    if last.marker != Some('<') {
                        last.colspan += 1;
                        continue;
                    }
                }
            }
            resolved.push(Resolved {
                cell,
                marker,
                colspan: 1,
            });
        }

        // PASS 1: place each entry at a running column position.
        let mut placed: BTreeMap<usize, Placed<'a>> = BTreeMap::new();
        let mut col = 0usize;
        let mut extended_this_row: BTreeSet<(usize, usize)> = BTreeSet::new();
        let mut marker_consumed: BTreeSet<usize> = BTreeSet::new();
        let covered_from_above = if has_rowspan {
            columns_covered_by_previous_rows(&grid, row_index)
        } else {
            BTreeSet::new()
        };

        for r in &resolved {
            let colspan = r.colspan;
            if r.marker == Some('^') {
                // A `^` over a column already covered from above belongs to that
                // rowspan; consume it without emitting a cell.
                if covered_from_above.contains(&col) {
                    for c in col..col + colspan {
                        marker_consumed.insert(c);
                    }
                    col += colspan;
                    continue;
                }

                let origin = column_origin.get(&col).copied();
                let origin_exists = match origin {
                    Some((origin_row, origin_col)) => {
                        origin_row < row_index
                            && grid
                                .get(origin_row)
                                .map(|g| g.cells.contains_key(&origin_col))
                                .unwrap_or(false)
                            && column_occupied_in_row(&grid, row_index.wrapping_sub(1), col)
                            // Clamp at the header/body boundary.
                            && !(origin_row < header_rows && row_index >= header_rows)
                    }
                    None => false,
                };

                if origin_exists {
                    let (origin_row, origin_col) = origin.unwrap();
                    // A cell kept only for tracking after being dropped must not
                    // gain a rowspan; consume the `^` silently.
                    let is_dropped = grid[origin_row]
                        .cells
                        .get(&origin_col)
                        .map(|p| p.dropped)
                        .unwrap_or(false);
                    if is_dropped {
                        for c in col..col + colspan {
                            marker_consumed.insert(c);
                        }
                        col += colspan;
                        continue;
                    }
                    // Extend the open cell above (only once per origin per row).
                    let origin_width;
                    {
                        let origin_cell = grid[origin_row].cells.get_mut(&origin_col).unwrap();
                        if extended_this_row.insert((origin_row, origin_col)) {
                            origin_cell.rowspan += 1;
                            has_rowspan = true;
                        }
                        origin_width = origin_cell.colspan;
                    }
                    // Columns this marker consumes beyond the origin emit no
                    // cell of their own; skip them.
                    for c in col..col + colspan {
                        marker_consumed.insert(c);
                    }
                    // Keep the origin's columns pointing at it so a later `^`
                    // continues the chain across its full width.
                    for c in origin_col..origin_col + origin_width {
                        column_origin.insert(c, (origin_row, origin_col));
                    }
                    col += colspan;
                    continue;
                }

                // No cell above to extend: an empty cell (pipe-table parity).
                placed.insert(
                    col,
                    Placed {
                        cell: r.cell,
                        rowspan: 1,
                        colspan,
                        empty: true,
                        dropped: false,
                    },
                );
                for c in col..col + colspan {
                    column_origin.insert(c, (row_index, col));
                }
                col += colspan;
                continue;
            }

            // A content cell, or a leading `<` (an empty cell, never literal).
            placed.insert(
                col,
                Placed {
                    cell: r.cell,
                    rowspan: 1,
                    colspan,
                    empty: r.marker == Some('<'),
                    dropped: false,
                },
            );
            for c in col..col + colspan {
                column_origin.insert(c, (row_index, col));
            }
            col += colspan;
        }
        let row_width = col;

        // PASS 2: drop placed cells whose start column is covered by a rowspan
        // reaching into this row from a previous one. Recompute occupancy now
        // that pass 1 may have extended a previous row's rowspan into this row.
        let occupied_by_previous = if has_rowspan {
            columns_covered_by_previous_rows(&grid, row_index)
        } else {
            BTreeSet::new()
        };
        let mut dropped_span: BTreeSet<usize> = BTreeSet::new();
        let start_cols: Vec<usize> = placed.keys().copied().collect();
        for start_col in start_cols {
            if occupied_by_previous.contains(&start_col) {
                let colspan = placed[&start_col].colspan;
                placed.get_mut(&start_col).unwrap().dropped = true;
                for c in start_col..start_col + colspan {
                    dropped_span.insert(c);
                }
            }
        }

        // Mark every grid column the renderer must skip.
        let mut covered: BTreeSet<usize> = BTreeSet::new();
        for (start_col, placed_cell) in &placed {
            for c in start_col + 1..start_col + placed_cell.colspan {
                covered.insert(c);
            }
        }
        for c in dropped_span {
            covered.insert(c);
        }
        for c in &occupied_by_previous {
            covered.insert(*c);
        }
        for c in marker_consumed {
            covered.insert(c);
        }

        grid.push(GridRow {
            cells: placed,
            covered,
            width: row_width,
        });
    }

    // A rowspan that would reach past the last row is naturally clamped: each
    // cell's rowspan is only ever incremented by a `^` in a row that actually
    // exists, so the grid can never produce an out-of-table span. carve-php
    // emits a warning here; carve-rs has no warning channel, so the clamp is
    // silent (the output is identical either way).

    grid
}

/// Columns covered, in the row at `current_row_index`, by a rowspan that started
/// in an EARLIER row and reaches into it.
fn columns_covered_by_previous_rows(
    grid: &[GridRow<'_>],
    current_row_index: usize,
) -> BTreeSet<usize> {
    let mut occupied_until: BTreeMap<usize, usize> = BTreeMap::new();
    for (row_index, placed_row) in grid.iter().enumerate() {
        if row_index >= current_row_index {
            break;
        }
        for (start_col, placed_cell) in &placed_row.cells {
            let end = row_index + placed_cell.rowspan;
            for c in *start_col..start_col + placed_cell.colspan {
                let slot = occupied_until.entry(c).or_insert(0);
                *slot = (*slot).max(end);
            }
        }
    }
    occupied_until
        .into_iter()
        .filter(|(_, end)| *end > current_row_index)
        .map(|(col, _)| col)
        .collect()
}

/// Whether `col` was occupied in the already-built row at `row_index`. Gates
/// `^` continuation: a ragged row that omitted a column breaks the chain.
fn column_occupied_in_row(grid: &[GridRow<'_>], row_index: usize, col: usize) -> bool {
    let Some(row) = grid.get(row_index) else {
        return false;
    };
    if row.covered.contains(&col) {
        return true;
    }
    for (start_col, placed_cell) in &row.cells {
        if col >= *start_col && col < start_col + placed_cell.colspan {
            return true;
        }
    }
    false
}

/// Detect a span marker cell. Returns `Some('^')` / `Some('<')` when the cell's
/// sole inline content is exactly that marker character, or `None` otherwise. A
/// cell carrying its own attributes is never a marker (the `^`/`<` is literal),
/// matching the escape rule pipe tables use.
fn marker_of(cell: &ListItem) -> Option<char> {
    if cell.attrs.is_some() {
        return None;
    }
    let [BlockNode::Paragraph(p)] = cell.children.as_slice() else {
        return None;
    };
    if p.attrs.is_some() {
        return None;
    }
    let [InlineNode::Text(t)] = p.children.as_slice() else {
        return None;
    };
    match t.trim() {
        "^" => Some('^'),
        "<" => Some('<'),
        _ => None,
    }
}

/// Parse a non-negative integer block attribute, defaulting to 0.
/// Resolve a `header-rows` / `header-cols` attribute to a count.
///
/// - absent -> 0 (no header rows/cols)
/// - present but empty (the boolean form `{header-rows}`, which Carve stores as
///   `header-rows=""`) -> 1, i.e. the first row/column is the header - the
///   default a table with headers wants, so `{header-rows}` alone suffices
/// - an explicit number (`{header-rows=2}`) -> that count (clamped at 0)
fn attr_count(attrs: Option<&Attrs>, key: &str) -> usize {
    match attrs.and_then(|a| a.key_values.get(key)) {
        None => 0,
        Some(value) if value.trim().is_empty() => 1,
        Some(value) => value
            .trim()
            .parse::<i64>()
            .ok()
            .map(|n| n.max(0) as usize)
            .unwrap_or(0),
    }
}

/// Build the `<table>` tag attributes: drop the structural keys this extension
/// consumes (`header-rows`, `header-cols`) and emit the rest in source order.
/// The auto `list-table` class is the admonition's `kind`, not an attr class,
/// so it is naturally absent from the `<table>` tag (which is itself the styling
/// hook).
fn table_attrs(attrs: Option<&Attrs>, ctx: &RenderContext<'_>) -> String {
    let Some(attrs) = attrs else {
        return String::new();
    };
    let mut out = String::new();
    let emit_id = |out: &mut String| {
        if let Some(id) = &attrs.id {
            out.push_str(&format!(" id=\"{}\"", ctx.escape_attr(id)));
        }
    };
    let emit_class = |out: &mut String| {
        if !attrs.classes.is_empty() {
            out.push_str(&format!(
                " class=\"{}\"",
                ctx.escape_attr(&attrs.classes.join(" "))
            ));
        }
    };
    let is_consumed = |key: &str| key == "header-rows" || key == "header-cols";

    if attrs.order.is_empty() {
        emit_id(&mut out);
        emit_class(&mut out);
        for (key, value) in &attrs.key_values {
            if !is_consumed(key) {
                out.push_str(&format!(" {}=\"{}\"", key, ctx.escape_attr(value)));
            }
        }
        return out;
    }
    for slot in &attrs.order {
        match slot {
            AttrSlot::Id => emit_id(&mut out),
            AttrSlot::Class => emit_class(&mut out),
            AttrSlot::Key(key) => {
                if !is_consumed(key) {
                    if let Some(value) = attrs.key_values.get(key) {
                        out.push_str(&format!(" {}=\"{}\"", key, ctx.escape_attr(value)));
                    }
                }
            }
        }
    }
    out
}

/// Build a cell's own attribute markup for its `<td>`/`<th>` tag. Drops any
/// author-written `rowspan`/`colspan` (case-insensitively) so the structural
/// span attributes the caller emits are the only ones; emits the rest in source
/// order.
fn cell_attrs(attrs: Option<&Attrs>, ctx: &RenderContext<'_>) -> String {
    let Some(attrs) = attrs else {
        return String::new();
    };
    let mut out = String::new();
    let emit_id = |out: &mut String| {
        if let Some(id) = &attrs.id {
            out.push_str(&format!(" id=\"{}\"", ctx.escape_attr(id)));
        }
    };
    let emit_class = |out: &mut String| {
        if !attrs.classes.is_empty() {
            out.push_str(&format!(
                " class=\"{}\"",
                ctx.escape_attr(&attrs.classes.join(" "))
            ));
        }
    };
    let is_span = |key: &str| {
        let lower = key.to_ascii_lowercase();
        lower == "rowspan" || lower == "colspan"
    };

    if attrs.order.is_empty() {
        emit_id(&mut out);
        emit_class(&mut out);
        for (key, value) in &attrs.key_values {
            if !is_span(key) {
                out.push_str(&format!(" {}=\"{}\"", key, ctx.escape_attr(value)));
            }
        }
        return out;
    }
    for slot in &attrs.order {
        match slot {
            AttrSlot::Id => emit_id(&mut out),
            AttrSlot::Class => emit_class(&mut out),
            AttrSlot::Key(key) => {
                if !is_span(key) {
                    if let Some(value) = attrs.key_values.get(key) {
                        out.push_str(&format!(" {}=\"{}\"", key, ctx.escape_attr(value)));
                    }
                }
            }
        }
    }
    out
}

/// Flatten an inline tree to its text content (used for the caption title).
/// Mirrors the `details` extension's `inline_text`, dropping the same set of
/// nodes so a caption flattens identically across implementations.
fn inline_text(nodes: &[InlineNode]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(s) => out.push_str(s),
            InlineNode::Code(s, _) => out.push_str(s),
            InlineNode::Emphasis(e) => out.push_str(&inline_text(&e.children)),
            InlineNode::Link(l) => out.push_str(&inline_text(&l.children)),
            InlineNode::Span(s) => out.push_str(&inline_text(&s.children)),
            InlineNode::Extension(e) => out.push_str(&inline_text(&e.children)),
            InlineNode::CriticInsert(c) => out.push_str(&inline_text(&c.children)),
            InlineNode::CriticDelete(c) => out.push_str(&inline_text(&c.children)),
            _ => {}
        }
    }
    out
}
