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

use std::collections::BTreeMap;

use crate::ast::{
    Admonition, AttrSlot, Attrs, BlockExtension, BlockNode, Document, InlineNode, ListItem,
};
use crate::escape::{is_dangerous_attr_name, is_valid_attr_name, sanitize_attr_value};
use crate::extension::{BeforeRenderContext, CarveExtension, RenderContext};

/// The admonition kind this extension claims.
const KIND: &str = "list-table";

/// Sentinel name for the rewritten carrier node.
pub(crate) const CARRIER: &str = "carve-list-table";

/// DoS guards: span resolution is ~O(rows^2), so cap the dimensions and defer
/// anything larger to the plain nested-list div. Far beyond any legitimate
/// hand-authored table; kept identical across carve-php / carve-js / carve-rs.
const MAX_ROWS: usize = 10_000;
const MAX_CELLS: usize = 100_000;

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

    fn before_render(&self, mut doc: Document, _ctx: &BeforeRenderContext<'_>) -> Document {
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
                        summary: a.title.take(),
                        label: a.label.take(),
                        pos: None,
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
    if !outer.items.iter().all(|row| !row_cells(row).is_empty()) {
        return false;
    }
    // DoS guard: span resolution rescans prior rows (~O(rows^2)). Defer an
    // over-large table to the plain div (content preserved, no quadratic
    // blow-up). Limits match carve-php / carve-js.
    if outer.items.len() > MAX_ROWS {
        return false;
    }
    let total_cells: usize = outer.items.iter().map(|row| row_cells(row).len()).sum();
    total_cells <= MAX_CELLS
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

/// One source cell in the resolved grid (one entry per authored cell). Mirrors
/// carve-js's `GridEntry`: a `^`/`<` that found a source to merge into is
/// flagged `skip` and emits nothing; an unmergeable marker (first-row `^`,
/// leading `<`, or one clamped at the header/body boundary) keeps `skip = false`
/// and renders as an EMPTY cell occupying its grid position. The marker is never
/// rendered as literal text.
struct GridEntry<'a> {
    cell: &'a ListItem,
    marker: Option<char>,
    rowspan: usize,
    colspan: usize,
    /// Merged into another cell (a `^`/`<` that found a source): emits nothing.
    skip: bool,
}

/// Output-column placement for every grid entry (mirrors carve-js `Placement`).
struct Placement {
    /// Output start column of each source cell, per row (skipped cells: `None`).
    cols: Vec<Vec<Option<usize>>>,
    /// Highest output column reached by each row (rowspan coverage included).
    row_reach: Vec<usize>,
    /// Total table width = the widest row's reach.
    column_count: usize,
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
    let footer_rows =
        attr_count(node.attrs.as_ref(), "footer-rows").min(rows.len().saturating_sub(header_rows));
    let columns = list_table_columns(node.attrs.as_ref());

    // Resolve `^`/`<` span markers into a positional grid, mirroring the
    // pipe-table span model so the output matches an equivalent pipe table, then
    // flow each rendered cell into an output column past any rowspan from above.
    let grid = resolve_spans(&rows, header_rows, footer_rows);
    let placement = place_columns(&grid);
    let column_count = placement.column_count;

    let mut lines: Vec<String> = Vec::new();

    if columns.iter().any(|c| c.2.is_some()) {
        let cols = columns
            .iter()
            .map(|c| match c.2 {
                Some(w) => format!("    <col style=\"width: {w}%;\">"),
                None => "    <col>".to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        lines.push(format!("  <colgroup>\n{cols}\n  </colgroup>"));
    }

    if let Some(summary) = &node.summary {
        let rendered = ctx.render_inlines(summary);
        if !rendered.trim().is_empty() {
            lines.push(format!("  <caption>{rendered}</caption>"));
        }
    }
    // Graceful degradation: a grouping `[label]` is uncommon on a list-table,
    // but it must never be silently dropped. The table renderer consumes the
    // node, so the core caption floor never runs; surface the label here as the
    // same `<p class="div-label">` caption the floor would emit (after the
    // title `<caption>` when both are present).
    if let Some(label) = node.label.as_deref() {
        if !label.is_empty() {
            lines.push(format!(
                "  <p class=\"div-label\">{}</p>",
                ctx.escape_html(label)
            ));
        }
    }

    let head_rows = grid.len().min(header_rows);

    if head_rows > 0 {
        let mut thead = String::new();
        for (row_index, grid_row) in grid.iter().take(head_rows).enumerate() {
            thead.push_str(&render_row(
                grid_row,
                row_index,
                header_rows,
                header_cols,
                column_count,
                &placement,
                ctx,
                &columns,
            ));
        }
        lines.push(format!("  <thead>{thead}</thead>"));
    }

    let foot_start = grid.len().saturating_sub(footer_rows);
    if head_rows < foot_start {
        let mut tbody = String::new();
        for (offset, grid_row) in grid.iter().take(foot_start).skip(head_rows).enumerate() {
            tbody.push_str("    ");
            tbody.push_str(&render_row(
                grid_row,
                offset + head_rows,
                header_rows,
                header_cols,
                column_count,
                &placement,
                ctx,
                &columns,
            ));
            tbody.push('\n');
        }
        let tbody = tbody.trim_end_matches('\n');
        lines.push(format!("  <tbody>\n{tbody}\n  </tbody>"));
    }
    if foot_start < grid.len() {
        let mut foot = String::new();
        for (offset, grid_row) in grid.iter().skip(foot_start).enumerate() {
            foot.push_str(&render_row(
                grid_row,
                foot_start + offset,
                header_rows,
                header_cols,
                column_count,
                &placement,
                ctx,
                &columns,
            ));
        }
        lines.push(format!("  <tfoot>{foot}</tfoot>"));
    }

    let attrs = table_attrs(node.attrs.as_ref(), ctx);

    format!("<table{attrs}>\n{}\n</table>", lines.join("\n"))
}

/// Render one grid row as a `<tr>...</tr>`. Mirrors carve-js `renderRow`: emit
/// every non-skipped entry at its placed output column, then pad trailing
/// columns so a ragged row stays rectangular (a rowspan from above that already
/// reaches the row end suppresses that padding via `row_reach`).
#[allow(clippy::too_many_arguments)]
fn render_row(
    grid_row: &[GridEntry<'_>],
    row_index: usize,
    header_rows: usize,
    header_cols: usize,
    column_count: usize,
    placement: &Placement,
    ctx: &RenderContext<'_>,
    columns: &[(Option<&str>, Option<&str>, Option<f64>)],
) -> String {
    let is_header_row = row_index < header_rows;
    let row_cols = &placement.cols[row_index];
    let mut html = String::new();
    let mut next_col = 0usize;
    for (i, entry) in grid_row.iter().enumerate() {
        // A merged `^`/`<` emits nothing - its column was absorbed by the cell
        // it merged into (a rowspan above, or the cell to its left).
        if entry.skip {
            continue;
        }
        let Some(col) = row_cols[i] else {
            continue;
        };
        let is_header_cell = is_header_row || col < header_cols;
        let tag = if is_header_cell { "th" } else { "td" };
        let mut attr_html = String::new();
        // PART 10 SST9, the same rule and the same position as the pipe table:
        // `col` in the header-row run, `row` for a cell that is a header only
        // because it falls inside `header-cols`. A ListTable and the pipe table
        // it is equivalent to must not differ in accessibility markup.
        if is_header_cell {
            attr_html.push_str(if is_header_row {
                " scope=\"col\""
            } else {
                " scope=\"row\""
            });
        }
        if entry.rowspan > 1 {
            attr_html.push_str(&format!(" rowspan=\"{}\"", entry.rowspan));
        }
        if entry.colspan > 1 {
            attr_html.push_str(&format!(" colspan=\"{}\"", entry.colspan));
        }
        if let Some((align, valign, _)) = columns.get(col) {
            let mut style = String::new();
            if let Some(v) = align {
                style.push_str(&format!("text-align: {v};"));
            }
            if align.is_some() && valign.is_some() {
                style.push(' ');
            }
            if let Some(v) = valign {
                style.push_str(&format!("vertical-align: {v};"));
            }
            if !style.is_empty() {
                attr_html.push_str(&format!(" style=\"{style}\""));
            }
        }
        attr_html.push_str(&cell_attrs(entry.cell.attrs.as_ref(), ctx));
        // A `^`/`<` marker (merged or not) renders no content, never literal
        // `^`/`<` (pipe-table parity); an unmergeable marker is an empty cell.
        let content = if entry.marker.is_some() {
            String::new()
        } else {
            render_cell(entry.cell, ctx)
        };
        html.push_str(&format!("<{tag}{attr_html}>{content}</{tag}>"));
        next_col = col + entry.colspan;
    }

    // Pad trailing columns so a ragged row stays rectangular.
    let mut col = next_col.max(placement.row_reach[row_index]);
    while col < column_count {
        let is_pad_header = is_header_row || col < header_cols;
        let tag = if is_pad_header { "th" } else { "td" };
        // A padded cell is still a header cell where the grid says so.
        let scope = if is_pad_header {
            if is_header_row {
                " scope=\"col\""
            } else {
                " scope=\"row\""
            }
        } else {
            ""
        };
        html.push_str(&format!("<{tag}{scope}></{tag}>"));
        col += 1;
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

/// Resolve `^` / `<` span markers into a positional grid (one entry per authored
/// cell), EXACTLY mirroring carve-js's pipe-table span model (and carve-rs's own
/// `render.rs` pipe-table renderer) so the output is identical to the equivalent
/// pipe table.
///
/// - A `^` cell grows the rowspan of the nearest non-skipped cell directly above
///   it in the same SOURCE column and is flagged `skip` (emits nothing).
/// - A `<` cell grows the colspan of the nearest non-skipped cell to its LEFT in
///   the same row and is flagged `skip`.
/// - A marker that finds no source to merge into (a first-row `^`, a leading `<`,
///   a `^` clamped at the header/body boundary, or a `<` whose only left neighbor
///   is a skipped continuation) keeps `skip = false` and renders as an EMPTY
///   cell occupying its grid position - never dropped, never literal.
/// - A cell carrying its own attributes is never a bare marker (its `^`/`<` is
///   literal).
/// - `header_rows` clamps rowspans at the header/body boundary: a `^` in a body
///   row whose source sits in the header rows finds no valid source and degrades
///   to an empty cell (an HTML cell cannot span row groups reliably).
fn resolve_spans<'a>(
    rows: &[Vec<&'a ListItem>],
    header_rows: usize,
    footer_rows: usize,
) -> Vec<Vec<GridEntry<'a>>> {
    let mut grid: Vec<Vec<GridEntry<'a>>> = rows
        .iter()
        .map(|cells| {
            cells
                .iter()
                .map(|cell| GridEntry {
                    cell,
                    marker: marker_of(cell),
                    rowspan: 1,
                    colspan: 1,
                    skip: false,
                })
                .collect()
        })
        .collect();

    // Per SOURCE column, the last row index (above the current one) whose cell is
    // not skipped - the nearest source a `^` can extend.
    let mut last_non_skip: Vec<Option<usize>> = Vec::new();
    for r in 0..grid.len() {
        let cols = grid[r].len();
        for c in 0..cols {
            if grid[r][c].skip {
                continue;
            }
            let marker = grid[r][c].marker;

            if marker == Some('^') && r > 0 {
                let up = last_non_skip.get(c).copied().flatten();
                // Clamp at the header/body boundary: a `^` in a body row must not
                // extend a cell that originated in the header rows. Leave it
                // unmerged (it then renders as an empty cell) so no `th rowspan`
                // crosses into the body group.
                let crosses_header = matches!(up, Some(u) if u < header_rows && r >= header_rows);
                let footer_start = grid.len().saturating_sub(footer_rows);
                let crosses_footer = matches!(up, Some(u) if u < footer_start && r >= footer_start);
                let has_source = matches!(up, Some(u) if u < grid.len() && c < grid[u].len());
                if has_source && !crosses_header && !crosses_footer {
                    let u = up.unwrap();
                    grid[u][c].rowspan += 1;
                    grid[r][c].skip = true;
                }
            } else if marker == Some('<') && c > 0 {
                let mut left = c as isize - 1;
                while left >= 0 && grid[r][left as usize].skip {
                    left -= 1;
                }
                if left >= 0 {
                    grid[r][left as usize].colspan += 1;
                    grid[r][c].skip = true;
                }
            }

            // A cell that ends up non-skipped becomes the nearest source for the
            // cells below it in this source column.
            if !grid[r][c].skip {
                if c >= last_non_skip.len() {
                    last_non_skip.resize(c + 1, None);
                }
                last_non_skip[c] = Some(r);
            }
        }
    }

    grid
}

fn list_table_columns(attrs: Option<&Attrs>) -> Vec<(Option<&str>, Option<&str>, Option<f64>)> {
    let value = |key| {
        attrs
            .and_then(|a| a.key_values.get(key))
            .map(|v| v.split(',').collect::<Vec<_>>())
            .unwrap_or_default()
    };
    let aligns = value("aligns");
    let valigns = value("valigns");
    let widths = value("widths");
    let len = aligns.len().max(valigns.len()).max(widths.len());
    (0..len)
        .map(|i| {
            (
                aligns
                    .get(i)
                    .copied()
                    .filter(|v| matches!(*v, "left" | "right" | "center")),
                valigns
                    .get(i)
                    .copied()
                    .filter(|v| matches!(*v, "top" | "middle" | "bottom")),
                widths
                    .get(i)
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| *v > 0.0 && *v <= 100.0),
            )
        })
        .collect()
}

/// Assign each rendered cell an output column by flowing it top-down past any
/// column a rowspan from an earlier row still holds - the same flow a browser
/// (and carve-rs's pipe table) uses. Skipped cells take no column. Mirrors
/// carve-js's `placeColumns`.
fn place_columns(grid: &[Vec<GridEntry<'_>>]) -> Placement {
    // occupied_until[col] = exclusive row index through which a rowspan holds col.
    let mut occupied_until: BTreeMap<usize, usize> = BTreeMap::new();
    let mut cols: Vec<Vec<Option<usize>>> = Vec::with_capacity(grid.len());
    let mut row_reach: Vec<usize> = Vec::with_capacity(grid.len());
    let mut column_count = 0usize;

    for (r, grid_row) in grid.iter().enumerate() {
        let mut row_cols: Vec<Option<usize>> = Vec::with_capacity(grid_row.len());
        let mut col = 0usize;
        let mut reach = 0usize;
        // A rowspan descending from above into this row reaches at least its col.
        for (c, end) in &occupied_until {
            if *end > r {
                reach = reach.max(c + 1);
            }
        }

        for entry in grid_row {
            if entry.skip {
                row_cols.push(None);
                continue;
            }
            // Flow past columns a rowspan from above still holds in this row.
            while occupied_until.get(&col).copied().unwrap_or(0) > r {
                col += 1;
            }
            row_cols.push(Some(col));
            if entry.rowspan > 1 {
                for c in col..col + entry.colspan {
                    let slot = occupied_until.entry(c).or_insert(0);
                    *slot = (*slot).max(r + entry.rowspan);
                }
            }
            col += entry.colspan;
            reach = reach.max(col);
        }

        cols.push(row_cols);
        row_reach.push(reach);
        column_count = column_count.max(reach);
    }

    Placement {
        cols,
        row_reach,
        column_count,
    }
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
    match t.value.trim() {
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
    let is_consumed = |key: &str| {
        matches!(
            key,
            "header-rows" | "header-cols" | "footer-rows" | "aligns" | "valigns" | "widths"
        )
    };

    if attrs.order.is_empty() {
        emit_id(&mut out);
        emit_class(&mut out);
        for (key, value) in &attrs.key_values {
            if !is_consumed(key) && !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                out.push_str(&format!(
                    " {}=\"{}\"",
                    ctx.escape_attr(key),
                    ctx.escape_attr(&sanitize_attr_value(key, value))
                ));
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
                        if !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                            out.push_str(&format!(
                                " {}=\"{}\"",
                                ctx.escape_attr(key),
                                ctx.escape_attr(&sanitize_attr_value(key, value))
                            ));
                        }
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
            if !is_span(key) && !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                out.push_str(&format!(
                    " {}=\"{}\"",
                    ctx.escape_attr(key),
                    ctx.escape_attr(&sanitize_attr_value(key, value))
                ));
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
                        if !is_dangerous_attr_name(key) && is_valid_attr_name(key) {
                            out.push_str(&format!(
                                " {}=\"{}\"",
                                ctx.escape_attr(key),
                                ctx.escape_attr(&sanitize_attr_value(key, value))
                            ));
                        }
                    }
                }
            }
        }
    }
    out
}
