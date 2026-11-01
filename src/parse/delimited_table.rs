use crate::ast::*;

pub(crate) struct PayloadLine {
    pub start: usize,
    pub text: String,
    pub line: usize,
    pub column: usize,
    pub file_offset: Option<usize>,
}

struct FieldRange {
    start: usize,
    end: usize,
    quoted: bool,
    escaped: bool,
}

struct RowRange {
    start: usize,
    end: usize,
    fields: Vec<FieldRange>,
}

fn position(lines: &[PayloadLine], start: usize, end: usize) -> Option<Pos> {
    fn point(lines: &[PayloadLine], index: usize) -> Option<(usize, usize, usize)> {
        let line = lines.get(
            lines
                .partition_point(|line| line.start <= index)
                .checked_sub(1)?,
        )?;
        let local = index.checked_sub(line.start)?;
        let prefix = line.text.get(..local)?;
        let width = prefix.chars().count();
        Some((
            line.line,
            line.column + width,
            line.file_offset.map_or(0, |offset| offset + width),
        ))
    }
    let (start_line, start_column, start_offset) = point(lines, start)?;
    let (end_line, end_column, end_offset) = point(lines, end)?;
    Some(Pos {
        start_line,
        end_line,
        start_column,
        end_column,
        start_offset,
        end_offset,
        file: None,
    })
}

pub(crate) fn from_code(code: &CodeBlock, payload: &[PayloadLine]) -> Option<Table> {
    from_code_without(code, payload, |_| false)
}

/// As [`from_code`], leaving out every one-line record `skip` accepts.
pub(crate) fn from_code_without(
    code: &CodeBlock,
    payload: &[PayloadLine],
    skip: impl Fn(&str) -> bool,
) -> Option<Table> {
    let format = TableSourceFormat::from_name(code.lang.as_deref()?)?;
    let separator = format.separator();
    let mut attrs = code.attrs.clone()?;
    if !attrs.key_values.get("table").is_some_and(String::is_empty) {
        return None;
    }
    let (mut data, mut ranges) = rows_with_ranges(&code.content, separator)?;
    skip_records(&code.content, &mut data, &mut ranges, skip);
    let width = data.iter().map(Vec::len).max().unwrap_or(0);
    let head = effective_header_rows(&attrs, data.len());
    if attrs
        .key_values
        .get("header-rows")
        .is_some_and(|value| !is_count(value))
    {
        attrs.key_values.remove("header-rows");
        attrs
            .order
            .retain(|slot| !matches!(slot, AttrSlot::Key(key) if key == "header-rows"));
    }
    attrs.key_values.remove("table");
    attrs
        .order
        .retain(|slot| !matches!(slot, AttrSlot::Key(key) if key == "table"));
    let attrs = (attrs.id.is_some() || !attrs.classes.is_empty() || !attrs.key_values.is_empty())
        .then_some(attrs);
    let rows: Vec<TableRow> = data
        .into_iter()
        .enumerate()
        .map(|(index, fields)| {
            let empty_record = ranges[index].start == ranges[index].end;
            TableRow {
                cells: (0..width)
                    .map(|column| {
                        let range = if empty_record {
                            None
                        } else {
                            ranges[index].fields.get(column)
                        };
                        let cell_pos =
                            range.and_then(|range| position(payload, range.start, range.end));
                        let text_pos = range.filter(|range| !range.escaped).and_then(|range| {
                            let start = range.start + usize::from(range.quoted);
                            let end = range.end - usize::from(range.quoted);
                            position(payload, start, end)
                        });
                        TableCell {
                            header: index < head,
                            span: None,
                            colspan: None,
                            rowspan: None,
                            align: None,
                            valign: None,
                            attrs: None,
                            children: vec![InlineNode::Text(Text {
                                value: fields.get(column).cloned().unwrap_or_default(),
                                pos: text_pos,
                            })],
                            blocks: None,
                            pos: cell_pos,
                        }
                    })
                    .collect(),
                attrs: None,
                continuation_lines: None,
                pos: if empty_record {
                    None
                } else {
                    position(payload, ranges[index].start, ranges[index].end)
                },
            }
        })
        .collect();
    let mut columns = attrs
        .as_ref()
        .map(super::table_columns_from_attrs)
        .unwrap_or_default();
    columns.truncate(rows.iter().map(|row| row.cells.len()).max().unwrap_or(0));
    let row_groups = attrs.as_ref().and_then(|attrs| {
        if ![
            "header-rows",
            "footer-rows",
            "body-rows",
            "body-header-rows",
            "body-header-cols",
        ]
        .iter()
        .any(|key| attrs.key_values.contains_key(*key))
        {
            return None;
        }
        let mut metadata = attrs.clone();
        if !metadata.key_values.contains_key("header-rows") {
            metadata
                .key_values
                .insert("header-rows".into(), head.to_string());
        }
        crate::table_source_metadata::row_groups(&metadata, rows.len()).map(Box::new)
    });
    Some(Table {
        attrs,
        source_format: Some(format),
        caption: None,
        short_caption: None,
        columns,
        rows,
        row_groups,
        separator_widths: None,
        pos: code.pos.clone(),
    })
}

fn skip_records(
    source: &str,
    rows: &mut Vec<Vec<String>>,
    ranges: &mut Vec<RowRange>,
    skip: impl Fn(&str) -> bool,
) {
    let kept: Vec<bool> = ranges
        .iter()
        .map(|range| {
            let text = &source[range.start..range.end];
            text.contains('\n') || !skip(text)
        })
        .collect();
    if kept.contains(&false) {
        let mut keep = kept.iter();
        rows.retain(|_| *keep.next().unwrap());
        let mut keep = kept.iter();
        ranges.retain(|_| *keep.next().unwrap());
    }
}

pub(crate) fn count(raw: Option<&String>, default: usize) -> usize {
    match raw {
        None => default,
        Some(value) if value.trim().is_empty() => 1,
        Some(value) if value.trim().bytes().all(|byte| byte.is_ascii_digit()) => {
            value.trim().parse().unwrap_or(default)
        }
        Some(_) => default,
    }
}

fn is_count(value: &str) -> bool {
    let value = value.trim();
    value.is_empty()
        || (value.bytes().all(|byte| byte.is_ascii_digit()) && value.parse::<usize>().is_ok())
}

pub(crate) fn effective_header_rows(attrs: &Attrs, rows: usize) -> usize {
    let foot = count(attrs.key_values.get("footer-rows"), 0);
    let head = count(
        attrs.key_values.get("header-rows"),
        rows.saturating_sub(foot).min(1),
    );
    if attrs.key_values.contains_key("header-rows")
        && attrs.key_values.contains_key("footer-rows")
        && head.saturating_add(foot) > rows
    {
        // An explicit overlap has no partition and no header cells. A lone
        // oversized count still uses the delimited table rule (PART 9 §13).
        0
    } else {
        head
    }
}

/// Lints `source` as [`from_code_without`] reads it, leaving out the records `skip` accepts.
pub(crate) fn lint_payload(
    source: &str,
    separator: char,
    skip: impl Fn(&str) -> bool,
) -> Option<(&'static str, &'static str, usize)> {
    let (mut rows, mut ranges) = match rows_with_error(source, separator) {
        Ok(value) => value,
        Err(start) => return Some(("delimited-table-malformed", "This marked CSV/TSV record has malformed quoting, so the include remains a code block.", 1 + source[..start].bytes().filter(|b| *b == b'\n').count())),
    };
    skip_records(source, &mut rows, &mut ranges, skip);
    let width = rows.first()?.len();
    rows.iter().zip(ranges).find(|(row, _)| row.len() > width).map(|(_, range)| ("delimited-table-wide-row", "A CSV/TSV record has more fields than the first record; the table keeps the extra columns.", 1 + source[..range.start].bytes().filter(|b| *b == b'\n').count()))
}

pub(crate) fn rows(source: &str, separator: char) -> Option<Vec<Vec<String>>> {
    rows_with_ranges(source, separator).map(|(rows, _)| rows)
}

fn rows_with_ranges(source: &str, separator: char) -> Option<(Vec<Vec<String>>, Vec<RowRange>)> {
    rows_with_error(source, separator).ok()
}

fn rows_with_error(
    source: &str,
    separator: char,
) -> Result<(Vec<Vec<String>>, Vec<RowRange>), usize> {
    if source.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut records = Vec::new();
    let mut ranges = Vec::new();
    let mut row = Vec::new();
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = source.char_indices().peekable();
    let mut quoted = false;
    let mut closed = false;
    let mut at_start = true;
    let mut was_quoted = false;
    let mut escaped = false;
    let mut field_start = 0;
    let mut row_start = 0;
    while let Some((index, ch)) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek().is_some_and(|(_, next)| *next == '"') {
                    field.push('"');
                    chars.next();
                    escaped = true;
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(ch);
            }
        } else if ch == separator || ch == '\n' {
            row.push(std::mem::take(&mut field));
            fields.push(FieldRange {
                start: field_start,
                end: index,
                quoted: was_quoted,
                escaped,
            });
            closed = false;
            at_start = true;
            field_start = index + ch.len_utf8();
            was_quoted = false;
            escaped = false;
            if ch == '\n' {
                records.push(std::mem::take(&mut row));
                ranges.push(RowRange {
                    start: row_start,
                    end: index,
                    fields: std::mem::take(&mut fields),
                });
                row_start = index + 1;
            }
        } else if ch == '"' {
            if !at_start || closed {
                return Err(row_start);
            }
            quoted = true;
            at_start = false;
            was_quoted = true;
        } else {
            if closed {
                return Err(row_start);
            }
            field.push(ch);
            at_start = false;
        }
    }
    if quoted {
        return Err(row_start);
    }
    if !source.ends_with('\n') {
        row.push(field);
        fields.push(FieldRange {
            start: field_start,
            end: source.len(),
            quoted: was_quoted,
            escaped,
        });
        records.push(row);
        ranges.push(RowRange {
            start: row_start,
            end: source.len(),
            fields,
        });
    }
    Ok((records, ranges))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_records_and_invalid_quotes() {
        assert_eq!(
            rows("A,Note\nB,\"one\ntwo\"\n", ','),
            Some(vec![
                vec!["A".into(), "Note".into()],
                vec!["B".into(), "one\ntwo".into()],
            ])
        );
        assert_eq!(rows("\"unclosed\n", ','), None);
        assert_eq!(rows("a\"b,c\n", ','), None);
        assert_eq!(rows("\"a\" ,b\n", ','), None);
    }

    #[test]
    fn marks_only_csv_and_tsv() {
        let code = CodeBlock {
            attrs: Some(Attrs {
                key_values: [("table".into(), String::new())].into(),
                ..Attrs::default()
            }),
            lang: Some("tsv".into()),
            title: None,
            label: None,
            content: "A\tB\n1\t2\n".into(),
            pos: None,
        };
        let table = from_code(&code, &[]).unwrap();
        assert_eq!(
            table.source_format.map(|format| format.as_str()),
            Some("tsv")
        );
        assert!(table.rows[0].cells[0].header);
        assert!(!table.rows[1].cells[0].header);
        assert!(table.attrs.is_none());
        let mut non_boolean = code.clone();
        non_boolean
            .attrs
            .as_mut()
            .unwrap()
            .key_values
            .insert("table".into(), "x".into());
        assert!(from_code(&non_boolean, &[]).is_none());
        assert!(from_code(
            &CodeBlock {
                attrs: None,
                ..code
            },
            &[]
        )
        .is_none());
    }
}
