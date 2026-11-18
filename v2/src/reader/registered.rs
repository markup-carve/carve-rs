//! C2-X-031, C2-X-038 and C2-X-046: the registered extension schema is
//! checked in addition to the tree's own declaration of slot names.

use super::{TreeError, definitions, reader};
use serde_json::Value;
use std::sync::OnceLock;

struct Visit<'a> {
    value: &'a Value,
    path: String,
    depth: usize,
    parent_slot: Option<(&'a str, &'a str)>,
}

fn catalog() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../spec-data/extensions.json")).unwrap_or(Value::Null)
    })
}

pub(super) fn validate(root: &Value) -> Result<(), TreeError> {
    let mut pending = vec![Visit {
        value: root,
        path: String::new(),
        depth: 0,
        parent_slot: None,
    }];
    while let Some(visit) = pending.pop() {
        match visit.value {
            Value::Array(items) => {
                pending.extend(items.iter().enumerate().map(|(i, value)| Visit {
                    value,
                    path: format!("{}/{i}", visit.path),
                    depth: visit.depth,
                    parent_slot: visit.parent_slot,
                }));
            }
            Value::Object(fields) => {
                let name = visit.value["name"].as_str();
                let qualified = matches!(
                    visit.value["type"].as_str(),
                    Some("inline_extension" | "block_extension")
                ) && name.is_some_and(|name| name.contains('.'));
                let depth = visit.depth + usize::from(qualified);
                if depth > 32 {
                    return Err(reader(
                        &visit.path,
                        "qualified extension nesting exceeds 32 levels",
                    ));
                }
                if qualified {
                    check_node(visit.value, &visit.path, visit.parent_slot)?;
                    push_slots(visit.value, &visit.path, depth, &mut pending);
                }
                for (key, value) in fields {
                    if matches!(key.as_str(), "slots" | "data" | "props") {
                        continue;
                    }
                    pending.push(Visit {
                        value,
                        path: format!("{}/{key}", visit.path),
                        depth,
                        parent_slot: None,
                    });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn push_slots<'a>(node: &'a Value, path: &str, depth: usize, pending: &mut Vec<Visit<'a>>) {
    let Some(name) = node["name"].as_str() else {
        return;
    };
    for (slot, items) in node["slots"]
        .as_object()
        .into_iter()
        .flat_map(|slots| slots.iter())
    {
        pending.push(Visit {
            value: items,
            path: format!("{path}/slots/{slot}"),
            depth,
            parent_slot: Some((name, slot)),
        });
    }
}

fn check_node(
    node: &Value,
    path: &str,
    parent_slot: Option<(&str, &str)>,
) -> Result<(), TreeError> {
    let name = node["name"].as_str().unwrap_or("");
    let Some(rule) = catalog()["extensions"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|rule| rule["name"] == name && rule["version"] == node["version"])
    else {
        return Ok(());
    };
    if let Some(parent) = rule.get("parent")
        && parent_slot
            != Some((
                parent["name"].as_str().unwrap_or(""),
                parent["slot"].as_str().unwrap_or(""),
            ))
    {
        return Err(reader(
            path,
            "extension is outside its registered parent slot",
        ));
    }
    for key in node["props"]
        .as_object()
        .into_iter()
        .flat_map(|props| props.keys())
    {
        if rule["props"].get(key).is_none() {
            return Err(reader(
                &format!("{path}/props"),
                "unregistered extension prop",
            ));
        }
    }
    for (slot, spec) in rule["slots"]
        .as_object()
        .into_iter()
        .flat_map(|slots| slots.iter())
    {
        let items = node
            .get("slots")
            .and_then(|slots| slots.get(slot))
            .and_then(Value::as_array);
        if items.is_none() && spec["optional"] != true {
            return Err(reader(path, "required extension slot is missing"));
        }
        if let Some(items) = items {
            if items.len()
                < usize::try_from(spec["minItems"].as_u64().unwrap_or(0)).unwrap_or(usize::MAX)
            {
                return Err(reader(
                    &format!("{path}/slots/{slot}"),
                    "extension slot has too few items",
                ));
            }
            for (i, item) in items.iter().enumerate() {
                if !slot_item_matches(item, spec) {
                    return Err(reader(
                        &format!("{path}/slots/{slot}/{i}"),
                        "extension slot item has the wrong kind",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn slot_item_matches(item: &Value, spec: &Value) -> bool {
    let kind = item["type"].as_str().unwrap_or("");
    match spec["kind"].as_str().unwrap_or("") {
        "block_extension" => kind == "block_extension" && item["name"] == spec["name"],
        "inline_extension" => kind == "inline_extension" && item["name"] == spec["name"],
        "block" => in_ast_category(kind, "document"),
        "inline" => in_ast_category(kind, "paragraph"),
        _ => false,
    }
}

fn in_ast_category(kind: &str, host: &str) -> bool {
    let Ok(defs) = definitions() else {
        return false;
    };
    defs.pointer(&format!("/$defs/{host}/properties/children/items/oneOf"))
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item["$ref"] == format!("#/$defs/{kind}"))
        })
}
