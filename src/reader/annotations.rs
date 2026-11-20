//! C2-A-203, C2-A-207 and C2-A-214 annotation-range sidecar checks.

use super::{TreeError, reader, schema};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

fn data() -> &'static Value {
    static SLOTS: OnceLock<Value> = OnceLock::new();
    SLOTS.get_or_init(|| {
        serde_json::from_str(include_str!("../../spec-data/node-slots.json")).unwrap_or(Value::Null)
    })
}

fn sidecar_schema() -> Result<&'static Value, TreeError> {
    static SCHEMA: OnceLock<Result<Value, String>> = OnceLock::new();
    let parsed = SCHEMA.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../spec-data/annotation-ranges.schema.json"
        ))
        .map_err(|error: serde_json::Error| error.to_string())
    });
    parsed.as_ref().map_err(|reason| TreeError::Schema {
        path: String::new(),
        reason: reason.clone(),
    })
}

fn node_at<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let node = root.pointer(path)?;
    node.get("type").is_some().then_some(node)
}

fn child_text(value: &Value, root: &Value) -> String {
    match value {
        Value::Array(items) => items.iter().map(|item| child_text(item, root)).collect(),
        Value::Object(map) if map.contains_key("type") => annotation_text(value, root),
        Value::Object(map) => map.values().map(|item| child_text(item, root)).collect(),
        _ => String::new(),
    }
}

fn annotation_text(node: &Value, root: &Value) -> String {
    let ty = node["type"].as_str().unwrap_or("");
    let mut out = match ty {
        "soft_break" | "hard_break" => "\n".to_string(),
        "non_breaking_space" => "\u{a0}".to_string(),
        "smart_punctuation" => node["source"].as_str().unwrap_or("").to_string(),
        _ => node["value"]
            .as_str()
            .or_else(|| node["alt"].as_str())
            .unwrap_or("")
            .to_string(),
    };
    if let Some(slots) = data()
        .get("nodes")
        .and_then(|nodes| nodes.get(ty))
        .and_then(Value::as_array)
    {
        for slot in slots.iter().filter_map(Value::as_str) {
            let mut values = vec![node];
            for part in slot.split('.') {
                if part == "*" {
                    let name = node["name"].as_str().unwrap_or("");
                    let declared = root["extensions"]
                        .as_array()
                        .and_then(|list| list.iter().find(|decl| decl["name"] == name));
                    let keys = declared.and_then(|decl| decl["slots"].as_array());
                    values = values
                        .iter()
                        .flat_map(|value| {
                            keys.into_iter()
                                .flatten()
                                .filter_map(|key| value.get(key.as_str()?))
                        })
                        .collect();
                } else {
                    values = values.iter().filter_map(|value| value.get(part)).collect();
                }
            }
            for value in values {
                out.push_str(&child_text(value, root));
            }
        }
    }
    out
}

fn under(endpoint: &str, node: &str) -> bool {
    endpoint == node
        || endpoint
            .strip_prefix(node)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Checks the supplied range schema and its binding to the already accepted tree.
pub(crate) fn check_annotations(root: &Value, annotations: &Value) -> Result<(), TreeError> {
    schema::validate(annotations, sidecar_schema()?)?;
    let mut ranges = HashMap::new();
    for (index, range) in annotations["ranges"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let id = range["id"].as_str().unwrap_or("");
        if ranges.insert(id, range).is_some() {
            return Err(reader(
                &format!("/ranges/{index}/id"),
                "duplicate annotation range id",
            ));
        }
        for side in ["start", "end"] {
            let endpoint = &range[side];
            let path = endpoint["path"].as_str().unwrap_or("");
            let at = format!("/ranges/{index}/{side}");
            let node = node_at(root, path)
                .ok_or_else(|| reader(&format!("{at}/path"), "range endpoint names no node"))?;
            let offset = endpoint["offset"].as_u64().unwrap_or(u64::MAX);
            let text = annotation_text(node, root);
            if usize::try_from(offset)
                .map_or(true, |at| at > text.len() || !text.is_char_boundary(at))
            {
                return Err(reader(
                    &format!("{at}/offset"),
                    "offset is not a UTF-8 character boundary",
                ));
            }
        }
    }
    let mut seen = HashSet::new();
    let mut pending = vec![(root, String::new())];
    while let Some((value, path)) = pending.pop() {
        match value {
            Value::Array(items) => pending.extend(
                items
                    .iter()
                    .enumerate()
                    .map(|(i, value)| (value, format!("{path}/{i}"))),
            ),
            Value::Object(map) => {
                if value.get("type").is_some()
                    && let Some(ids) = value
                        .pointer("/data/carve.annotation/ids")
                        .and_then(Value::as_array)
                {
                    for id in ids.iter().filter_map(Value::as_str) {
                        if !seen.insert(id) {
                            continue;
                        }
                        let range = ranges.get(id).ok_or_else(|| {
                            reader(
                                &format!("{path}/data/carve.annotation"),
                                "annotation has no range",
                            )
                        })?;
                        if !["start", "end"]
                            .iter()
                            .filter_map(|side| range.get(*side)?.get("path")?.as_str())
                            .any(|endpoint| under(endpoint, &path))
                        {
                            return Err(reader(
                                &format!("{path}/data/carve.annotation"),
                                "range has no endpoint on this node or a descendant",
                            ));
                        }
                    }
                }
                pending.extend(
                    map.iter()
                        .filter(|(key, _)| {
                            !matches!(
                                key.as_str(),
                                "data"
                                    | "props"
                                    | "payload"
                                    | "source"
                                    | "resources"
                                    | "extensions"
                            )
                        })
                        .map(|(key, value)| {
                            (
                                value,
                                format!("{path}/{}", key.replace('~', "~0").replace('/', "~1")),
                            )
                        }),
                );
            }
            _ => {}
        }
    }
    Ok(())
}
