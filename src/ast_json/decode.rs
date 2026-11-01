use super::*;

pub(super) fn decode_cell_wraps(
    obj: &Map<String, Json>,
    node_type: &str,
) -> Result<Option<Vec<VerbatimCellWrap>>, AstJsonError> {
    let Some(value) = obj.get("cellWraps") else {
        return Ok(None);
    };
    let context = format!("{node_type}.cellWraps");
    value
        .expect_array(&context)?
        .iter()
        .map(|entry| {
            let entry = entry.expect_object(&context)?;
            Ok(VerbatimCellWrap {
                offset: required_usize(entry, &context, "offset")?,
                line: positive_usize(
                    Some(required_usize(entry, &context, "line")?),
                    &format!("{context}.line"),
                )?
                .unwrap_or(1),
                row: positive_usize(optional_usize(entry, "row")?, &format!("{context}.row"))?,
                col: positive_usize(optional_usize(entry, "col")?, &format!("{context}.col"))?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

pub(super) fn decode_blocks(values: &[Json]) -> Result<Vec<BlockNode>, AstJsonError> {
    values.iter().map(decode_block).collect()
}

pub(super) fn decode_inlines(values: &[Json]) -> Result<Vec<InlineNode>, AstJsonError> {
    values.iter().map(decode_inline).collect()
}

pub(super) fn decode_block(value: &Json) -> Result<BlockNode, AstJsonError> {
    let obj = value.expect_object("block node")?;
    let ty = required_string(obj, "block node", "type")?;
    match ty {
        "paragraph" => decode_block_paragraph(obj),
        "heading" => decode_block_heading(obj),
        "block_quote" => decode_block_block_quote(obj),
        "list" => decode_block_list(obj),
        "code_block" => decode_block_code_block(obj),
        "thematic_break" => decode_block_thematic_break(obj),
        "table" => decode_block_table(obj),
        "table_row" => Err(AstJsonError::new(
            "table_row is only valid inside table.rows",
        )),
        "table_cell" => Err(AstJsonError::new(
            "table_cell is only valid inside table_row.cells",
        )),
        "admonition" => decode_block_admonition(obj),
        // CARVE-P12-057. The `kind` is NOT re-checked against the six here: the
        // schema's enum is what refuses one outside them, and an engine that
        // second-guessed it would refuse a tree a later clause admits.
        "directive" => decode_block_directive(obj),
        "div" => decode_block_div(obj),
        "section" => decode_block_section(obj),
        "line_block" => decode_block_line_block(obj),
        "definition_list" => decode_block_definition_list(obj),
        "figure" => decode_block_figure(obj),
        "figure_group" => decode_block_figure_group(obj),
        "link_reference_definition" => decode_block_link_reference_definition(obj),
        "citation_definition" => decode_block_citation_definition(obj),
        "abbreviation_def" => decode_block_abbreviation_def(obj),
        "raw_block" => decode_block_raw_block(obj),
        "comment" => decode_block_comment(obj),
        "block_extension" => decode_block_block_extension(obj),
        "image" => decode_block_image(obj),
        "frontmatter" => Err(AstJsonError::new(
            "frontmatter is only valid as a document child",
        )),
        "footnote" => Err(AstJsonError::new(
            "footnote is only valid as a document child",
        )),
        other => Err(AstJsonError::new(format!(
            "unknown block node type {other:?}"
        ))),
    }
}

pub(super) fn decode_block_paragraph(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::Paragraph(Paragraph {
        attrs: optional_attrs(obj)?,
        children: decode_inlines(required_array(obj, "paragraph", "children")?)?,
        // Parse-internal and NOT on the wire: it records whether the
        // paragraph's first line sat at its container's content column, and
        // the only reader is the image-figure promotion, which runs during
        // parsing and has already run by the time anything is serialized.
        // The default is the conservative answer - `true` would be a claim
        // about the source this tree no longer has, and the one thing it
        // could still do is promote a figure the author did not write.
        at_content_column: false,
        // TRUSTED, and promotion runs only where it is ABSENT (PART 12
        // section 23). Unlike `at_content_column` above, this IS on the
        // wire: it is a resolution result the producer published, not a fact
        // about source this tree no longer has. See
        // `promote_ingested_block_images`, which fills in the absent case -
        // absence means the producer did not run the phase, not that the
        // paragraph is ordinary, so a tree is never refused for omitting it.
        block_image: optional_bool(obj, "blockImage")?.unwrap_or(false),
        pos: optional_pos(obj, "paragraph")?,
    }))
}

pub(super) fn decode_block_heading(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::Heading(Heading {
        attrs: optional_attrs(obj)?,
        level: required_usize(obj, "heading", "level")? as u8,
        children: decode_inlines(required_array(obj, "heading", "children")?)?,
        pos: optional_pos(obj, "heading")?,
    }))
}

pub(super) fn decode_block_block_quote(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::BlockQuote(BlockQuote {
        attrs: optional_attrs(obj)?,
        children: decode_blocks(required_array(obj, "block_quote", "children")?)?,
        fenced: optional_bool(obj, "fenced")?.unwrap_or(false),
        pos: optional_pos(obj, "block_quote")?,
    }))
}

pub(super) fn decode_block_list(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    let mut list = List {
        attrs: optional_attrs(obj)?,
        ordered: required_bool(obj, "list", "ordered")?,
        tight: required_bool(obj, "list", "tight")?,
        items: required_array(obj, "list", "items")?
            .iter()
            .map(decode_list_item)
            .collect::<Result<_, _>>()?,
        start: optional_usize(obj, "start")?,
        ol_type: optional_string(obj, "olType")?
            .map(decode_ol_type)
            .transpose()?,
        bare_marker: optional_bool(obj, "bareMarker")?.unwrap_or(false),
        delim: optional_marker_char(obj, "delim")?,
        bullet_char: optional_marker_char(obj, "bulletChar")?,
        pos: optional_pos(obj, "list")?,
    };
    // JSON carries effective tightness. Keep the source gap hint used by the
    // parser when a directive paragraph supplies the only loose boundary.
    if !list.tight && list.has_literal_directive_pair() {
        list.tight = true;
    }
    Ok(BlockNode::List(list))
}

pub(super) fn decode_block_code_block(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::CodeBlock(CodeBlock {
        attrs: optional_attrs(obj)?,
        lang: optional_string(obj, "lang")?.map(str::to_string),
        // `header` only: `title` is not a name the schema gives this node,
        // so the unknown-field check refused the payload before this could
        // read it. A fallback that cannot fire is a check that cannot fail
        // (carve-rs#820).
        title: optional_string(obj, "header")?.map(str::to_string),
        label: optional_string(obj, "label")?.map(str::to_string),
        content: required_string(obj, "code_block", "content")?.to_string(),
        pos: optional_pos(obj, "code_block")?,
    }))
}

pub(super) fn decode_block_thematic_break(
    obj: &Map<String, Json>,
) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::ThematicBreak(ThematicBreak {
        marker: optional_thematic_break_marker(obj)?,
        attrs: optional_attrs(obj)?,
        pos: optional_pos(obj, "thematic_break")?,
    }))
}

pub(super) fn decode_block_table(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::Table(decode_table(obj)?))
}

pub(super) fn decode_block_admonition(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::Admonition(Admonition {
        attrs: optional_attrs(obj)?,
        kind: required_string(obj, "admonition", "kind")?.to_string(),
        title: optional_inlines(obj, "title")?,
        label: optional_string(obj, "label")?.map(str::to_string),
        children: decode_blocks(required_array(obj, "admonition", "children")?)?,
        pos: optional_pos(obj, "admonition")?,
    }))
}

pub(super) fn decode_block_directive(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::Directive(Directive {
        attrs: optional_attrs(obj)?,
        kind: required_string(obj, "directive", "kind")?.to_string(),
        title: optional_inlines(obj, "title")?,
        label: optional_string(obj, "label")?.map(str::to_string),
        children: decode_blocks(required_array(obj, "directive", "children")?)?,
        pos: optional_pos(obj, "directive")?,
    }))
}

pub(super) fn decode_block_div(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::Div(Div {
        attrs: optional_attrs(obj)?,
        label: optional_string(obj, "label")?.map(str::to_string),
        children: decode_blocks(required_array(obj, "div", "children")?)?,
        pos: optional_pos(obj, "div")?,
    }))
}

pub(super) fn decode_block_section(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::Section(Section {
        attrs: optional_attrs(obj)?,
        level: optional_usize(obj, "level")?
            .map(|level| {
                u8::try_from(level)
                    .ok()
                    .filter(|level| (1..=6).contains(level))
                    .ok_or_else(|| AstJsonError::new("section.level must be between 1 and 6"))
            })
            .transpose()?,
        children: decode_blocks(required_array(obj, "section", "children")?)?,
        pos: optional_pos(obj, "section")?,
    }))
}

pub(super) fn decode_block_line_block(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::LineBlock(LineBlock {
        attrs: optional_attrs(obj)?,
        children: decode_blocks(required_array(obj, "line_block", "children")?)?,
        lines: decode_line_block_lines(obj)?,
        pos: optional_pos(obj, "line_block")?,
    }))
}

pub(super) fn decode_block_definition_list(
    obj: &Map<String, Json>,
) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::DefinitionList(DefinitionList {
        attrs: optional_attrs(obj)?,
        items: decode_definition_entries(required_array(obj, "definition_list", "items")?)?,
        // PART 12 §8 types this `const: true`, so `false` is spelled by
        // ABSENCE and an explicit `false` is not the default arriving the
        // long way - it is a value the schema does not name.
        loose: optional_bool(obj, "loose")?.unwrap_or(false),
        pos: optional_pos(obj, "definition_list")?,
    }))
}

pub(super) fn decode_block_figure(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::Figure(Figure {
        attrs: optional_attrs(obj)?,
        target: Box::new(decode_figure_target(required_value(
            obj, "figure", "target",
        )?)?),
        rendered_target: None,
        caption: decode_inlines(required_array(obj, "figure", "caption")?)?,
        short_caption: optional_inlines(obj, "shortCaption")?,
        pos: optional_pos(obj, "figure")?,
    }))
}

pub(super) fn decode_block_figure_group(
    obj: &Map<String, Json>,
) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::FigureGroup(FigureGroup {
        attrs: optional_attrs(obj)?,
        children: decode_blocks(required_array(obj, "figure_group", "children")?)?,
        caption: optional_inlines(obj, "caption")?,
        pos: optional_pos(obj, "figure_group")?,
    }))
}

pub(super) fn decode_block_link_reference_definition(
    obj: &Map<String, Json>,
) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::LinkReferenceDefinition(
        LinkReferenceDefinition {
            label: required_string(obj, "link_reference_definition", "label")?.to_string(),
            href: required_string(obj, "link_reference_definition", "href")?.to_string(),
            title: optional_string(obj, "title")?.map(str::to_string),
            attrs: optional_attrs(obj)?,
            pos: optional_pos(obj, "link_reference_definition")?,
        },
    ))
}

pub(super) fn decode_block_citation_definition(
    obj: &Map<String, Json>,
) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::CitationDefinition(CitationDefinition {
        key: required_string(obj, "citation_definition", "key")?.to_string(),
        children: decode_inlines(required_array(obj, "citation_definition", "children")?)?,
        attrs: optional_attrs(obj)?,
        pos: optional_pos(obj, "citation_definition")?,
    }))
}

pub(super) fn decode_block_abbreviation_def(
    obj: &Map<String, Json>,
) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::AbbreviationDef(AbbreviationDef {
        abbr: required_string(obj, "abbreviation_def", "abbr")?.to_string(),
        expansion: required_string(obj, "abbreviation_def", "expansion")?.to_string(),
        pos: optional_pos(obj, "abbreviation_def")?,
    }))
}

pub(super) fn decode_block_raw_block(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::RawBlock(RawBlock {
        attrs: optional_attrs(obj)?,
        format: required_string(obj, "raw_block", "format")?.to_string(),
        content: required_string(obj, "raw_block", "content")?.to_string(),
        pos: optional_pos(obj, "raw_block")?,
    }))
}

pub(super) fn decode_block_comment(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::Comment(Comment {
        block: required_bool(obj, "comment", "block")?,
        delimited: optional_bool(obj, "delimited")?.unwrap_or(false),
        content: required_string(obj, "comment", "content")?.to_string(),
        pos: optional_pos(obj, "comment")?,
    }))
}

pub(super) fn comment_form(comment: &Comment) -> &'static str {
    if comment.block {
        "block"
    } else if comment.delimited {
        "braced"
    } else {
        "line"
    }
}

pub(super) fn decode_block_block_extension(
    obj: &Map<String, Json>,
) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::BlockExtension(BlockExtension {
        name: required_string(obj, "block_extension", "name")?.to_string(),
        version: optional_string(obj, "version")?.map(str::to_string),
        fallback: Box::new(decode_block(required_value(
            obj,
            "block_extension",
            "fallback",
        )?)?),
        payload: optional_payload(obj)?,
        attrs: optional_attrs(obj)?,
        pos: optional_pos(obj, "block_extension")?,
    }))
}

pub(super) fn decode_block_image(obj: &Map<String, Json>) -> Result<BlockNode, AstJsonError> {
    Ok(BlockNode::BlockImage(decode_image(obj)?))
}

/// A block extension's payload.
///
/// `format` is REQUIRED even though the payload itself is optional: a reader
/// cannot tell whether it can parse `value` at all without it. `value` is kept as
/// JSON text - opaque, and never walked. See `refuse_unknown_fields`, which does
/// not descend into it for the same reason.
pub(super) fn optional_payload(
    obj: &Map<String, Json>,
) -> Result<Option<ExtensionPayload>, AstJsonError> {
    let Some(value) = obj.get("payload") else {
        return Ok(None);
    };
    let payload = value.expect_object("block_extension.payload")?;
    Ok(Some(ExtensionPayload {
        format: required_string(payload, "block_extension.payload", "format")?.to_string(),
        value: payload.get("value").map(Json::to_string),
    }))
}

pub(super) fn decode_list_item(value: &Json) -> Result<ListItem, AstJsonError> {
    let obj = value.expect_object("list_item")?;
    expect_type(obj, "list_item")?;
    let checked = optional_bool(obj, "checked")?;
    Ok(ListItem {
        attrs: optional_attrs(obj)?,
        checked,
        task_state: optional_task_state(obj, checked)?,
        children: decode_blocks(required_array(obj, "list_item", "children")?)?,
        pos: optional_pos(obj, "list_item")?,
    })
}

pub(super) fn decode_table(obj: &Map<String, Json>) -> Result<Table, AstJsonError> {
    let rows: Vec<TableRow> = required_array(obj, "table", "rows")?
        .iter()
        .map(decode_table_row)
        .collect::<Result<_, _>>()?;
    let row_groups = match obj.get("rowGroups") {
        Some(value) => Some(Box::new(decode_row_groups(value, rows.len())?)),
        None => None,
    };
    let source_format = optional_string(obj, "sourceFormat")?
        .map(|format| {
            TableSourceFormat::from_name(format).ok_or_else(|| {
                AstJsonError::new("table.sourceFormat must be csv or tsv".to_owned())
            })
        })
        .transpose()?;
    Ok(Table {
        source_format,
        attrs: optional_attrs(obj)?,
        caption: optional_inlines(obj, "caption")?,
        short_caption: optional_inlines(obj, "shortCaption")?,
        columns: match obj.get("columns") {
            Some(Json::Array(values)) => values
                .iter()
                .map(decode_table_column)
                .collect::<Result<_, _>>()?,
            Some(_) => {
                return Err(AstJsonError::new(
                    "table.columns must be an array".to_owned(),
                ))
            }
            None => Vec::new(),
        },
        rows,
        row_groups,
        separator_widths: decode_separator_widths(obj)?,
        pos: optional_pos(obj, "table")?,
    })
}

/// `separatorWidths` is a nonempty array of positive integers (PART 12 §3).
pub(super) fn decode_separator_widths(
    obj: &Map<String, Json>,
) -> Result<Option<Vec<u64>>, AstJsonError> {
    let entries = match obj.get("separatorWidths") {
        None => return Ok(None),
        Some(Json::Array(entries)) if !entries.is_empty() => entries,
        Some(_) => {
            return Err(AstJsonError::new(
                "table.separatorWidths must be a nonempty array".to_owned(),
            ))
        }
    };
    entries
        .iter()
        .map(|entry| {
            entry.as_u64().filter(|v| *v >= 1).ok_or_else(|| {
                AstJsonError::new(
                    "table.separatorWidths[] must be a positive integer representable as u64"
                        .to_owned(),
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

pub(super) fn decode_table_column(value: &Json) -> Result<TableColumn, AstJsonError> {
    let obj = value.expect_object("table.columns[]")?;
    let width = match obj.get("width") {
        Some(Json::Number(v)) if v.as_f64().is_some_and(|v| v > 0.0 && v <= 1.0) => v.as_f64(),
        Some(_) => {
            return Err(AstJsonError::new(
                "table.columns[].width must be in (0, 1]".to_owned(),
            ))
        }
        None => None,
    };
    Ok(TableColumn {
        align: optional_string(obj, "align")?
            .map(decode_table_align)
            .transpose()?,
        valign: optional_string(obj, "valign")?
            .map(decode_table_valign)
            .transpose()?,
        width,
    })
}

/// PART 12 §15's partition MUST, checked HERE because here it can fail.
///
/// The counts have to account for every row exactly once. JSON Schema cannot
/// express a sum across fields, so the schema does not check it and a
/// non-summing partition validates; the importer cannot check it either, because
/// there the counts and the rows are built from the same list. A payload
/// arriving from elsewhere is the one place the two can disagree.
pub(super) fn decode_row_groups(value: &Json, rows: usize) -> Result<TableRowGroups, AstJsonError> {
    let obj = value.expect_object("table.rowGroups")?;
    let head_rows = required_usize(obj, "table.rowGroups", "headRows")?;
    let foot_rows = required_usize(obj, "table.rowGroups", "footRows")?;
    let bodies = required_array(obj, "table.rowGroups", "bodies")?
        .iter()
        .map(|body| {
            let body = body.expect_object("table.rowGroups.bodies[]")?;
            Ok(TableBodyGroup {
                head_rows: required_usize(body, "table.rowGroups.bodies[]", "headRows")?,
                body_rows: required_usize(body, "table.rowGroups.bodies[]", "bodyRows")?,
                row_head_columns: optional_usize(body, "rowHeadColumns")?,
                attrs: optional_attrs(body)?,
            })
        })
        .collect::<Result<Vec<_>, AstJsonError>>()?;
    // CHECKED, because every one of these is a number off untrusted JSON and
    // `as_usize` bounds none of them. Two counts near `usize::MAX` wrap to a
    // small total in release and panic in debug, so the sum could both abort the
    // process and, wrapped, ACCEPT a partition that consumes nothing.
    let counted = bodies
        .iter()
        .try_fold(head_rows, |total, body| {
            total
                .checked_add(body.head_rows)?
                .checked_add(body.body_rows)
        })
        .and_then(|total| total.checked_add(foot_rows));
    let Some(counted) = counted else {
        return Err(AstJsonError::new(
            "table.rowGroups does not partition the table's rows: its counts do not add up to a number of rows (PART 12 §15)".to_owned(),
        ));
    };
    if counted != rows {
        return Err(AstJsonError::new(format!(
            "table.rowGroups does not partition the table's rows: the head, bodies and foot account for {counted} row{} of {rows} (PART 12 §15)",
            if counted == 1 { "" } else { "s" }
        )));
    }
    Ok(TableRowGroups {
        head_attrs: optional_named_attrs(obj, "headAttrs")?,
        foot_attrs: optional_named_attrs(obj, "footAttrs")?,
        head_rows,
        bodies,
        foot_rows,
    })
}

pub(super) fn decode_table_row(value: &Json) -> Result<TableRow, AstJsonError> {
    let obj = value.expect_object("table_row")?;
    expect_type(obj, "table_row")?;
    Ok(TableRow {
        cells: required_array(obj, "table_row", "cells")?
            .iter()
            .map(decode_table_cell)
            .collect::<Result<_, _>>()?,
        attrs: optional_attrs(obj)?,
        continuation_lines: positive_usize(
            optional_usize(obj, "continuationLines")?,
            "table_row.continuationLines",
        )?,
        pos: optional_pos(obj, "table_row")?,
    })
}

pub(super) fn decode_table_cell(value: &Json) -> Result<TableCell, AstJsonError> {
    let obj = value.expect_object("table_cell")?;
    expect_type(obj, "table_cell")?;
    if obj.contains_key("children") == obj.contains_key("blocks") {
        return Err(AstJsonError::new(
            "table_cell requires exactly one of children or blocks",
        ));
    }
    Ok(TableCell {
        // PART 12 §26: an ingested count WINS and is not recomputed. An importer
        // may have resolved it from markers this engine never saw, which is how
        // §23 settles the same question for `blockImage`.
        colspan: positive_usize(optional_usize(obj, "colspan")?, "table_cell.colspan")?
            .filter(|count| *count != 1),
        rowspan: positive_usize(optional_usize(obj, "rowspan")?, "table_cell.rowspan")?
            .filter(|count| *count != 1),
        header: required_bool(obj, "table_cell", "header")?,
        span: optional_string(obj, "span")?
            .map(decode_cell_span)
            .transpose()?,
        align: optional_string(obj, "align")?
            .map(decode_table_align)
            .transpose()?,
        valign: optional_string(obj, "valign")?
            .map(decode_table_valign)
            .transpose()?,
        attrs: optional_attrs(obj)?,
        children: optional_inlines(obj, "children")?.unwrap_or_default(),
        blocks: obj
            .get("blocks")
            .map(|value| decode_blocks(value.expect_array("table_cell.blocks")?))
            .transpose()?,
        pos: optional_pos(obj, "table_cell")?,
    })
}

/// The flat `<dt>` / `<dd>` sequence back to this engine's grouped entries.
///
/// The grouping rule is the renderer's, which is the only one all three engines
/// agree on: a run of terms opens an entry, the descriptions after it belong to
/// it, and the next term after a description starts the next entry.
///
/// A payload in the OLD `{terms, definitions}` form still decodes - trees in
/// that shape are stored, and this engine wrote them.
pub(super) fn decode_definition_entries(
    values: &[Json],
) -> Result<Vec<DefinitionItem>, AstJsonError> {
    let mut items: Vec<DefinitionItem> = Vec::new();

    for value in values {
        let obj = value.expect_object("definition_list.items[]")?;
        if obj.contains_key("terms") || obj.contains_key("definitions") {
            items.push(decode_definition_item(value)?);
            continue;
        }

        let ty = required_string(obj, "definition_list.items[]", "type")?;
        match ty {
            "definition_term" => {
                let term = DefinitionTerm {
                    attrs: optional_attrs(obj)?,
                    children: decode_inlines(required_array(obj, "definition_term", "children")?)?,
                    pos: optional_pos(obj, "definition_term")?,
                };
                let start_new = items
                    .last()
                    .map(|item| !item.definitions.is_empty())
                    .unwrap_or(true);
                if start_new {
                    items.push(DefinitionItem {
                        terms: Vec::new(),
                        definitions: Vec::new(),
                        pos: None,
                    });
                }
                items
                    .last_mut()
                    .expect("an entry was just pushed")
                    .terms
                    .push(term);
            }
            "definition_description" => {
                let definition = DefinitionDef {
                    attrs: optional_attrs(obj)?,
                    children: decode_blocks(required_array(
                        obj,
                        "definition_description",
                        "children",
                    )?)?,
                    pos: optional_pos(obj, "definition_description")?,
                };
                if items.is_empty() {
                    // A description with no term before it: the parser cannot
                    // produce one, a hand-built payload can, and dropping it
                    // would lose content the caller handed us.
                    items.push(DefinitionItem {
                        terms: Vec::new(),
                        definitions: Vec::new(),
                        pos: None,
                    });
                }
                items
                    .last_mut()
                    .expect("an entry exists")
                    .definitions
                    .push(definition);
            }
            other => {
                return Err(AstJsonError::new(format!(
                "a definition list holds definition_term and definition_description, not {other}"
            )))
            }
        }
    }

    Ok(items)
}

pub(super) fn decode_definition_item(value: &Json) -> Result<DefinitionItem, AstJsonError> {
    let obj = value.expect_object("definition_item")?;
    let terms = required_array(obj, "definition_item", "terms")?
        .iter()
        .map(|value| {
            Ok(DefinitionTerm {
                attrs: None,
                children: decode_inlines(value.expect_array("definition_item.terms[]")?)?,
                pos: None,
            })
        })
        .collect::<Result<_, AstJsonError>>()?;
    let definitions = required_array(obj, "definition_item", "definitions")?
        .iter()
        .map(|value| {
            Ok(DefinitionDef {
                attrs: None,
                children: decode_blocks(value.expect_array("definition_item.definitions[]")?)?,
                pos: None,
            })
        })
        .collect::<Result<_, AstJsonError>>()?;
    Ok(DefinitionItem {
        terms,
        definitions,
        pos: None,
    })
}

pub(super) fn decode_figure_target(value: &Json) -> Result<FigureTarget, AstJsonError> {
    let obj = value.expect_object("figure.target")?;
    match required_string(obj, "figure.target", "type")? {
        "image" => Ok(FigureTarget::Image(decode_image(obj)?)),
        "block_quote" => match decode_block(value)? {
            BlockNode::BlockQuote(n) => Ok(FigureTarget::BlockQuote(n)),
            _ => unreachable!(),
        },
        "table" => Ok(FigureTarget::Table(decode_table(obj)?)),
        "code_block" => match decode_block(value)? {
            BlockNode::CodeBlock(n) => Ok(FigureTarget::CodeBlock(n)),
            _ => unreachable!(),
        },
        "paragraph" => match decode_block(value)? {
            BlockNode::Paragraph(n) => Ok(FigureTarget::Paragraph(n)),
            _ => unreachable!(),
        },
        other => Err(AstJsonError::new(format!(
            "unknown figure.target node type {other:?}"
        ))),
    }
}

pub(super) fn decode_inline(value: &Json) -> Result<InlineNode, AstJsonError> {
    let obj = value.expect_object("inline node")?;
    let ty = required_string(obj, "inline node", "type")?;
    match ty {
        "text" => decode_inline_text(obj),
        "escaped_text" => decode_inline_escaped_text(obj),
        "smart_punctuation" => decode_inline_smart_punctuation(obj),
        "emphasis" | "strong" | "underline" | "strike" | "superscript" | "subscript"
        | "highlight" | "small_caps" => decode_inline_emphasis(ty, obj),
        "code" => decode_inline_code(obj),
        "link" => decode_inline_link(obj),
        "image" => Ok(InlineNode::Image(decode_image(obj)?)),
        "span" => decode_inline_span(obj),
        "ruby" => decode_inline_ruby(obj),
        "math" => decode_inline_math(obj),
        "raw_inline" => decode_inline_raw_inline(obj),
        "literal_inline" => decode_inline_literal_inline(obj),
        "symbol" => decode_inline_symbol(obj),
        "autolink" => decode_inline_autolink(obj),
        "heading_ref" => decode_inline_heading_ref(obj),
        "caption_number" => decode_inline_caption_number(obj),
        "mention" => decode_inline_mention(obj),
        "tag" => decode_inline_tag(obj),
        "citation_group" => decode_inline_citation_group(obj),
        "inline_extension" => decode_inline_inline_extension(obj),
        "abbreviation" => decode_inline_abbreviation(obj),
        "footnote_ref" => decode_inline_footnote_ref(obj),
        "inline_footnote" => decode_inline_inline_footnote(obj),
        "non_breaking_space" => decode_inline_non_breaking_space(obj),
        "soft_break" => decode_inline_soft_break(obj),
        "cell_wrap" => decode_inline_cell_wrap(obj),
        "hard_break" => decode_inline_hard_break(obj),
        "insert" => decode_inline_insert(obj),
        "delete" => decode_inline_delete(obj),
        "substitution" => decode_inline_substitution(obj),
        "critic_comment" => decode_inline_critic_comment(obj),
        // The inline half of `comment`. The block half decodes in the block
        // table above; `block` says which one a payload names.
        "comment" => decode_inline_comment(obj),
        other => Err(AstJsonError::new(format!(
            "unknown inline node type {other:?}"
        ))),
    }
}

pub(super) fn decode_inline_text(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Text(Text {
        value: required_string(obj, "text", "value")?.to_string(),
        pos: optional_pos(obj, "text")?,
    }))
}

pub(super) fn decode_inline_escaped_text(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::EscapedText(EscapedText {
        value: required_string(obj, "escaped_text", "value")?.to_string(),
        pos: optional_pos(obj, "escaped_text")?,
    }))
}

pub(super) fn decode_inline_smart_punctuation(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::SmartPunctuation(SmartPunctuation {
        kind: required_string(obj, "smart_punctuation", "kind")?.to_string(),
        value: required_string(obj, "smart_punctuation", "value")?.to_string(),
        glyph: optional_string(obj, "glyph")?.map(str::to_string),
        pos: optional_pos(obj, "smart_punctuation")?,
    }))
}

pub(super) fn decode_inline_emphasis(
    ty: &str,
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    let kind = decode_emphasis_kind(ty, obj)?;
    let mut children = decode_inlines(required_array(obj, ty, "children")?)?;
    // The combined form is ONE node here and TWO on the wire: a
    // `strong` marked `boldItalic` wrapping an `emphasis` (PART 11 §6).
    // Unwrap that single emphasis child on the way in, or the kind and
    // the child both add italic and `/*x*/` round-trips to
    // `<strong><em><em>` (#513).
    if kind == EmphasisKind::BoldItalic && children.len() == 1 {
        if let InlineNode::Emphasis(inner) = &children[0] {
            if inner.kind == EmphasisKind::Italic && inner.attrs.is_none() {
                children = inner.children.clone();
            }
        }
    }
    Ok(InlineNode::Emphasis(Emphasis {
        attrs: optional_attrs(obj)?,
        kind,
        children,
        pos: optional_pos(obj, ty)?,
    }))
}

pub(super) fn decode_inline_code(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Code(Code {
        cell_wraps: decode_cell_wraps(obj, "code")?,
        value: required_string(obj, "code", "value")?.to_string(),
        attrs: optional_attrs(obj)?,
        pos: optional_pos(obj, "code")?,
    }))
}

pub(super) fn decode_inline_link(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Link(Link {
        attrs: optional_attrs(obj)?,
        href: required_string(obj, "link", "href")?.to_string(),
        title: optional_string(obj, "title")?.map(str::to_string),
        children: decode_inlines(required_array(obj, "link", "children")?)?,
        ref_label: optional_string(obj, "ref")?.map(str::to_string),
        raw_ref: optional_string(obj, "rawRef")?.map(str::to_string),
        // Neither flag is on the wire: both are a writer's concern, and a
        // decoded document rebuilds them from the heading index the same way
        // a parse does. Reading `fromCrossref` here could never fire anyway -
        // `refuse_unknown_fields` turns the whole payload away first
        // (carve-rs#776).
        from_crossref: false,
        from_heading_reference: false,
        pos: optional_pos(obj, "link")?,
        resolved_empty_reference: optional_resolved(obj, "link", "href")?,
    }))
}

pub(super) fn decode_inline_span(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Span(Span {
        // Ingested content is AUTHORED; `injected` is this crate's own
        // render-time bookkeeping and never a field of the payload.
        injected: false,
        attrs: optional_attrs(obj)?,
        children: decode_inlines(required_array(obj, "span", "children")?)?,
        pos: optional_pos(obj, "span")?,
    }))
}

pub(super) fn decode_inline_ruby(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    let pairs = required_array(obj, "ruby", "pairs")?;
    if pairs.is_empty() {
        return Err(AstJsonError::new("ruby.pairs must not be empty"));
    }
    let pairs = pairs
        .iter()
        .map(|value| {
            let pair = value.expect_object("ruby.pairs[]")?;
            let base = decode_inlines(required_array(pair, "ruby.pairs[]", "base")?)?;
            if base.is_empty() {
                return Err(AstJsonError::new("ruby.pairs[].base must not be empty"));
            }
            Ok(RubyPair {
                base,
                annotation: decode_inlines(required_array(pair, "ruby.pairs[]", "annotation")?)?,
            })
        })
        .collect::<Result<Vec<_>, AstJsonError>>()?;
    Ok(InlineNode::Ruby(Ruby {
        attrs: optional_attrs(obj)?,
        pairs,
        pos: optional_pos(obj, "ruby")?,
    }))
}

pub(super) fn decode_inline_math(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    let display = required_bool(obj, "math", "display")?;
    let label = optional_string(obj, "label")?.map(str::to_string);
    if let Some(label) = &label {
        // CARVE-P12-051 pins the label's shape in the schema itself:
        // non-empty, and no leading or trailing whitespace, so it is the
        // same plain-text counter key PART 2 derives from a caption label.
        if label.is_empty() || label.trim() != label.as_str() {
            return Err(AstJsonError::new(
                "math.label is a non-empty string with no leading or trailing whitespace",
            ));
        }
    }
    let number = optional_usize(obj, "number")?;
    if let Some(number) = number {
        // A number without a label has no counter bucket and no visible
        // prefix, and inline math never draws one (CARVE-P12-051).
        if number == 0 {
            return Err(AstJsonError::new("math.number is a positive integer"));
        }
        if label.is_none() {
            return Err(AstJsonError::new(
                "math.number needs math.label: the label is the counter bucket and the visible prefix",
            ));
        }
        if !display {
            return Err(AstJsonError::new(
                "math.number is only valid on display math: inline math draws no number",
            ));
        }
    }
    Ok(InlineNode::Math(Math {
        cell_wraps: decode_cell_wraps(obj, "math")?,
        attrs: optional_attrs(obj)?,
        display,
        content: required_string(obj, "math", "content")?.to_string(),
        label,
        number,
        pos: optional_pos(obj, "math")?,
    }))
}

pub(super) fn decode_inline_raw_inline(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::RawInline(RawInline {
        cell_wraps: decode_cell_wraps(obj, "raw_inline")?,
        format: required_string(obj, "raw_inline", "format")?.to_string(),
        content: required_string(obj, "raw_inline", "content")?.to_string(),
        // An ingested node is AUTHORED content: `injected` is a render-time
        // fact this crate sets, never a field of the payload (PART 12 §7).
        injected: false,
        pos: optional_pos(obj, "raw_inline")?,
    }))
}

pub(super) fn decode_inline_literal_inline(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::LiteralInline(LiteralInline {
        content: required_string(obj, "literal_inline", "content")?.to_string(),
        attrs: optional_attrs(obj)?,
        pos: optional_pos(obj, "literal_inline")?,
    }))
}

pub(super) fn decode_inline_symbol(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Symbol(Symbol {
        name: required_string(obj, "symbol", "name")?.to_string(),
        attrs: optional_attrs(obj)?,
        pos: optional_pos(obj, "symbol")?,
    }))
}

pub(super) fn decode_inline_autolink(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::AutoLink(AutoLink {
        attrs: optional_attrs(obj)?,
        href: required_string(obj, "autolink", "href")?.to_string(),
        text: optional_string(obj, "text")?.unwrap_or("").to_string(),
        pos: optional_pos(obj, "autolink")?,
    }))
}

pub(super) fn decode_inline_heading_ref(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::CrossRef(CrossRef {
        target: required_string(obj, "heading_ref", "target")?.to_string(),
        href: optional_string(obj, "href")?.map(str::to_string),
        pos: optional_pos(obj, "heading_ref")?,
    }))
}

pub(super) fn decode_inline_caption_number(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::CaptionNumber(CaptionNumber {
        number: optional_usize(obj, "n")?,
        pos: optional_pos(obj, "caption_number")?,
    }))
}

pub(super) fn decode_inline_mention(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Mention(Mention {
        attrs: optional_attrs(obj)?,
        user: required_string(obj, "mention", "user")?.to_string(),
        pos: optional_pos(obj, "mention")?,
    }))
}

pub(super) fn decode_inline_tag(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Tag(Tag {
        attrs: optional_attrs(obj)?,
        name: required_string(obj, "tag", "name")?.to_string(),
        pos: optional_pos(obj, "tag")?,
    }))
}

pub(super) fn decode_inline_citation_group(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::CitationGroup(CitationGroup {
        items: decode_citation_items(
            required_array(obj, "citation_group", "items")?,
            optional_string(obj, "mode")? == Some("integral"),
        )?,
        raw: required_string(obj, "citation_group", "raw")?.to_string(),
        render_mode: None,
        pos: optional_pos(obj, "citation_group")?,
    }))
}

pub(super) fn decode_inline_inline_extension(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Extension(InlineExtension {
        attrs: optional_attrs(obj)?,
        name: required_string(obj, "inline_extension", "name")?.to_string(),
        // `content` only, for the reason given at `code_block` above:
        // `children` is not a name the schema gives this node, so the
        // fallback could never be reached.
        children: decode_inlines(required_array(obj, "inline_extension", "content")?)?,
        pos: optional_pos(obj, "inline_extension")?,
    }))
}

pub(super) fn decode_inline_abbreviation(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Abbreviation(Abbreviation {
        abbr: required_string(obj, "abbreviation", "abbr")?.to_string(),
        expansion: required_string(obj, "abbreviation", "expansion")?.to_string(),
        pos: optional_pos(obj, "abbreviation")?,
    }))
}

pub(super) fn decode_inline_footnote_ref(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Footnote(Footnote {
        attrs: optional_attrs(obj)?,
        // REQUIRED since carve#2193. Every other reference node names what
        // it points at, and a `footnote_ref` with no label is a reference
        // to nothing - a shape PART 12 section 3a cannot have produced,
        // since a pre-resolve tree records what the author wrote.
        // `id` is REFUSED rather than aliased: §11's narrow exception is a MAY,
        // and this engine already refuses `footnote.id` on the definition half
        // of the same pair. Two spellings of one field is what §3 prevents.
        id: Some(required_string(obj, "footnote_ref", "label")?.to_string()),
        inline: None,
        number: optional_usize(obj, "number")?,
        // NOT read from the wire. `refId` is a rendering convention -
        // `fnref1`, the anchor an endnotes section links back to - and
        // carve#762 removed it from the schema, so a tree carrying one is
        // invalid under `additionalProperties: false`. Reading it echoed a
        // payload's value straight back out, and an inherited anchor would
        // carry the previous document's numbering (carve-rs#648). The HTML
        // renderer assigns this itself while numbering.
        ref_id: None,
        pos: optional_pos(obj, "footnote_ref")?,
    }))
}

pub(super) fn decode_inline_inline_footnote(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Footnote(Footnote {
        attrs: optional_attrs(obj)?,
        id: None,
        inline: Some(decode_inlines(required_array(
            obj,
            "inline_footnote",
            "inline",
        )?)?),
        number: optional_usize(obj, "number")?,
        // NOT read from the wire. `refId` is a rendering convention -
        // `fnref1`, the anchor an endnotes section links back to - and
        // carve#762 removed it from the schema, so a tree carrying one is
        // invalid under `additionalProperties: false`. Reading it echoed a
        // payload's value straight back out, and an inherited anchor would
        // carry the previous document's numbering (carve-rs#648). The HTML
        // renderer assigns this itself while numbering.
        ref_id: None,
        pos: optional_pos(obj, "inline_footnote")?,
    }))
}

pub(super) fn decode_inline_non_breaking_space(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::NonBreakingSpace(NonBreakingSpace {
        attrs: optional_attrs(obj)?,
        pos: optional_pos(obj, "non_breaking_space")?,
    }))
}

pub(super) fn decode_inline_soft_break(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::SoftBreak(Break {
        pos: optional_pos(obj, "soft_break")?,
    }))
}

pub(super) fn decode_inline_cell_wrap(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::CellWrap(CellWrap {
        line: positive_usize(
            Some(required_usize(obj, "cell_wrap", "line")?),
            "cell_wrap.line",
        )?
        .unwrap_or(1),
        row: positive_usize(optional_usize(obj, "row")?, "cell_wrap.row")?,
        col: positive_usize(optional_usize(obj, "col")?, "cell_wrap.col")?,
        pos: optional_pos(obj, "cell_wrap")?,
    }))
}

pub(super) fn decode_inline_hard_break(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::HardBreak(Break {
        pos: optional_pos(obj, "hard_break")?,
    }))
}

pub(super) fn decode_inline_insert(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::CriticInsert(CriticInsert {
        attrs: optional_attrs(obj)?,
        children: decode_inlines(required_array(obj, "insert", "children")?)?,
        pos: optional_pos(obj, "insert")?,
    }))
}

pub(super) fn decode_inline_delete(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::CriticDelete(CriticDelete {
        attrs: optional_attrs(obj)?,
        children: decode_inlines(required_array(obj, "delete", "children")?)?,
        pos: optional_pos(obj, "delete")?,
    }))
}

pub(super) fn decode_inline_substitution(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::CriticSubstitute(CriticSubstitute {
        old: decode_inlines(required_array(obj, "substitution", "old")?)?,
        new: decode_inlines(required_array(obj, "substitution", "new")?)?,
        pos: optional_pos(obj, "substitution")?,
    }))
}

pub(super) fn decode_inline_critic_comment(
    obj: &Map<String, Json>,
) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::CriticComment(CriticComment {
        text: required_string(obj, "critic_comment", "text")?.to_string(),
        pos: optional_pos(obj, "critic_comment")?,
    }))
}

pub(super) fn decode_inline_comment(obj: &Map<String, Json>) -> Result<InlineNode, AstJsonError> {
    Ok(InlineNode::Comment(Comment {
        block: required_bool(obj, "comment", "block")?,
        delimited: optional_bool(obj, "delimited")?.unwrap_or(false),
        content: required_string(obj, "comment", "content")?.to_string(),
        pos: optional_pos(obj, "comment")?,
    }))
}

pub(super) fn decode_image(obj: &Map<String, Json>) -> Result<Image, AstJsonError> {
    Ok(Image {
        attrs: optional_attrs(obj)?,
        src: required_string(obj, "image", "src")?.to_string(),
        alt: required_string(obj, "image", "alt")?.to_string(),
        title: optional_string(obj, "title")?.map(str::to_string),
        ref_label: optional_string(obj, "ref")?.map(str::to_string),
        raw_ref: optional_string(obj, "rawRef")?.map(str::to_string),
        pos: optional_pos(obj, "image")?,
        resolved_empty_reference: optional_resolved(obj, "image", "src")?,
    })
}

/// A group's items, with the group's own `mode` honored when NO item spells
/// one. The schema calls the group flag the shorthand every item of a
/// source-spelled group carries, so a tree that only has the summary - which
/// is every tree this engine wrote before carve-rs#1858 - still round-trips.
/// A tree whose items do spell a mode is taken at its word, mixed or not.
pub(super) fn decode_citation_items(
    values: &[Json],
    group_integral: bool,
) -> Result<Vec<Citation>, AstJsonError> {
    let mut items = values
        .iter()
        .map(decode_citation)
        .collect::<Result<Vec<_>, _>>()?;
    if group_integral && items.iter().all(|item| item.mode.is_none()) {
        for item in &mut items {
            item.mode = Some(CitationItemMode::Integral);
        }
    }
    Ok(items)
}

pub(super) fn decode_citation(value: &Json) -> Result<Citation, AstJsonError> {
    let obj = value.expect_object("citation")?;
    expect_type(obj, "citation")?;
    Ok(Citation {
        key: required_string(obj, "citation", "key")?.to_string(),
        mode: (optional_string(obj, "mode")? == Some("integral"))
            .then_some(CitationItemMode::Integral),
        prefix: optional_inlines(obj, "prefix")?,
        locator: optional_inlines(obj, "locator")?,
        locator_label: optional_string(obj, "locatorLabel")?.map(str::to_string),
        locator_value: optional_string(obj, "locatorValue")?.map(str::to_string),
        suffix: optional_inlines(obj, "suffix")?,
        suppress_author: required_bool(obj, "citation", "suppressAuthor")?,
        number: optional_usize(obj, "number")?,
        label: None,
        use_index: optional_usize(obj, "useIndex")?,
        pos: optional_pos(obj, "citation")?,
    })
}

pub(super) fn decode_emphasis_kind(
    ty: &str,
    obj: &Map<String, Json>,
) -> Result<EmphasisKind, AstJsonError> {
    Ok(match ty {
        "emphasis" => EmphasisKind::Italic,
        "strong" if optional_bool(obj, "boldItalic")?.unwrap_or(false) => EmphasisKind::BoldItalic,
        "strong" => EmphasisKind::Strong,
        "underline" => EmphasisKind::Underline,
        "strike" => EmphasisKind::Strike,
        "superscript" => EmphasisKind::Super,
        "subscript" => EmphasisKind::Sub,
        "highlight" => EmphasisKind::Highlight,
        "small_caps" => EmphasisKind::SmallCaps,
        _ => return Err(AstJsonError::new(format!("unknown emphasis type {ty:?}"))),
    })
}

pub(super) fn decode_ol_type(value: &str) -> Result<OrderedListType, AstJsonError> {
    match value {
        "a" => Ok(OrderedListType::LowerAlpha),
        "A" => Ok(OrderedListType::UpperAlpha),
        "i" => Ok(OrderedListType::LowerRoman),
        "I" => Ok(OrderedListType::UpperRoman),
        other => Err(AstJsonError::new(format!(
            "list.olType has invalid value {other:?}"
        ))),
    }
}

pub(super) fn decode_cell_span(value: &str) -> Result<TableCellSpan, AstJsonError> {
    match value {
        "rowspan" => Ok(TableCellSpan::Rowspan),
        "colspan" => Ok(TableCellSpan::Colspan),
        other => Err(AstJsonError::new(format!(
            "table_cell.span has invalid value {other:?}"
        ))),
    }
}

pub(super) fn decode_table_align(value: &str) -> Result<TableAlign, AstJsonError> {
    match value {
        "left" => Ok(TableAlign::Left),
        "right" => Ok(TableAlign::Right),
        "center" => Ok(TableAlign::Center),
        other => Err(AstJsonError::new(format!(
            "table_cell.align has invalid value {other:?}"
        ))),
    }
}

pub(super) fn decode_table_valign(value: &str) -> Result<TableVerticalAlign, AstJsonError> {
    match value {
        "top" => Ok(TableVerticalAlign::Top),
        "middle" => Ok(TableVerticalAlign::Middle),
        "bottom" => Ok(TableVerticalAlign::Bottom),
        other => Err(AstJsonError::new(format!(
            "table_cell.valign has invalid value {other:?}"
        ))),
    }
}

pub(super) fn optional_marker_char(
    obj: &Map<String, Json>,
    field: &str,
) -> Result<Option<char>, AstJsonError> {
    optional_string(obj, field)?
        .map(|s| {
            let mut chars = s.chars();
            let Some(ch) = chars.next() else {
                return Err(AstJsonError::new(format!("list.{field} cannot be empty")));
            };
            if chars.next().is_some() {
                return Err(AstJsonError::new(format!(
                    "list.{field} must be one character"
                )));
            }
            Ok(ch)
        })
        .transpose()
}

/// The schema's enum and its `if`/`then` together. The generated wire fields
/// name the property but cannot weigh a PAIR.
pub(super) fn optional_task_state(
    obj: &Map<String, Json>,
    checked: Option<bool>,
) -> Result<Option<char>, AstJsonError> {
    let Some(state) = optional_marker_char(obj, "taskState")? else {
        return Ok(None);
    };
    if !matches!(state, ' ' | 'x' | '-' | '_' | '/' | '!' | '>' | '?') {
        return Err(AstJsonError::new(format!(
            "list_item.taskState must be one of ` `, `x`, `-`, `_`, `/`, `!`, `>`, or `?`, got `{state}`"
        )));
    }
    if checked != Some(state == 'x') {
        return Err(AstJsonError::new(format!(
            "list_item.taskState `{state}` needs checked {}",
            state == 'x'
        )));
    }
    Ok(Some(state))
}

pub(super) fn optional_thematic_break_marker(
    obj: &Map<String, Json>,
) -> Result<Option<char>, AstJsonError> {
    let marker = optional_marker_char(obj, "marker")?;
    match marker {
        None | Some('-' | '*' | '_') => Ok(marker),
        Some(value) => Err(AstJsonError::new(format!(
            "thematic_break.marker must be one of `-`, `*`, or `_`, got `{value}`"
        ))),
    }
}

pub(super) fn optional_attrs(obj: &Map<String, Json>) -> Result<Option<Attrs>, AstJsonError> {
    optional_named_attrs(obj, "attrs")
}

pub(super) fn optional_named_attrs(
    obj: &Map<String, Json>,
    name: &str,
) -> Result<Option<Attrs>, AstJsonError> {
    let Some(value) = obj.get(name) else {
        return Ok(None);
    };
    let attrs_obj = value.expect_object("attrs")?;
    let mut key_values = BTreeMap::new();
    if let Some(kv) = attrs_obj.get("keyValues") {
        for (key, value) in kv.expect_object("attrs.keyValues")? {
            key_values.insert(
                key.clone(),
                value.expect_string("attrs.keyValues value")?.to_string(),
            );
        }
    }
    let classes = match attrs_obj.get("classes") {
        Some(value) => value
            .expect_array("attrs.classes")?
            .iter()
            .map(|value| value.expect_string("attrs.classes[]").map(str::to_string))
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    let order = match attrs_obj.get("order") {
        Some(value) => value
            .expect_array("attrs.order")?
            .iter()
            .map(|value| {
                let slot = value.expect_string("attrs.order[]")?;
                Ok(match slot {
                    "#id" => AttrSlot::Id,
                    ".class" => AttrSlot::Class,
                    key => AttrSlot::Key(key.to_string()),
                })
            })
            .collect::<Result<_, AstJsonError>>()?,
        None => Vec::new(),
    };
    Ok(Some(Attrs {
        id: optional_string(attrs_obj, "id")?.map(str::to_string),
        classes,
        key_values,
        order,
    }))
}

pub(super) fn optional_pos(
    obj: &Map<String, Json>,
    node_type: &str,
) -> Result<Option<Pos>, AstJsonError> {
    let Some(value) = obj.get("pos") else {
        return Ok(None);
    };
    let pos = value.expect_object("pos")?;
    Ok(Some(Pos {
        start_line: required_usize(pos, node_type, "startLine")?,
        end_line: required_usize(pos, node_type, "endLine")?,
        start_column: required_usize(pos, node_type, "startColumn")?,
        end_column: required_usize(pos, node_type, "endColumn")?,
        start_offset: required_usize(pos, node_type, "startOffset")?,
        end_offset: required_usize(pos, node_type, "endOffset")?,
        file: optional_string(pos, "file")?.map(SourceFile::new),
    }))
}

pub(super) fn optional_inlines(
    obj: &Map<String, Json>,
    field: &str,
) -> Result<Option<Vec<InlineNode>>, AstJsonError> {
    obj.get(field)
        .map(|value| decode_inlines(value.expect_array(field)?))
        .transpose()
}

pub(super) fn expect_type(obj: &Map<String, Json>, expected: &str) -> Result<(), AstJsonError> {
    let actual = required_string(obj, expected, "type")?;
    if actual == expected {
        Ok(())
    } else {
        Err(AstJsonError::new(format!(
            "{expected}.type must be {expected:?}, got {actual:?}"
        )))
    }
}

pub(super) fn required_value<'a>(
    obj: &'a Map<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<&'a Json, AstJsonError> {
    obj.get(field)
        .ok_or_else(|| AstJsonError::new(format!("{node_type}.{field} is required")))
}

pub(super) fn required_array<'a>(
    obj: &'a Map<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<&'a [Json], AstJsonError> {
    required_value(obj, node_type, field)?.expect_array(&format!("{node_type}.{field}"))
}

pub(super) fn required_string<'a>(
    obj: &'a Map<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<&'a str, AstJsonError> {
    required_value(obj, node_type, field)?.expect_string(&format!("{node_type}.{field}"))
}

pub(super) fn required_bool(
    obj: &Map<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<bool, AstJsonError> {
    required_value(obj, node_type, field)?.expect_bool(&format!("{node_type}.{field}"))
}

pub(super) fn required_usize(
    obj: &Map<String, Json>,
    node_type: &str,
    field: &str,
) -> Result<usize, AstJsonError> {
    required_value(obj, node_type, field)?.expect_usize(&format!("{node_type}.{field}"))
}

pub(super) fn optional_string<'a>(
    obj: &'a Map<String, Json>,
    field: &str,
) -> Result<Option<&'a str>, AstJsonError> {
    obj.get(field)
        .map(|value| value.expect_string(field))
        .transpose()
}

pub(super) fn optional_bool(
    obj: &Map<String, Json>,
    field: &str,
) -> Result<Option<bool>, AstJsonError> {
    obj.get(field)
        .map(|value| value.expect_bool(field))
        .transpose()
}

pub(super) fn optional_resolved(
    obj: &Map<String, Json>,
    node_type: &str,
    destination: &str,
) -> Result<bool, AstJsonError> {
    match obj.get("resolved") {
        None => Ok(false),
        Some(Json::Bool(true))
            if obj.get("ref").is_some()
                && obj.get(destination).and_then(Json::as_str) == Some("") =>
        {
            Ok(true)
        }
        _ => Err(AstJsonError::new(format!(
            "{node_type}.resolved requires true, ref, and an empty {destination}"
        ))),
    }
}

pub(super) fn positive_usize(
    value: Option<usize>,
    field: &str,
) -> Result<Option<usize>, AstJsonError> {
    if value == Some(0) {
        return Err(AstJsonError::new(format!("{field} must be at least 1")));
    }
    Ok(value)
}

pub(super) fn optional_usize(
    obj: &Map<String, Json>,
    field: &str,
) -> Result<Option<usize>, AstJsonError> {
    obj.get(field)
        .map(|value| value.expect_usize(field))
        .transpose()
}

pub(super) fn decode_line_block_lines(
    obj: &Map<String, Json>,
) -> Result<Option<Vec<Vec<String>>>, AstJsonError> {
    let Some(value) = obj.get("lines") else {
        return Ok(None);
    };
    let stanzas = value.expect_array("line_block.lines")?;
    let children = required_array(obj, "line_block", "children")?;
    if stanzas.len() != children.len() {
        return Err(AstJsonError::new(
            "line_block.lines must have one entry per child stanza",
        ));
    }
    let mut lines = Vec::with_capacity(stanzas.len());
    for (index, stanza) in stanzas.iter().enumerate() {
        let pointers = stanza.expect_array(&format!("line_block.lines[{index}]"))?;
        if pointers.is_empty() {
            return Err(AstJsonError::new(format!(
                "line_block.lines[{index}] must contain a line end"
            )));
        }
        let mut ends = Vec::with_capacity(pointers.len());
        let mut terminators = 0;
        for pointer in pointers {
            let pointer = pointer.expect_string("line_block.lines pointer")?;
            let valid = pointer.starts_with('/')
                && !pointer[1..].split('/').any(|segment| {
                    let mut chars = segment.chars();
                    while let Some(ch) = chars.next() {
                        if ch == '~' && !matches!(chars.next(), Some('0' | '1')) {
                            return true;
                        }
                    }
                    false
                });
            if !valid {
                return Err(AstJsonError::new(format!(
                    "line_block.lines[{index}] contains an invalid JSON Pointer"
                )));
            }
            if pointer == "/children/-" {
                terminators += 1;
            }
            ends.push(pointer.to_string());
        }
        if terminators != 1 || ends.last().map_or(true, |end| end != "/children/-") {
            return Err(AstJsonError::new(format!(
                "line_block.lines[{index}] must end once at /children/-"
            )));
        }
        lines.push(ends);
    }
    Ok(Some(lines))
}

/// Reading a wire value as the shape the schema pins, with the field path in
/// the message rather than a bare type mismatch. Named off `as_*` so serde_json's
/// own inherent methods cannot shadow these.
pub(super) trait Expect {
    fn expect_object(&self, context: &str) -> Result<&Map<String, Json>, AstJsonError>;
    fn expect_array(&self, context: &str) -> Result<&[Json], AstJsonError>;
    fn expect_string(&self, context: &str) -> Result<&str, AstJsonError>;
    fn expect_bool(&self, context: &str) -> Result<bool, AstJsonError>;
    fn expect_usize(&self, context: &str) -> Result<usize, AstJsonError>;
}

impl Expect for Json {
    fn expect_object(&self, context: &str) -> Result<&Map<String, Json>, AstJsonError> {
        match self {
            Json::Object(obj) => Ok(obj),
            _ => Err(AstJsonError::new(format!("{context} must be an object"))),
        }
    }

    fn expect_array(&self, context: &str) -> Result<&[Json], AstJsonError> {
        match self {
            Json::Array(values) => Ok(values),
            _ => Err(AstJsonError::new(format!("{context} must be an array"))),
        }
    }

    fn expect_string(&self, context: &str) -> Result<&str, AstJsonError> {
        match self {
            Json::String(value) => Ok(value),
            _ => Err(AstJsonError::new(format!("{context} must be a string"))),
        }
    }

    fn expect_bool(&self, context: &str) -> Result<bool, AstJsonError> {
        match self {
            Json::Bool(value) => Ok(*value),
            _ => Err(AstJsonError::new(format!("{context} must be a boolean"))),
        }
    }

    fn expect_usize(&self, context: &str) -> Result<usize, AstJsonError> {
        match self {
            Json::Number(value) if value.is_u64() => Ok(value.as_u64().unwrap_or(0) as usize),
            _ => Err(AstJsonError::new(format!(
                "{context} must be a non-negative integer"
            ))),
        }
    }
}
