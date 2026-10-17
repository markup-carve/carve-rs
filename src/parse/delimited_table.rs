use crate::ast::*;

pub(super) struct PayloadLine {
    pub start: usize,
    pub text: String,
    pub line: usize,
    pub column: usize,
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
    fn point(lines: &[PayloadLine], index: usize) -> Option<(usize, usize)> {
        lines.iter().find_map(|line| {
            let local = index.checked_sub(line.start)?;
            let prefix = line.text.get(..local)?;
            Some((line.line, line.column + prefix.chars().count()))
        })
    }
    let (start_line, start_column) = point(lines, start)?;
    let (end_line, end_column) = point(lines, end)?;
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

pub(super) fn from_code(code: &CodeBlock, payload: &[PayloadLine]) -> Option<Table> {
    let format = TableSourceFormat::from_name(code.lang.as_deref()?)?;
    let separator = format.separator();
    let mut attrs = code.attrs.clone()?;
    if !attrs.key_values.get("table").is_some_and(String::is_empty) {
        return None;
    }
    let (data, ranges) = rows_with_ranges(&code.content, separator)?;
    let width = data.iter().map(Vec::len).max().unwrap_or(0);
    let foot = count(attrs.key_values.get("footer-rows"), 0);
    let head = count(
        attrs.key_values.get("header-rows"),
        data.len().saturating_sub(foot).min(1),
    );
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
    let columns = attrs
        .as_ref()
        .map(super::table_columns_from_attrs)
        .unwrap_or_default();
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

pub(crate) fn rows(source: &str, separator: char) -> Option<Vec<Vec<String>>> {
    rows_with_ranges(source, separator).map(|(rows, _)| rows)
}

fn rows_with_ranges(source: &str, separator: char) -> Option<(Vec<Vec<String>>, Vec<RowRange>)> {
    if source.is_empty() {
        return Some((Vec::new(), Vec::new()));
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
                return None;
            }
            quoted = true;
            at_start = false;
            was_quoted = true;
        } else {
            if closed {
                return None;
            }
            field.push(ch);
            at_start = false;
        }
    }
    if quoted {
        return None;
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
    Some((records, ranges))
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
