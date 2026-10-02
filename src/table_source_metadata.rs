use crate::ast::{AttrSlot, Attrs, TableBodyGroup, TableRowGroups};

fn count(value: &str) -> Option<usize> {
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|v| v.is_ascii_digit()) {
        return None;
    }
    let parsed: usize = value.parse().ok()?;
    (parsed as u128 <= 9_007_199_254_740_991).then_some(parsed)
}

pub(crate) fn row_groups(attrs: &Attrs, rows: usize) -> Option<TableRowGroups> {
    let keys = &attrs.key_values;
    if ![
        "header-rows",
        "footer-rows",
        "body-rows",
        "body-header-rows",
        "body-header-cols",
    ]
    .iter()
    .any(|key| keys.contains_key(*key))
    {
        return None;
    }
    let edge = |key: &str| match keys.get(key) {
        None => Some(0),
        Some(value) if value.trim().is_empty() => Some(1),
        Some(value) => count(value),
    };
    let head_rows = edge("header-rows")?;
    let foot_rows = edge("footer-rows")?;
    let mut remaining = rows.checked_sub(head_rows)?.checked_sub(foot_rows)?;
    let mut bodies = Vec::new();
    if let Some(raw) = keys.get("body-rows") {
        let counts: Vec<_> = if raw.trim().is_empty() {
            vec![]
        } else {
            raw.split(',').collect()
        };
        let headers = keys
            .get("body-header-rows")
            .map(|v| v.split(',').collect::<Vec<_>>());
        let columns = keys
            .get("body-header-cols")
            .map(|v| v.split(',').collect::<Vec<_>>());
        if headers.as_ref().is_some_and(|v| v.len() != counts.len())
            || columns.as_ref().is_some_and(|v| v.len() != counts.len())
        {
            return None;
        }
        for (i, raw) in counts.iter().enumerate() {
            let body_rows = count(raw)?;
            let body_head = match &headers {
                Some(values) => count(values[i])?,
                None => 0,
            };
            let row_head_columns = match &columns {
                Some(values) if !values[i].trim().is_empty() => Some(count(values[i])?),
                _ => None,
            };
            remaining = remaining.checked_sub(body_head)?.checked_sub(body_rows)?;
            bodies.push(TableBodyGroup {
                head_rows: body_head,
                body_rows,
                row_head_columns,
                attrs: None,
            });
        }
        if remaining != 0 {
            return None;
        }
    } else {
        if keys.contains_key("body-header-rows") || keys.contains_key("body-header-cols") {
            return None;
        }
        bodies.push(TableBodyGroup {
            head_rows: 0,
            body_rows: remaining,
            row_head_columns: None,
            attrs: None,
        });
    }
    Some(TableRowGroups {
        head_rows,
        bodies,
        foot_rows,
        head_attrs: None,
        foot_attrs: None,
    })
}

pub(crate) fn add_row_groups(attrs: &mut Attrs, groups: &TableRowGroups) {
    let mut put = |key: &str, value: String| {
        if !attrs.key_values.contains_key(key) {
            attrs.key_values.insert(key.into(), value);
            attrs.order.push(AttrSlot::Key(key.into()));
        }
    };
    if groups.head_rows > 0 {
        put("header-rows", groups.head_rows.to_string());
    }
    if groups.foot_rows > 0 {
        put("footer-rows", groups.foot_rows.to_string());
    }
    let simple = groups.bodies.len() == 1
        && groups.bodies[0].head_rows == 0
        && groups.bodies[0].row_head_columns.is_none();
    if simple {
        if groups.head_rows == 0 && groups.foot_rows == 0 {
            put("header-rows", "0".into());
        }
    } else {
        put(
            "body-rows",
            groups
                .bodies
                .iter()
                .map(|b| b.body_rows.to_string())
                .collect::<Vec<_>>()
                .join(","),
        );
        if groups.bodies.iter().any(|b| b.head_rows != 0) {
            put(
                "body-header-rows",
                groups
                    .bodies
                    .iter()
                    .map(|b| b.head_rows.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        if groups.bodies.iter().any(|b| b.row_head_columns.is_some()) {
            put(
                "body-header-cols",
                groups
                    .bodies
                    .iter()
                    .map(|b| {
                        b.row_head_columns
                            .map(|v| v.to_string())
                            .unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
    }
}
