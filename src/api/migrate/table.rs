//! Tables: 0.1 rows and row groups to 2.0 table groups (T-007, T-037,
//! T-053, C2-A-088), as migrate/table.ts of carve-js.

use super::{Fields, at_i, coalesce_text, field, integer, joined_pos, node_list, put};
use serde_json::{Value, json};

/// `rows.slice(at, at + count)` of JavaScript, negative ends included.
fn take(rows: &[Value], at: i64, count: i64) -> Vec<Value> {
    let len = i64::try_from(rows.len()).unwrap_or(i64::MAX);
    let clamp =
        |i: i64| usize::try_from(if i < 0 { (len + i).max(0) } else { i.min(len) }).unwrap_or(0);
    let (from, to) = (clamp(at), clamp(at + count));
    rows.get(from..to.max(from)).unwrap_or(&[]).to_vec()
}

fn group(kind: &str, rows: Vec<Value>, fields: &[(&str, Option<&Value>)]) -> Option<Value> {
    let first = rows.first()?;
    let mut g = json!({"type": kind});
    if let Some(pos) = first.get("pos") {
        put(&mut g, "pos", pos.clone());
    }
    put(&mut g, "children", Value::Array(rows));
    for (name, value) in fields {
        if let Some(v) = value {
            put(&mut g, name, (*v).clone());
        }
    }
    Some(g)
}

fn declared_head_rows(old: &Fields) -> Option<i64> {
    let values = old.get("attrs")?.as_object()?.get("keyValues")?;
    let number = |v: &Value| match v {
        Value::Number(_) => integer(Some(v)),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .ok()
            .and_then(|n| integer(Some(&json!(n)))),
        _ => None,
    };
    match values {
        Value::Object(m) => number(m.get("header-rows")?),
        Value::Array(items) => {
            let pair = items
                .iter()
                .find(|p| p.is_array() && at_i(p, 0) == "header-rows")?;
            at_i(pair, 1).as_str().and(number(at_i(pair, 1)))
        }
        _ => None,
    }
}

fn explicit_groups(rows: &[Value], old: &Fields) -> Option<Vec<Value>> {
    let groups = old.get("rowGroups")?.as_object()?;
    let mut out = Vec::new();
    let mut at = 0i64;
    let heads = integer(groups.get("headRows")).unwrap_or(0);
    out.extend(group(
        "table_head",
        take(rows, at, heads),
        &[("attrs", groups.get("headAttrs"))],
    ));
    at += heads;
    for body in groups
        .get("bodies")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(b) = body.as_object() else { continue };
        let n = integer(b.get("headRows")).unwrap_or(0) + integer(b.get("bodyRows")).unwrap_or(0);
        out.extend(group(
            "table_body",
            take(rows, at, n),
            &[
                ("rowHeadColumns", b.get("rowHeadColumns")),
                ("attrs", b.get("attrs")),
            ],
        ));
        at += n;
    }
    let foots = integer(groups.get("footRows")).unwrap_or(0);
    out.extend(group(
        "table_foot",
        take(rows, at, foots),
        &[("attrs", groups.get("footAttrs"))],
    ));
    let rest = usize::try_from(at + foots).unwrap_or(0);
    if rest < rows.len() {
        out.extend(group(
            "table_body",
            rows.get(rest..).unwrap_or(&[]).to_vec(),
            &[],
        ));
    }
    Some(out)
}

fn implicit_groups(rows: &[Value]) -> Vec<Value> {
    let heads = rows
        .iter()
        .take_while(|row| {
            let cells = node_list(row.get("children"));
            let visible: Vec<&Value> = cells.iter().filter(|c| c.get("span").is_none()).collect();
            !visible.is_empty() && visible.iter().all(|c| field(c, "header") == true)
        })
        .count();
    let (head, body) = rows.split_at(heads);
    group("table_head", head.to_vec(), &[])
        .into_iter()
        .chain(group("table_body", body.to_vec(), &[]))
        .collect()
}

fn cell_paragraph(children: Vec<Value>) -> Value {
    let mut p = json!({"type": "cell_paragraph"});
    put(&mut p, "children", Value::Array(children));
    p
}

/// C2-B-172, C2-B-173: a span mark that joins nothing.
fn joins_nothing(cell: &Value, left: Option<&Value>, above: Option<&Value>) -> bool {
    match field(cell, "span").as_str() {
        Some("colspan") => left.is_none_or(|l| field(l, "span") == "rowspan"),
        Some("rowspan") => above.is_none_or(|a| {
            field(a, "span") == "colspan" || integer(a.get("colspan")).unwrap_or(1) > 1
        }),
        _ => false,
    }
}

fn normalize_spans(groups: &mut [Value]) {
    for group in groups.iter_mut() {
        let foot = field(group, "type") == "table_foot";
        let Some(rows) = group.get_mut("children").and_then(Value::as_array_mut) else {
            continue;
        };
        for r in 0..rows.len() {
            let above_row = r.checked_sub(1).and_then(|i| rows.get(i)).cloned();
            let Some(cells) = rows
                .get_mut(r)
                .and_then(|row| row.get_mut("children"))
                .and_then(Value::as_array_mut)
            else {
                continue;
            };
            for c in 0..cells.len() {
                let left = c.checked_sub(1).and_then(|i| cells.get(i)).cloned();
                let above = above_row
                    .as_ref()
                    .and_then(|row| field(row, "children").get(c))
                    .cloned();
                let Some(cell) = cells.get_mut(c).and_then(Value::as_object_mut) else {
                    continue;
                };
                if r == 0 && foot && cell.get("span").is_some_and(|s| s == "rowspan") {
                    cell.remove("span");
                    let mark = vec![json!({"type": "text", "value": "^"})];
                    cell.insert("children".into(), json!([cell_paragraph(mark)]));
                    continue;
                }
                if !joins_nothing(&Value::Object(cell.clone()), left.as_ref(), above.as_ref()) {
                    continue;
                }
                cell.remove("span");
                if node_list(cell.get("children")).is_empty() {
                    cell.insert("children".into(), json!([cell_paragraph(Vec::new())]));
                }
            }
        }
    }
}

fn recount_rowspans(groups: &mut [Value]) {
    for group in groups.iter_mut() {
        let Some(rows) = group.get_mut("children").and_then(Value::as_array_mut) else {
            continue;
        };
        let snapshot = rows.clone();
        for (at, row) in rows.iter_mut().enumerate() {
            let Some(cells) = row.get_mut("children").and_then(Value::as_array_mut) else {
                continue;
            };
            for (column, cell) in cells.iter_mut().enumerate() {
                if integer(cell.get("rowspan")).is_none() {
                    continue;
                }
                let count = 1 + snapshot
                    .get(at + 1..)
                    .unwrap_or(&[])
                    .iter()
                    .take_while(|r| {
                        field(r, "children")
                            .get(column)
                            .is_some_and(|c| field(c, "span") == "rowspan")
                    })
                    .count();
                if let Some(m) = cell.as_object_mut() {
                    if count > 1 {
                        m.insert("rowspan".into(), count.into());
                    } else {
                        m.remove("rowspan");
                    }
                }
            }
        }
    }
}

fn column(value: Option<&Value>) -> Value {
    let mut out = json!({"type": "table_column"});
    for name in ["align", "valign", "width"] {
        if let Some(v) = value.filter(|v| v.is_object()).and_then(|v| v.get(name)) {
            put(&mut out, name, v.clone());
        }
    }
    out
}

fn consume_table_attrs(node: &mut Fields) {
    const REMOVED: &[&str] = &[
        "header-rows",
        "footer-rows",
        "row-head-columns",
        "aligns",
        "widths",
        "table",
    ];
    let Some(attrs) = node.get_mut("attrs").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(Value::Array(values)) = attrs.get("keyValues") else {
        return;
    };
    let pairs: Vec<Value> = values
        .iter()
        .filter(|p| {
            p.is_array()
                && !REMOVED
                    .iter()
                    .any(|r| at_i(p, 0).as_str().is_some_and(|k| k == *r))
        })
        .cloned()
        .collect();
    if pairs.is_empty() {
        attrs.remove("keyValues");
    } else {
        attrs.insert("keyValues".into(), Value::Array(pairs));
    }
    if attrs.is_empty() {
        node.remove("attrs");
    }
}

pub(super) fn migrate_table(node: &mut Fields, old: &Fields) {
    let rows = node_list(node.get("rows"));
    let columns = node
        .get("columns")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let width = rows
        .iter()
        .map(|r| node_list(r.get("children")).len())
        .max()
        .unwrap_or(0);
    let cols: Vec<Value> = (0..width.max(columns.len()))
        .map(|i| column(columns.get(i)))
        .collect();
    node.insert("columns".into(), Value::Array(cols));
    let declared = declared_head_rows(old);
    let mut groups = explicit_groups(&rows, old).unwrap_or_else(|| {
        if declared == Some(0) && !rows.is_empty() {
            vec![json!({"type": "table_body", "children": rows.clone()})]
        } else {
            implicit_groups(&rows)
        }
    });
    normalize_spans(&mut groups);
    recount_rowspans(&mut groups);
    node.insert("children".into(), Value::Array(groups));
    for f in ["rows", "rowGroups", "sourceFormat", "separatorWidths"] {
        node.remove(f);
    }
    consume_table_attrs(node);
}

fn is_block(v: &Value) -> bool {
    matches!(
        field(v, "type").as_str(),
        Some("paragraph" | "cell_paragraph" | "list" | "block_quote" | "code_block")
    )
}

fn wrap_inline_cell(node: &mut Fields) {
    coalesce_text(node);
    let children = node_list(node.get("children"));
    if !children.is_empty() {
        node.remove("span");
    }
    if children.is_empty() && !node.contains_key("span") {
        node.insert("children".into(), json!([cell_paragraph(Vec::new())]));
    }
    if !children.is_empty() && !children.iter().any(is_block) {
        let pos = match (children.first(), children.last()) {
            (Some(a), Some(b)) if field(a, "pos").is_object() && field(b, "pos").is_object() => {
                Some(joined_pos(field(a, "pos"), field(b, "pos")))
            }
            _ => None,
        };
        let mut p = cell_paragraph(children);
        if let Some(pos) = pos {
            put(&mut p, "pos", pos);
        }
        node.insert("children".into(), json!([p]));
    }
}

pub(super) fn migrate_cell(node: &mut Fields, old: &Fields) {
    let leading = node_list(old.get("children"))
        .iter()
        .take_while(|c| field(c, "type") == "cell_wrap")
        .count();
    if leading > 0 {
        let kids = node_list(node.get("children"));
        node.insert(
            "children".into(),
            Value::Array(kids.get(leading..).unwrap_or(&[]).to_vec()),
        );
    }
    wrap_inline_cell(node);
    let blocks = node_list(node.get("blocks"));
    node.remove("blocks");
    if blocks.is_empty() {
        return;
    }
    let mut children: Vec<Value> = node_list(node.get("children"))
        .into_iter()
        .filter(|c| {
            field(c, "type") != "cell_paragraph" || !node_list(c.get("children")).is_empty()
        })
        .map(|mut c| {
            if field(&c, "type") == "cell_paragraph" {
                put(&mut c, "type", "paragraph".into());
            }
            c
        })
        .collect();
    children.extend(blocks);
    node.insert("children".into(), Value::Array(children));
    node.remove("span");
    node.remove("sourceContent");
}
