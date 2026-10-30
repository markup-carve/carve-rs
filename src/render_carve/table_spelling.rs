use super::*;

pub(super) fn colon_fence_for(ctx: &CarveContext) -> String {
    ":".repeat(3 + ctx.colon_fence_depth)
}

pub(super) fn render_delimited_table(
    table: &Table,
    session: &RenderSession,
    ctx: &mut CarveContext,
) -> Option<String> {
    let format = table.source_format?;
    let separator = format.separator();
    if table.short_caption.is_some()
        || table.separator_widths.is_some()
        || table
            .rows
            .iter()
            .any(|row| row.continuation_lines.is_some())
    {
        return None;
    }
    let foot = crate::parse::delimited_table::count(
        table
            .attrs
            .as_ref()
            .and_then(|attrs| attrs.key_values.get("footer-rows")),
        0,
    );
    let head = crate::parse::delimited_table::count(
        table
            .attrs
            .as_ref()
            .and_then(|attrs| attrs.key_values.get("header-rows")),
        table.rows.len().saturating_sub(foot).min(1),
    );
    let mut lines = Vec::new();
    for (index, row) in table.rows.iter().enumerate() {
        if row.attrs.is_some() {
            return None;
        }
        let mut fields = Vec::new();
        for cell in &row.cells {
            if cell.header != (index < head)
                || cell.attrs.is_some()
                || cell.blocks.is_some()
                || cell.span.is_some()
                || cell.colspan.is_some()
                || cell.rowspan.is_some()
                || cell.align.is_some()
                || cell.valign.is_some()
            {
                return None;
            }
            let mut value = String::new();
            for child in &cell.children {
                if let InlineNode::Text(text) = child {
                    value.push_str(&text.value);
                } else {
                    return None;
                }
            }
            if value.contains('\r') {
                return None;
            }
            if value.contains([separator, '"', '\n']) {
                fields.push(format!("\"{}\"", value.replace('"', "\"\"")));
            } else {
                fields.push(value);
            }
        }
        let line = fields.join(&separator.to_string());
        lines.push(if line.is_empty() {
            "\"\"".to_owned()
        } else {
            line
        });
    }
    let payload = lines.join("\n");
    let fence = safe_fence(&payload, 3);
    let mut source = format!(
        "{fence}{}\n{payload}{}{fence}",
        format.as_str(),
        if payload.is_empty() { "" } else { "\n" }
    );
    if let Some(caption) = &table.caption {
        source.push_str("\n^ ");
        source.push_str(&render_inlines(session, caption, ctx));
    }
    Some(source)
}

pub(super) fn table_written_width(session: &RenderSession, cell: &str) -> usize {
    let marker = escaped_space(session);
    let written = cell.replace(marker.as_str(), "\\ ");
    crate::table_display_width::table_display_width(&written)
}

/// Tables prefer the NATIVE header form: an `=` on each header cell, plus the
/// per-cell `<`/`>`/`~` alignment markers.
///
/// A colspan cell is always written plain (`| < |`), so a header row can keep
/// the native form when its span markers form a TRAILING run of colspans after
/// at least one real header cell: each `<` absorbs into the `|=` header on its
/// left, and the row is still promoted by those markers. Everything else needs
/// a delimiter row: a LEADING span has no `|=` anchor before it; a real cell
/// AFTER a span would have to be written `|=< K`, read as an aligned header; and
/// a trailing ROWSPAN (`^`) does not absorb left, so a native `| ^ |` in the
/// first row is not a header cell and the row would fall out of the head.
pub(super) fn render_table(
    session: &RenderSession,
    node: &Table,
    ctx: &mut CarveContext,
) -> String {
    if node.rows.is_empty()
        && node
            .separator_widths
            .as_ref()
            .is_some_and(|widths| !widths.is_empty())
    {
        crate::render_carve_error::record_unspellable(
            "table",
            "a separator row needs a preceding table row",
        );
        return String::new();
    }
    let mut rows = Vec::new();
    let mut row_slots: Vec<(usize, Vec<String>, char, String)> = Vec::new();
    let header_row = node
        .rows
        .first()
        .is_some_and(|row| !row.cells.is_empty() && row.cells.iter().all(|cell| cell.header));
    // PART 12 §3 records the author's separator spelling. The canonical writer
    // ignores it when choosing between native headers and a delimiter row.
    let authored_widths = node
        .separator_widths
        .as_ref()
        .filter(|widths| !widths.is_empty());
    if authored_widths.is_some() && !header_row {
        crate::render_carve_error::record_unspellable(
            "table",
            "field separatorWidths needs a header row",
        );
        return String::new();
    }
    let needs_delimiter = header_row
        && node.rows[0].cells.iter().any(|cell| {
            cell.children.is_empty()
                && cell.blocks.is_none()
                && cell.align == Some(TableAlign::Left)
        })
        || header_row
            && node.rows.first().is_some_and(|row| {
                let first_span = row.cells.iter().position(|cell| cell.span.is_some());
                match first_span {
                    None => false,
                    Some(index) => {
                        // Native only for a trailing run of colspans after a
                        // real header cell; every other span shape needs the
                        // delimiter.
                        !(index >= 1
                            && row.cells[index..]
                                .iter()
                                .all(|cell| cell.span == Some(TableCellSpan::Colspan)))
                    }
                }
            });

    // CARVE-P12-069: each `cell_wrap` puts the part after it on its row's
    // continuation line, one slot per cell.
    let mut continuations: Vec<Vec<Vec<String>>> = node
        .rows
        .iter()
        .map(|row| {
            vec![vec![" ".to_string(); row.cells.len()]; row.continuation_lines.unwrap_or(0)]
        })
        .collect();
    let mut occupied: Vec<Vec<Vec<bool>>> = continuations
        .iter()
        .map(|lines| lines.iter().map(|slots| vec![false; slots.len()]).collect())
        .collect();
    for (row_index, row) in node.rows.iter().enumerate() {
        let mut cells = Vec::new();
        for (cell_index, cell) in row.cells.iter().enumerate() {
            ctx.cell_not_last = cell_index + 1 < row.cells.len();
            // In the delimiter form the promoted row is written as ordinary
            // data cells - the row after it is what makes them headers.
            let mark_header = !(needs_delimiter && row_index == 0);
            // A generated separator carries the column alignment, so the
            // promoted row does not spell it a second time.
            let suppress_align = row_index == 0 && needs_delimiter;
            let rendered = render_table_cell(
                session,
                cell,
                ctx,
                mark_header,
                suppress_align,
                row_index == 0 && header_row && !needs_delimiter,
                row.continuation_lines.unwrap_or(0) > 0,
            );
            match place_cell_wraps(
                &rendered,
                cell.span.is_some(),
                row_index,
                cell_index,
                &mut continuations,
                &mut occupied,
            ) {
                Ok(own) => cells.push(own),
                Err(reason) => {
                    crate::render_carve_error::record_unspellable("table_row", reason);
                    cells.push(rendered);
                }
            }
        }
        ctx.cell_not_last = false;
        // A row of empty cells is written as a row, one slot per cell
        // (CARVE-P11-014). Only a row with no cell has no spelling: `|` is prose.
        if cells.is_empty() {
            crate::render_carve_error::record_unspellable(
                "table_row",
                "a table row with no cells has no Carve source spelling",
            );
        }
        let attrs = render_row_attrs(&row.attrs);
        row_slots.push((rows.len(), cells.clone(), '|', attrs.clone()));
        rows.push(render_table_row(&cells, &attrs));
        let mut run = crate::parse::table_run_open_after(&cells.join("|"), None, 0);
        for line in &continuations[row_index] {
            // CARVE-P2-019: a pipe inside a run that stays open is run content,
            // so an empty slot past it would read back as text (CARVE-P11-002).
            let last = line
                .iter()
                .rposition(|slot| !slot.trim().is_empty())
                .unwrap_or(0);
            let written =
                crate::parse::table_run_open_after(&line[..=last].join("|"), run.0, run.1);
            let line = if written.0.is_some() {
                &line[..=last]
            } else {
                &line[..]
            };
            run = written;
            row_slots.push((rows.len(), line.to_vec(), '+', String::new()));
            rows.push(format!("+{}|", line.join("|")));
        }
    }
    let mut separator_index = None;
    if needs_delimiter {
        let widths = vec![3; node.rows[0].cells.len()];
        let sep = match render_table_separator(node, &widths) {
            Some(separator) => separator,
            None => return String::new(),
        };
        let head_lines = node.rows[0].continuation_lines.unwrap_or(0);
        let index = 1 + head_lines;
        rows.insert(index, sep);
        separator_index = Some(index);
    }
    if session.align_tables && !row_slots.is_empty() {
        let mut widths = Vec::<usize>::new();
        for (_, cells, _, _) in &row_slots {
            for (index, cell) in cells.iter().enumerate() {
                if widths.len() <= index {
                    widths.resize(index + 1, 0);
                }
                widths[index] = widths[index].max(table_written_width(session, cell));
            }
        }
        let separator_cells = separator_index.map(|index| {
            rows[index][1..rows[index].len() - 1]
                .split('|')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        });
        if let Some(cells) = &separator_cells {
            for (index, cell) in cells.iter().enumerate() {
                if widths.len() <= index {
                    widths.resize(index + 1, 0);
                }
                widths[index] = widths[index].max(table_written_width(session, cell));
            }
        }
        for (index, cells, marker, attrs) in row_slots {
            let index = index + usize::from(separator_index.is_some_and(|sep| index >= sep));
            let padded = cells
                .iter()
                .enumerate()
                .map(|(column, cell)| {
                    format!(
                        "{cell}{}",
                        " ".repeat(widths[column] - table_written_width(session, cell))
                    )
                })
                .collect::<Vec<_>>();
            rows[index] = if marker == '|' {
                render_table_row(&padded, &attrs)
            } else {
                format!("+{}|", padded.join("|"))
            };
        }
        if let (Some(index), Some(cells)) = (separator_index, separator_cells) {
            let padded = cells
                .iter()
                .enumerate()
                .map(|(column, cell)| {
                    format!(
                        "{cell}{}",
                        " ".repeat(widths[column] - table_written_width(session, cell))
                    )
                })
                .collect::<Vec<_>>();
            rows[index] = format!("|{}|", padded.join("|"));
        }
    }
    if let Some(row) = node
        .caption
        .as_ref()
        .and_then(|caption| caption_row(session, caption, ctx))
    {
        rows.push(row);
    }
    rows.join("\n")
}

/// A cell's written form: its PREFIX glued to the opening pipe, then one
/// space, then the content, then one space before the closing pipe.
///
/// The prefix has to touch the pipe - a space in front of `=` or of an
/// attribute block makes it literal content - but the CONTENT does not, and
/// the padded form is the readable one. It is also the safe one: the alignment
/// sigil and the attribute slot are both read GLUED off the untrimmed cell, so
/// a glued content character was handed to one of them (carve-rs#819). This
/// used to be two guards, each enumerating the characters that merge; the
/// space covers every cell without a list.
///
/// An EMPTY cell takes a single space, not two, so a column does not grow a
/// space each time the document is formatted.
pub(super) fn pad_cell(prefix: &str, content: &str) -> String {
    if content.is_empty() {
        format!("{prefix} ")
    } else {
        format!("{prefix} {content} ")
    }
}

/// Splits a rendered cell at its `cell_wrap` sentinels: the part before the
/// first stays on the standard row; each later part fills its named column
/// on the continuation line it names.
pub(super) fn place_cell_wraps(
    rendered: &str,
    span: bool,
    row_index: usize,
    cell_index: usize,
    continuations: &mut [Vec<Vec<String>>],
    occupied: &mut [Vec<Vec<bool>>],
) -> Result<String, &'static str> {
    let Some(first) = rendered.find('\u{2}') else {
        return Ok(rendered.to_string());
    };
    let own = if span {
        rendered[..first].to_string()
    } else {
        format!("{} ", &rendered[..first])
    };
    let mut rest = &rendered[first..];
    while let Some(tail) = rest.strip_prefix('\u{2}') {
        let close = tail
            .find('\u{3}')
            .ok_or("cell_wrap sentinel is not closed")?;
        let (row, line, col) = tail[..close]
            .split_once(',')
            .and_then(|(row, tail)| {
                let (line, col) = tail.split_once(',')?;
                Some((
                    row.parse::<usize>().ok()?,
                    line.parse::<usize>().ok()?,
                    col.parse::<usize>().ok()?,
                ))
            })
            .ok_or("cell_wrap sentinel is malformed")?;
        let body = &tail[close + 1..];
        let next = body.find('\u{2}');
        let part = &body[..next.unwrap_or(body.len())];
        let part = if next.is_none() {
            part.strip_suffix(' ').unwrap_or(part)
        } else {
            part
        };
        let slots = line
            .checked_sub(1)
            .and_then(|line| continuations.get_mut(row_index + row)?.get_mut(line))
            .filter(|slots| cell_index < slots.len())
            .ok_or("cell_wrap points outside its continuation row")?;
        let slot = cell_index
            .checked_add(col)
            .filter(|&slot| slot < slots.len())
            .ok_or("cell_wrap points outside its continuation row")?;
        let used = &mut occupied[row_index + row][line - 1][slot];
        if *used {
            return Err("cell_wrap continuation cell is occupied or out of bounds");
        }
        *used = true;
        slots[slot] = pad_cell("", part);
        rest = &body[part.len()..];
        rest = rest
            .strip_prefix(' ')
            .filter(|_| next.is_none())
            .unwrap_or(rest);
        if next.is_none() {
            break;
        }
    }
    Ok(own)
}

pub(super) fn render_table_row(cells: &[String], attrs: &str) -> String {
    format!("|{}|{}", cells.join("|"), attrs)
}

pub(super) fn render_table_separator(table: &Table, widths: &[u64]) -> Option<String> {
    let fail = || {
        crate::render_carve_error::record_unspellable(
            "table",
            "separator row exceeds source allocation capacity",
        );
        None
    };
    let Some(capacity) = widths
        .iter()
        .enumerate()
        .try_fold(2usize, |total, (index, width)| {
            let dashes = usize::try_from((*width).max(1)).ok()?;
            let colons = match table.rows[0].cells.get(index).and_then(|cell| cell.align) {
                Some(TableAlign::Center) => 2,
                Some(_) => 1,
                None => 0,
            };
            total
                .checked_add(dashes)?
                .checked_add(colons)?
                .checked_add(usize::from(index > 0))
        })
    else {
        return fail();
    };
    let mut separator = String::new();
    if separator.try_reserve_exact(capacity).is_err() {
        return fail();
    }
    separator.push('|');
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            separator.push('|');
        }
        let align = table.rows[0].cells.get(index).and_then(|cell| cell.align);
        if matches!(align, Some(TableAlign::Left | TableAlign::Center)) {
            separator.push(':');
        }
        separator.extend(std::iter::repeat('-').take((*width).max(1) as usize));
        if matches!(align, Some(TableAlign::Right | TableAlign::Center)) {
            separator.push(':');
        }
    }
    separator.push('|');
    Some(separator)
}

pub(super) fn cell_blocks_have_thematic_break(blocks: &[BlockNode]) -> bool {
    fn push_table<'a>(pending: &mut Vec<&'a BlockNode>, table: &'a Table) {
        for row in &table.rows {
            for cell in &row.cells {
                if let Some(blocks) = &cell.blocks {
                    pending.extend(blocks);
                }
            }
        }
    }

    let mut pending: Vec<_> = blocks.iter().collect();
    while let Some(block) = pending.pop() {
        match block {
            BlockNode::ThematicBreak(_) => return true,
            BlockNode::BlockQuote(node) => pending.extend(&node.children),
            BlockNode::List(node) => {
                for item in &node.items {
                    pending.extend(&item.children);
                }
            }
            BlockNode::Table(node) => push_table(&mut pending, node),
            BlockNode::Admonition(node) => pending.extend(&node.children),
            BlockNode::Directive(node) => pending.extend(&node.children),
            BlockNode::Div(node) => pending.extend(&node.children),
            BlockNode::Section(node) => pending.extend(&node.children),
            BlockNode::LineBlock(node) => pending.extend(&node.children),
            BlockNode::DefinitionList(node) => {
                for item in &node.items {
                    for definition in &item.definitions {
                        pending.extend(&definition.children);
                    }
                }
            }
            BlockNode::Figure(node) => match node.target.as_ref() {
                FigureTarget::BlockQuote(target) => pending.extend(&target.children),
                FigureTarget::Table(target) => push_table(&mut pending, target),
                _ => {}
            },
            BlockNode::FigureGroup(node) => pending.extend(&node.children),
            BlockNode::BlockExtension(node) => pending.push(&node.fallback),
            BlockNode::ExtensionCarrier(node) => pending.extend(&node.children),
            _ => {}
        }
    }
    false
}

pub(super) fn render_table_cell(
    session: &RenderSession,
    cell: &TableCell,
    ctx: &mut CarveContext,
    mark_header: bool,
    suppress_align: bool,
    header_default_without_delimiter: bool,
    has_continuations: bool,
) -> String {
    if cell.blocks.is_some() && !cell.children.is_empty() {
        crate::render_carve_error::record_unspellable(
            "table_cell",
            "a cell cannot carry both inline children and blocks",
        );
    }
    let attrs = render_attrs(&cell.attrs);
    // A lone span marker keeps a space before it. Every padding form has the
    // same span reading, so the writer uses the padded form for both glyphs.
    //
    // A cell attribute stays GLUED to the pipe, where the grammar puts it; the
    // space goes between it and the marker.
    if let Some(span) = cell.span {
        let marker = if span == TableCellSpan::Rowspan {
            "^"
        } else {
            "<"
        };
        let own = pad_cell(&attrs, marker);
        if cell.children.is_empty() || !has_continuations {
            return own;
        }
        // A blocked marker keeps its continuation in its own cell. Its first
        // fragment has no cell_wrap node because the marker has no inline
        // content on the ordinary row; place it on continuation line one.
        ctx.table_cell_depth += 1;
        let content = render_inlines(session, &cell.children, ctx);
        ctx.table_cell_depth -= 1;
        let first_wrap = if matches!(cell.children.first(), Some(InlineNode::CellWrap(_))) {
            ""
        } else {
            "\u{2}0,1,0\u{3}"
        };
        return format!("{own}{first_wrap}{content}");
    }
    // CELL ATTRIBUTES BIND LAST (grammar §20 T10): the kind marker first, then
    // the alignment marker, then the attribute block, glued to the marker run.
    // Writing the block AHEAD of the markers had no spelling for an attributed
    // header cell at all -- it emitted `|{#x}=R|`, which the reader takes as a
    // data cell whose content is `=R`, so `toHtml(fmt(x)) != toHtml(x)` on
    // every attributed header cell.
    ctx.table_cell_depth += 1;
    let mut content = match &cell.blocks {
        Some(blocks) => {
            if cell_blocks_have_thematic_break(blocks) {
                crate::render_carve_error::record_unspellable(
                    "thematic_break",
                    "a thematic break has no inline spelling in a table cell",
                );
            }
            render_inlines(
                session,
                &crate::render_plain::flatten_cell_block_inlines(blocks),
                ctx,
            )
        }
        None => match merge_break_layout(&cell.children) {
            Some(merged) => render_inlines(session, &merged, ctx),
            None => render_inlines(session, &cell.children, ctx),
        },
    };
    ctx.table_cell_depth -= 1;
    content = content.replace(['\r', '\n'], " ");
    // A break at the cell's edge separates nothing, so its space is not written.
    let is_break =
        |node: &&InlineNode| matches!(node, InlineNode::SoftBreak(_) | InlineNode::HardBreak(_));
    let leading = cell.children.iter().take_while(is_break).count();
    if cell.blocks.is_some() {
        // Block content has already been flattened to a single cell line.
    } else if leading == cell.children.len() {
        content.clear();
    } else {
        let trailing = cell.children.iter().rev().take_while(is_break).count();
        content.truncate(content.len() - trailing);
        content.drain(..leading);
    }
    if !session.flatten_terminal_cell_breaks
        && cell.blocks.is_none()
        && matches!(cell.children.last(), Some(InlineNode::HardBreak(_)))
    {
        content.push('\\');
    }
    let empty = content.is_empty();
    if empty
        && header_default_without_delimiter
        && (cell.align == Some(TableAlign::Left) || cell.valign == Some(TableVerticalAlign::Top))
    {
        crate::render_carve_error::record_unspellable(
            "table_cell",
            "an empty header alignment default needs a delimiter row",
        );
    }
    let align = if suppress_align || empty && cell.align == Some(TableAlign::Left) {
        ""
    } else {
        align_marker(cell.align)
    };
    let valign = match cell.valign {
        Some(TableVerticalAlign::Top) if !empty || cell.header && !mark_header => "^",
        Some(TableVerticalAlign::Middle) => "~",
        Some(TableVerticalAlign::Bottom) => "v",
        _ => "",
    };
    let inherited_horizontal = if align.is_empty() && !valign.is_empty() {
        "?"
    } else {
        ""
    };
    let prefix = format!(
        "{}{}{}{}{}",
        if cell.header && mark_header { "=" } else { "" },
        align,
        inherited_horizontal,
        valign,
        attrs
    );
    if prefix.is_empty() && matches!(content.as_str(), "<" | "^") {
        content.insert(0, '\\');
    }
    if !cell.header && content.starts_with("\\= ") {
        content.remove(0);
    }
    // The space `pad_cell` writes after the prefix is what keeps the content's
    // first character content. The header `=` is read glued to the pipe and the
    // alignment sigil glued after it, off the UNTRIMMED cell, and a cell whose
    // text begins with an attribute block used to hand that block to the
    // reader as the CELL's attributes: `| ~x~ |` came back as `|=~x~|`, a
    // CENTERED column holding `x~` (carve-rs#819). Padding every cell parts
    // them without enumerating which characters merge. A bare span glyph
    // still needs the escape above.
    pad_cell(&prefix, &content)
}
