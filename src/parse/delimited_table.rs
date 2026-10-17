use crate::ast::*;

pub(super) fn from_code(code: &CodeBlock) -> Option<Table> {
    let format = code.lang.as_deref()?;
    let separator = match format {
        "csv" => ',',
        "tsv" => '\t',
        _ => return None,
    };
    let mut attrs = code.attrs.clone()?;
    if !attrs.key_values.contains_key("table") {
        return None;
    }
    let data = rows(&code.content, separator)?;
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
    let rows = data
        .into_iter()
        .enumerate()
        .map(|(index, fields)| TableRow {
            cells: (0..width)
                .map(|column| TableCell {
                    header: index < head,
                    span: None,
                    colspan: None,
                    rowspan: None,
                    align: None,
                    valign: None,
                    attrs: None,
                    children: vec![InlineNode::Text(Text::from(
                        fields.get(column).cloned().unwrap_or_default(),
                    ))],
                    blocks: None,
                    pos: None,
                })
                .collect(),
            attrs: None,
            continuation_lines: None,
            pos: None,
        })
        .collect();
    let columns = attrs
        .as_ref()
        .map(super::table_columns_from_attrs)
        .unwrap_or_default();
    Some(Table {
        attrs,
        source_format: Some(format.to_owned()),
        caption: None,
        short_caption: None,
        columns,
        rows,
        row_groups: None,
        separator_widths: None,
        pos: code.pos.clone(),
    })
}

fn count(raw: Option<&String>, default: usize) -> usize {
    match raw {
        None => default,
        Some(value) if value.is_empty() => 1,
        Some(value) => value.parse().unwrap_or(default),
    }
}

fn rows(source: &str, separator: char) -> Option<Vec<Vec<String>>> {
    if source.is_empty() {
        return Some(Vec::new());
    }
    let mut records = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = source.chars().peekable();
    let mut quoted = false;
    let mut closed = false;
    let mut at_start = true;
    while let Some(ch) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                    closed = true;
                }
            } else {
                field.push(ch);
            }
        } else if ch == separator || ch == '\n' {
            row.push(std::mem::take(&mut field));
            closed = false;
            at_start = true;
            if ch == '\n' {
                records.push(std::mem::take(&mut row));
            }
        } else if ch == '"' {
            if !at_start || closed {
                return None;
            }
            quoted = true;
            at_start = false;
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
        records.push(row);
    }
    Some(records)
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
        let table = from_code(&code).unwrap();
        assert_eq!(table.source_format.as_deref(), Some("tsv"));
        assert!(table.rows[0].cells[0].header);
        assert!(!table.rows[1].cells[0].header);
        assert!(table.attrs.is_none());
        assert!(from_code(&CodeBlock {
            attrs: None,
            ..code
        })
        .is_none());
    }
}
